//! Function bodies whose builtin facts are proofs.
//!
//! A function body is lowered once, but runs at an unknown later time: any
//! statement ahead of a call may have replaced a builtin global or edited an
//! intrinsic through a write no lowering site can name (a computed key, an
//! alias, `Reflect.set`, `Object.defineProperty`, an indirect eval). The
//! fresh-realm facts a body starts from are therefore assumptions, and a
//! result kind derived from one is wrong after the replacement.
//!
//! Each ordinary body is lowered twice here: from the builtin facts of the
//! state it is lowered in, and from none. When the two lowerings agree, no
//! fact became a proof and the one body is emitted as it always was.
//! Otherwise both are emitted as a [`BuiltinEntryRoleIr`] pair: the first
//! keeps the function's identity and checks at entry the builtin globals its
//! lowering consulted, and tail-calls the second on a miss. Within the
//! guarded body the ordinary flow facts then hold as they do in top-level
//! code, whose entry state is known.
//!
//! A caller reaches either body through the guarded one's identity, so the
//! pair publishes the joined return facts, and the proof-free lowering runs
//! last so that its summary is the one later lowerings read.
use super::*;

impl<'a> ScriptLowerer<'a> {
    /// Lowers `function` for emission and appends its body, or its guarded
    /// body and that body's fallback, to `functions`.
    pub(super) fn lower_function_bodies(
        &mut self,
        function: &FunctionPlan<'a>,
        output_id: Option<FunctionId>,
        context_key_override: Option<ExactCallbackContextKey>,
        functions: &mut Vec<FunctionIr>,
    ) {
        debug_assert!(self.dormant_builtin_facts == DormantBuiltinFacts::Guarded);
        // A resumable body's builtin facts would have to hold again after
        // every suspension: generators and async bodies stay proof-free. A
        // module activation is entered once, from the state it is lowered
        // in, and `lower_function` keeps that state's facts in either mode.
        let module_activation = matches!(
            function.protocol,
            FunctionProtocolIr::ModuleActivation | FunctionProtocolIr::AsyncModuleActivation
        );
        if module_activation
            || function.protocol.execution_kind() != FunctionExecutionKind::Ordinary
        {
            let body = self.lower_function_from(
                DormantBuiltinFacts::ProofFree,
                false,
                function,
                output_id,
                context_key_override,
            );
            functions.push(body);
            return;
        }
        let id = output_id.clone().unwrap_or_else(|| function.id.clone());
        let entry_signature = self.function_signatures.get(&function.id).cloned();
        let mut guarded = self.lower_function_from(
            DormantBuiltinFacts::Guarded,
            false,
            function,
            output_id.clone(),
            context_key_override.clone(),
        );
        let consulted = std::mem::take(&mut self.last_consulted_builtin_globals);
        // The fallback must not read the summary the guarded body just
        // published of itself: a recursive call may reach either body.
        if let Some(signature) = entry_signature.clone() {
            self.function_signatures
                .insert(function.id.clone(), signature);
        }
        let fallback_id = format!("{id}$builtin-fallback");
        // The fallback stands in for the guarded body's activation, so its
        // cell witnesses name the guarded identity
        // (`FunctionIr::binding_cell_proof_owner`).
        self.binding_cell_proof_owner_override = Some(id.clone());
        let mut fallback = self.lower_function_from(
            DormantBuiltinFacts::ProofFree,
            true,
            function,
            Some(fallback_id.clone()),
            context_key_override.clone(),
        );
        // The proof-free lowering ran last, so the published signature is the
        // one that holds for both bodies.
        fallback.id.clone_from(&id);
        let agree = guarded == fallback;
        fallback.id = fallback_id;
        if agree {
            functions.push(guarded);
            return;
        }
        let facts = self.builtin_entry_facts(&consulted);
        let pair = facts.filter(|facts| {
            !facts.is_empty()
                && guarded.protocol == fallback.protocol
                && guarded.params.len() == fallback.params.len()
                && guarded.captured_bindings == fallback.captured_bindings
        });
        let Some(facts) = pair else {
            // A fact no entry can prove was consulted. The proof-free body
            // is the function.
            if let Some(signature) = entry_signature {
                self.function_signatures
                    .insert(function.id.clone(), signature);
            }
            let body = self.lower_function_from(
                DormantBuiltinFacts::ProofFree,
                true,
                function,
                output_id,
                context_key_override,
            );
            functions.push(body);
            return;
        };
        // A caller reaches either body through the guarded one's identity.
        let returned = self.merge_value_infos(
            Self::emitted_return_info(&guarded),
            Self::emitted_return_info(&fallback),
        );
        guarded.return_kind = returned.kind;
        guarded.return_shape = returned.heap_shape;
        guarded.return_targets = returned.function_targets;
        guarded.constructor_instance = self.merge_value_infos(
            guarded.constructor_instance.clone(),
            fallback.constructor_instance.clone(),
        );
        let guarded_alone = std::sync::Arc::new(guarded.clone());
        guarded.builtin_entry = Some(BuiltinEntryRoleIr::Guarded {
            facts,
            fallback: std::sync::Arc::new(fallback.clone()),
        });
        fallback.builtin_entry = Some(BuiltinEntryRoleIr::Fallback {
            guarded: guarded_alone,
        });
        functions.push(guarded);
        functions.push(fallback);
    }

    /// One emitted lowering of `function` from `facts`. A source function is
    /// lowered more than once, so the records a lowering leaves behind are
    /// kept once: the functions it generates under an identity an earlier
    /// lowering also generated replace the earlier ones (the later lowering
    /// never assumes more), a repeated diagnostic is dropped, and the usage
    /// counters keep the first lowering's counts.
    fn lower_function_from(
        &mut self,
        facts: DormantBuiltinFacts,
        repeated: bool,
        function: &FunctionPlan<'a>,
        output_id: Option<FunctionId>,
        context_key_override: Option<ExactCallbackContextKey>,
    ) -> FunctionIr {
        let generated_before = self.generated_functions.len();
        let diagnostics_before = self.diagnostics.len();
        let counters = (self.host_builtin_calls, self.top_level_this_uses);
        let previous = std::mem::replace(&mut self.dormant_builtin_facts, facts);
        let body = self.lower_function(function, output_id, context_key_override);
        self.dormant_builtin_facts = previous;
        let later: BTreeSet<FunctionId> = self.generated_functions[generated_before..]
            .iter()
            .map(|generated| generated.id.clone())
            .collect();
        if !later.is_empty() {
            let mut index = 0;
            self.generated_functions.retain(|generated| {
                let earlier = index < generated_before;
                index += 1;
                !(earlier && later.contains(&generated.id))
            });
        }
        if repeated {
            (self.host_builtin_calls, self.top_level_this_uses) = counters;
            let mut index = 0;
            let earlier = self.diagnostics[..diagnostics_before].to_vec();
            self.diagnostics.retain(|diagnostic| {
                let repeated = index >= diagnostics_before && earlier.contains(diagnostic);
                index += 1;
                !repeated
            });
        }
        body
    }

    fn emitted_return_info(function: &FunctionIr) -> ValueInfo {
        ValueInfo {
            kind: function.return_kind,
            possible_kinds: if function.return_kind == ValueKind::Dynamic {
                KindSet::all_runtime_tags()
            } else {
                KindSet::from_kind(function.return_kind)
            },
            heap_shape: function.return_shape.clone(),
            function_targets: function.return_targets.clone(),
        }
    }

    /// The entry facts for the builtin globals a guarded lowering consulted,
    /// or `None` when one of them has no fact an entry can prove.
    fn builtin_entry_facts(&self, consulted: &BTreeSet<String>) -> Option<Vec<BuiltinEntryFactIr>> {
        let mut facts = BTreeSet::new();
        for name in consulted {
            // Neither writable nor configurable: always true.
            if !Self::global_property_is_writable_name(name) {
                continue;
            }
            let row = builtin_catalog::builtin_global(self.host_surface_policy, name)?;
            if name == GLOBAL_THIS_NAME {
                facts.insert(BuiltinEntryFactIr::GlobalThis);
                continue;
            }
            if let Some(namespace) = BuiltinNamespaceIr::ALL
                .into_iter()
                .find(|namespace| namespace.global_name() == name)
            {
                facts.insert(BuiltinEntryFactIr::Namespace(namespace));
                continue;
            }
            if row.value_info.kind != ValueKind::Function {
                return None;
            }
            let function = row.value_info.function_targets.exact_single_target()?;
            facts.insert(BuiltinEntryFactIr::Function {
                name: name.clone(),
                function: function.clone(),
            });
        }
        Some(facts.into_iter().collect())
    }
}
