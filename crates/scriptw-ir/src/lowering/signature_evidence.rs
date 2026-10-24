use super::*;

impl ScriptLowerer<'_> {
    pub(super) fn propagate_function_signatures(&mut self) {
        if self.analysis.script_root_functions.is_empty() {
            return;
        }

        const MAX_SIGNATURE_PROPAGATION_PASSES: usize = 6;

        for _ in 0..MAX_SIGNATURE_PROPAGATION_PASSES {
            let before = self.function_signatures.clone();
            let before_exact_contexts = self.exact_context_function_observations.clone();
            let before_callback_contexts = self.exact_context_callback_observations.clone();
            let before_source_parameters = self.function_source_parameter_candidates.clone();
            let before_source_bindings = self.function_source_binding_candidates.clone();
            let mut pass = ScriptLowerer::new(
                self.interner,
                self.analysis,
                self.source_text,
                self.root_this_binding,
                SCRIPT_OWNER_ID.to_string(),
                self.host_surface_policy,
            );
            pass.function_signatures = self.function_signatures.clone();
            pass.visible_function_names = self.visible_function_names.clone();
            pass.global_properties = self.global_properties.clone();
            pass.observed_script_global_writes = self.observed_script_global_writes.clone();
            pass.array_prototype_mutated = self.array_prototype_mutated;
            pass.number_prototype_to_string_state = self.number_prototype_to_string_state;
            pass.boolean_prototype_to_string_state = self.boolean_prototype_to_string_state;
            pass.dynamically_installed_getters = self.dynamically_installed_getters.clone();
            pass.dynamically_installed_setters = self.dynamically_installed_setters.clone();
            pass.unknown_user_code_effects_observed = self.unknown_user_code_effects_observed;
            pass.function_signature_shape_evidence = self.function_signature_shape_evidence;
            pass.static_boolean_bindings = self.static_boolean_bindings.clone();
            pass.static_string_bindings = self.static_string_bindings.clone();
            pass.function_source_binding_candidates =
                self.function_source_binding_candidates.clone();
            pass.function_source_parameter_candidates =
                self.function_source_parameter_candidates.clone();
            pass.static_to_string_regexp_object_bindings =
                self.static_to_string_regexp_object_bindings.clone();
            pass.var_bindings = self.var_bindings.clone();
            pass.exact_context_function_observations =
                self.exact_context_function_observations.clone();
            pass.exact_context_callback_observations =
                self.exact_context_callback_observations.clone();
            pass.exact_context_callback_specializations =
                self.exact_context_callback_specializations.clone();
            pass.exact_context_function_specializations =
                self.exact_context_function_specializations.clone();
            pass.is_prepass = true;
            pass.prepare_root_function_bindings(self.analysis.script_root_functions.as_slice());

            for function in &self.analysis.script_root_functions {
                let plan = self
                    .analysis
                    .function_plans
                    .get(&function.id)
                    .expect("function plan must exist");
                pass.infer_function(plan, None);
            }

            self.merge_function_source_binding_candidates(pass.function_source_binding_candidates);
            self.merge_function_source_parameter_candidates(
                pass.function_source_parameter_candidates,
            );
            self.merge_signature_propagation(pass.function_signatures);
            self.merge_exact_context_function_observations(
                pass.exact_context_function_observations,
            );
            self.merge_context_keyed_callback_observations(
                pass.exact_context_callback_observations,
            );
            if self.function_signatures == before
                && self.exact_context_function_observations == before_exact_contexts
                && self.exact_context_callback_observations == before_callback_contexts
                && self.function_source_parameter_candidates == before_source_parameters
                && self.function_source_binding_candidates == before_source_bindings
            {
                break;
            }
        }
    }

    pub(super) fn prepare_exact_context_specializations(&mut self) {
        if self.exact_context_function_observations.is_empty()
            && self.exact_context_callback_observations.is_empty()
            && self.exact_context_function_specializations.is_empty()
            && self.exact_context_callback_specializations.is_empty()
        {
            return;
        }

        const MAX_EXACT_CONTEXT_PASSES: usize = 8;

        for _ in 0..MAX_EXACT_CONTEXT_PASSES {
            let before_signatures = self.function_signatures.clone();
            let before_source_parameters = self.function_source_parameter_candidates.clone();
            let before_source_bindings = self.function_source_binding_candidates.clone();
            let before_function_observations = self.exact_context_function_observations.clone();
            let before_callback_observations = self.exact_context_callback_observations.clone();
            let before_function_specializations =
                self.exact_context_function_specializations.clone();
            let before_callback_specializations =
                self.exact_context_callback_specializations.clone();

            self.allocate_exact_context_specializations();
            self.propagate_exact_context_specializations();

            if self.function_signatures == before_signatures
                && self.exact_context_function_observations == before_function_observations
                && self.exact_context_callback_observations == before_callback_observations
                && self.exact_context_function_specializations == before_function_specializations
                && self.exact_context_callback_specializations == before_callback_specializations
                && self.function_source_parameter_candidates == before_source_parameters
                && self.function_source_binding_candidates == before_source_bindings
            {
                break;
            }
        }
    }

    fn can_specialize_function_body(&self, function_id: &FunctionId) -> bool {
        // Cloning an outer body does not clone its class execution owners or
        // remap the constructors, fields and accessors that reference them.
        // Keep those bodies canonical until the complete identity graph can
        // be specialized together; dropping duplicate bodies is not sound.
        // Exact contexts refine callsite facts independently of cloning the
        // callee. Bound body duplication: large callbacks retain their fully
        // general canonical body instead of repeatedly lowering/emitting it.
        // This changes compiler precision and cost, never source evaluation.
        const MAX_SPECIALIZED_SOURCE_BYTES: usize = 4096;
        self.analysis
            .function_plans
            .get(function_id)
            .is_some_and(|plan| {
                plan.captures.is_empty()
                    && match &plan.to_string_representation {
                        CallableToStringRepresentation::ExactSource(source) => {
                            source.len() <= MAX_SPECIALIZED_SOURCE_BYTES
                        }
                        CallableToStringRepresentation::NativeNamed(_)
                        | CallableToStringRepresentation::NativeAnonymous => false,
                    }
            })
            && !self.analysis.class_execution_ids.values().any(|class_id| {
                self.analysis.owner_plans[class_id]
                    .parent_owner_id
                    .as_deref()
                    == Some(function_id.as_str())
            })
    }

    fn allocate_exact_context_specializations(&mut self) {
        let mut contexts_by_function = BTreeMap::<FunctionId, Vec<ExactHelperContextId>>::new();
        for (callback_id, helper_context_id) in self.exact_context_callback_observations.keys() {
            contexts_by_function
                .entry(callback_id.clone())
                .or_default()
                .push(helper_context_id.clone());
        }
        for (callback_id, helper_contexts) in contexts_by_function {
            if !self.can_specialize_function_body(&callback_id) {
                continue;
            }
            for (index, helper_context_id) in helper_contexts.into_iter().enumerate() {
                let key = (callback_id.clone(), helper_context_id);
                if self
                    .exact_context_callback_specializations
                    .contains_key(&key)
                {
                    continue;
                }
                let synthetic_id = format!("{callback_id}$exact_context${index}");
                if let Some(signature) = self.exact_context_callback_observations.get(&key).cloned()
                {
                    self.function_signatures
                        .insert(synthetic_id.clone(), signature);
                }
                self.exact_context_callback_specializations
                    .insert(key, synthetic_id);
            }
        }

        let mut contexts_by_function = BTreeMap::<FunctionId, Vec<ExactHelperContextId>>::new();
        for (function_id, helper_context_id) in self.exact_context_function_observations.keys() {
            contexts_by_function
                .entry(function_id.clone())
                .or_default()
                .push(helper_context_id.clone());
        }
        for (function_id, helper_contexts) in contexts_by_function {
            if !self.can_specialize_function_body(&function_id) {
                continue;
            }
            for (index, helper_context_id) in helper_contexts.into_iter().enumerate() {
                let key = (function_id.clone(), helper_context_id);
                if self
                    .exact_context_function_specializations
                    .contains_key(&key)
                {
                    continue;
                }
                let synthetic_id = format!("{function_id}$exact_helper_context${index}");
                if let Some(signature) = self.exact_context_function_observations.get(&key).cloned()
                {
                    self.function_signatures
                        .insert(synthetic_id.clone(), signature);
                }
                self.exact_context_function_specializations
                    .insert(key, synthetic_id);
            }
        }
    }

    fn propagate_exact_context_specializations(&mut self) {
        let function_specializations = self
            .exact_context_function_specializations
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let callback_specializations = self
            .exact_context_callback_specializations
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        if function_specializations.is_empty() && callback_specializations.is_empty() {
            return;
        }

        let mut pass = ScriptLowerer::new(
            self.interner,
            self.analysis,
            self.source_text,
            self.root_this_binding,
            SCRIPT_OWNER_ID.to_string(),
            self.host_surface_policy,
        );
        pass.function_signatures = self.function_signatures.clone();
        pass.visible_function_names = self.visible_function_names.clone();
        pass.global_properties = self.global_properties.clone();
        pass.observed_script_global_writes = self.observed_script_global_writes.clone();
        pass.array_prototype_mutated = self.array_prototype_mutated;
        pass.number_prototype_to_string_state = self.number_prototype_to_string_state;
        pass.boolean_prototype_to_string_state = self.boolean_prototype_to_string_state;
        pass.dynamically_installed_getters = self.dynamically_installed_getters.clone();
        pass.dynamically_installed_setters = self.dynamically_installed_setters.clone();
        pass.unknown_user_code_effects_observed = self.unknown_user_code_effects_observed;
        pass.function_signature_shape_evidence = self.function_signature_shape_evidence;
        pass.static_boolean_bindings = self.static_boolean_bindings.clone();
        pass.static_string_bindings = self.static_string_bindings.clone();
        pass.function_source_binding_candidates = self.function_source_binding_candidates.clone();
        pass.function_source_parameter_candidates =
            self.function_source_parameter_candidates.clone();
        pass.static_to_string_regexp_object_bindings =
            self.static_to_string_regexp_object_bindings.clone();
        pass.var_bindings = self.var_bindings.clone();
        pass.exact_context_function_observations = self.exact_context_function_observations.clone();
        pass.exact_context_callback_observations = self.exact_context_callback_observations.clone();
        pass.exact_context_callback_specializations =
            self.exact_context_callback_specializations.clone();
        pass.exact_context_function_specializations =
            self.exact_context_function_specializations.clone();
        pass.is_prepass = true;
        pass.prepare_root_function_bindings(self.analysis.script_root_functions.as_slice());

        for key in function_specializations
            .into_iter()
            .chain(callback_specializations.into_iter())
        {
            let function_id = key.0.clone();
            let Some(plan) = self.analysis.function_plans.get(&function_id) else {
                continue;
            };
            pass.infer_function(plan, Some(key));
        }

        self.merge_function_source_binding_candidates(pass.function_source_binding_candidates);
        self.merge_function_source_parameter_candidates(pass.function_source_parameter_candidates);
        self.merge_exact_context_function_observations(pass.exact_context_function_observations);
        self.merge_context_keyed_callback_observations(pass.exact_context_callback_observations);
    }

    pub(super) fn propagate_direct_call_context(
        &mut self,
        function_id: &FunctionId,
        args: &[ValueInfo],
    ) -> Option<ValueInfo> {
        if !self.analysis.function_plans.contains_key(function_id) {
            return None;
        }
        if self.current_function_id.as_ref() == Some(function_id) {
            return None;
        }
        if self.active_direct_call_propagations.contains(function_id) {
            return None;
        }
        let Some(exact_signature) = self.function_signatures.get(function_id).cloned() else {
            return None;
        };
        let canonical_args = self.canonical_exact_context_arg_infos(args);
        let helper_context_id = Self::exact_helper_context_id(function_id, &canonical_args);
        let propagation_key = (function_id.clone(), helper_context_id.clone());
        let context_is_reusable = !self.function_has_untracked_captures(function_id);
        let capture_context_is_authoritative =
            self.function_captures_current_script_activation(function_id);
        if !capture_context_is_authoritative
            && !self
                .completed_direct_call_propagations
                .insert(propagation_key)
        {
            return None;
        }
        let mut pass = ScriptLowerer::new(
            self.interner,
            self.analysis,
            self.source_text,
            self.root_this_binding,
            if capture_context_is_authoritative {
                self.current_owner_id.clone()
            } else {
                function_id.clone()
            },
            self.host_surface_policy,
        );
        pass.function_signatures = self.function_signatures.clone();
        pass.visible_function_names = self.visible_function_names.clone();
        pass.global_properties = self.global_properties.clone();
        pass.observed_script_global_writes = self.observed_script_global_writes.clone();
        pass.array_prototype_mutated = self.array_prototype_mutated;
        pass.number_prototype_to_string_state = self.number_prototype_to_string_state;
        pass.boolean_prototype_to_string_state = self.boolean_prototype_to_string_state;
        pass.dynamically_installed_getters = self.dynamically_installed_getters.clone();
        pass.dynamically_installed_setters = self.dynamically_installed_setters.clone();
        pass.unknown_user_code_effects_observed = self.unknown_user_code_effects_observed;
        pass.function_signature_shape_evidence = self.function_signature_shape_evidence;
        pass.static_boolean_bindings = self.static_boolean_bindings.clone();
        pass.static_string_bindings = self.static_string_bindings.clone();
        pass.function_source_binding_candidates = self.function_source_binding_candidates.clone();
        pass.function_source_parameter_candidates =
            self.function_source_parameter_candidates.clone();
        pass.static_to_string_regexp_object_bindings =
            self.static_to_string_regexp_object_bindings.clone();
        pass.var_bindings = self.var_bindings.clone();
        if capture_context_is_authoritative {
            pass.scopes = self.scopes.clone();
        }
        pass.exact_context_function_observations = self.exact_context_function_observations.clone();
        pass.exact_context_callback_observations = self.exact_context_callback_observations.clone();
        pass.exact_context_callback_specializations =
            self.exact_context_callback_specializations.clone();
        pass.exact_context_function_specializations =
            self.exact_context_function_specializations.clone();
        pass.is_prepass = true;
        pass.active_direct_call_propagations = self.active_direct_call_propagations.clone();
        pass.active_direct_call_propagations
            .insert(function_id.clone());
        pass.completed_direct_call_propagations = self.completed_direct_call_propagations.clone();
        pass.script_global_call_observation_mode = self.script_global_call_observation_mode;
        let callback_targets = args
            .iter()
            .flat_map(|arg| {
                arg.function_targets
                    .exact_targets()
                    .into_iter()
                    .flatten()
                    .map(|callback_id| {
                        (
                            self.original_exact_function_id(callback_id),
                            helper_context_id.clone(),
                        )
                    })
            })
            .collect::<BTreeMap<_, _>>();
        let observed_exact_helper_callbacks =
            self.observe_exact_helper_callback_args(function_id, &canonical_args);
        pass.exact_context_callback_targets = callback_targets.clone();
        pass.exact_context_callback_observations = BTreeMap::new();
        if let Some(signature) = pass.function_signatures.get_mut(function_id) {
            for (param, arg) in signature.params.iter_mut().zip(canonical_args.iter()) {
                if param.is_rest {
                    break;
                }
                Self::set_signature_param_observation(param, arg);
            }
            Self::reset_omitted_signature_params_to_undefined(signature, canonical_args.len());
        }
        let plan = self
            .analysis
            .function_plans
            .get(function_id)
            .expect("function plan must exist");
        pass.infer_function(plan, None);
        let observed_signature = pass.function_signatures.get(function_id).cloned();
        self.merge_called_script_global_value_infos(&pass.called_script_global_value_infos);
        self.completed_direct_call_propagations = pass.completed_direct_call_propagations.clone();
        if context_is_reusable {
            let observed_signature = observed_signature
                .clone()
                .expect("lowered source function must retain its signature");
            pass.exact_context_function_observations.insert(
                (function_id.clone(), helper_context_id.clone()),
                observed_signature,
            );
        }
        self.merge_function_source_binding_candidates(pass.function_source_binding_candidates);
        self.merge_function_source_parameter_candidates(pass.function_source_parameter_candidates);
        self.merge_exact_context_function_observations(pass.exact_context_function_observations);
        let callback_observations = pass.exact_context_callback_observations;
        self.merge_context_keyed_callback_observations(callback_observations.clone());
        if !callback_targets.is_empty() && !observed_exact_helper_callbacks {
            self.merge_exact_callback_observations(callback_observations);
        }
        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            signature.params = exact_signature.params;
            signature.this_info = exact_signature.this_info;
            signature.this_observed = exact_signature.this_observed;
            signature.source_call_flow_effects = exact_signature.source_call_flow_effects;
        }
        if !capture_context_is_authoritative {
            return None;
        }

        // The script activation is a singleton, so its live scope is
        // authoritative for this call. Captures remain absent from the reusable
        // context key, so never publish this observation for a later call.
        observed_signature
            .filter(|signature| {
                !signature.this_observed
                    || signature
                        .return_targets
                        .exact_targets()
                        .is_some_and(BTreeSet::is_empty)
            })
            .map(|signature| self.function_call_return_info(&signature))
    }

    fn reset_omitted_signature_params_to_undefined(
        signature: &mut FunctionSignature,
        supplied_arg_count: usize,
    ) {
        for param in signature.params.iter_mut().skip(supplied_arg_count) {
            if param.is_rest {
                break;
            }
            param.kind = ValueKind::Undefined;
            param.possible_kinds = KindSet::from_kind(ValueKind::Undefined);
            param.heap_shape = None;
            param.function_targets.replace_with_no_function();
            param.observed = true;
        }
    }

    pub(super) fn merge_omitted_signature_params_as_undefined(
        signature: &mut FunctionSignature,
        supplied_arg_count: usize,
    ) {
        let undefined_info = ValueInfo::undefined();
        for param in signature.params.iter_mut().skip(supplied_arg_count) {
            if param.is_rest {
                break;
            }
            Self::merge_signature_param_observation(param, &undefined_info);
        }
    }

    pub(super) fn merge_signature_param_observation(
        param: &mut FunctionParamSignature,
        arg: &ValueInfo,
    ) {
        if !param.observed {
            Self::set_signature_param_observation(param, arg);
            return;
        }

        let possible_kinds = param.possible_kinds.union(arg.possible_kinds);
        param.kind = possible_kinds.as_value_kind();
        param.possible_kinds = possible_kinds;
        if param.heap_shape != arg.heap_shape {
            param.heap_shape = None;
        }
        param.function_targets = param
            .function_targets
            .clone()
            .join(arg.function_targets.clone());
    }

    fn set_signature_param_observation(param: &mut FunctionParamSignature, arg: &ValueInfo) {
        param.kind = arg.kind;
        param.possible_kinds = arg.possible_kinds;
        param.heap_shape = arg.heap_shape.clone();
        param.function_targets = arg.function_targets.clone();
        param.observed = true;
    }

    pub(super) fn clear_signature_param_observation(param: &mut FunctionParamSignature) {
        param.kind = ValueKind::Dynamic;
        param.possible_kinds = KindSet::all_runtime_tags();
        param.heap_shape = None;
        param.function_targets.replace_with_unknown();
        param.observed = false;
    }

    fn merge_signature_param_signature(
        current: &mut FunctionParamSignature,
        propagated: &FunctionParamSignature,
    ) {
        if !propagated.observed {
            return;
        }
        let info = ValueInfo {
            kind: propagated.kind,
            possible_kinds: propagated.possible_kinds,
            heap_shape: propagated.heap_shape.clone(),
            function_targets: propagated.function_targets.clone(),
        };
        Self::merge_signature_param_observation(current, &info);
    }

    pub(super) fn exact_helper_context_id(
        function_id: &FunctionId,
        args: &[ValueInfo],
    ) -> ExactHelperContextId {
        format!("{function_id}:{args:?}")
    }

    fn observe_exact_helper_callback_args(
        &mut self,
        function_id: &FunctionId,
        args: &[ValueInfo],
    ) -> bool {
        let Some(callback_arg) = args.first() else {
            return false;
        };
        let Some(callback_targets) = callback_arg.function_targets.exact_targets() else {
            return false;
        };
        if callback_arg.kind != ValueKind::Function || callback_targets.is_empty() {
            return false;
        }
        let Some(constructor_arg) = args.get(1) else {
            return false;
        };
        let Some(HeapShape::Array(array_shape)) = constructor_arg.heap_shape.as_deref() else {
            return false;
        };
        let mut constructor_targets = BTreeSet::new();
        for element in &array_shape.elements {
            if element.kind != ValueKind::Function {
                return false;
            }
            let Some(element_targets) = element.function_targets.exact_targets() else {
                return false;
            };
            if element_targets.is_empty()
                || !element_targets.iter().all(|function_id| {
                    StandardBuiltinId::from_function_id(function_id)
                        .is_some_and(Self::is_typed_array_constructor)
                })
            {
                return false;
            }
            constructor_targets.extend(element_targets.iter().cloned());
        }
        if constructor_targets.is_empty() {
            return false;
        }
        let constructor_info = ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::exact_many(constructor_targets),
        };
        let factory_info = ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        let callback_args = [constructor_info, factory_info];
        let helper_context_id = Self::exact_helper_context_id(function_id, args);
        for callback_target in callback_targets {
            self.observe_exact_callback_param_infos(
                callback_target,
                &helper_context_id,
                &callback_args,
            );
            self.observe_exact_callback_this_info(
                callback_target,
                &helper_context_id,
                self.global_this_info(),
            );
        }
        true
    }

    fn merge_exact_context_function_observations(
        &mut self,
        observations: BTreeMap<ExactCallbackContextKey, FunctionSignature>,
    ) {
        for ((function_id, helper_context_id), signature) in observations {
            self.merge_exact_context_function_observation(
                &function_id,
                &helper_context_id,
                signature,
            );
        }
    }

    pub(super) fn function_call_return_info(&self, signature: &FunctionSignature) -> ValueInfo {
        let mut info = signature.return_info();
        let shape_is_available = match &signature.return_shape {
            FunctionReturnShape::Unobserved | FunctionReturnShape::Absent => false,
            FunctionReturnShape::FlowSensitive(_) => {
                self.function_signature_shape_evidence == FunctionSignatureShapeEvidence::Available
            }
            FunctionReturnShape::RecreatedPerCall { dependencies, .. } => dependencies
                .global_function_targets
                .iter()
                .all(|(name, function_id)| self.global_function_target_matches(name, function_id)),
        };
        if !shape_is_available {
            info.heap_shape = None;
        }
        info
    }

    pub(super) fn canonical_function_target(&self, function_id: &FunctionId) -> FunctionId {
        self.function_signatures
            .get(function_id)
            .map(|signature| signature.id.clone())
            .unwrap_or_else(|| function_id.clone())
    }

    pub(super) fn global_function_target_matches(&self, name: &str, expected: &FunctionId) -> bool {
        let Some(property) = self.lookup_global_property_info(name) else {
            return false;
        };
        let function_kind = KindSet::from_kind(ValueKind::Function);
        property.proven_present
            && property.value_info.possible_kinds == function_kind
            && property
                .value_info
                .function_targets
                .exact_targets()
                .is_some_and(|targets| {
                    !targets.is_empty()
                        && targets
                            .iter()
                            .all(|observed| self.canonical_function_target(observed) == *expected)
                })
    }

    pub(super) fn function_construct_instance_info(
        &self,
        signature: &FunctionSignature,
    ) -> ValueInfo {
        self.function_signature_shape_evidence
            .apply(signature.constructor_instance.clone())
    }

    pub(super) fn function_signature_for_current_flow(
        &self,
        function_id: &str,
    ) -> Option<FunctionSignature> {
        self.function_signatures
            .get(function_id)
            .cloned()
            .map(|signature| self.function_signature_with_current_flow_evidence(signature))
    }

    pub(super) fn function_signature_with_current_flow_evidence(
        &self,
        mut signature: FunctionSignature,
    ) -> FunctionSignature {
        if self.function_signature_shape_evidence == FunctionSignatureShapeEvidence::Available {
            return signature;
        }

        signature.return_shape.invalidate_flow_sensitive();
        signature.constructor_instance.heap_shape = None;
        signature.this_info.heap_shape = None;
        for param in &mut signature.params {
            param.heap_shape = None;
        }
        signature
    }

    pub(super) fn exact_signature_covers_aggregate_inputs(
        &self,
        function_id: &FunctionId,
        exact: &FunctionSignature,
    ) -> bool {
        let Some(aggregate) = self.function_signatures.get(function_id) else {
            return false;
        };
        let params_match = aggregate
            .params
            .iter()
            .zip(exact.params.iter())
            .all(|(aggregate, exact)| !aggregate.observed || aggregate == exact);
        let this_matches = !aggregate.this_observed
            || (exact.this_observed && aggregate.this_info == exact.this_info);
        params_match && this_matches
    }

    pub(super) fn function_has_untracked_captures(&self, function_id: &FunctionId) -> bool {
        self.analysis
            .function_plans
            .get(function_id)
            .is_some_and(|plan| !plan.captures.is_empty())
    }

    fn function_captures_current_script_activation(&self, function_id: &FunctionId) -> bool {
        if self.current_owner_id != SCRIPT_OWNER_ID {
            return false;
        }
        let Some(script_activation_id) = self
            .analysis
            .owner_plans
            .get(SCRIPT_OWNER_ID)
            .map(|owner| owner.activation_environment_id)
        else {
            return false;
        };
        self.analysis
            .function_plans
            .get(function_id)
            .is_some_and(|plan| {
                !plan.captures.is_empty()
                    && plan.captures.values().all(|capture| {
                        capture.owner_id == SCRIPT_OWNER_ID
                            && capture.environment_id == script_activation_id
                    })
            })
    }

    fn merge_signature_return_observations(
        &self,
        current: &FunctionSignature,
        propagated: &FunctionSignature,
    ) -> (ValueInfo, FunctionReturnShape) {
        self.merge_signature_return_value_observation(
            current,
            propagated.return_info(),
            &propagated.return_shape,
        )
    }

    pub(super) fn merge_signature_return_value_observation(
        &self,
        current: &FunctionSignature,
        observed_info: ValueInfo,
        observed_shape: &FunctionReturnShape,
    ) -> (ValueInfo, FunctionReturnShape) {
        if matches!(current.return_shape, FunctionReturnShape::Unobserved) {
            return (observed_info, observed_shape.clone());
        }
        if matches!(observed_shape, FunctionReturnShape::Unobserved) {
            return (current.return_info(), current.return_shape.clone());
        }
        let merged = self.merge_value_infos(current.return_info(), observed_info);
        let shape = FunctionReturnShape::merged(
            &current.return_shape,
            observed_shape,
            merged.heap_shape.clone(),
        );
        (merged, shape)
    }

    fn merge_exact_context_function_observation(
        &mut self,
        function_id: &FunctionId,
        helper_context_id: &ExactHelperContextId,
        propagated_signature: FunctionSignature,
    ) {
        let key = (function_id.clone(), helper_context_id.clone());
        if !self.exact_context_function_observations.contains_key(&key) {
            self.exact_context_function_observations
                .insert(key, propagated_signature);
            return;
        }
        let Some(current_signature) = self.exact_context_function_observations.get(&key).cloned()
        else {
            return;
        };
        let merged_this = if propagated_signature.this_observed {
            if current_signature.this_observed {
                self.merge_value_infos(
                    current_signature.this_info.clone(),
                    propagated_signature.this_info.clone(),
                )
            } else {
                propagated_signature.this_info.clone()
            }
        } else {
            current_signature.this_info.clone()
        };
        let (merged_return, merged_return_shape) =
            self.merge_signature_return_observations(&current_signature, &propagated_signature);
        let merged_source_call_flow_effects =
            current_signature.merged_source_call_flow_effects(&propagated_signature);
        if let Some(signature) = self.exact_context_function_observations.get_mut(&key) {
            for (current_param, propagated_param) in signature
                .params
                .iter_mut()
                .zip(propagated_signature.params.iter())
            {
                if current_param.is_rest {
                    continue;
                }
                Self::merge_signature_param_signature(current_param, propagated_param);
            }
            if propagated_signature.this_observed {
                signature.this_info = merged_this;
                signature.this_observed = true;
            }
            signature.return_kind = merged_return.kind;
            signature.return_possible_kinds = merged_return.possible_kinds;
            signature.return_shape = merged_return_shape;
            signature.return_targets = merged_return.function_targets;
            signature.source_call_flow_effects = merged_source_call_flow_effects;
        }
    }

    fn merge_signature_propagation(&mut self, propagated: FunctionSignatures) {
        let function_ids = self.analysis.function_order.clone();
        for function_id in function_ids {
            let Some(propagated_signature) = propagated.get(&function_id) else {
                continue;
            };
            let Some(current_signature) = self.function_signatures.get(&function_id).cloned()
            else {
                continue;
            };

            let merged_this = if propagated_signature.this_observed {
                if current_signature.this_observed {
                    self.merge_value_infos(
                        current_signature.this_info.clone(),
                        propagated_signature.this_info.clone(),
                    )
                } else {
                    propagated_signature.this_info.clone()
                }
            } else {
                current_signature.this_info.clone()
            };
            let (merged_return, merged_return_shape) =
                self.merge_signature_return_observations(&current_signature, propagated_signature);
            let merged_source_call_flow_effects =
                current_signature.merged_source_call_flow_effects(propagated_signature);

            if let Some(signature) = self.function_signatures.get_mut(&function_id) {
                for (current_param, propagated_param) in signature
                    .params
                    .iter_mut()
                    .zip(propagated_signature.params.iter())
                {
                    if current_param.is_rest {
                        continue;
                    }
                    Self::merge_signature_param_signature(current_param, propagated_param);
                }
                if propagated_signature.this_observed {
                    signature.this_info = merged_this;
                    signature.this_observed = true;
                }
                signature.return_kind = merged_return.kind;
                signature.return_possible_kinds = merged_return.possible_kinds;
                signature.return_shape = merged_return_shape;
                signature.return_targets = merged_return.function_targets;
                signature.source_call_flow_effects = merged_source_call_flow_effects;
            }
        }
    }

    fn merge_exact_callback_observations(
        &mut self,
        observations: BTreeMap<ExactCallbackContextKey, FunctionSignature>,
    ) {
        for ((function_id, _context_id), propagated_signature) in observations {
            self.merge_single_signature_observation(&function_id, propagated_signature);
        }
    }

    fn merge_context_keyed_callback_observations(
        &mut self,
        observations: BTreeMap<ExactCallbackContextKey, FunctionSignature>,
    ) {
        for ((function_id, helper_context_id), propagated_signature) in observations {
            self.merge_callback_observation_signature(
                &function_id,
                &helper_context_id,
                propagated_signature,
            );
        }
    }

    fn merge_callback_observation_signature(
        &mut self,
        function_id: &FunctionId,
        helper_context_id: &ExactHelperContextId,
        propagated_signature: FunctionSignature,
    ) {
        let key = (function_id.clone(), helper_context_id.clone());
        if !self.exact_context_callback_observations.contains_key(&key) {
            self.exact_context_callback_observations
                .insert(key, propagated_signature);
            return;
        }
        let Some(current_signature) = self.exact_context_callback_observations.get(&key).cloned()
        else {
            return;
        };
        let merged_this = if propagated_signature.this_observed {
            if current_signature.this_observed {
                self.merge_value_infos(
                    current_signature.this_info.clone(),
                    propagated_signature.this_info.clone(),
                )
            } else {
                propagated_signature.this_info.clone()
            }
        } else {
            current_signature.this_info.clone()
        };
        let (merged_return, merged_return_shape) =
            self.merge_signature_return_observations(&current_signature, &propagated_signature);
        let merged_source_call_flow_effects =
            current_signature.merged_source_call_flow_effects(&propagated_signature);
        if let Some(signature) = self.exact_context_callback_observations.get_mut(&key) {
            for (current_param, propagated_param) in signature
                .params
                .iter_mut()
                .zip(propagated_signature.params.iter())
            {
                if current_param.is_rest {
                    continue;
                }
                Self::merge_signature_param_signature(current_param, propagated_param);
            }
            if propagated_signature.this_observed {
                signature.this_info = merged_this;
                signature.this_observed = true;
            }
            signature.return_kind = merged_return.kind;
            signature.return_possible_kinds = merged_return.possible_kinds;
            signature.return_shape = merged_return_shape;
            signature.return_targets = merged_return.function_targets;
            signature.source_call_flow_effects = merged_source_call_flow_effects;
        }
    }

    fn merge_single_signature_observation(
        &mut self,
        function_id: &FunctionId,
        propagated_signature: FunctionSignature,
    ) {
        let Some(current_signature) = self.function_signatures.get(function_id).cloned() else {
            return;
        };

        let merged_this = if propagated_signature.this_observed {
            if current_signature.this_observed {
                self.merge_value_infos(
                    current_signature.this_info.clone(),
                    propagated_signature.this_info.clone(),
                )
            } else {
                propagated_signature.this_info.clone()
            }
        } else {
            current_signature.this_info.clone()
        };
        let (merged_return, merged_return_shape) =
            self.merge_signature_return_observations(&current_signature, &propagated_signature);
        let merged_source_call_flow_effects =
            current_signature.merged_source_call_flow_effects(&propagated_signature);

        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            for (current_param, propagated_param) in signature
                .params
                .iter_mut()
                .zip(propagated_signature.params.iter())
            {
                if current_param.is_rest {
                    continue;
                }
                Self::merge_signature_param_signature(current_param, propagated_param);
            }
            if propagated_signature.this_observed {
                signature.this_info = merged_this;
                signature.this_observed = true;
            }
            signature.return_kind = merged_return.kind;
            signature.return_possible_kinds = merged_return.possible_kinds;
            signature.return_shape = merged_return_shape;
            signature.return_targets = merged_return.function_targets;
            signature.source_call_flow_effects = merged_source_call_flow_effects;
        }
    }
}
