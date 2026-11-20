use super::dynamic_source::dynamic_source_kind_for_function_id;
use super::*;

pub(super) use crate::prepared_source_cache::MAX_SOURCE_CANDIDATES;

// Optional compilation hints never replace an expression or authorize a call.
// Prepared constructors and eval still check the live arguments before dispatch.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FiniteSourceValue {
    Text(String),
    // Additive hints may use spare capacity but never displace existing sources.
    SupplementalText(String),
    Function(FunctionId),
    ReturnedCallableSource(crate::prepared_source_cache::ReturnedCallableSourceId),
    FunctionConstructor(DynamicFunctionKind),
    Record(BTreeMap<String, Vec<FiniteSourceValue>>),
    Array(Vec<FiniteSourceValue>),
}

impl FiniteSourceValue {
    pub(super) fn supplemental(self) -> Self {
        match self {
            Self::Text(text) | Self::SupplementalText(text) => Self::SupplementalText(text),
            Self::Record(properties) => Self::Record(
                properties
                    .into_iter()
                    .map(|(name, values)| {
                        (name, values.into_iter().map(Self::supplemental).collect())
                    })
                    .collect(),
            ),
            Self::Array(values) => {
                Self::Array(values.into_iter().map(Self::supplemental).collect())
            }
            value @ (Self::Function(_)
            | Self::ReturnedCallableSource(_)
            | Self::FunctionConstructor(_)) => value,
        }
    }

    fn is_supplemental(&self) -> bool {
        match self {
            Self::SupplementalText(_) | Self::ReturnedCallableSource(_) => true,
            Self::Record(properties) => {
                !properties.is_empty() && properties.values().flatten().all(Self::is_supplemental)
            }
            Self::Array(values) => !values.is_empty() && values.iter().all(Self::is_supplemental),
            Self::Text(_) | Self::Function(_) | Self::FunctionConstructor(_) => false,
        }
    }

    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) | Self::SupplementalText(text) => Some(text),
            Self::Function(_)
            | Self::ReturnedCallableSource(_)
            | Self::FunctionConstructor(_)
            | Self::Record(_)
            | Self::Array(_) => None,
        }
    }

    pub(super) fn primary_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::SupplementalText(_)
            | Self::Function(_)
            | Self::ReturnedCallableSource(_)
            | Self::FunctionConstructor(_)
            | Self::Record(_)
            | Self::Array(_) => None,
        }
    }

    pub(super) fn concatenate_text(&self, right: &Self) -> Option<Self> {
        let text = format!("{}{}", self.text()?, right.text()?);
        Some(
            if matches!(self, Self::SupplementalText(_))
                || matches!(right, Self::SupplementalText(_))
            {
                Self::SupplementalText(text)
            } else {
                Self::Text(text)
            },
        )
    }
}

pub(super) fn bounded_candidates(
    candidates: impl IntoIterator<Item = FiniteSourceValue>,
) -> Vec<FiniteSourceValue> {
    let mut distinct = BTreeSet::new();
    let mut supplemental = BTreeSet::new();
    for candidate in candidates {
        if candidate.is_supplemental() {
            if supplemental.len() < MAX_SOURCE_CANDIDATES {
                supplemental.insert(candidate);
            }
        } else {
            distinct.insert(candidate);
            if distinct.len() > MAX_SOURCE_CANDIDATES {
                return Vec::new();
            }
        }
    }
    for candidate in supplemental {
        if distinct.len() == MAX_SOURCE_CANDIDATES {
            break;
        }
        if !matches!(&candidate, FiniteSourceValue::SupplementalText(text)
            if distinct.contains(&FiniteSourceValue::Text(text.clone())))
        {
            distinct.insert(candidate);
        }
    }
    distinct.into_iter().collect()
}

/// One entry of a lowerer's finite source candidate maps.
///
/// Every child lowerer starts from its parent's maps and merges each entry
/// back through `bounded_candidates`. Entries are shared, so that copy does
/// not clone candidate trees, and `bounded` marks a list that is itself a
/// `bounded_candidates` output: merging such a list with itself returns it
/// unchanged, which lets the merge skip entries a child never replaced.
#[derive(Clone, Default)]
pub(super) struct SourceCandidates {
    values: std::sync::Arc<Vec<FiniteSourceValue>>,
    bounded: bool,
}

impl SourceCandidates {
    pub(super) fn bounded(candidates: impl IntoIterator<Item = FiniteSourceValue>) -> Self {
        Self {
            values: std::sync::Arc::new(bounded_candidates(candidates)),
            bounded: true,
        }
    }

    pub(super) fn into_values(self) -> Vec<FiniteSourceValue> {
        std::sync::Arc::unwrap_or_clone(self.values)
    }

    /// Mutating in place may leave a list `bounded_candidates` would change.
    pub(super) fn make_mut(&mut self) -> &mut Vec<FiniteSourceValue> {
        self.bounded = false;
        std::sync::Arc::make_mut(&mut self.values)
    }

    /// `bounded_candidates(previous ++ observed)`. A bounded list merged with
    /// the same shared list is a fixed point: it is sorted and distinct, its
    /// non-supplemental values fit the budget, and every supplemental value in
    /// it was admitted against the same non-supplemental values.
    pub(super) fn merged(previous: Option<Self>, observed: Self) -> Self {
        match previous {
            Some(previous)
                if previous.bounded
                    && std::sync::Arc::ptr_eq(&previous.values, &observed.values) =>
            {
                previous
            }
            previous => Self::bounded(
                previous
                    .map(Self::into_values)
                    .unwrap_or_default()
                    .into_iter()
                    .chain(observed.into_values()),
            ),
        }
    }
}

impl From<Vec<FiniteSourceValue>> for SourceCandidates {
    fn from(values: Vec<FiniteSourceValue>) -> Self {
        Self {
            values: std::sync::Arc::new(values),
            bounded: false,
        }
    }
}

impl std::ops::Deref for SourceCandidates {
    type Target = [FiniteSourceValue];

    fn deref(&self) -> &[FiniteSourceValue] {
        &self.values
    }
}

// Equality is the candidate list; `bounded` only caches a property of it.
impl PartialEq for SourceCandidates {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl ScriptLowerer<'_> {
    // Optional hints may outlive a cell's value, but cannot cross a different
    // Reference selected by ResolveBinding. Use the same declarative cutoff
    // as call lowering: a lexical binding inside with still precedes its
    // Object Environment, including when both are captured by a closure.
    fn finite_source_identifier_has_unintercepted_reference(&self, name: &str) -> bool {
        if self.with_environment_chain.is_empty() {
            return true;
        }
        let reference = self.locate_identifier_reference(name);
        self.with_environment_chain
            .select_preceding(reference.declarative_position())
            .is_none()
    }

    fn finite_function_constructor_kind(&self, function_id: &str) -> Option<DynamicFunctionKind> {
        self.analysis.function_plans.get(function_id).map(|plan| {
            match plan.protocol.source_execution_kind() {
                FunctionExecutionKind::Ordinary => DynamicFunctionKind::Ordinary,
                FunctionExecutionKind::Generator => DynamicFunctionKind::Generator,
                FunctionExecutionKind::Async => DynamicFunctionKind::Async,
                FunctionExecutionKind::AsyncGenerator => DynamicFunctionKind::AsyncGenerator,
            }
        })
    }

    // A resolved data property can retain possible intrinsic targets after
    // effects widen its identity. This supplies source hints, never a Get fold.
    fn finite_source_property_function_candidates(
        &self,
        target: &Expression,
        name: &str,
    ) -> Vec<FiniteSourceValue> {
        let Expression::Identifier(identifier) = Self::unwrap_parenthesized_expr(target) else {
            return Vec::new();
        };
        let name_of_binding = self.interner.resolve_expect(identifier.sym()).to_string();
        if !self.finite_source_identifier_has_unintercepted_reference(&name_of_binding) {
            return Vec::new();
        }
        let info = self
            .lookup_binding(&name_of_binding)
            .map(|binding| binding.heap_shape)
            .unwrap_or_else(|| {
                self.lookup_global_property_info(&name_of_binding)
                    .and_then(|property| property.value_info.heap_shape.clone())
            });
        let Some(ObjectShapeProperty::Data(info)) = info
            .as_deref()
            .and_then(|shape| crate::read_heap_shape_property(shape, name))
        else {
            return Vec::new();
        };
        info.function_targets
            .known_targets()
            .iter()
            .cloned()
            .map(FiniteSourceValue::Function)
            .collect()
    }

    /// A synthetic derived constructor forwards its original argument list.
    /// Heritage grammar hints only add optional compilation units; they cannot
    /// resolve the class's live [[Prototype]], select super, or replace newTarget.
    pub(super) fn finite_synthetic_class_constructor_candidates(
        &self,
        heritage: Option<&Expression>,
        constructor: Option<&FunctionExpression>,
    ) -> Vec<FiniteSourceValue> {
        if constructor.is_some() {
            // An explicit constructor may transform or replace the arguments.
            // Those source tuples need separate discovery; do not guess forwarding.
            return Vec::new();
        }
        let mut candidates = heritage
            .map(|expression| self.function_source_value_candidates(expression))
            .unwrap_or_default();
        if candidates.is_empty() {
            if let Some(Expression::Identifier(identifier)) =
                heritage.map(Self::unwrap_parenthesized_expr)
            {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                // A preceding with can intercept the global Function Reference.
                // Its intrinsic fallback is still a possible grammar, not a
                // borrowed declarative-cell hint or a resolved heritage value.
                // Class evaluation retains HasBinding/GetValue and live super.
                if name == "Function"
                    && !self.finite_source_identifier_has_unintercepted_reference(&name)
                    && self.lookup_binding(&name).is_none()
                    && self
                        .lookup_global_property_info(&name)
                        .is_some_and(|property| {
                            property
                                .value_info
                                .function_targets
                                .known_targets()
                                .iter()
                                .any(|target| {
                                    dynamic_source_kind_for_function_id(target)
                                        == Some(DynamicSourceKind::Function(
                                            DynamicFunctionKind::Ordinary,
                                        ))
                                })
                        })
                {
                    candidates.push(FiniteSourceValue::FunctionConstructor(
                        DynamicFunctionKind::Ordinary,
                    ));
                }
            }
        }
        bounded_candidates(
            candidates
                .into_iter()
                .filter_map(|candidate| match candidate {
                    FiniteSourceValue::FunctionConstructor(kind) => {
                        Some(FiniteSourceValue::FunctionConstructor(kind))
                    }
                    FiniteSourceValue::Function(function_id) => {
                        match dynamic_source_kind_for_function_id(&function_id) {
                            Some(DynamicSourceKind::Function(kind)) => {
                                Some(FiniteSourceValue::FunctionConstructor(kind))
                            }
                            Some(
                                DynamicSourceKind::DirectEval
                                | DynamicSourceKind::IndirectEval
                                | DynamicSourceKind::RealmEvalScript
                                | DynamicSourceKind::ShadowRealmEvaluate,
                            )
                            | None => None,
                        }
                    }
                    FiniteSourceValue::Text(_)
                    | FiniteSourceValue::SupplementalText(_)
                    | FiniteSourceValue::ReturnedCallableSource(_)
                    | FiniteSourceValue::Record(_)
                    | FiniteSourceValue::Array(_) => None,
                }),
        )
    }

    pub(super) fn register_finite_undefined_var_source_candidate(&mut self, name: &str) {
        let Some(key) = self.finite_source_binding_key(name) else {
            return;
        };
        // Install this hint during hoisting so forward references inside each
        // fresh child lowerer see it before calls invalidate their flow facts.
        // Redeclarations write nothing; runtime values and conversions stay live.
        let previous = self
            .function_source_binding_candidates
            .remove(&key)
            .unwrap_or_default();
        self.function_source_binding_candidates.insert(
            key,
            SourceCandidates::bounded(
                previous
                    .into_values()
                    .into_iter()
                    .chain([FiniteSourceValue::SupplementalText("undefined".to_string())]),
            ),
        );
    }

    pub(super) fn finite_binding_source_candidates(
        &self,
        name: &str,
    ) -> Option<&[FiniteSourceValue]> {
        let key = self.finite_source_binding_key(name)?;
        self.function_source_binding_candidates
            .get(&key)
            .map(|candidates| &**candidates)
    }

    pub(super) fn function_source_value_candidates(
        &self,
        expression: &Expression,
    ) -> Vec<FiniteSourceValue> {
        #[cfg(test)]
        let _discovery_work = super::finite_regexp_source::enter_source_candidate_discovery();
        if super::binary_chain_longer_than(expression, super::MAX_SOURCE_CANDIDATE_CHAIN) {
            return Vec::new();
        }
        let expression = Self::unwrap_parenthesized_expr(expression);
        let prototype_source = match expression {
            Expression::Call(call) => match Self::unwrap_parenthesized_expr(call.function()) {
                Expression::PropertyAccess(PropertyAccess::Simple(access))
                    if matches!(access.field(), PropertyAccessField::Const(name)
                        if self.interner.resolve_expect(name.sym()).to_string() == "getPrototypeOf")
                        && matches!(Self::unwrap_parenthesized_expr(access.target()), Expression::Identifier(identifier)
                        if matches!(self.interner.resolve_expect(identifier.sym()).to_string().as_str(), "Object" | "Reflect")) =>
                {
                    call.args().first().map(Self::unwrap_parenthesized_expr)
                }
                _ => None,
            },
            _ => Some(expression),
        };
        let constructor_kind = prototype_source.and_then(|source| match source {
            Expression::FunctionExpression(_) | Expression::ArrowFunction(_) => {
                Some(DynamicFunctionKind::Ordinary)
            }
            Expression::GeneratorExpression(_) => Some(DynamicFunctionKind::Generator),
            Expression::AsyncFunctionExpression(_) | Expression::AsyncArrowFunction(_) => {
                Some(DynamicFunctionKind::Async)
            }
            Expression::AsyncGeneratorExpression(_) => Some(DynamicFunctionKind::AsyncGenerator),
            _ => None,
        });
        if let Some(kind) = constructor_kind {
            let mut candidates = vec![FiniteSourceValue::Record(BTreeMap::from([(
                "constructor".to_string(),
                vec![FiniteSourceValue::FunctionConstructor(kind)],
            )]))];
            if let Some(function_id) = match expression {
                Expression::FunctionExpression(function) => self
                    .analysis
                    .function_expr_ids
                    .get(&function_expression_key(function)),
                Expression::ArrowFunction(function) => self
                    .analysis
                    .function_expr_ids
                    .get(&arrow_function_key(function)),
                _ => None,
            } {
                candidates.push(FiniteSourceValue::Function(function_id.clone()));
            }
            return candidates;
        }
        // Filter result and callee views share the same receiver discovery.
        // Rediscovering a chained receiver for both views would double its
        // complete subtree at each call, despite the finite candidate budget.
        let filter_receiver = match expression {
            Expression::Call(call) => match Self::unwrap_parenthesized_expr(call.function()) {
                Expression::PropertyAccess(PropertyAccess::Simple(access))
                    if self.finite_source_access_name(access).as_deref() == Some("filter") =>
                {
                    Some((
                        access,
                        self.function_source_value_candidates(access.target()),
                    ))
                }
                _ => None,
            },
            _ => None,
        };
        // Prototype, RegExp and String views also reuse this callee discovery.
        let callee_candidates = match expression {
            Expression::Call(call) => match &filter_receiver {
                Some((access, receiver)) => self.finite_property_access_source_candidates(
                    Self::unwrap_parenthesized_expr(call.function()),
                    access,
                    receiver.clone(),
                ),
                None => self.function_source_value_candidates(call.function()),
            },
            Expression::New(construct) => {
                self.function_source_value_candidates(construct.constructor())
            }
            _ => Vec::new(),
        };
        if let Expression::Call(call) = expression {
            let reads_prototype = callee_candidates.iter().any(|candidate| {
                matches!(candidate,
                    FiniteSourceValue::Function(function_id)
                    if matches!(StandardBuiltinId::from_function_id(function_id),
                        Some(StandardBuiltinId::ObjectGetPrototypeOf | StandardBuiltinId::ReflectGetPrototypeOf)))
            });
            if reads_prototype {
                let constructors = bounded_candidates(
                    call.args()
                        .first()
                        .into_iter()
                        .flat_map(|source| self.function_source_value_candidates(source))
                        .filter_map(|candidate| match candidate {
                            FiniteSourceValue::Function(function_id) => self
                                .finite_function_constructor_kind(&function_id)
                                .map(FiniteSourceValue::FunctionConstructor),
                            _ => None,
                        }),
                );
                if !constructors.is_empty() {
                    if let Some(text) = self.function_source_candidate(expression) {
                        // Generic call discovery already retained this primary
                        // conversion view. An additive prototype record must
                        // not replace it or consume another primary slot in an
                        // enclosing candidate set. Keep the original text view
                        // when both interpretations are possible.
                        return vec![FiniteSourceValue::Text(text)];
                    }
                    // The actual prototype call and later constructor Get may
                    // invoke arbitrary code. Only the possible grammar travels.
                    return vec![FiniteSourceValue::Record(BTreeMap::from([(
                        "constructor".to_string(),
                        constructors,
                    )]))];
                }
            }
        }
        match expression {
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if !self.finite_source_identifier_has_unintercepted_reference(&name) {
                    return Vec::new();
                }
                let mut candidates = self
                    .finite_binding_source_candidates(&name)
                    .unwrap_or_default()
                    .to_vec();
                if let Some(binding) = self.lookup_binding(&name) {
                    candidates.extend(
                        binding
                            .function_targets
                            .known_targets()
                            .iter()
                            .cloned()
                            .map(FiniteSourceValue::Function),
                    );
                } else {
                    if let Some(function_id) = self.visible_function_names.get(&name) {
                        candidates.push(FiniteSourceValue::Function(function_id.clone()));
                    }
                    if let Some(property) = self.lookup_global_property_info(&name) {
                        candidates.extend(
                            property
                                .value_info
                                .function_targets
                                .known_targets()
                                .iter()
                                .cloned()
                                .map(FiniteSourceValue::Function),
                        );
                    }
                }
                if name == "eval" {
                    candidates.push(FiniteSourceValue::Function(
                        StandardBuiltinId::EvalFunction.function_id(),
                    ));
                }
                // Identifier source hints belong to the resolved cell. A bare
                // static-string map cloned from another owner cannot seed it.
                candidates.extend(
                    self.function_source_candidate(expression)
                        .map(FiniteSourceValue::Text),
                );
                if !candidates.is_empty() {
                    return bounded_candidates(candidates);
                }
            }
            Expression::ClassExpression(class) => {
                return self.finite_synthetic_class_constructor_candidates(
                    class.super_ref(),
                    class.constructor(),
                );
            }
            Expression::ArrayLiteral(array) => {
                let elements = bounded_candidates(
                    array
                        .as_ref()
                        .iter()
                        .flatten()
                        .flat_map(|element| self.function_source_value_candidates(element)),
                );
                return vec![FiniteSourceValue::Array(elements)];
            }
            Expression::ObjectLiteral(object) => {
                let properties = object
                    .properties()
                    .iter()
                    .filter_map(|property| {
                        let (name, candidates) = match property {
                            PropertyDefinition::Property(name, value) => {
                                (name, self.function_source_value_candidates(value))
                            }
                            PropertyDefinition::MethodDefinition(method)
                                if matches!(
                                    method.kind(),
                                    MethodDefinitionKind::Ordinary | MethodDefinitionKind::Get
                                ) =>
                            {
                                let function_id = self
                                    .analysis
                                    .function_expr_ids
                                    .get(&object_method_key(method))?;
                                let candidates =
                                    vec![FiniteSourceValue::Function(function_id.clone())];
                                let candidates = if method.kind() == MethodDefinitionKind::Get {
                                    self.finite_source_call_returns(candidates)
                                } else {
                                    candidates
                                };
                                (method.name(), candidates)
                            }
                            _ => return None,
                        };
                        let name = match name {
                            PropertyName::Computed(expression) => self
                                .finite_source_computed_key_candidate(expression)
                                .or_else(|| self.property_name_to_static_key(name)),
                            PropertyName::Literal(_) => self.property_name_to_static_key(name),
                        }?;
                        (!candidates.is_empty()).then_some((name, candidates))
                    })
                    .collect();
                let mut candidates = vec![FiniteSourceValue::Record(properties)];
                if let Some(text) = self.function_source_candidate(expression) {
                    // A possible conversion is another view of the same object,
                    // not an extra primary value competing with its properties.
                    candidates.push(FiniteSourceValue::SupplementalText(text));
                }
                return candidates;
            }
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                return self.finite_property_access_source_candidates(
                    expression,
                    access,
                    self.function_source_value_candidates(access.target()),
                );
            }
            Expression::Binary(binary) if binary.op() == BinaryOp::Comma => {
                return self.function_source_value_candidates(binary.rhs());
            }
            Expression::Conditional(conditional) => {
                return bounded_candidates(
                    self.function_source_value_candidates(conditional.if_true())
                        .into_iter()
                        .chain(self.function_source_value_candidates(conditional.if_false())),
                );
            }
            Expression::TemplateLiteral(template) => {
                let hints = self.finite_template_source_candidates(template);
                if !hints.is_empty() {
                    return bounded_candidates(
                        self.function_source_candidate(expression)
                            .map(FiniteSourceValue::Text)
                            .into_iter()
                            .chain(hints),
                    );
                }
            }
            Expression::Binary(binary)
                if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) =>
            {
                if let Some(text) = self.function_source_candidate(expression) {
                    return vec![FiniteSourceValue::Text(text)];
                }
                let left = self.function_source_value_candidates(binary.lhs());
                let right = self.function_source_value_candidates(binary.rhs());
                return bounded_candidates(left.iter().flat_map(|left| {
                    right
                        .iter()
                        .filter_map(move |right| left.concatenate_text(right))
                }));
            }
            Expression::RegExpLiteral(_) | Expression::Call(_) | Expression::New(_) => {
                let mut hints =
                    self.finite_regexp_source_candidates(expression, &callee_candidates);
                if let Expression::Call(call) = expression {
                    hints.extend(
                        self.finite_returned_callable_source_candidates(call, &callee_candidates),
                    );
                    hints.extend(
                        self.finite_string_call_source_candidates(
                            call,
                            &callee_candidates,
                            filter_receiver
                                .as_ref()
                                .map(|(_, receiver)| receiver.as_slice()),
                        ),
                    );
                }
                if !hints.is_empty() {
                    return bounded_candidates(
                        self.function_source_candidate(expression)
                            .map(FiniteSourceValue::Text)
                            .into_iter()
                            .chain(hints),
                    );
                }
            }
            _ => {}
        }
        self.function_source_candidate(expression)
            .map(FiniteSourceValue::Text)
            .into_iter()
            .collect()
    }

    pub(super) fn finite_source_access_name(
        &self,
        access: &boa_ast::expression::access::SimplePropertyAccess,
    ) -> Option<String> {
        match access.field() {
            PropertyAccessField::Const(identifier) => {
                Some(self.interner.resolve_expect(identifier.sym()).to_string())
            }
            PropertyAccessField::Expr(expression) => {
                self.finite_source_computed_key_candidate(expression)
            }
        }
    }

    fn finite_property_access_source_candidates(
        &self,
        expression: &Expression,
        access: &boa_ast::expression::access::SimplePropertyAccess,
        target_candidates: Vec<FiniteSourceValue>,
    ) -> Vec<FiniteSourceValue> {
        let name = self.finite_source_access_name(access);
        let mut candidates =
            bounded_candidates(target_candidates.into_iter().flat_map(|candidate| {
                match candidate {
                    FiniteSourceValue::Record(mut properties) => name
                        .as_ref()
                        .and_then(|name| properties.remove(name))
                        .unwrap_or_default(),
                    FiniteSourceValue::Array(elements)
                        if matches!(access.field(), PropertyAccessField::Expr(_)) =>
                    {
                        elements
                    }
                    FiniteSourceValue::Function(function_id)
                        if name.as_deref() == Some("constructor") =>
                    {
                        // A possible inherited constructor supplies
                        // source hints only. Own properties, prototype
                        // mutations and the resolved callable stay live.
                        self.finite_function_constructor_kind(&function_id)
                            .map(FiniteSourceValue::FunctionConstructor)
                            .into_iter()
                            .collect()
                    }
                    FiniteSourceValue::Text(_)
                    | FiniteSourceValue::SupplementalText(_)
                    | FiniteSourceValue::Function(_)
                    | FiniteSourceValue::ReturnedCallableSource(_)
                    | FiniteSourceValue::FunctionConstructor(_)
                    | FiniteSourceValue::Array(_) => Vec::new(),
                }
            }));
        if let Some(name) = &name {
            candidates
                .extend(self.finite_source_property_function_candidates(access.target(), name));
        }
        if candidates.is_empty()
            && name.as_deref() == Some("Function")
            && self.function_source_candidate(expression).is_none()
        {
            // Like a directly called `.Function`, an otherwise unknown
            // property can supply a possible grammar, including another
            // realm's intrinsic. Its Get, identity, arguments and realm
            // remain live; do not displace an existing finite value view.
            candidates.push(FiniteSourceValue::FunctionConstructor(
                DynamicFunctionKind::Ordinary,
            ));
        }
        let evaluator = match name.as_deref() {
            Some("eval") => Some(StandardBuiltinId::EvalFunction.function_id()),
            Some("evalScript") => Some(
                DynamicSourceIntrinsic::RealmEvalScript
                    .function_id()
                    .to_string(),
            ),
            Some("evaluate") => Some(StandardBuiltinId::ShadowRealmPrototypeEvaluate.function_id()),
            Some("importValue") => {
                Some(StandardBuiltinId::ShadowRealmPrototypeImportValue.function_id())
            }
            _ => None,
        };
        candidates.extend(evaluator.map(FiniteSourceValue::Function));
        if !candidates.is_empty() {
            return bounded_candidates(candidates);
        }
        self.function_source_candidate(expression)
            .map(FiniteSourceValue::Text)
            .into_iter()
            .collect()
    }

    pub(super) fn function_source_argument_candidates(
        &self,
        arguments: &[&Expression],
    ) -> Vec<Vec<String>> {
        let candidates = arguments
            .iter()
            .map(|argument| self.function_source_value_candidates(argument))
            .collect::<Vec<_>>();
        let expanded = Self::source_candidate_tuples(&candidates, true);
        if expanded.is_empty() {
            // A supplemental hint must not turn a previously fitting complete
            // Cartesian product into an unsupported call.
            Self::source_candidate_tuples(&candidates, false)
        } else {
            expanded
        }
    }

    pub(super) fn finite_correlated_template_source_hints(
        &self,
        template: &TemplateLiteral,
        parts: &[Vec<FiniteSourceValue>],
    ) -> Vec<FiniteSourceValue> {
        let mut bindings = BTreeMap::new();
        let mut choices = Vec::new();
        let mut positions = Vec::new();
        let mut repeated_binding = false;
        for (index, element) in template.elements().iter().enumerate() {
            let key = match element {
                TemplateElement::Expr(expression) => {
                    match Self::unwrap_parenthesized_expr(expression) {
                        Expression::Identifier(identifier) => {
                            let name = self.interner.resolve_expect(identifier.sym()).to_string();
                            if !self.finite_source_identifier_has_unintercepted_reference(&name) {
                                return Vec::new();
                            }
                            self.finite_source_binding_key(&name)
                        }
                        _ => None,
                    }
                }
                TemplateElement::String(_) => None,
            };
            if let Some(position) = key.as_ref().and_then(|key| bindings.get(key)) {
                repeated_binding = true;
                positions.push(*position);
                continue;
            }
            let position = choices.len();
            if let Some(key) = key {
                bindings.insert(key, position);
            }
            positions.push(position);
            // Share the original discovery results. Rediscovering a nested
            // template after every failed product would double subtree work.
            choices.push(parts[index].clone());
        }
        if !repeated_binding {
            return Vec::new();
        }
        // This is an additional bounded search, not a proof that live reads
        // correlate. Each analyzed cell proposes one text choice; other holes
        // stay independent. Assignment, Get and ToString still execute in the
        // original template, and only its exact actual source can dispatch.
        // Effectful or otherwise unlisted strings retain the prior typed AOT
        // rejection. Never replace the complete product when it already fits.
        Self::source_candidate_tuples(&choices, true)
            .into_iter()
            .map(|choice| {
                let mut source = String::new();
                for position in &positions {
                    source.push_str(&choice[*position]);
                }
                FiniteSourceValue::SupplementalText(source)
            })
            .collect()
    }

    pub(super) fn source_candidate_tuples(
        arguments: &[Vec<FiniteSourceValue>],
        include_supplemental: bool,
    ) -> Vec<Vec<String>> {
        let mut tuples = vec![Vec::new()];
        for candidates in arguments {
            let strings = candidates
                .iter()
                .filter_map(|candidate| {
                    if include_supplemental {
                        candidate.text()
                    } else {
                        candidate.primary_text()
                    }
                })
                .collect::<BTreeSet<_>>();
            if strings.is_empty()
                || tuples.len().saturating_mul(strings.len()) > MAX_SOURCE_CANDIDATES
            {
                return Vec::new();
            }
            tuples = tuples
                .into_iter()
                .flat_map(|tuple| {
                    strings.iter().map(move |text| {
                        let mut next = tuple.clone();
                        next.push((*text).to_string());
                        next
                    })
                })
                .collect();
        }
        tuples
    }

    pub(super) fn register_finite_source_binding_assignment(
        &mut self,
        name: &str,
        value: &Expression,
    ) {
        let Some(key) = self.finite_source_binding_key(name) else {
            return;
        };
        let candidates = self.function_source_value_candidates(value);
        let previous = self
            .function_source_binding_candidates
            .remove(&key)
            .unwrap_or_default();
        self.function_source_binding_candidates.insert(
            key,
            SourceCandidates::bounded(previous.into_values().into_iter().chain(candidates)),
        );
    }

    pub(super) fn merge_function_source_parameter_candidates(
        &mut self,
        observations: BTreeMap<(FunctionId, usize), impl Into<SourceCandidates>>,
    ) {
        for (key, candidates) in observations {
            let previous = self.function_source_parameter_candidates.remove(&key);
            self.function_source_parameter_candidates
                .insert(key, SourceCandidates::merged(previous, candidates.into()));
        }
    }

    pub(super) fn register_source_call_argument_candidates(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) {
        let callees = self.function_source_value_candidates(callee);
        self.register_returned_callable_argument_candidates(&callees, arguments);
        let mut targets = callees
            .into_iter()
            .filter_map(|candidate| match candidate {
                FiniteSourceValue::Function(function_id) => Some((function_id, false)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        if let Expression::Identifier(identifier) = Self::unwrap_parenthesized_expr(callee) {
            let name = self.interner.resolve_expect(identifier.sym()).to_string();
            if self.with_environment_chain.is_empty()
                && !self.uses_runtime_identifier_environment()
                && self.is_unshadowed_script_global_binding(&name)
            {
                // Body entry replaces the visible map with owner-local
                // declarations. Recover this resolved global declaration only
                // to observe actual recursive/mutual call arguments; its value
                // and the runtime call remain untouched.
                if let Some(function_id) = self.analysis.owner_plans[SCRIPT_OWNER_ID]
                    .function_bindings
                    .get(&name)
                {
                    targets.entry(function_id.clone()).or_insert(true);
                }
            }
        }
        let mut observations = BTreeMap::new();
        let mut supplemental_observations = BTreeMap::new();
        for (function_id, supplemental) in targets {
            let Some(plan) = self.analysis.function_plans.get(&function_id) else {
                continue;
            };
            for (index, (parameter, argument)) in
                plan.parameters.as_ref().iter().zip(arguments).enumerate()
            {
                // A spread destroys the positional correspondence. Hints never
                // supply values or replace the call's runtime argument iterator.
                if parameter.is_rest_param() || matches!(argument, Expression::Spread(_)) {
                    break;
                }
                let candidates = self.function_source_value_candidates(argument);
                if !candidates.is_empty() {
                    let key = (function_id.clone(), index);
                    if supplemental {
                        supplemental_observations.insert(
                            key,
                            candidates
                                .into_iter()
                                .map(FiniteSourceValue::supplemental)
                                .collect::<Vec<_>>(),
                        );
                    } else {
                        observations.insert(key, candidates);
                    }
                }
            }
        }
        self.merge_function_source_parameter_candidates(observations);
        for (key, candidates) in supplemental_observations {
            let previous = self
                .function_source_parameter_candidates
                .remove(&key)
                .unwrap_or_default();
            let merged = SourceCandidates::bounded(previous.iter().cloned().chain(candidates));
            // Optional structured hints can also overflow the closed budget.
            // They must not erase an already admitted primary candidate set.
            self.function_source_parameter_candidates.insert(
                key,
                if merged.is_empty() && !previous.is_empty() {
                    previous
                } else {
                    merged
                },
            );
        }
    }

    pub(super) fn register_array_callback_source_candidates(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) {
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) =
            Self::unwrap_parenthesized_expr(callee)
        else {
            return;
        };
        let PropertyAccessField::Const(method) = access.field() else {
            return;
        };
        let method = self.interner.resolve_expect(method.sym()).to_string();
        if !matches!(
            method.as_str(),
            "forEach" | "map" | "filter" | "some" | "every" | "find" | "findIndex"
        ) {
            return;
        }
        let Some(callback) = arguments.first() else {
            return;
        };
        let key = match Self::unwrap_parenthesized_expr(callback) {
            Expression::ArrowFunction(function) => arrow_function_key(function),
            Expression::FunctionExpression(function) => function_expression_key(function),
            _ => return,
        };
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            return;
        };
        let candidates = bounded_candidates(
            self.function_source_value_candidates(access.target())
                .into_iter()
                .flat_map(|candidate| match candidate {
                    FiniteSourceValue::Array(elements) => elements,
                    FiniteSourceValue::Text(_)
                    | FiniteSourceValue::SupplementalText(_)
                    | FiniteSourceValue::Function(_)
                    | FiniteSourceValue::ReturnedCallableSource(_)
                    | FiniteSourceValue::FunctionConstructor(_)
                    | FiniteSourceValue::Record(_) => Vec::new(),
                }),
        );
        if candidates.is_empty() {
            return;
        }
        self.merge_function_source_parameter_candidates(BTreeMap::from([(
            (function_id, 0),
            candidates,
        )]));
    }

    pub(super) fn install_function_parameter_source_candidates(
        &mut self,
        function_id: &str,
        index: usize,
        parameter: &Binding,
        default_candidates: Vec<FiniteSourceValue>,
    ) {
        let Binding::Identifier(identifier) = parameter else {
            return;
        };
        let candidates = bounded_candidates(
            self.function_source_parameter_candidates
                .get(&(function_id.to_string(), index))
                .into_iter()
                .flat_map(|candidates| candidates.iter())
                .cloned()
                .chain(default_candidates),
        );
        if candidates.is_empty() {
            return;
        }
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        self.set_finite_source_binding_candidates(&name, candidates);
    }
}
