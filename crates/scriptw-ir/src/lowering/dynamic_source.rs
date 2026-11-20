use super::*;

/// Proof that the dynamic source text is fixed by syntax before lowering.
///
/// Its constructors are private to this module. A folded `ExprIr::String`
/// therefore cannot be promoted to AOT-known source by a downstream caller.
enum DynamicSourceProof {
    Runtime,
    AotSyntax,
}

/// The exhaustive result of resolving a call to a dynamic-source identity.
///
/// Call lowering must consume this value before it can emit executable IR. The
/// pass-through variant proves that `%eval%` never reaches source evaluation.
/// Invocation variants retain runtime argument checks and guarded source dispatch.
#[must_use = "resolved dynamic-source calls must consume their admitted proof or record their typed gap"]
pub(super) enum ResolvedDynamicSourceCall {
    EvalPassThrough(ProvenEvalPassThrough),
    IndirectEvalInvocation(AdmittedIndirectEvalInvocation),
    RealmScriptInvocation(AdmittedRealmScriptInvocation),
    ShadowRealmInvocation(AdmittedShadowRealmInvocation),
    RealmScriptConversionThrow(ProvenRealmScriptConversionThrow),
    FunctionInvocation(AdmittedFunctionInvocation),
    CompiledScript(ProvenCompiledScript),
    Unsupported(UnsupportedDynamicSourceCall),
}

// A prepared source is available; the live callable and source must still match
// at runtime, including after arbitrary mutation outside the finite candidates.
pub(super) struct ProvenCompiledScript(());

impl ProvenCompiledScript {
    pub(super) fn into_result_info(self) -> ValueInfo {
        ValueInfo::new(ValueKind::Dynamic)
    }
}

/// The intrinsic checks the live first argument before selecting source text.
/// Non-Strings pass through unchanged; a native host compiles an unprepared
/// String after the actual owned prepared hook misses. This proves neither a
/// pass-through value nor a source unit and emits no parser into Wasm.
pub(super) struct AdmittedIndirectEvalInvocation(());

impl AdmittedIndirectEvalInvocation {
    pub(super) fn into_result_info(self) -> ValueInfo {
        ValueInfo::new(ValueKind::Dynamic)
    }
}

/// The actual Realm Script callable retains ordered ToString and captured
/// Realm execution. This admits an invocation, never an inferred receiver or a
/// source unit; finite dispatch and native global compilation stay runtime-owned.
pub(super) struct AdmittedRealmScriptInvocation(());
impl AdmittedRealmScriptInvocation {
    pub(super) fn into_result_info(self) -> ValueInfo {
        ValueInfo::new(ValueKind::Dynamic)
    }
}

/// The native receiver and String checks precede finite-source dispatch. A
/// non-String throws without coercion; an unmatched String uses the typed AOT
/// rejection rather than a catchable JavaScript exception.
pub(super) struct AdmittedShadowRealmInvocation(());

impl AdmittedShadowRealmInvocation {
    pub(super) fn into_result_info(self) -> ValueInfo {
        unknown_runtime_value_info()
    }
}

/// A Symbol first argument must throw during Realm Script ToString, before
/// source dispatch. The actual host invocation still owns coercion and the
/// captured Realm's TypeError; this proof never manufactures a prepared source.
pub(super) struct ProvenRealmScriptConversionThrow(());

impl ProvenRealmScriptConversionThrow {
    fn from_args(arguments: &[TypedExpr]) -> Option<Self> {
        let first = arguments.first()?;
        if matches!(
            first.expr,
            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
        ) {
            return None;
        }
        (first.possible_kinds == KindSet::from_kind(ValueKind::Symbol)).then_some(Self(()))
    }

    pub(super) fn into_result_info(self) -> ValueInfo {
        ValueInfo::new(ValueKind::Dynamic)
    }
}

/// A Function invocation whose coercions and source selection execute at runtime.
/// Each invocation allocates a fresh function with its execution protocol
/// and active constructor's realm.
pub(super) struct AdmittedFunctionInvocation(DynamicFunctionKind);

impl AdmittedFunctionInvocation {
    pub(super) fn into_result_info(self) -> ValueInfo {
        ScriptLowerer::empty_dynamic_function_info(self.0)
    }
}

/// One-shot ownership of an unsupported dynamic-source invocation.
///
/// The fields stay private so the builtin-accounting identity and diagnostic
/// gap cannot be paired independently after target resolution.
pub(super) struct UnsupportedDynamicSourceCall {
    standard_builtin: Option<StandardBuiltinId>,
    gap: DynamicSourceGap,
}

/// Proof that the intrinsic `%eval%` call returns before parsing source.
///
/// The field and constructors stay private to this module so lowered call sites
/// cannot skip the dynamic-source diagnostic from an arbitrary return fact.
#[derive(Debug)]
pub(super) struct ProvenEvalPassThrough {
    result: ValueInfo,
}

impl ProvenEvalPassThrough {
    fn from_args(source_args: Option<&[Expression]>, lowered_args: &[TypedExpr]) -> Option<Self> {
        let source_has_spread = source_args
            .is_some_and(|args| args.iter().any(|arg| matches!(arg, Expression::Spread(_))));
        let lowered_has_spread = lowered_args.iter().any(|arg| {
            matches!(
                arg.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        });
        if source_has_spread || lowered_has_spread {
            return None;
        }

        match lowered_args {
            [] if source_args.is_none_or(<[Expression]>::is_empty) => Some(Self {
                result: ValueInfo::undefined(),
            }),
            [first, ..]
                if source_args.is_none_or(|args| !args.is_empty())
                    && first.possible_kinds != KindSet::EMPTY
                    && !first.possible_kinds.contains(ValueKind::String) =>
            {
                Some(Self {
                    result: first.value_info(),
                })
            }
            _ => None,
        }
    }

    pub(super) fn into_result_info(self) -> ValueInfo {
        self.result
    }
}

pub(super) enum OptionalCallSource<'a> {
    AlreadyAccounted,
    Syntax(&'a [Expression]),
}

pub(super) fn already_accounted_optional_calls<'a>(
    chain: &[OptionalChainOperationIr],
) -> Vec<OptionalCallSource<'a>> {
    chain
        .iter()
        .filter(|operation| matches!(operation, OptionalChainOperationIr::Call { .. }))
        .map(|_| OptionalCallSource::AlreadyAccounted)
        .collect()
}

impl DynamicSourceProof {
    fn from_expression(expression: &Expression) -> Self {
        if has_aot_source_text_proof(expression) {
            Self::AotSyntax
        } else {
            Self::Runtime
        }
    }

    fn for_args(kind: DynamicSourceKind, args: &[Expression]) -> Self {
        match kind {
            DynamicSourceKind::DirectEval
            | DynamicSourceKind::IndirectEval
            | DynamicSourceKind::RealmEvalScript
            | DynamicSourceKind::ShadowRealmEvaluate => args
                .first()
                .map_or(Self::Runtime, |source| Self::from_expression(source)),
            DynamicSourceKind::Function(
                DynamicFunctionKind::Ordinary
                | DynamicFunctionKind::Generator
                | DynamicFunctionKind::Async
                | DynamicFunctionKind::AsyncGenerator,
            ) => {
                if args.iter().all(has_aot_source_text_proof) {
                    Self::AotSyntax
                } else {
                    Self::Runtime
                }
            }
        }
    }
}

/// Recognizes only syntax whose evaluation is already the primitive string
/// value. This deliberately does not consult lowered facts or constant folds.
fn has_aot_source_text_proof(expression: &Expression) -> bool {
    match ScriptLowerer::unwrap_parenthesized_expr(expression) {
        Expression::Literal(literal) => matches!(literal.kind(), LiteralKind::String(_)),
        Expression::TemplateLiteral(template)
            if template
                .elements()
                .iter()
                .all(|element| matches!(element, TemplateElement::String(_))) =>
        {
            true
        }
        Expression::Binary(binary) if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) => {
            // A left-deep `+` chain is checked operand by operand in a loop.
            let mut operand = expression;
            loop {
                match ScriptLowerer::unwrap_parenthesized_expr(operand) {
                    Expression::Binary(binary)
                        if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) =>
                    {
                        if !has_aot_source_text_proof(binary.rhs()) {
                            return false;
                        }
                        operand = binary.lhs();
                    }
                    first => return has_aot_source_text_proof(first),
                }
            }
        }
        _ => false,
    }
}

/// Source compilation can use a possible conversion view without granting
/// that same view permission to request an eagerly resolved host module.
#[derive(Clone, Copy)]
enum FirstArgumentTextPurpose {
    ScriptCompilation,
    RealmModuleRequest,
}

/// The closed call-site contexts observed by standard-builtin analysis.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BuiltinCallContext {
    Call,
    Construct,
    RegExpLiteral,
}

pub(super) fn dynamic_source_kind_for_function_id(function_id: &str) -> Option<DynamicSourceKind> {
    match StandardBuiltinId::from_function_id(function_id) {
        Some(StandardBuiltinId::EvalFunction) => return Some(DynamicSourceKind::IndirectEval),
        Some(StandardBuiltinId::ShadowRealmPrototypeEvaluate) => {
            return Some(DynamicSourceKind::ShadowRealmEvaluate);
        }
        _ => {}
    }
    DynamicSourceIntrinsic::from_function_id(function_id).map(DynamicSourceIntrinsic::source_kind)
}

const fn gap_for_source_proof(
    kind: DynamicSourceKind,
    proof: DynamicSourceProof,
) -> DynamicSourceGap {
    match proof {
        DynamicSourceProof::Runtime => DynamicSourceGap::runtime_source(kind),
        DynamicSourceProof::AotSyntax => DynamicSourceGap::aot_known_source(kind),
    }
}

impl ScriptLowerer<'_> {
    pub(super) fn register_dynamic_function_source(&mut self, source: DynamicFunctionSource) {
        if let Some(existing) = self
            .dynamic_function_sources
            .iter_mut()
            .find(|existing| existing.kind == source.kind && existing.arguments == source.arguments)
        {
            if source.admission
                == crate::prepared_function::DynamicFunctionSourceAdmission::Intrinsic
            {
                existing.admission = source.admission;
            }
        } else {
            self.dynamic_function_sources.push(source);
        }
    }

    pub(super) fn register_dynamic_script_source(&mut self, source: DynamicScriptSource) {
        if let Some(existing) = self
            .dynamic_script_sources
            .iter_mut()
            .find(|existing| existing.kind == source.kind && existing.source == source.source)
        {
            if source.admission == PreparedScriptAdmission::ResolvedIntrinsic {
                existing.admission = source.admission;
            }
        } else {
            self.dynamic_script_sources.push(source);
        }
    }

    fn finite_first_argument_text_candidates(
        &self,
        arguments: &[&Expression],
        purpose: FirstArgumentTextPurpose,
    ) -> Option<BTreeSet<String>> {
        use super::finite_function_source::MAX_SOURCE_CANDIDATES;
        let mut sources = BTreeSet::new();
        let mut conversions = BTreeSet::new();
        for argument in arguments {
            let (candidates, spread) = match Self::unwrap_parenthesized_expr(argument) {
                Expression::Spread(spread) => {
                    (self.finite_spread_source_candidates(spread.target()), true)
                }
                argument => (self.function_source_value_candidates(argument), false),
            };
            for candidate in candidates {
                match candidate {
                    FiniteSourceValue::Text(source) => {
                        sources.insert(source);
                    }
                    FiniteSourceValue::SupplementalText(source) => match purpose {
                        FirstArgumentTextPurpose::ScriptCompilation => {
                            if conversions.len() < MAX_SOURCE_CANDIDATES {
                                conversions.insert(source);
                            }
                        }
                        // Speculative conversion views cannot authorize eager
                        // host resolution of a Realm module specifier.
                        FirstArgumentTextPurpose::RealmModuleRequest => {}
                    },
                    FiniteSourceValue::Function(_)
                    | FiniteSourceValue::ReturnedCallableSource(_)
                    | FiniteSourceValue::FunctionConstructor(_)
                    | FiniteSourceValue::Record(_)
                    | FiniteSourceValue::Array(_) => {}
                }
            }
            if sources.len() > MAX_SOURCE_CANDIDATES {
                return None;
            }
            // An earlier spread can be empty; execution still obtains the live
            // first argument before matching any prepared source.
            if !spread {
                break;
            }
        }
        // Existing primitive sources have priority. An optional conversion view
        // uses spare capacity and cannot erase an admitted primary source set.
        for source in conversions {
            if sources.len() == MAX_SOURCE_CANDIDATES {
                break;
            }
            sources.insert(source);
        }
        Some(sources)
    }

    pub(super) fn finite_script_argument_text_candidates(
        &self,
        arguments: &[Expression],
    ) -> Option<BTreeSet<String>> {
        self.finite_first_argument_text_candidates(
            &arguments.iter().collect::<Vec<_>>(),
            FirstArgumentTextPurpose::ScriptCompilation,
        )
    }

    fn register_realm_import_source_candidates(&self, arguments: &[&Expression]) {
        if let Some(sources) = self.finite_first_argument_text_candidates(
            arguments,
            FirstArgumentTextPurpose::RealmModuleRequest,
        ) {
            self.analysis
                .allocations
                .prepared_sources
                .record_realm_module_requests(sources.into_iter().map(ModuleRequestKeyIr::plain));
        }
    }

    fn named_script_source_kind(name: &str) -> Option<PreparedScriptKind> {
        match name {
            "evalScript" => Some(PreparedScriptKind::RealmScript),
            "eval" => Some(PreparedScriptKind::IndirectEval),
            "evaluate" => Some(PreparedScriptKind::ShadowRealmEvaluate),
            _ => None,
        }
    }

    /// A temporary syntax projection reuses ordinary finite-source discovery,
    /// including forwarded argument positions. It is never lowered or emitted:
    /// the original chain owns its short circuit, property Get and actual Call.
    pub(super) fn register_optional_source_candidates(&mut self, optional: &Optional) {
        if !optional
            .chain()
            .iter()
            .any(|operation| matches!(operation.kind(), OptionalOperationKind::Call { .. }))
        {
            return;
        }
        // Candidate function identities use retained source spans; this path
        // only reads their real analysis plans and records bounded source hints.
        // No pointer-keyed environment/continuation owner sees this projection.
        let mut candidate = optional.target().clone();
        for operation in optional.chain() {
            let span = boa_ast::Span::new(optional.span().start(), operation.span().end());
            candidate = match operation.kind() {
                OptionalOperationKind::SimplePropertyAccess { field } => {
                    PropertyAccess::Simple(boa_ast::expression::access::SimplePropertyAccess::new(
                        candidate,
                        field.clone(),
                    ))
                    .into()
                }
                OptionalOperationKind::PrivatePropertyAccess { field } => PropertyAccess::Private(
                    boa_ast::expression::access::PrivatePropertyAccess::new(
                        candidate, *field, span,
                    ),
                )
                .into(),
                OptionalOperationKind::Call { args } => {
                    self.register_dynamic_source_candidates(&candidate, args);
                    boa_ast::expression::Call::new(candidate, args.clone(), span).into()
                }
            };
        }
    }

    fn register_script_source_candidates(
        &mut self,
        kind: PreparedScriptKind,
        arguments: &[&Expression],
        admission: PreparedScriptAdmission,
    ) -> bool {
        // A syntactic non-String literal cannot become source text during
        // callee evaluation. Eval returns it unchanged; Realm evalScript still
        // performs ToString. Leave mutable values to runtime admission.
        if kind == PreparedScriptKind::IndirectEval
            && arguments.first().is_some_and(|argument| {
                matches!(
                    Self::unwrap_parenthesized_expr(argument),
                    Expression::Literal(literal)
                        if !matches!(literal.kind(), LiteralKind::String(_))
                )
            })
        {
            return false;
        }
        let Some(sources) = self.finite_first_argument_text_candidates(
            arguments,
            FirstArgumentTextPurpose::ScriptCompilation,
        ) else {
            return false;
        };
        let found = !sources.is_empty();
        for source in sources {
            self.register_dynamic_script_source(DynamicScriptSource {
                admission,
                kind: kind.clone(),
                source,
            });
        }
        found
    }

    pub(super) fn register_dynamic_source_candidates(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) {
        self.register_function_global_hint_candidates(callee, arguments);
        self.register_source_call_argument_candidates(callee, arguments);
        let property_name =
            |access: &boa_ast::expression::access::SimplePropertyAccess| match access.field() {
                PropertyAccessField::Const(name) => {
                    Some(self.interner.resolve_expect(name.sym()).to_string())
                }
                PropertyAccessField::Expr(expression) => {
                    self.finite_source_computed_key_candidate(expression)
                }
            };
        let mut candidate_callee = Self::unwrap_parenthesized_expr(callee);
        let mut candidate_arguments = arguments.iter().collect::<Vec<_>>();
        let mut forwarded = false;
        // A comma produces the right-hand value, never its Reference. Keep
        // the original expression in call lowering; this only discovers
        // optional source units when named lookup erased callable facts.
        while let Expression::Binary(binary) = candidate_callee {
            if binary.op() != BinaryOp::Comma {
                break;
            }
            candidate_callee = Self::unwrap_parenthesized_expr(binary.rhs());
            forwarded = true;
        }
        let mut reflected_constructor = false;
        if let Expression::PropertyAccess(PropertyAccess::Simple(access)) = candidate_callee {
            match property_name(access).as_deref() {
                Some("call") => {
                    candidate_callee = Self::unwrap_parenthesized_expr(access.target());
                    candidate_arguments = candidate_arguments.into_iter().skip(1).collect();
                    forwarded = true;
                }
                Some(method @ ("apply" | "construct")) => {
                    let is_reflect = matches!(
                        Self::unwrap_parenthesized_expr(access.target()),
                        Expression::Identifier(identifier)
                            if self.interner.resolve_expect(identifier.sym()).to_string() == "Reflect"
                    );
                    reflected_constructor = method == "construct" && is_reflect;
                    let (target, argument_list_index) = match (method, is_reflect) {
                        ("apply", true) => (arguments.first(), 2),
                        ("construct", true) => (arguments.first(), 1),
                        ("apply", false) => (Some(access.target()), 1),
                        ("construct", false) => return,
                        _ => unreachable!("forwarding method was matched above"),
                    };
                    let Some(target) = target else {
                        return;
                    };
                    let Some(arguments) = arguments.get(argument_list_index) else {
                        return;
                    };
                    let Expression::ArrayLiteral(array) =
                        Self::unwrap_parenthesized_expr(arguments)
                    else {
                        return;
                    };
                    let Some(arguments) = array
                        .as_ref()
                        .iter()
                        .map(Option::as_ref)
                        .collect::<Option<Vec<_>>>()
                    else {
                        return;
                    };
                    candidate_callee = Self::unwrap_parenthesized_expr(target);
                    candidate_arguments = arguments;
                    forwarded = true;
                }
                _ => {}
            }
        }
        // Forwarded calls may themselves obtain their target from a comma.
        while let Expression::Binary(binary) = candidate_callee {
            if binary.op() != BinaryOp::Comma {
                break;
            }
            candidate_callee = Self::unwrap_parenthesized_expr(binary.rhs());
            forwarded = true;
        }
        let mut known_constructor_kinds = Vec::new();
        let mut known_script_kinds = Vec::new();
        let mut known_realm_import = false;
        for candidate in self.function_source_value_candidates(candidate_callee) {
            match candidate {
                FiniteSourceValue::FunctionConstructor(kind) => known_constructor_kinds.push(kind),
                FiniteSourceValue::Function(function_id) => {
                    known_realm_import |= StandardBuiltinId::from_function_id(&function_id)
                        == Some(StandardBuiltinId::ShadowRealmPrototypeImportValue);
                    match dynamic_source_kind_for_function_id(&function_id) {
                        Some(DynamicSourceKind::Function(kind)) => {
                            known_constructor_kinds.push(kind)
                        }
                        Some(DynamicSourceKind::IndirectEval) => {
                            known_script_kinds.push(PreparedScriptKind::IndirectEval)
                        }
                        Some(DynamicSourceKind::RealmEvalScript) => {
                            known_script_kinds.push(PreparedScriptKind::RealmScript)
                        }
                        Some(DynamicSourceKind::ShadowRealmEvaluate) => {
                            known_script_kinds.push(PreparedScriptKind::ShadowRealmEvaluate)
                        }
                        Some(DynamicSourceKind::DirectEval) | None => {}
                    }
                }
                FiniteSourceValue::Text(_)
                | FiniteSourceValue::SupplementalText(_)
                | FiniteSourceValue::ReturnedCallableSource(_)
                | FiniteSourceValue::Record(_)
                | FiniteSourceValue::Array(_) => {}
            }
        }
        let name = match candidate_callee {
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let targets = self
                    .lookup_binding(&name)
                    .map(|binding| binding.function_targets)
                    .or_else(|| {
                        self.lookup_global_property_info(&name)
                            .map(|property| property.value_info.function_targets.clone())
                    });
                if let Some(targets) = targets {
                    for function_id in targets.known_targets() {
                        known_realm_import |= StandardBuiltinId::from_function_id(function_id)
                            == Some(StandardBuiltinId::ShadowRealmPrototypeImportValue);
                        match dynamic_source_kind_for_function_id(function_id) {
                            Some(DynamicSourceKind::Function(kind)) => {
                                if !known_constructor_kinds.contains(&kind) {
                                    known_constructor_kinds.push(kind);
                                }
                            }
                            Some(DynamicSourceKind::IndirectEval) => {
                                known_script_kinds.push(PreparedScriptKind::IndirectEval);
                            }
                            Some(DynamicSourceKind::RealmEvalScript) => {
                                known_script_kinds.push(PreparedScriptKind::RealmScript);
                            }
                            Some(DynamicSourceKind::ShadowRealmEvaluate) => {
                                known_script_kinds.push(PreparedScriptKind::ShadowRealmEvaluate);
                            }
                            Some(DynamicSourceKind::DirectEval) | None => {}
                        }
                    }
                }
                if name != "Function"
                    && !(forwarded && name == "eval")
                    && !reflected_constructor
                    && known_constructor_kinds.is_empty()
                    && known_script_kinds.is_empty()
                    && !known_realm_import
                {
                    return;
                }
                name
            }
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                let Some(name) = property_name(access) else {
                    return;
                };
                name
            }
            _ if reflected_constructor || !known_constructor_kinds.is_empty() => String::new(),
            _ => return,
        };
        if let Some(kind) = Self::named_script_source_kind(&name) {
            known_script_kinds.push(kind);
        }
        known_realm_import |= name == "importValue";
        if known_realm_import {
            self.register_realm_import_source_candidates(&candidate_arguments);
        }
        for kind in known_script_kinds {
            self.register_script_source_candidates(
                kind,
                &candidate_arguments,
                PreparedScriptAdmission::RuntimeCandidate,
            );
        }
        if matches!(name.as_str(), "evalScript" | "eval" | "evaluate") {
            return;
        }
        let kinds = if name == "Function" {
            &[DynamicFunctionKind::Ordinary][..]
        } else if reflected_constructor {
            // An unknown Reflect.construct target may be any source constructor.
            // Prepare its finite argument tuples under each constructor grammar;
            // actual callable dispatch remains the only execution authority.
            DynamicFunctionKind::ALL
        } else if !known_constructor_kinds.is_empty() {
            // Known possible targets admit source candidates even when a named
            // environment call must retain runtime Reference resolution.
            known_constructor_kinds.as_slice()
        } else {
            return;
        };
        for kind in kinds {
            for arguments in self
                .function_source_argument_candidates_with_body_hints(*kind, &candidate_arguments)
            {
                self.register_dynamic_function_source(DynamicFunctionSource {
                    admission:
                        crate::prepared_function::DynamicFunctionSourceAdmission::RuntimeCandidate,
                    kind: *kind,
                    arguments: arguments.clone(),
                });
            }
        }
    }

    pub(super) fn aot_source_text(&self, expression: &Expression) -> Option<String> {
        match Self::unwrap_parenthesized_expr(expression) {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::String(symbol) => Some(self.interned_runtime_string(*symbol)),
                _ => None,
            },
            Expression::TemplateLiteral(template) => {
                let mut units = Vec::new();
                for element in template.elements() {
                    let TemplateElement::String(symbol) = element else {
                        return None;
                    };
                    units.extend_from_slice(self.interner.resolve_expect(*symbol).utf16());
                }
                Some(Self::utf16_units_to_runtime_string(&units))
            }
            Expression::Binary(binary)
                if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) =>
            {
                // A left-deep `+` chain concatenates its operands in a loop.
                let mut right_operands = vec![binary.rhs()];
                let mut first = binary.lhs();
                while let Expression::Binary(binary) = first {
                    if binary.op() != BinaryOp::Arithmetic(ArithmeticOp::Add) {
                        break;
                    }
                    right_operands.push(binary.rhs());
                    first = binary.lhs();
                }
                let mut units = decode_js_string_utf16(&self.aot_source_text(first)?);
                for operand in right_operands.into_iter().rev() {
                    units.extend(decode_js_string_utf16(&self.aot_source_text(operand)?));
                }
                Some(Self::utf16_units_to_runtime_string(&units))
            }
            _ => None,
        }
    }

    pub(super) fn dynamic_source_intrinsic_signature(
        intrinsic: DynamicSourceIntrinsic,
    ) -> FunctionSignature {
        let return_info = match intrinsic {
            DynamicSourceIntrinsic::Function(kind) => Self::empty_dynamic_function_info(kind),
            DynamicSourceIntrinsic::RealmEvalScript => ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
        };
        FunctionSignature {
            id: intrinsic.function_id().to_string(),
            to_string_representation: CallableToStringRepresentation::NativeNamed(
                intrinsic.source_kind().operation_name().to_string(),
            ),
            protocol: if intrinsic.constructable() {
                FunctionProtocolIr::OrdinaryCallAndConstruct
            } else {
                FunctionProtocolIr::OrdinaryCallOnly
            },
            callable: true,
            class_heritage_kind: ClassHeritageKind::None,
            params: Vec::new(),
            return_kind: return_info.kind,
            return_possible_kinds: return_info.possible_kinds,
            return_shape: FunctionReturnShape::flow_sensitive(return_info.heap_shape.clone()),
            return_targets: return_info.function_targets.clone(),
            constructor_instance: return_info,
            this_info: ValueInfo::undefined(),
            this_observed: false,
            source_call_flow_effects: SourceCallFlowEffects::unobserved(),
        }
    }

    pub(super) fn record_boxed_builtin_invocation(
        &mut self,
        builtin: StandardBuiltinId,
        context: &BuiltinCallContext,
    ) {
        if !builtin.is_boxed_primitive_constructor() {
            return;
        }
        match context {
            BuiltinCallContext::Call => self.boxed_builtin_calls += 1,
            BuiltinCallContext::Construct => self.boxed_builtin_constructs += 1,
            BuiltinCallContext::RegExpLiteral => {}
        }
    }

    /// Classifies a resolved dynamic-source identity before call lowering can
    /// manufacture executable IR.
    pub(super) fn resolve_dynamic_source_call(
        &mut self,
        function_id: &str,
        source_args: Option<&[Expression]>,
        lowered_args: &[TypedExpr],
    ) -> Option<ResolvedDynamicSourceCall> {
        if StandardBuiltinId::from_function_id(function_id)
            == Some(StandardBuiltinId::ShadowRealmPrototypeImportValue)
        {
            if let Some(arguments) = source_args {
                self.register_realm_import_source_candidates(&arguments.iter().collect::<Vec<_>>());
            } else if let Some(specifier) = lowered_args
                .first()
                .and_then(Self::lowered_function_source_candidate)
            {
                self.analysis
                    .allocations
                    .prepared_sources
                    .record_realm_module_requests([ModuleRequestKeyIr::plain(specifier)]);
            }
        }
        let Some(kind) = dynamic_source_kind_for_function_id(function_id) else {
            return None;
        };

        if matches!(
            kind,
            DynamicSourceKind::DirectEval | DynamicSourceKind::IndirectEval
        ) {
            if let Some(proof) = ProvenEvalPassThrough::from_args(source_args, lowered_args) {
                return Some(ResolvedDynamicSourceCall::EvalPassThrough(proof));
            }
        }

        if kind == DynamicSourceKind::RealmEvalScript {
            if let Some(proof) = ProvenRealmScriptConversionThrow::from_args(lowered_args) {
                return Some(ResolvedDynamicSourceCall::RealmScriptConversionThrow(proof));
            }
        }

        if let DynamicSourceKind::Function(kind) = kind {
            if source_args.is_none_or(<[Expression]>::is_empty) && lowered_args.is_empty() {
                return Some(ResolvedDynamicSourceCall::FunctionInvocation(
                    AdmittedFunctionInvocation(kind),
                ));
            }
        }

        if let DynamicSourceKind::Function(function_kind) = kind {
            if let Some(arguments) = source_args {
                for arguments in self.function_source_argument_candidates_with_body_hints(
                    function_kind,
                    &arguments.iter().collect::<Vec<_>>(),
                ) {
                    self.register_dynamic_function_source(DynamicFunctionSource {
                        admission: crate::prepared_function::DynamicFunctionSourceAdmission::RuntimeCandidate,
                        kind: function_kind,
                        arguments,
                    });
                }
            }
            let candidates = source_args
                .and_then(|arguments| {
                    arguments
                        .iter()
                        .enumerate()
                        .map(|(index, argument)| {
                            self.function_source_candidate(argument).or_else(|| {
                                (arguments.len() == lowered_args.len())
                                    .then(|| lowered_args.get(index))
                                    .flatten()
                                    .and_then(Self::lowered_function_source_candidate)
                            })
                        })
                        .collect::<Option<Vec<_>>>()
                })
                .or_else(|| {
                    lowered_args
                        .iter()
                        .map(Self::lowered_function_source_candidate)
                        .collect::<Option<Vec<_>>>()
                });
            if let Some(arguments) = candidates {
                let source = DynamicFunctionSource {
                    admission: if source_args.is_some_and(|arguments| {
                        arguments
                            .iter()
                            .all(|argument| self.aot_source_text(argument).is_some())
                    }) {
                        crate::prepared_function::DynamicFunctionSourceAdmission::Intrinsic
                    } else {
                        crate::prepared_function::DynamicFunctionSourceAdmission::RuntimeCandidate
                    },
                    kind: function_kind,
                    arguments,
                };
                self.register_dynamic_function_source(source);
            }
            // ToString on an argument can invoke arbitrary user code.
            self.invalidate_unknown_user_code_effects();
            return Some(ResolvedDynamicSourceCall::FunctionInvocation(
                AdmittedFunctionInvocation(function_kind),
            ));
        }

        let script_kind = match kind {
            DynamicSourceKind::RealmEvalScript => Some(PreparedScriptKind::RealmScript),
            DynamicSourceKind::IndirectEval => Some(PreparedScriptKind::IndirectEval),
            DynamicSourceKind::ShadowRealmEvaluate => Some(PreparedScriptKind::ShadowRealmEvaluate),
            DynamicSourceKind::DirectEval | DynamicSourceKind::Function(_) => None,
        };
        if let Some(kind) = script_kind {
            if let Some(arguments) = source_args {
                let admission = if arguments
                    .first()
                    .is_some_and(|argument| self.aot_source_text(argument).is_some())
                {
                    PreparedScriptAdmission::ResolvedIntrinsic
                } else {
                    PreparedScriptAdmission::RuntimeCandidate
                };
                if self.register_script_source_candidates(
                    kind,
                    &arguments.iter().collect::<Vec<_>>(),
                    admission,
                ) {
                    self.invalidate_unknown_user_code_effects();
                    return Some(ResolvedDynamicSourceCall::CompiledScript(
                        ProvenCompiledScript(()),
                    ));
                }
            }
        }

        if kind == DynamicSourceKind::IndirectEval {
            self.invalidate_unknown_user_code_effects();
            return Some(ResolvedDynamicSourceCall::IndirectEvalInvocation(
                AdmittedIndirectEvalInvocation(()),
            ));
        }

        if kind == DynamicSourceKind::RealmEvalScript {
            self.invalidate_unknown_user_code_effects();
            return Some(ResolvedDynamicSourceCall::RealmScriptInvocation(
                AdmittedRealmScriptInvocation(()),
            ));
        }

        if kind == DynamicSourceKind::ShadowRealmEvaluate {
            self.invalidate_unknown_user_code_effects();
            return Some(ResolvedDynamicSourceCall::ShadowRealmInvocation(
                AdmittedShadowRealmInvocation(()),
            ));
        }

        let proof = source_args
            .map(|args| DynamicSourceProof::for_args(kind, args))
            .unwrap_or(DynamicSourceProof::Runtime);
        Some(ResolvedDynamicSourceCall::Unsupported(
            UnsupportedDynamicSourceCall {
                standard_builtin: StandardBuiltinId::from_function_id(function_id),
                gap: gap_for_source_proof(kind, proof),
            },
        ))
    }

    pub(super) fn record_unsupported_dynamic_source(
        &mut self,
        unsupported: UnsupportedDynamicSourceCall,
    ) {
        let UnsupportedDynamicSourceCall {
            standard_builtin,
            gap,
        } = unsupported;
        if let Some(builtin) = standard_builtin {
            self.note_standard_builtin_call(builtin);
        }
        self.diagnostics
            .push(IrDiagnostic::unsupported_dynamic_source(gap));
    }

    pub(super) fn lower_dynamic_source_construct(
        &mut self,
        function_id: &str,
        mut callee: TypedExpr,
        source_args: &[Expression],
    ) -> TypedExpr {
        let lowered_args = self
            .lower_call_args_expanding_spread(source_args)
            .into_arguments_after_expression(&mut callee);
        let resolved = self
            .resolve_dynamic_source_call(function_id, Some(source_args), &lowered_args)
            .expect("dynamic-source construct lowering requires a dynamic-source identity");
        match resolved {
            ResolvedDynamicSourceCall::EvalPassThrough(_)
            | ResolvedDynamicSourceCall::IndirectEvalInvocation(_)
            | ResolvedDynamicSourceCall::RealmScriptInvocation(_)
            | ResolvedDynamicSourceCall::ShadowRealmInvocation(_)
            | ResolvedDynamicSourceCall::RealmScriptConversionThrow(_) => {
                unreachable!("source-evaluation intrinsics are not constructable")
            }
            ResolvedDynamicSourceCall::FunctionInvocation(proof) => {
                self.mark_host_builtin_from_function_id(function_id);
                return TypedExpr::from_info(
                    proof.into_result_info(),
                    ExprIr::Construct {
                        callee: Box::new(callee),
                        args: lowered_args,
                        static_regexp_compilation: None,
                    },
                );
            }
            ResolvedDynamicSourceCall::CompiledScript(proof) => {
                self.mark_host_builtin_from_function_id(function_id);
                return TypedExpr::from_info(
                    proof.into_result_info(),
                    ExprIr::Construct {
                        callee: Box::new(callee),
                        args: lowered_args,
                        static_regexp_compilation: None,
                    },
                );
            }
            ResolvedDynamicSourceCall::Unsupported(unsupported) => {
                self.record_unsupported_dynamic_source(unsupported);
            }
        }
        TypedExpr::undefined()
    }
}

#[cfg(test)]
mod native_global_script_tests;
