//! Static resolution of a source `arguments` reference.
//!
//! Without direct eval, `with` or an Annex B `function arguments` (which keep
//! the whole Script eval-visible), ResolveBinding of `arguments` is decided by
//! the lexical chain alone. FunctionDeclarationInstantiation creates the
//! implicit binding in the nearest non-arrow function's own record, so a
//! reference names that one owner's arguments object, a source declaration
//! between the reference and that record, or, with no enclosing function, the
//! global binding. Nothing about it widens any other owner's environments.
use super::*;

/// The binding a source `arguments` reference resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArgumentsReference {
    /// The implicit arguments binding of the nearest ordinary function: the
    /// reference reads its `LEXICAL_ARGUMENTS_NAME` storage.
    OwnerImplicit,
    /// A parameter, var, function, lexical or catch binding spelled
    /// `arguments` that precedes any implicit binding on the chain.
    Source,
    /// No enclosing function: an ordinary global identifier reference.
    Global,
}

impl AnalysisBuilder<'_> {
    /// Walks the current scan position's environment chain. `scoped_storage`
    /// is the storage a block-scoped `arguments` declaration was renamed to,
    /// when one is visible here.
    pub(super) fn resolve_arguments_reference(
        &self,
        scoped_storage: Option<&String>,
    ) -> ArgumentsReference {
        let mut cursor = Some(self.current_environment_cursor());
        while let Some(current) = cursor {
            let environment = &self.environment_plans[&current.environment_id];
            if environment.binding_storage_names.contains("arguments")
                || scoped_storage
                    .is_some_and(|storage| environment.binding_storage_names.contains(storage))
            {
                return ArgumentsReference::Source;
            }
            if self.holds_implicit_arguments(environment) {
                return ArgumentsReference::OwnerImplicit;
            }
            cursor = environment.parent_cursor.clone();
        }
        ArgumentsReference::Global
    }

    /// Only an ordinary (non-arrow) function's parameter-scope record holds
    /// the implicit binding; arrows, class bodies and the Script have none.
    fn holds_implicit_arguments(&self, environment: &EnvironmentPlan) -> bool {
        matches!(
            environment.kind,
            EnvironmentKind::Activation | EnvironmentKind::FunctionParameters
        ) && environment.owner_id != SCRIPT_OWNER_ID
            && environment
                .binding_storage_names
                .contains(LEXICAL_ARGUMENTS_NAME)
            && self
                .owner_plans
                .get(&environment.owner_id)
                .is_some_and(|owner| {
                    owner.flavor == FunctionFlavor::Ordinary
                        && owner.activation_environment_id == environment.id
                })
    }
}
