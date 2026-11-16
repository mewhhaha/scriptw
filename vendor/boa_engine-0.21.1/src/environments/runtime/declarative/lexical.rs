use boa_ast::scope::{BindingLocator, Scope};
use boa_gc::{Finalize, Trace};

use crate::{JsString, JsValue};

use super::PoisonableEnvironment;

#[derive(Debug, Trace, Finalize)]
pub(crate) struct LexicalEnvironment {
    inner: PoisonableEnvironment,

    /// The runtime copy of the scope when this record is the variable
    /// environment of a sloppy function body whose parameter list contains an
    /// expression (`FunctionDeclarationInstantiation` step 28). A sloppy direct
    /// eval in that body declares its `var` and function names here, as it
    /// does in a function environment.
    // Safety: Nothing in `Scope` needs tracing.
    #[unsafe_ignore_trace]
    variable_scope: Option<Scope>,

    /// The number of bindings the compiled function gave `variable_scope`.
    /// A direct eval adds its bindings after them.
    compiled_bindings: u32,
}

impl LexicalEnvironment {
    /// Creates a new `LexicalEnvironment`.
    pub(crate) fn new(bindings: u32, poisoned: bool, with: bool) -> Self {
        Self {
            inner: PoisonableEnvironment::new(bindings, poisoned, with),
            variable_scope: None,
            compiled_bindings: 0,
        }
    }

    /// Creates the variable environment of a function body from the runtime
    /// copy of its scope.
    pub(crate) fn new_variable(poisoned: bool, with: bool, scope: Scope) -> Self {
        Self {
            inner: PoisonableEnvironment::new(scope.num_bindings_non_local(), poisoned, with),
            compiled_bindings: scope.num_bindings(),
            variable_scope: Some(scope),
        }
    }

    /// Gets the binding `name` that a direct eval added to this variable
    /// environment.
    ///
    /// The bindings of the compiled scope are not found by name. Code inside
    /// the record resolved them when it was compiled, and code outside it
    /// means the binding it names: the read of a parameter that initializes
    /// the body `var` of its name must not find that `var`.
    pub(crate) fn eval_declared_binding(&self, name: &JsString) -> Option<BindingLocator> {
        self.variable_scope
            .as_ref()?
            .get_binding(name)
            .filter(|binding| binding.binding_index() >= self.compiled_bindings)
    }

    /// Gets the runtime scope of this record when it is the variable
    /// environment of a function body.
    pub(crate) const fn variable_scope(&self) -> Option<&Scope> {
        self.variable_scope.as_ref()
    }

    /// Gets the `poisonable_environment` of this lexical environment.
    pub(crate) const fn poisonable_environment(&self) -> &PoisonableEnvironment {
        &self.inner
    }

    /// Gets the binding value from the environment by it's index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range or not initialized.
    #[track_caller]
    pub(crate) fn get(&self, index: u32) -> Option<JsValue> {
        self.inner.get(index)
    }

    /// Sets the binding value from the environment by index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range.
    #[track_caller]
    pub(crate) fn set(&self, index: u32, value: JsValue) {
        self.inner.set(index, value);
    }

    /// Returns whether the given binding index exists.
    #[track_caller]
    pub(crate) fn has_binding_index(&self, index: u32) -> bool {
        self.inner.has_binding_index(index)
    }
}
