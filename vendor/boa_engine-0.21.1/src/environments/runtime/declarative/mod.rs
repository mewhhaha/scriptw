mod function;
mod global;
mod lexical;
mod module;

pub(crate) use function::{FunctionEnvironment, FunctionSlots, ThisBindingStatus};
pub(crate) use global::GlobalEnvironment;
pub(crate) use lexical::LexicalEnvironment;
pub(crate) use module::ModuleEnvironment;

use crate::{JsObject, JsResult, JsString, JsValue};
use boa_ast::scope::{BindingLocator, Scope};
use boa_gc::{Finalize, GcRefCell, Trace};
use std::cell::Cell;

#[derive(Debug, Trace, Finalize, Clone)]
enum BindingSlot {
    Uninitialized,
    Initialized(JsValue),
    Deleted,
}

#[derive(Debug, Trace, Finalize, Clone)]
pub(crate) struct DisposableResource {
    this_value: JsValue,
    argument: Option<JsValue>,
    method: Option<JsObject>,
    r#async: bool,
}

impl DisposableResource {
    pub(crate) fn new(
        this_value: JsValue,
        argument: Option<JsValue>,
        method: Option<JsObject>,
        r#async: bool,
    ) -> Self {
        Self {
            this_value,
            argument,
            method,
            r#async,
        }
    }

    pub(crate) fn from_value(value: JsValue, method: Option<JsObject>, r#async: bool) -> Self {
        Self::new(value, None, method, r#async)
    }

    pub(crate) fn from_callback(
        argument: Option<JsValue>,
        method: JsObject,
        r#async: bool,
    ) -> Self {
        Self::new(JsValue::undefined(), argument, Some(method), r#async)
    }

    pub(crate) fn this_value(&self) -> &JsValue {
        &self.this_value
    }

    pub(crate) fn argument(&self) -> Option<&JsValue> {
        self.argument.as_ref()
    }

    pub(crate) fn method(&self) -> Option<&JsObject> {
        self.method.as_ref()
    }

    pub(crate) const fn r#async(&self) -> bool {
        self.r#async
    }
}

/// A declarative environment holds binding values at runtime.
///
/// Bindings are stored in a fixed size list of runtime slots.
/// Slots track whether a binding is uninitialized, initialized, or deleted.
///
/// Optionally, an environment can hold a `this` value.
/// The `this` value is present only if the environment is a function environment.
///
/// Code evaluation at runtime (e.g. the `eval` built-in function) can add
/// bindings to existing, compiled function environments.
/// This makes it impossible to determine the location of all bindings at compile time.
/// To dynamically check for added bindings at runtime, a reference to the
/// corresponding compile time environment is needed.
///
/// Checking all environments for potential added bindings at runtime on every get/set
/// would offset the performance improvement of determining binding locations at compile time.
/// To minimize this, each environment holds a `poisoned` flag.
/// If bindings where added at runtime, the current environment and all inner environments
/// are marked as poisoned.
/// All poisoned environments have to be checked for added bindings.
#[derive(Debug, Trace, Finalize)]
pub(crate) struct DeclarativeEnvironment {
    kind: DeclarativeEnvironmentKind,
    disposable_resources: GcRefCell<Vec<DisposableResource>>,
}

impl DeclarativeEnvironment {
    /// Creates a new global `DeclarativeEnvironment`.
    pub(crate) fn global() -> Self {
        Self {
            kind: DeclarativeEnvironmentKind::Global(GlobalEnvironment::new()),
            disposable_resources: GcRefCell::new(Vec::new()),
        }
    }

    /// Creates a new `DeclarativeEnvironment` from its kind and compile environment.
    pub(crate) fn new(kind: DeclarativeEnvironmentKind) -> Self {
        Self {
            kind,
            disposable_resources: GcRefCell::new(Vec::new()),
        }
    }

    /// Returns a reference to the the kind of the environment.
    pub(crate) const fn kind(&self) -> &DeclarativeEnvironmentKind {
        &self.kind
    }

    /// Returns whether this environment is a function environment.
    pub(crate) fn is_function(&self) -> bool {
        matches!(self.kind(), DeclarativeEnvironmentKind::Function(_))
    }

    /// Gets the binding value from the environment by index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range or not initialized.
    #[track_caller]
    pub(crate) fn get(&self, index: u32) -> Option<JsValue> {
        self.kind.get(index)
    }

    /// Sets the binding value from the environment by index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range.
    #[track_caller]
    pub(crate) fn set(&self, index: u32, value: JsValue) {
        self.kind.set(index, value);
    }

    /// Returns whether the given binding index exists in this environment.
    #[track_caller]
    pub(crate) fn has_binding_index(&self, index: u32) -> bool {
        self.kind.has_binding_index(index)
    }

    /// Returns whether the binding is still uninitialized.
    #[track_caller]
    pub(crate) fn is_uninitialized(&self, index: u32) -> bool {
        self.kind.is_uninitialized(index)
    }

    /// Marks a mutable binding as deletable.
    #[track_caller]
    pub(crate) fn set_mutable_binding_deletable(&self, index: u32) {
        self.kind.set_mutable_binding_deletable(index);
    }

    /// Deletes a binding when its environment record allows it.
    #[track_caller]
    pub(crate) fn delete(&self, index: u32) -> bool {
        self.kind.delete(index)
    }

    /// `GetThisBinding`
    ///
    /// Returns the `this` binding of this environment.
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-function-environment-records-getthisbinding
    pub(crate) fn get_this_binding(&self) -> JsResult<Option<JsValue>> {
        self.kind.get_this_binding()
    }

    /// `HasThisBinding`
    ///
    /// Returns if the environment has a `this` binding.
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-function-environment-records-hasthisbinding
    pub(crate) fn has_this_binding(&self) -> bool {
        self.kind.has_this_binding()
    }

    /// Returns `true` if this environment is poisoned.
    pub(crate) fn poisoned(&self) -> bool {
        self.kind.poisoned()
    }

    /// Returns `true` if this environment is inside a `with` environment.
    pub(crate) fn with(&self) -> bool {
        self.kind.with()
    }

    /// Poisons this environment for future binding searchs.
    pub(crate) fn poison(&self) {
        self.kind.poison();
    }

    /// Returns the runtime scope of this environment when it can be the
    /// VariableEnvironment of running function code: a function environment,
    /// or the separate record of a function body
    /// (`FunctionDeclarationInstantiation` step 28).
    ///
    /// A sloppy direct eval adds its `var` and function names to this scope
    /// (`EvalDeclarationInstantiation`).
    pub(crate) fn variable_scope(&self) -> Option<&Scope> {
        match self.kind() {
            DeclarativeEnvironmentKind::Function(env) => Some(env.compile()),
            DeclarativeEnvironmentKind::Lexical(env) => env.variable_scope(),
            DeclarativeEnvironmentKind::Global(_) | DeclarativeEnvironmentKind::Module(_) => None,
        }
    }

    /// Looks up by name a binding of this environment that the compiled code
    /// may not know, because a direct eval can have added it to the
    /// environment's variable scope.
    pub(crate) fn runtime_binding(&self, name: &JsString) -> Option<BindingLocator> {
        match self.kind() {
            DeclarativeEnvironmentKind::Function(env) => env.compile().get_binding(name),
            DeclarativeEnvironmentKind::Lexical(env) => env.eval_declared_binding(name),
            DeclarativeEnvironmentKind::Global(_) | DeclarativeEnvironmentKind::Module(_) => None,
        }
    }

    /// Extends the environment with the bindings from the compile time environment.
    pub(crate) fn extend_from_compile(&self) {
        let (environment, scope) = match self.kind() {
            DeclarativeEnvironmentKind::Function(env) => {
                (env.poisonable_environment(), env.compile())
            }
            DeclarativeEnvironmentKind::Lexical(env) => {
                let Some(scope) = env.variable_scope() else {
                    return;
                };
                (env.poisonable_environment(), scope)
            }
            DeclarativeEnvironmentKind::Global(_) | DeclarativeEnvironmentKind::Module(_) => return,
        };
        environment.extend_bindings_from_compile(scope.num_bindings() as usize);
    }

    pub(crate) fn push_disposable_resource(&self, resource: DisposableResource) {
        self.disposable_resources.borrow_mut().push(resource);
    }

    pub(crate) fn take_disposable_resources(&self) -> Vec<DisposableResource> {
        std::mem::take(&mut *self.disposable_resources.borrow_mut())
    }
}

/// The kind of the declarative environment.
#[derive(Debug, Trace, Finalize)]
pub(crate) enum DeclarativeEnvironmentKind {
    /// Only stores lexical bindings.
    Lexical(LexicalEnvironment),
    /// Stores lexical bindings, global var bindings and the global this.
    Global(GlobalEnvironment),
    /// Stores lexical bindings, var bindings and the `FunctionSlots` of the function environment.
    Function(FunctionEnvironment),
    /// Stores module bindings, which include references to bindings on other environments.
    Module(ModuleEnvironment),
}

impl DeclarativeEnvironmentKind {
    /// Unwraps the inner function environment if possible. Returns `None` otherwise.
    pub(crate) const fn as_function(&self) -> Option<&FunctionEnvironment> {
        if let Self::Function(fun) = &self {
            Some(fun)
        } else {
            None
        }
    }

    /// Unwraps the inner global environment if possible. Returns `None` otherwise.
    pub(crate) const fn as_global(&self) -> Option<&GlobalEnvironment> {
        if let Self::Global(fun) = &self {
            Some(fun)
        } else {
            None
        }
    }

    /// Unwraps the inner module environment if possible. Returns `None` otherwise.
    pub(crate) const fn as_module(&self) -> Option<&ModuleEnvironment> {
        if let Self::Module(module) = &self {
            Some(module)
        } else {
            None
        }
    }

    /// Get the binding value from the environment by it's index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range or not initialized.
    #[track_caller]
    pub(crate) fn get(&self, index: u32) -> Option<JsValue> {
        match self {
            Self::Lexical(inner) => inner.get(index),
            Self::Global(inner) => inner.get(index),
            Self::Function(inner) => inner.get(index),
            Self::Module(inner) => inner.get(index),
        }
    }

    /// Sets the binding value from the environment by index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range.
    #[track_caller]
    pub(crate) fn set(&self, index: u32, value: JsValue) {
        match self {
            Self::Lexical(inner) => inner.set(index, value),
            Self::Global(inner) => inner.set(index, value),
            Self::Function(inner) => inner.set(index, value),
            Self::Module(inner) => inner.set(index, value),
        }
    }

    /// Returns whether the given binding index exists in this environment.
    #[track_caller]
    pub(crate) fn has_binding_index(&self, index: u32) -> bool {
        match self {
            Self::Lexical(inner) => inner.has_binding_index(index),
            Self::Global(inner) => inner.has_binding_index(index),
            Self::Function(inner) => inner.has_binding_index(index),
            Self::Module(inner) => inner.has_binding_index(index),
        }
    }

    /// Returns whether the binding is still uninitialized.
    #[track_caller]
    pub(crate) fn is_uninitialized(&self, index: u32) -> bool {
        match self {
            Self::Lexical(inner) => inner.poisonable_environment().is_uninitialized(index),
            Self::Global(inner) => inner.poisonable_environment().is_uninitialized(index),
            Self::Function(inner) => inner.poisonable_environment().is_uninitialized(index),
            Self::Module(inner) => inner.get(index).is_none(),
        }
    }

    /// Marks a mutable binding as deletable.
    #[track_caller]
    pub(crate) fn set_mutable_binding_deletable(&self, index: u32) {
        match self {
            Self::Lexical(inner) => inner
                .poisonable_environment()
                .set_mutable_binding_deletable(index),
            Self::Global(inner) => inner
                .poisonable_environment()
                .set_mutable_binding_deletable(index),
            Self::Function(inner) => inner
                .poisonable_environment()
                .set_mutable_binding_deletable(index),
            Self::Module(_) => {}
        }
    }

    /// Deletes a binding when its environment record allows it.
    #[track_caller]
    pub(crate) fn delete(&self, index: u32) -> bool {
        match self {
            Self::Lexical(inner) => inner.poisonable_environment().delete(index),
            Self::Global(inner) => inner.poisonable_environment().delete(index),
            Self::Function(inner) => inner.poisonable_environment().delete(index),
            Self::Module(_) => false,
        }
    }

    /// `GetThisBinding`
    ///
    /// Returns the `this` binding of this environment.
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-function-environment-records-getthisbinding
    pub(crate) fn get_this_binding(&self) -> JsResult<Option<JsValue>> {
        match self {
            Self::Lexical(_) | Self::Global(_) => Ok(None),
            Self::Function(f) => f.get_this_binding(),
            Self::Module(_) => Ok(Some(JsValue::undefined())),
        }
    }

    /// `HasThisBinding`
    ///
    /// Returns if the environment has a `this` binding.
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-function-environment-records-hasthisbinding
    pub(crate) fn has_this_binding(&self) -> bool {
        match self {
            Self::Lexical(_) => false,
            Self::Function(f) => f.has_this_binding(),
            Self::Global(_) | Self::Module(_) => true,
        }
    }

    /// Returns `true` if this environment is poisoned.
    pub(crate) fn poisoned(&self) -> bool {
        match self {
            Self::Lexical(lex) => lex.poisonable_environment().poisoned(),
            Self::Global(g) => g.poisonable_environment().poisoned(),
            Self::Function(f) => f.poisonable_environment().poisoned(),
            Self::Module(_) => false,
        }
    }

    /// Returns `true` if this environment is inside a `with` environment.
    pub(crate) fn with(&self) -> bool {
        match self {
            Self::Lexical(lex) => lex.poisonable_environment().with(),
            Self::Global(g) => g.poisonable_environment().with(),
            Self::Function(f) => f.poisonable_environment().with(),
            Self::Module(_) => false,
        }
    }

    /// Poisons this environment for future binding searches.
    pub(crate) fn poison(&self) {
        match self {
            Self::Lexical(lex) => lex.poisonable_environment().poison(),
            Self::Global(g) => g.poisonable_environment().poison(),
            Self::Function(f) => f.poisonable_environment().poison(),
            Self::Module(_) => {
                unreachable!("modules are always run in strict mode")
            }
        }
    }
}

#[derive(Debug, Trace, Finalize)]
pub(crate) struct PoisonableEnvironment {
    bindings: GcRefCell<Vec<BindingSlot>>,
    deletable_bindings: GcRefCell<Vec<bool>>,
    #[unsafe_ignore_trace]
    poisoned: Cell<bool>,
    with: bool,
}

impl PoisonableEnvironment {
    /// Creates a new `PoisonableEnvironment`.
    pub(crate) fn new(bindings_count: u32, poisoned: bool, with: bool) -> Self {
        Self {
            bindings: GcRefCell::new(vec![BindingSlot::Uninitialized; bindings_count as usize]),
            deletable_bindings: GcRefCell::new(vec![false; bindings_count as usize]),
            poisoned: Cell::new(poisoned),
            with,
        }
    }

    /// Gets the binding value from the environment by it's index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range.
    #[track_caller]
    fn get(&self, index: u32) -> Option<JsValue> {
        match &self.bindings.borrow()[index as usize] {
            BindingSlot::Initialized(value) => Some(value.clone()),
            BindingSlot::Uninitialized | BindingSlot::Deleted => None,
        }
    }

    /// Sets the binding value from the environment by index.
    ///
    /// # Panics
    ///
    /// Panics if the binding value is out of range.
    #[track_caller]
    pub(crate) fn set(&self, index: u32, value: JsValue) {
        self.bindings.borrow_mut()[index as usize] = BindingSlot::Initialized(value);
    }

    /// Returns whether the given binding index exists.
    #[track_caller]
    pub(crate) fn has_binding_index(&self, index: u32) -> bool {
        (index as usize) < self.bindings.borrow().len()
    }

    /// Returns whether the binding is still uninitialized.
    #[track_caller]
    pub(crate) fn is_uninitialized(&self, index: u32) -> bool {
        matches!(
            self.bindings.borrow()[index as usize],
            BindingSlot::Uninitialized
        )
    }

    /// Marks a mutable binding as deletable.
    #[track_caller]
    pub(crate) fn set_mutable_binding_deletable(&self, index: u32) {
        self.deletable_bindings.borrow_mut()[index as usize] = true;
    }

    /// Deletes a binding when its environment record allows it.
    #[track_caller]
    pub(crate) fn delete(&self, index: u32) -> bool {
        if !self.deletable_bindings.borrow()[index as usize] {
            return false;
        }
        self.bindings.borrow_mut()[index as usize] = BindingSlot::Deleted;
        true
    }

    /// Extends the environment to match compile-time binding counts.
    pub(crate) fn extend_bindings_from_compile(&self, bindings_count: usize) {
        let mut bindings = self.bindings.borrow_mut();
        if bindings_count > bindings.len() {
            bindings.resize(bindings_count, BindingSlot::Uninitialized);
        }
        drop(bindings);

        let mut deletable_bindings = self.deletable_bindings.borrow_mut();
        if bindings_count > deletable_bindings.len() {
            deletable_bindings.resize(bindings_count, false);
        }
    }

    /// Returns the current binding count.
    pub(crate) fn len(&self) -> usize {
        self.bindings.borrow().len()
    }

    /// Returns `true` if this environment is poisoned.
    fn poisoned(&self) -> bool {
        self.poisoned.get()
    }

    /// Returns `true` if this environment is inside a `with` environment.
    fn with(&self) -> bool {
        self.with
    }

    /// Poisons this environment for future binding searches.
    fn poison(&self) {
        self.poisoned.set(true);
    }
}
