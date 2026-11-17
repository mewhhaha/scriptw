//! Optional source discovery follows analyzed cells, never bare local names.

use super::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FiniteSourceBindingKey {
    environment_id: EnvironmentId,
    storage_name: String,
}

impl FiniteSourceBindingKey {
    fn new(environment_id: EnvironmentId, storage_name: String) -> Self {
        Self {
            environment_id,
            storage_name,
        }
    }
}

impl ScriptLowerer<'_> {
    pub(super) fn finite_source_binding_key(&self, name: &str) -> Option<FiniteSourceBindingKey> {
        let Some((binding, location)) = self.lookup_binding_with_location(name) else {
            return self.is_script_global_var_name(name).then(|| {
                FiniteSourceBindingKey::new(
                    self.analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id,
                    name.to_string(),
                )
            });
        };
        if let BindingLookupLocation::Scope(index) = location {
            if let Some(key) = self
                .finite_source_capture_keys
                .get(&(index, name.to_string()))
            {
                return (key.storage_name == binding.storage_name).then(|| key.clone());
            }
        }
        let owner = &self.analysis.owner_plans[&self.current_owner_id];
        let in_body = self
            .finite_source_body_scope
            .is_some_and(|body_index| match location {
                BindingLookupLocation::Scope(index) => index >= body_index,
                BindingLookupLocation::VariableEnvironment => true,
            });
        // Parameter and body records can both use the same physical spelling.
        // The entered body scope decides which analyzed cell a local read names.
        let body_id = owner.body_environment_id.filter(|id| {
            in_body
                && self.analysis.environment_plans[id]
                    .binding_storage_names
                    .contains(&binding.storage_name)
        });
        let environment_id = if let Some(id) = body_id {
            id
        } else if self.analysis.environment_plans[&owner.activation_environment_id]
            .binding_storage_names
            .contains(&binding.storage_name)
        {
            owner.activation_environment_id
        } else {
            let mut physical = self
                .analysis
                .physical_binding_environments
                .get(&binding.storage_name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|id| {
                    let environment = &self.analysis.environment_plans[id];
                    environment.owner_id == self.current_owner_id
                        && environment
                            .binding_storage_names
                            .contains(&binding.storage_name)
                });
            let first = physical.next();
            if physical.next().is_some() {
                return None;
            }
            // Uncaptured compiler-private temporaries have no source cell.
            // Their unique storage keys stay within the current owner record.
            first.unwrap_or_else(|| {
                if in_body {
                    owner
                        .body_environment_id
                        .unwrap_or(owner.activation_environment_id)
                } else {
                    owner.activation_environment_id
                }
            })
        };
        Some(FiniteSourceBindingKey::new(
            environment_id,
            binding.storage_name,
        ))
    }

    pub(super) fn install_finite_source_capture_key(
        &mut self,
        name: &str,
        environment_id: EnvironmentId,
        storage_name: &str,
    ) {
        let scope_index = self.scopes.len() - 1;
        let binding = self.scopes[scope_index]
            .get(name)
            .expect("a capture is declared before installing its source identity");
        assert_eq!(binding.storage_name, storage_name);
        self.finite_source_capture_keys.insert(
            (scope_index, name.to_string()),
            FiniteSourceBindingKey::new(environment_id, storage_name.to_string()),
        );
    }

    pub(super) fn set_finite_source_binding_candidates(
        &mut self,
        name: &str,
        candidates: Vec<FiniteSourceValue>,
    ) {
        if let Some(key) = self.finite_source_binding_key(name) {
            self.function_source_binding_candidates
                .insert(key, SourceCandidates::from(candidates));
        }
    }

    pub(super) fn merge_function_source_binding_candidates(
        &mut self,
        observations: BTreeMap<FiniteSourceBindingKey, SourceCandidates>,
    ) {
        for (key, candidates) in observations {
            let environment = &self.analysis.environment_plans[&key.environment_id];
            // Uncaptured local hints already served their own body. Retaining
            // those across every sibling adds clone cost without a reader.
            // Script vars can instead be read through global properties.
            if environment.owner_id != SCRIPT_OWNER_ID
                && !environment.owned_env_slots.contains_key(&key.storage_name)
            {
                continue;
            }
            let previous = self.function_source_binding_candidates.remove(&key);
            self.function_source_binding_candidates
                .insert(key, SourceCandidates::merged(previous, candidates));
        }
    }
}
