//! Captured branch facts and their authoritative conservative join algebra.

use super::*;

/// Flow-sensitive facts whose post-expression value depends on whether a
/// short-circuit or conditional branch executes.
#[derive(Clone)]
pub(super) struct ConditionalFlowFacts {
    scopes: Vec<BTreeMap<String, BindingInfo>>,
    var_bindings: BTreeMap<String, VarBindingInfo>,
    current_this_binding: CurrentThisBinding,
    current_construct_this_info: Option<ValueInfo>,
    global_properties: GlobalProperties,
    well_known_symbol_prototype_properties: BTreeMap<(String, WellKnownSymbol), ValueInfo>,
    nested_script_global_value_infos: BTreeMap<String, ValueInfo>,
    array_prototype_mutated: bool,
    number_prototype_to_string_state: PrototypeToStringState,
    boolean_prototype_to_string_state: PrototypeToStringState,
    dynamically_installed_getters: BTreeSet<FunctionId>,
    dynamically_installed_setters: BTreeSet<FunctionId>,
    unknown_user_code_effects_observed: bool,
    function_signature_shape_evidence: FunctionSignatureShapeEvidence,
    static_boolean_bindings: BTreeMap<String, bool>,
    static_string_bindings: StaticStringBindingFacts,
    static_to_string_regexp_object_bindings: BTreeSet<String>,
    invalidated_static_binding_names: BTreeSet<String>,
    boolean_alias_shapes_invalidated: bool,
}

pub(super) fn equal_map_intersection<K, V>(
    left: &BTreeMap<K, V>,
    right: &BTreeMap<K, V>,
) -> BTreeMap<K, V>
where
    K: Ord + Clone,
    V: Clone + PartialEq,
{
    left.iter()
        .filter_map(|(key, left_value)| {
            right
                .get(key)
                .filter(|right_value| *right_value == left_value)
                .map(|_| (key.clone(), left_value.clone()))
        })
        .collect()
}

impl<'a> ScriptLowerer<'a> {
    pub(super) fn capture_conditional_flow_facts(&self) -> ConditionalFlowFacts {
        ConditionalFlowFacts {
            scopes: self.scopes.clone(),
            var_bindings: self.var_bindings.clone(),
            current_this_binding: self.current_this_binding.clone(),
            current_construct_this_info: self.current_construct_this_info.clone(),
            global_properties: self.global_properties.clone(),
            well_known_symbol_prototype_properties: self
                .well_known_symbol_prototype_properties
                .clone(),
            nested_script_global_value_infos: self.nested_script_global_value_infos.clone(),
            array_prototype_mutated: self.array_prototype_mutated,
            number_prototype_to_string_state: self.number_prototype_to_string_state,
            boolean_prototype_to_string_state: self.boolean_prototype_to_string_state,
            dynamically_installed_getters: self.dynamically_installed_getters.clone(),
            dynamically_installed_setters: self.dynamically_installed_setters.clone(),
            unknown_user_code_effects_observed: self.unknown_user_code_effects_observed,
            function_signature_shape_evidence: self.function_signature_shape_evidence,
            static_boolean_bindings: self.static_boolean_bindings.clone(),
            static_string_bindings: self.static_string_bindings.clone(),
            static_to_string_regexp_object_bindings: self
                .static_to_string_regexp_object_bindings
                .clone(),
            invalidated_static_binding_names: self.invalidated_static_binding_names.clone(),
            boolean_alias_shapes_invalidated: self.boolean_alias_shapes_invalidated,
        }
    }

    pub(super) fn install_conditional_flow_facts(&mut self, facts: ConditionalFlowFacts) {
        self.scopes = facts.scopes;
        self.var_bindings = facts.var_bindings;
        self.current_this_binding = facts.current_this_binding;
        self.current_construct_this_info = facts.current_construct_this_info;
        self.global_properties = facts.global_properties;
        self.well_known_symbol_prototype_properties = facts.well_known_symbol_prototype_properties;
        self.nested_script_global_value_infos = facts.nested_script_global_value_infos;
        self.array_prototype_mutated = facts.array_prototype_mutated;
        self.number_prototype_to_string_state = facts.number_prototype_to_string_state;
        self.boolean_prototype_to_string_state = facts.boolean_prototype_to_string_state;
        self.dynamically_installed_getters = facts.dynamically_installed_getters;
        self.dynamically_installed_setters = facts.dynamically_installed_setters;
        self.unknown_user_code_effects_observed = facts.unknown_user_code_effects_observed;
        self.function_signature_shape_evidence = facts.function_signature_shape_evidence;
        self.static_boolean_bindings = facts.static_boolean_bindings;
        self.static_string_bindings = facts.static_string_bindings;
        self.static_to_string_regexp_object_bindings =
            facts.static_to_string_regexp_object_bindings;
        self.invalidated_static_binding_names = facts.invalidated_static_binding_names;
        self.boolean_alias_shapes_invalidated = facts.boolean_alias_shapes_invalidated;
    }

    pub(super) fn merge_conditional_flow_facts(
        &mut self,
        left: ConditionalFlowFacts,
        right: ConditionalFlowFacts,
    ) {
        let scopes = self.merge_scope_facts(&left.scopes, &right.scopes);
        let var_bindings = self.merge_var_bindings(&left.var_bindings, &right.var_bindings);
        let current_this_binding = match (&left.current_this_binding, &right.current_this_binding) {
            (CurrentThisBinding::Root(left), CurrentThisBinding::Root(right)) => {
                debug_assert_eq!(left, right);
                CurrentThisBinding::Root(*left)
            }
            // A branch that left the binding as it was shares it.
            (CurrentThisBinding::Activation(left), CurrentThisBinding::Activation(right))
                if std::sync::Arc::ptr_eq(left, right) =>
            {
                CurrentThisBinding::Activation(std::sync::Arc::clone(left))
            }
            (CurrentThisBinding::Activation(left), CurrentThisBinding::Activation(right)) => {
                CurrentThisBinding::activation(
                    self.merge_value_infos(ValueInfo::clone(left), ValueInfo::clone(right)),
                )
            }
            (CurrentThisBinding::Root(_), CurrentThisBinding::Activation(_))
            | (CurrentThisBinding::Activation(_), CurrentThisBinding::Root(_)) => {
                unreachable!("conditional branches cannot change this-binding ownership")
            }
        };
        let current_construct_this_info = match (
            &left.current_construct_this_info,
            &right.current_construct_this_info,
        ) {
            (Some(left), Some(right)) => Some(self.merge_value_infos(left.clone(), right.clone())),
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => None,
        };
        let global_properties =
            self.merge_global_properties(&left.global_properties, &right.global_properties);
        let mut well_known_symbol_prototype_properties = BTreeMap::new();
        for (key, left_info) in &left.well_known_symbol_prototype_properties {
            if let Some(right_info) = right.well_known_symbol_prototype_properties.get(key) {
                well_known_symbol_prototype_properties.insert(
                    key.clone(),
                    self.merge_value_infos(left_info.clone(), right_info.clone()),
                );
            }
        }
        let mut nested_script_global_value_infos = left.nested_script_global_value_infos.clone();
        for (name, right_info) in &right.nested_script_global_value_infos {
            let next = match nested_script_global_value_infos.remove(name) {
                Some(left_info) => self.merge_value_infos(left_info, right_info.clone()),
                None => right_info.clone(),
            };
            nested_script_global_value_infos.insert(name.clone(), next);
        }
        let static_boolean_bindings = equal_map_intersection(
            &left.static_boolean_bindings,
            &right.static_boolean_bindings,
        );
        let static_string_bindings = StaticStringBindingFacts::equal_intersection(
            &left.static_string_bindings,
            &right.static_string_bindings,
        );
        let static_to_string_regexp_object_bindings = left
            .static_to_string_regexp_object_bindings
            .intersection(&right.static_to_string_regexp_object_bindings)
            .cloned()
            .collect();
        let dynamically_installed_getters = left
            .dynamically_installed_getters
            .union(&right.dynamically_installed_getters)
            .cloned()
            .collect();
        let dynamically_installed_setters = left
            .dynamically_installed_setters
            .union(&right.dynamically_installed_setters)
            .cloned()
            .collect();

        self.install_conditional_flow_facts(ConditionalFlowFacts {
            scopes,
            var_bindings,
            current_this_binding,
            current_construct_this_info,
            global_properties,
            well_known_symbol_prototype_properties,
            nested_script_global_value_infos,
            array_prototype_mutated: left.array_prototype_mutated || right.array_prototype_mutated,
            number_prototype_to_string_state: left
                .number_prototype_to_string_state
                .join(right.number_prototype_to_string_state),
            boolean_prototype_to_string_state: left
                .boolean_prototype_to_string_state
                .join(right.boolean_prototype_to_string_state),
            dynamically_installed_getters,
            dynamically_installed_setters,
            unknown_user_code_effects_observed: left.unknown_user_code_effects_observed
                || right.unknown_user_code_effects_observed,
            function_signature_shape_evidence: left
                .function_signature_shape_evidence
                .join(right.function_signature_shape_evidence),
            static_boolean_bindings,
            static_string_bindings,
            static_to_string_regexp_object_bindings,
            invalidated_static_binding_names: left
                .invalidated_static_binding_names
                .union(&right.invalidated_static_binding_names)
                .cloned()
                .collect(),
            boolean_alias_shapes_invalidated: left.boolean_alias_shapes_invalidated
                || right.boolean_alias_shapes_invalidated,
        });
    }

    fn merge_scope_facts(
        &self,
        left: &[BTreeMap<String, BindingInfo>],
        right: &[BTreeMap<String, BindingInfo>],
    ) -> Vec<BTreeMap<String, BindingInfo>> {
        debug_assert_eq!(left.len(), right.len());
        left.iter()
            .enumerate()
            .map(|(index, left_scope)| {
                let Some(right_scope) = right.get(index) else {
                    return left_scope.clone();
                };
                debug_assert_eq!(left_scope.len(), right_scope.len());
                left_scope
                    .iter()
                    .map(|(name, left_binding)| {
                        let Some(right_binding) = right_scope.get(name) else {
                            return (name.clone(), left_binding.clone());
                        };
                        debug_assert_eq!(left_binding.mode, right_binding.mode);
                        debug_assert_eq!(left_binding.storage_name, right_binding.storage_name);
                        debug_assert_eq!(left_binding.initialization, right_binding.initialization);
                        let info = self.merge_value_infos(
                            ValueInfo {
                                kind: left_binding.kind,
                                possible_kinds: left_binding.possible_kinds,
                                heap_shape: left_binding.heap_shape.clone(),
                                function_targets: left_binding.function_targets.clone(),
                            },
                            ValueInfo {
                                kind: right_binding.kind,
                                possible_kinds: right_binding.possible_kinds,
                                heap_shape: right_binding.heap_shape.clone(),
                                function_targets: right_binding.function_targets.clone(),
                            },
                        );
                        (
                            name.clone(),
                            BindingInfo {
                                mode: left_binding.mode,
                                storage_name: left_binding.storage_name.clone(),
                                kind: info.kind,
                                possible_kinds: info.possible_kinds,
                                heap_shape: info.heap_shape,
                                function_targets: info.function_targets,
                                initialization: left_binding.initialization,
                            },
                        )
                    })
                    .collect()
            })
            .collect()
    }

    pub(super) fn lower_conditionally_reached_expression(
        &mut self,
        expression: &Expression,
    ) -> TypedExpr {
        let skipped = self.capture_conditional_flow_facts();
        let value = self.lower_expression(expression);
        let taken = self.capture_conditional_flow_facts();
        self.merge_conditional_flow_facts(skipped, taken);
        value
    }

    pub(super) fn merge_var_bindings(
        &self,
        left: &BTreeMap<String, VarBindingInfo>,
        right: &BTreeMap<String, VarBindingInfo>,
    ) -> BTreeMap<String, VarBindingInfo> {
        let mut merged = BTreeMap::new();
        for name in left.keys().chain(right.keys()) {
            if merged.contains_key(name) {
                continue;
            }
            let left_info = left.get(name).map(|binding| ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            });
            let right_info = right.get(name).map(|binding| ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            });
            let info = match (left_info, right_info) {
                (Some(lhs), Some(rhs)) => self.merge_value_infos(lhs, rhs),
                (Some(lhs), None) => lhs,
                (None, Some(rhs)) => rhs,
                (None, None) => continue,
            };
            merged.insert(
                name.clone(),
                VarBindingInfo {
                    kind: info.kind,
                    possible_kinds: info.possible_kinds,
                    heap_shape: info.heap_shape,
                    function_targets: info.function_targets,
                    is_script_global: left
                        .get(name)
                        .map(|binding| binding.is_script_global)
                        .or_else(|| right.get(name).map(|binding| binding.is_script_global))
                        .unwrap_or(false),
                    is_lexical_metadata: left
                        .get(name)
                        .map(|binding| binding.is_lexical_metadata)
                        .or_else(|| right.get(name).map(|binding| binding.is_lexical_metadata))
                        .unwrap_or(false),
                },
            );
        }
        merged
    }

    pub(super) fn merge_global_properties(
        &self,
        left: &GlobalProperties,
        right: &GlobalProperties,
    ) -> GlobalProperties {
        // An unchanged baseline fact joins with itself. Visit only deviations
        // when both branches share that immutable recipe catalog.
        let shared = left.same_baseline(right);
        let mut merged = if shared {
            left.empty_changes()
        } else {
            GlobalProperties::default()
        };
        let names = if shared {
            left.changed_keys()
                .chain(right.changed_keys())
                .cloned()
                .collect::<BTreeSet<_>>()
        } else {
            left.keys()
                .chain(right.keys())
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        for name in names {
            // Both branches kept the row they forked with; the join below
            // would rebuild that same row.
            if let (Some(left_row), Some(right_row)) =
                (left.shared_change(&name), right.shared_change(&name))
            {
                if std::sync::Arc::ptr_eq(left_row, right_row)
                    && Self::value_info_joins_to_itself(&left_row.value_info)
                {
                    merged.set_effective_shared(name, std::sync::Arc::clone(left_row));
                    continue;
                }
            }
            let (Some(left_info), Some(right_info)) = (left.get(&name), right.get(&name)) else {
                merged.set_effective(name, None);
                continue;
            };
            merged.set_effective(
                name,
                Some(GlobalPropertyInfo {
                    value_info: self.merge_value_infos(
                        left_info.value_info.clone(),
                        right_info.value_info.clone(),
                    ),
                    proven_present: left_info.proven_present && right_info.proven_present,
                    configurable: left_info.configurable && right_info.configurable,
                    source: if left_info.source == right_info.source {
                        left_info.source
                    } else {
                        GlobalPropertySource::Merged
                    },
                }),
            );
        }
        merged
    }

    pub(super) fn merge_value_kinds(&self, left: ValueKind, right: ValueKind) -> ValueKind {
        KindSet::from_kind(left)
            .union(KindSet::from_kind(right))
            .as_value_kind()
    }

    fn merge_heap_shapes(
        &self,
        kind: ValueKind,
        left: &Option<Arc<HeapShape>>,
        right: &Option<Arc<HeapShape>>,
    ) -> Option<Arc<HeapShape>> {
        match kind {
            ValueKind::Object | ValueKind::Array | ValueKind::Function if left == right => {
                left.clone()
            }
            ValueKind::Object | ValueKind::Array | ValueKind::Function => None,
            _ => None,
        }
    }

    /// Whether `merge_value_infos(info, info)` is `info`: its kind is the
    /// one its possible kinds name, and it keeps a heap shape only for a kind
    /// whose shapes survive a join.
    fn value_info_joins_to_itself(info: &ValueInfo) -> bool {
        match info.kind {
            ValueKind::Function => info.possible_kinds == KindSet::from_kind(ValueKind::Function),
            kind => {
                kind == info.possible_kinds.as_value_kind()
                    && (info.heap_shape.is_none()
                        || matches!(kind, ValueKind::Object | ValueKind::Array))
            }
        }
    }

    pub(super) fn merge_value_infos(&self, left: ValueInfo, right: ValueInfo) -> ValueInfo {
        let function_targets = left.function_targets.join(right.function_targets);
        if left.kind == ValueKind::Function && right.kind == ValueKind::Function {
            let heap_shape =
                self.merge_heap_shapes(ValueKind::Function, &left.heap_shape, &right.heap_shape);
            return ValueInfo {
                kind: ValueKind::Function,
                possible_kinds: KindSet::from_kind(ValueKind::Function),
                heap_shape,
                function_targets,
            };
        }
        let possible_kinds = left.possible_kinds.union(right.possible_kinds);
        if left.kind == right.kind {
            return ValueInfo {
                kind: possible_kinds.as_value_kind(),
                possible_kinds,
                heap_shape: self.merge_heap_shapes(left.kind, &left.heap_shape, &right.heap_shape),
                function_targets,
            };
        }
        ValueInfo {
            kind: possible_kinds.as_value_kind(),
            possible_kinds,
            heap_shape: None,
            function_targets,
        }
    }
}
