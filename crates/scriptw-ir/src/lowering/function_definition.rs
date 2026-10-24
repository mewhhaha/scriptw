use super::*;

mod receiver_info;

#[derive(Clone, Copy)]
enum FunctionLoweringPurpose {
    Emit,
    Infer,
}

enum LoweredFunctionBody {
    Complete(BlockIr),
    Inferred(super::function_inference::InferenceBodySummary),
}

impl<'a> ScriptLowerer<'a> {
    pub(super) fn lower_function(
        &mut self,
        function: &FunctionPlan<'a>,
        output_id: Option<FunctionId>,
        context_key_override: Option<ExactCallbackContextKey>,
    ) -> FunctionIr {
        self.lower_function_for_purpose(
            function,
            output_id,
            context_key_override,
            FunctionLoweringPurpose::Emit,
        )
        .expect("emission retains the complete function")
    }

    pub(super) fn infer_function(
        &mut self,
        function: &FunctionPlan<'a>,
        context_key_override: Option<ExactCallbackContextKey>,
    ) {
        #[cfg(test)]
        if super::function_inference::retain_complete_inference() {
            let _ = self.lower_function(function, None, context_key_override);
            return;
        }
        let _ = self.lower_function_for_purpose(
            function,
            None,
            context_key_override,
            FunctionLoweringPurpose::Infer,
        );
    }

    fn lower_function_for_purpose(
        &mut self,
        function: &FunctionPlan<'a>,
        output_id: Option<FunctionId>,
        context_key_override: Option<ExactCallbackContextKey>,
        purpose: FunctionLoweringPurpose,
    ) -> Option<FunctionIr> {
        let output_id = output_id.unwrap_or_else(|| function.id.clone());
        let resumable_plan = if function.protocol.execution_kind()
            == FunctionExecutionKind::AsyncGenerator
        {
            match async_generator_resumable_plan(function.body) {
                Ok(plan) => Some(plan),
                Err(error) => {
                    self.unsupported_with_message(format!(
                        "unsupported in scriptw wasm-aot: async-generator source has no complete continuation owner: {error:?}"
                    ));
                    None
                }
            }
        } else {
            None
        };
        let captures_private_environment = self
            .analysis
            .owner_plans
            .get(&function.id)
            .is_some_and(|owner| owner.private_environment_id.is_some());
        let mut lowerer = ScriptLowerer::new(
            self.interner,
            self.analysis,
            self.source_text,
            self.root_this_binding_for_function_owner(&function.id),
            function.id.clone(),
            self.host_surface_policy,
        );
        // These five maps are moved back into `self` on every exit path below,
        // so hand them to the nested lowerer by move rather than deep clone.
        // FunctionSignature values carry deep ValueInfo/HeapShape trees; cloning
        // the full map per lowered function dominated harness-heavy compiles
        // (~30ms x hundreds of lower_function calls).
        lowerer.function_signatures = std::mem::take(&mut self.function_signatures);
        lowerer.visible_function_names = self.visible_function_names.clone();
        lowerer.global_properties = self.global_properties.clone();
        lowerer.observed_script_global_writes = self.observed_script_global_writes.clone();
        lowerer.well_known_symbol_prototype_properties =
            self.well_known_symbol_prototype_properties.clone();
        lowerer.known_nested_script_global_value_infos =
            self.known_nested_script_global_value_infos.clone();
        for (name, info) in &self.nested_script_global_value_infos {
            let merged = match lowerer.known_nested_script_global_value_infos.remove(name) {
                Some(existing) => lowerer.merge_value_infos(existing, info.clone()),
                None => info.clone(),
            };
            lowerer
                .known_nested_script_global_value_infos
                .insert(name.clone(), merged);
        }
        lowerer.array_prototype_mutated = self.array_prototype_mutated;
        lowerer.number_prototype_to_string_state = self.number_prototype_to_string_state;
        lowerer.boolean_prototype_to_string_state = self.boolean_prototype_to_string_state;
        lowerer.dynamically_installed_getters = self.dynamically_installed_getters.clone();
        lowerer.dynamically_installed_setters = self.dynamically_installed_setters.clone();
        lowerer.unknown_user_code_effects_observed = self.unknown_user_code_effects_observed;
        lowerer.function_signature_shape_evidence = self.function_signature_shape_evidence;
        lowerer.static_boolean_bindings = self.static_boolean_bindings.clone();
        lowerer.static_string_bindings = self.static_string_bindings.clone();
        lowerer.function_source_binding_candidates =
            self.function_source_binding_candidates.clone();
        lowerer.function_source_parameter_candidates =
            self.function_source_parameter_candidates.clone();
        lowerer.static_to_string_regexp_object_bindings =
            self.static_to_string_regexp_object_bindings.clone();
        lowerer.var_bindings = self.var_bindings.clone();
        lowerer.seed_script_global_var_properties();
        // A final body may execute after any top-level write, but it is
        // lowered before the root-statement pass replays those writes.
        // Degrade every observed-written global so shape reads, static calls
        // and intrinsic folds in the emitted body all observe the possible
        // replacement. Prepass and propagation bodies keep baseline facts:
        // their output is signatures and summaries, never emitted code.
        if !self.is_prepass {
            self.degrade_observed_written_globals(&mut lowerer);
        }
        lowerer.exact_context_function_observations =
            std::mem::take(&mut self.exact_context_function_observations);
        lowerer.exact_context_callback_observations =
            std::mem::take(&mut self.exact_context_callback_observations);
        lowerer.exact_context_callback_specializations =
            std::mem::take(&mut self.exact_context_callback_specializations);
        lowerer.exact_context_function_specializations =
            std::mem::take(&mut self.exact_context_function_specializations);
        lowerer.active_direct_call_propagations = self.active_direct_call_propagations.clone();
        lowerer.completed_direct_call_propagations =
            self.completed_direct_call_propagations.clone();
        lowerer.is_prepass = self.is_prepass;
        lowerer.script_global_call_observation_mode = self.script_global_call_observation_mode;
        lowerer.is_function_body = true;
        lowerer.current_function_id = Some(function.id.clone());
        match function.protocol.source_execution_kind() {
            FunctionExecutionKind::Generator => {
                lowerer.current_generator_resume_state = Some(0);
            }
            FunctionExecutionKind::Async => {
                lowerer.current_async_resume_state = Some(0);
            }
            FunctionExecutionKind::AsyncGenerator => {
                if let Some(plan) = &resumable_plan {
                    lowerer.current_generator_resume_state = Some(plan.entry_state);
                    lowerer.current_async_resume_state = Some(plan.entry_state);
                    lowerer.current_resumable_plan = resumable_plan.clone();
                }
            }
            FunctionExecutionKind::Ordinary => {}
        }
        let exact_context_signature =
            lowerer.exact_signature_for_function(&function.id, context_key_override.as_ref());
        let exact_helper_context_id = context_key_override
            .as_ref()
            .map(|(_, context_id)| context_id);
        if let Some((_, helper_context_id)) = context_key_override.as_ref() {
            if let Some(signature) = exact_context_signature.as_ref() {
                for param in &signature.params {
                    if param.is_rest {
                        break;
                    }
                    for callback_id in param.function_targets.exact_targets().into_iter().flatten()
                    {
                        let original_callback_id = lowerer.original_exact_function_id(callback_id);
                        if self.is_prepass
                            || lowerer
                                .exact_context_callback_specializations
                                .contains_key(&(
                                    original_callback_id.clone(),
                                    helper_context_id.clone(),
                                ))
                        {
                            lowerer
                                .exact_context_callback_targets
                                .insert(original_callback_id, helper_context_id.clone());
                        }
                    }
                }
            }
        }
        let captures_direct_eval_context = function
            .captures
            .contains_key(DIRECT_EVAL_EXECUTION_CONTEXT_NAME);
        lowerer.current_this_binding = if captures_direct_eval_context {
            CurrentThisBinding::Activation(unknown_runtime_value_info())
        } else if function.protocol.flavor() == FunctionFlavor::Arrow {
            if let Some(capture) = function.captures.get(LEXICAL_THIS_NAME) {
                if capture.owner_id == SCRIPT_OWNER_ID {
                    CurrentThisBinding::Root(lowerer.root_this_binding)
                } else {
                    CurrentThisBinding::Activation(
                        lowerer.capture_value_info(capture.owner_id.as_str(), LEXICAL_THIS_NAME),
                    )
                }
            } else if function.lexical_derived_activation_owner.is_some() {
                CurrentThisBinding::Activation(ValueInfo::undefined())
            } else {
                CurrentThisBinding::Root(lowerer.root_this_binding)
            }
        } else {
            let this_info =
                lowerer.receiver_info_for_function(function, exact_context_signature.as_ref());
            CurrentThisBinding::Activation(this_info)
        };
        if function.protocol.flavor() != FunctionFlavor::Arrow
            && !function.strict
            && lowerer.current_this_info().possible_kinds.is_subset_of(
                KindSet::from_kind(ValueKind::Undefined).union(KindSet::from_kind(ValueKind::Null)),
            )
        {
            lowerer.current_this_binding =
                CurrentThisBinding::Activation(lowerer.global_this_info());
        }
        lowerer.current_construct_this_info = function
            .protocol
            .is_constructable()
            .then(Self::fresh_constructed_instance_info);
        lowerer.current_new_target_info = if captures_direct_eval_context {
            unknown_runtime_value_info()
        } else if function.protocol.flavor() == FunctionFlavor::Arrow {
            function
                .captures
                .get(LEXICAL_NEW_TARGET_NAME)
                .map(|capture| {
                    lowerer.capture_value_info(capture.owner_id.as_str(), LEXICAL_NEW_TARGET_NAME)
                })
                .unwrap_or_else(ValueInfo::undefined)
        } else if function.protocol.is_constructable() {
            lowerer.merge_value_infos(
                ValueInfo::undefined(),
                lowerer.function_value_info(&function.id),
            )
        } else {
            ValueInfo::undefined()
        };
        lowerer.current_owner_id = function.id.clone();
        lowerer.seed_definition_environment_positions(function);
        let lexical_derived_activation =
            function
                .lexical_derived_activation_owner
                .as_ref()
                .map(|owner_function_id| DerivedConstructorActivationIr {
                    owner_function_id: owner_function_id.clone(),
                    this_binding: DERIVED_ACTIVATION_THIS_NAME.to_string(),
                    this_status_binding: DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                    new_target_binding: DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                    active_function_binding: DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
                });
        if captures_direct_eval_context {
            lowerer.class_context = Some(ClassLoweringContext {
                is_derived_constructor: matches!(
                    lowerer.direct_eval_invocation(),
                    Some(scriptw_front::EvalInvocationContext::DerivedConstructor)
                ),
                ..ClassLoweringContext::default()
            });
        } else if lexical_derived_activation.is_some() {
            lowerer.class_context = Some(ClassLoweringContext {
                is_derived_constructor: true,
                ..ClassLoweringContext::default()
            });
        } else if function.protocol.is_object_literal_method()
            || function.captures.contains_key(LEXICAL_HOME_OBJECT_NAME)
        {
            lowerer.class_context = Some(ClassLoweringContext::default());
        }
        let Some(parameters) =
            lowerer.lower_function_parameters(function.parameters, function.name.as_str())
        else {
            self.merge_child_compilation_records(&mut lowerer);
            self.function_signatures = lowerer.function_signatures;
            self.exact_context_function_observations = lowerer.exact_context_function_observations;
            self.exact_context_callback_observations = lowerer.exact_context_callback_observations;
            self.exact_context_callback_specializations =
                lowerer.exact_context_callback_specializations;
            self.exact_context_function_specializations =
                lowerer.exact_context_function_specializations;
            self.merge_called_script_global_value_infos(&lowerer.called_script_global_value_infos);
            self.completed_direct_call_propagations = lowerer.completed_direct_call_propagations;
            if matches!(purpose, FunctionLoweringPurpose::Infer) {
                return None;
            }
            return Some(FunctionIr {
                template_source: self.analysis.template_source,
                eval_environment: self.analysis.owner_eval_environment(&function.id),
                id: output_id.clone(),
                name: function.name.clone(),
                to_string_representation: function.to_string_representation.clone(),
                protocol: function.protocol,
                generator_plan: (function.protocol.execution_kind()
                    == FunctionExecutionKind::Generator)
                    .then(|| {
                        if function.protocol == FunctionProtocolIr::ModuleActivation {
                            GeneratorPlanIr::module_instantiation()
                        } else {
                            linear_generator_plan(function.body)
                                .unwrap_or_else(GeneratorPlanIr::without_suspensions)
                        }
                    }),
                resumable_plan: resumable_plan.clone(),
                strict: function.strict,
                class_element_execution_kind: ClassElementExecutionKind::None,
                class_heritage_kind: ClassHeritageKind::None,
                is_static_class_member: false,
                is_derived_constructor: false,
                is_synthetic_default_derived_constructor: false,
                class_instance_element_plan: None,
                super_constructor_target: None,
                uses_super: false,
                this_before_super: false,
                lexical_derived_activation: lexical_derived_activation.clone(),
                private_name_ids: BTreeMap::new(),
                captures_private_environment,
                is_nested: function.parent_owner_id != SCRIPT_OWNER_ID,
                is_expression: function.is_expression,
                is_named_expression: function.is_expression && function.self_binding_name.is_some(),
                captures_lexical_this: function.captures.contains_key(LEXICAL_THIS_NAME),
                captures_lexical_arguments: function.captures.contains_key(LEXICAL_ARGUMENTS_NAME),
                params: Vec::new(),
                body: BlockIr {
                    statements: Vec::new(),
                    result_kind: ValueKind::Undefined,
                    lexical_environment: None,
                },
                return_kind: ValueKind::Undefined,
                return_shape: None,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                owned_env_bindings: Vec::new(),
                captured_bindings: Vec::new(),
            });
        };
        if let Some(self_binding_name) = function.self_binding_name.as_ref() {
            let function_info = lowerer.function_value_info(&function.id);
            lowerer
                .sloppy_immutable_binding_storage_names
                .insert(self_binding_name.clone());
            lowerer.declare_binding(
                self_binding_name.clone(),
                BindingInfo {
                    mode: BindingMode::Const,
                    storage_name: self_binding_name.clone(),
                    kind: function_info.kind,
                    possible_kinds: function_info.possible_kinds,
                    heap_shape: function_info.heap_shape,
                    function_targets: function_info.function_targets,
                    initialization: Initialization::Initialized,
                },
            );
        }
        for (name, capture) in &function.captures {
            if lowerer.is_script_global_object_capture(
                name,
                &capture.source_name,
                &capture.owner_id,
            ) {
                continue;
            }
            let source_name = capture.source_name.as_str();
            let mode = capture.mode;
            let info = if mode != BindingMode::Const {
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                }
            } else if name != source_name || TdzPlaceholderName::names_a_placeholder(name) {
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                }
            } else if capture.owner_id == self.current_owner_id {
                let binding_info = self.lookup_binding(source_name).map(|binding| ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape,
                    function_targets: binding.function_targets,
                });
                let inferred_info =
                    lowerer.infer_owner_var_binding_info(capture.owner_id.as_str(), source_name);
                // A hoisted function declaration is lowered before the
                // top-level `const` it captures, so `lookup_binding` can still
                // see the hoist-time TDZ placeholder, whose kind is
                // `Undefined`. Publishing that as a proven singleton kind is
                // unsound: it propagates into `signature.return_kind` and lets
                // constant folding claim the callee returns `undefined`. The
                // prepass ran the whole root statement list to completion, so
                // its root-scope metadata carries the post-initialization kind.
                let prepass_info = (capture.owner_id.as_str() == SCRIPT_OWNER_ID)
                    .then(|| self.var_bindings.get(source_name))
                    .flatten()
                    .filter(|metadata| metadata.is_lexical_metadata)
                    .filter(|metadata| {
                        metadata.kind != ValueKind::Undefined && metadata.kind != ValueKind::Dynamic
                    })
                    .map(|metadata| ValueInfo {
                        kind: metadata.kind,
                        possible_kinds: metadata.possible_kinds,
                        heap_shape: metadata.heap_shape.clone(),
                        function_targets: metadata.function_targets.clone(),
                    });
                match (binding_info, inferred_info) {
                    (Some(binding), Some(inferred))
                        if binding.kind == ValueKind::Undefined
                            || binding.kind == ValueKind::Dynamic =>
                    {
                        inferred
                    }
                    // Never publish the TDZ placeholder itself. Prefer the
                    // prepass kind, and fall back to `Dynamic` rather than to
                    // a singleton `Undefined` nothing ever proved.
                    (Some(binding), None)
                        if binding.kind == ValueKind::Undefined
                            || binding.kind == ValueKind::Dynamic =>
                    {
                        prepass_info.unwrap_or(ValueInfo {
                            kind: ValueKind::Dynamic,
                            possible_kinds: KindSet::all_runtime_tags(),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::unknown(),
                        })
                    }
                    (Some(binding), _) => binding,
                    (None, Some(inferred)) => inferred,
                    (None, None) => {
                        lowerer.capture_value_info(capture.owner_id.as_str(), source_name)
                    }
                }
            } else {
                lowerer.capture_value_info(capture.owner_id.as_str(), source_name)
            };
            if mode == BindingMode::Const
                && self
                    .analysis
                    .function_plans
                    .get(&capture.owner_id)
                    .and_then(|owner| owner.self_binding_name.as_deref())
                    == Some(source_name)
            {
                lowerer
                    .sloppy_immutable_binding_storage_names
                    .insert(name.clone());
            }
            lowerer.declare_binding(
                capture.source_name.clone(),
                BindingInfo {
                    mode,
                    storage_name: name.clone(),
                    kind: info.kind,
                    possible_kinds: info.possible_kinds,
                    heap_shape: info.heap_shape,
                    function_targets: info.function_targets,
                    initialization: Initialization::Initialized,
                },
            );
            lowerer.install_finite_source_capture_key(
                &capture.source_name,
                capture.environment_id,
                name,
            );
        }

        if function.protocol.flavor() == FunctionFlavor::Ordinary {
            lowerer.declare_binding(
                LEXICAL_ARGUMENTS_NAME.to_string(),
                BindingInfo {
                    mode: BindingMode::Let,
                    storage_name: LEXICAL_ARGUMENTS_NAME.to_string(),
                    kind: ValueKind::Arguments,
                    possible_kinds: KindSet::from_kind(ValueKind::Arguments),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                    initialization: Initialization::Initialized,
                },
            );
        }

        let mut params = Vec::with_capacity(parameters.as_ref().len());
        let mut parameter_prefix_statements = Vec::new();
        let parameter_names = parameters
            .as_ref()
            .iter()
            .flat_map(|parameter| {
                let mut names = Vec::new();
                collect_binding_names(self.interner, parameter.variable().binding(), &mut names);
                names
            })
            .collect::<Vec<_>>();
        for name in &parameter_names {
            // 10.2.11 step 21: every BoundName of the formals is created
            // before any default initializer evaluates, so `function f(a = b, b)`
            // throws. Step 24/27 initializes them left to right, which is the
            // `declare_binding` further down that overwrites this entry with an
            // `Initialization::Initialized` one — the old `clear_tdz_binding`
            // loop was a second, separately ordered spelling of that overwrite
            // and is gone.
            lowerer.declare_binding(
                name.clone(),
                BindingInfo::tdz_placeholder(
                    BindingMode::Let,
                    TdzPlaceholderName::for_source_name(name),
                ),
            );
        }
        for (index, parameter) in parameters.as_ref().iter().enumerate() {
            let binding = parameter.variable().binding();
            let name = binding_parameter_storage_name(self.interner, binding, index);
            let param_info = lowerer
                .function_parameter_info_for_current_flow(
                    &function.id,
                    context_key_override.as_ref(),
                    params.len(),
                )
                .unwrap_or_else(|| {
                    if parameter.is_rest_param() {
                        ValueInfo {
                            kind: ValueKind::Array,
                            possible_kinds: KindSet::from_kind(ValueKind::Array),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                        }
                    } else {
                        ValueInfo {
                            kind: ValueKind::Dynamic,
                            possible_kinds: KindSet::all_runtime_tags(),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::unknown(),
                        }
                    }
                });
            let param_info =
                lowerer.specialize_exact_context_function_info(param_info, exact_helper_context_id);
            let default_source_candidates = parameter
                .init()
                .map(|expression| lowerer.function_source_value_candidates(expression))
                .unwrap_or_default();
            let default_init = parameter
                .init()
                .map(|expression| lowerer.lower_expression(expression));
            // When an argument is omitted, the default initializer runs and its
            // value becomes the parameter's runtime value — it is never actually
            // `undefined`. Call-site observation alone (`merge_omitted_signature_
            // params_as_undefined`) has no visibility into the default expression
            // and records the omitted-argument case as kind `Undefined`, which is
            // wrong whenever the default produces something else (e.g. `x = 1`).
            // Union the default initializer's inferred kind into the parameter's
            // static kind so the binding — and everything derived from it,
            // including the function's return kind — reflects what the default
            // path can actually produce.
            let param_info = match default_init.as_ref() {
                Some(default_init) => {
                    lowerer.merge_value_infos(param_info, default_init.value_info())
                }
                None => param_info,
            };
            lowerer.declare_binding(
                name.clone(),
                BindingInfo {
                    mode: BindingMode::Let,
                    storage_name: name.clone(),
                    kind: param_info.kind,
                    possible_kinds: param_info.possible_kinds,
                    heap_shape: param_info.heap_shape,
                    function_targets: param_info.function_targets,
                    initialization: Initialization::Initialized,
                },
            );
            lowerer.current_param_names.push(name.clone());
            if !parameter.is_rest_param() {
                lowerer.install_function_parameter_source_candidates(
                    &function.id,
                    index,
                    binding,
                    default_source_candidates,
                );
            }
            params.push(FunctionParamIr {
                name: name.clone(),
                kind: param_info.kind,
                default_init,
                is_rest: parameter.is_rest_param(),
            });
            let binding_initializers = lowerer.lower_parameter_binding_pattern(binding, &name);
            if !binding_initializers.is_empty() {
                parameter_prefix_statements.push(StatementIr::ParameterInitialization {
                    parameter_index: index,
                    statements: binding_initializers,
                });
            }
        }

        let body_environment = lowerer.begin_function_body_environment();
        lowerer.hoist_root_statement_items(function.body.statements());

        // 10.2.11 step 30: a function body is a statement-list scope too, and
        // its `let`/`const`/`class` names are uninitialized until their
        // declarators run. `lexEnv` is the frame the parameter bindings above
        // were declared into, so the sweep joins it rather than pushing one.
        let body_scope = LexicalScopeInstantiation::instantiate_in_current_scope(
            &mut lowerer,
            function.body.statements(),
        );
        let inference_list = match purpose {
            FunctionLoweringPurpose::Infer => {
                super::function_inference::InferenceStatementList::new(function)
            }
            FunctionLoweringPurpose::Emit => None,
        };
        let lowered_body = match inference_list {
            Some(items) => LoweredFunctionBody::Inferred(lowerer.infer_root_statement_items(
                items,
                function.root_functions.as_slice(),
                body_scope,
            )),
            None => LoweredFunctionBody::Complete(lowerer.lower_root_statement_items(
                function.body.statements(),
                function.root_functions.as_slice(),
                body_scope,
            )),
        };
        if let Some(planned_suspensions) = lowerer
            .current_resumable_plan
            .as_ref()
            .map(|plan| plan.suspension_points.len())
        {
            let consumed = lowerer.next_resumable_suspension_index;
            if consumed != planned_suspensions {
                lowerer.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot first slice: async-generator body lowering for `{}` preserved {consumed} of {} planned suspension points",
                    function.id,
                    planned_suspensions
                ));
            }
        }
        if lowerer
            .current_resumable_plan
            .as_ref()
            .is_some_and(|plan| !plan.matches_resume_environment_plan())
        {
            lowerer.unsupported(
                "async-generator resume environments must retain their checked source owner",
            );
        }
        let (body, final_statement_is_return, finalized_effects) = match lowered_body {
            LoweredFunctionBody::Complete(lowered_body) => {
                let mut body_statements = parameter_prefix_statements;
                let result_kind = lowered_body.result_kind;
                body_statements.extend(
                    lowerer.finish_function_body_environment(lowered_body, body_environment),
                );
                let body = BlockIr {
                    result_kind,
                    statements: body_statements,
                    lexical_environment: None,
                };
                let final_statement_is_return =
                    Self::statement_list_ends_in_return(&body.statements);
                let effects = SourceCallFlowEffects::for_finalized_invocation(&params, &body);
                (Some(body), final_statement_is_return, effects)
            }
            LoweredFunctionBody::Inferred(summary) => {
                let mut prefix = super::function_inference::InferenceBodySummary::new();
                for statement in parameter_prefix_statements {
                    prefix.observe(&statement);
                }
                prefix.append(summary);
                // This is the lifecycle half of finish_function_body_environment;
                // inference has no retained BlockIr to attach the owner to.
                if body_environment.is_some() {
                    lowerer.pop_scope();
                }
                (
                    None,
                    prefix.ends_in_return(),
                    prefix.invocation_effects(&params),
                )
            }
        };
        lowerer.source_call_flow_effects = lowerer
            .source_call_flow_effects
            .merge_observation(finalized_effects);
        let mut return_info = lowerer
            .current_return
            .as_ref()
            .map(|returned| returned.info.clone())
            .unwrap_or_else(ValueInfo::undefined);
        if !final_statement_is_return {
            return_info = lowerer.merge_value_infos(return_info, ValueInfo::undefined());
        }

        if !params.is_empty() {
            let context_signature = lowerer
                .exact_signature_for_function(&function.id, context_key_override.as_ref())
                .or_else(|| lowerer.function_signature_for_current_flow(&function.id));
            if let Some(signature) = context_signature {
                for (param, signature_param) in params.iter_mut().zip(signature.params.iter()) {
                    param.kind = signature_param.kind;
                    lowerer.update_binding_shape_path(
                        &param.name,
                        &[],
                        ValueInfo {
                            kind: signature_param.kind,
                            possible_kinds: signature_param.possible_kinds,
                            heap_shape: signature_param.heap_shape.clone(),
                            function_targets: signature_param.function_targets.clone(),
                        },
                    );
                }
            }
        }
        if function.protocol.execution_kind() == FunctionExecutionKind::Generator {
            return_info = ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::generator_instance_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            };
        } else if function.protocol.execution_kind() == FunctionExecutionKind::AsyncGenerator {
            return_info = ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::async_generator_instance_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            };
        } else if function.protocol.execution_kind() == FunctionExecutionKind::Async {
            return_info = Self::value_info_from_shape(Some(Self::promise_instance_shape()));
        }
        if let Some(signature) = lowerer.function_signatures.get_mut(&function.id) {
            signature.return_kind = return_info.kind;
            signature.return_possible_kinds = return_info.possible_kinds;
            signature.return_shape =
                if function.protocol.execution_kind() == FunctionExecutionKind::Ordinary {
                    signature
                        .return_shape
                        .with_shape(return_info.heap_shape.clone())
                } else {
                    FunctionReturnShape::flow_sensitive(return_info.heap_shape.clone())
                };
            signature.return_targets = return_info.function_targets.clone();
            signature.constructor_instance = lowerer
                .current_construct_this_info
                .clone()
                .unwrap_or_else(ValueInfo::undefined);
            signature.source_call_flow_effects = lowerer.source_call_flow_effects;
        }
        let resumable_plan = lowerer.current_resumable_plan.clone().or(resumable_plan);

        let generator_plan = (matches!(purpose, FunctionLoweringPurpose::Emit)
            && function.protocol.execution_kind() == FunctionExecutionKind::Generator)
            .then(|| {
                if function.protocol == FunctionProtocolIr::ModuleActivation {
                    GeneratorPlanIr::module_instantiation()
                } else {
                    let source_plan = linear_generator_plan(function.body)
                        .unwrap_or_else(GeneratorPlanIr::without_suspensions);
                    lowerer.generator_plan_for_lowered_named_environment(
                        body.as_ref().expect("emission retains the body"),
                        source_plan,
                    )
                }
            });

        self.merge_nested_script_global_value_infos(&lowerer.nested_script_global_value_infos);
        self.merge_called_script_global_value_infos(&lowerer.called_script_global_value_infos);
        self.merge_child_compilation_records(&mut lowerer);
        self.function_signatures = lowerer.function_signatures;
        self.exact_context_function_observations = lowerer.exact_context_function_observations;
        self.exact_context_callback_observations = lowerer.exact_context_callback_observations;
        self.exact_context_callback_specializations =
            lowerer.exact_context_callback_specializations;
        self.exact_context_function_specializations =
            lowerer.exact_context_function_specializations;
        self.completed_direct_call_propagations = lowerer.completed_direct_call_propagations;
        self.dynamically_installed_getters
            .extend(lowerer.dynamically_installed_getters);
        self.dynamically_installed_setters
            .extend(lowerer.dynamically_installed_setters);
        self.observed_script_global_writes
            .extend(std::mem::take(&mut lowerer.observed_script_global_writes));
        self.used_host_builtins.extend(lowerer.used_host_builtins);
        self.host_builtin_calls += lowerer.host_builtin_calls;
        self.top_level_this_uses += lowerer.top_level_this_uses;

        if matches!(purpose, FunctionLoweringPurpose::Infer) {
            return None;
        }
        let body = body.expect("emission retains the body");
        let body_uses_super = summarize_block(&body).super_uses > 0;

        Some(FunctionIr {
            template_source: self.analysis.template_source,
            eval_environment: self.analysis.owner_eval_environment(&function.id),
            id: output_id,
            name: function.name.clone(),
            to_string_representation: function.to_string_representation.clone(),
            protocol: function.protocol,
            generator_plan,
            resumable_plan,
            strict: function.strict,
            class_element_execution_kind: ClassElementExecutionKind::None,
            class_heritage_kind: ClassHeritageKind::None,
            is_static_class_member: false,
            is_derived_constructor: false,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            super_constructor_target: None,
            uses_super: body_uses_super,
            this_before_super: false,
            lexical_derived_activation,
            private_name_ids: BTreeMap::new(),
            captures_private_environment,
            is_nested: function.parent_owner_id != SCRIPT_OWNER_ID,
            is_expression: function.is_expression,
            is_named_expression: function.is_expression && function.self_binding_name.is_some(),
            captures_lexical_this: function.captures.contains_key(LEXICAL_THIS_NAME),
            captures_lexical_arguments: function.captures.contains_key(LEXICAL_ARGUMENTS_NAME),
            params,
            body,
            return_kind: return_info.kind,
            return_shape: return_info.heap_shape,
            return_targets: return_info.function_targets,
            constructor_instance: lowerer
                .current_construct_this_info
                .clone()
                .unwrap_or_else(ValueInfo::undefined),
            owned_env_bindings: self
                .analysis
                .owner_plans
                .get(&function.id)
                .map(|owner| {
                    owner
                        .owned_env_slots
                        .iter()
                        .map(|(name, slot)| {
                            self.analysis.owner_environment_binding(
                                &function.id,
                                name,
                                *slot,
                                self.interner,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into_iter()
                .chain(lowerer.generated_owned_env_bindings)
                .collect(),
            captured_bindings: function
                .captures
                .iter()
                .filter(|(name, capture)| {
                    !self.is_script_global_object_capture(
                        name,
                        &capture.source_name,
                        &capture.owner_id,
                    )
                })
                .map(|(name, capture)| CapturedBindingIr {
                    name: name.clone(),
                    source_name: capture.source_name.clone(),
                    mode: capture.mode,
                    slot: capture.slot,
                    hops: capture.hops,
                })
                .collect(),
        })
    }
}
