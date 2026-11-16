use crate::{
    Context, JsError, JsNativeError, JsResult, JsString, JsSymbol, JsValue,
    builtins::{
        Promise,
        promise::{PromiseCapability, PromiseState},
    },
    native_function::NativeFunction,
    object::builtins::JsPromise,
    object::{FunctionObjectBuilder, JsObject, PrivateName},
};
use boa_ast::scope::{BindingLocator, BindingLocatorError, BindingLocatorScope, Scope};
use boa_gc::{Finalize, Gc, Trace};

mod declarative;
mod private;

pub(crate) use self::declarative::DisposableResource;
use self::declarative::ModuleEnvironment;
pub(crate) use self::{
    declarative::{
        DeclarativeEnvironment, DeclarativeEnvironmentKind, FunctionEnvironment, FunctionSlots,
        LexicalEnvironment, ThisBindingStatus,
    },
    private::PrivateEnvironment,
};

/// The environment stack holds all environments at runtime.
///
/// Environments themselves are garbage collected,
/// because they must be preserved for function calls.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct EnvironmentStack {
    stack: Vec<Environment>,
    global: Gc<DeclarativeEnvironment>,
    private_stack: Vec<Gc<PrivateEnvironment>>,
}

/// A runtime environment.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) enum Environment {
    Declarative(Gc<DeclarativeEnvironment>),
    Object(JsObject),
}

impl Environment {
    /// Returns the declarative environment if it is one.
    pub(crate) const fn as_declarative(&self) -> Option<&Gc<DeclarativeEnvironment>> {
        match self {
            Self::Declarative(env) => Some(env),
            Self::Object(_) => None,
        }
    }
}

impl EnvironmentStack {
    /// Create a new environment stack.
    pub(crate) fn new(global: Gc<DeclarativeEnvironment>) -> Self {
        assert!(matches!(
            global.kind(),
            DeclarativeEnvironmentKind::Global(_)
        ));
        Self {
            stack: Vec::new(),
            global,
            private_stack: Vec::new(),
        }
    }

    /// Replaces the current global with a new global environment.
    pub(crate) fn replace_global(&mut self, global: Gc<DeclarativeEnvironment>) {
        assert!(matches!(
            global.kind(),
            DeclarativeEnvironmentKind::Global(_)
        ));
        self.global = global;
    }

    /// Gets the current global environment.
    pub(crate) fn global(&self) -> &Gc<DeclarativeEnvironment> {
        &self.global
    }

    /// Gets the VariableEnvironment of the running function code with its
    /// runtime scope: the innermost function environment, or the separate
    /// record of a function body once it is pushed
    /// (`FunctionDeclarationInstantiation` step 28). The parameter list of
    /// such a function still runs with the function environment.
    pub(crate) fn variable_environment(&self) -> Option<(Gc<DeclarativeEnvironment>, Scope)> {
        self.stack
            .iter()
            .filter_map(Environment::as_declarative)
            .rev()
            .find_map(|env| {
                env.variable_scope()
                    .map(|scope| (env.clone(), scope.clone()))
            })
    }

    /// Pop all current environments except the global environment.
    pub(crate) fn pop_to_global(&mut self) -> Vec<Environment> {
        let mut envs = Vec::new();
        std::mem::swap(&mut envs, &mut self.stack);
        envs
    }

    /// Get the number of current environments.
    pub(crate) fn len(&self) -> usize {
        self.stack.len()
    }

    /// Get the top-most runtime environment.
    pub(crate) fn top(&self) -> Option<&Environment> {
        self.stack.last()
    }

    /// Truncate current environments to the given number.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.stack.truncate(len);
    }

    /// Extend the current environment stack with the given environments.
    pub(crate) fn extend(&mut self, other: Vec<Environment>) {
        self.stack.extend(other);
    }

    /// `GetThisEnvironment`
    ///
    /// Returns the environment that currently provides a `this` biding.
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-getthisenvironment
    pub(crate) fn get_this_environment(&self) -> &DeclarativeEnvironmentKind {
        for env in self.stack.iter().rev() {
            if let Some(decl) = env.as_declarative().filter(|decl| decl.has_this_binding()) {
                return decl.kind();
            }
        }

        self.global().kind()
    }

    /// `GetThisBinding`
    ///
    /// Returns the current `this` binding of the environment.
    /// Note: If the current environment is the global environment, this function returns `Ok(None)`.
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-function-environment-records-getthisbinding
    pub(crate) fn get_this_binding(&self) -> JsResult<Option<JsValue>> {
        for env in self.stack.iter().rev() {
            if let Environment::Declarative(decl) = env
                && let Some(this) = decl.get_this_binding()?
            {
                return Ok(Some(this));
            }
        }

        Ok(None)
    }

    /// Push a new object environment on the environments stack.
    pub(crate) fn push_object(&mut self, object: JsObject) {
        self.stack.push(Environment::Object(object));
    }

    /// Returns whether a declarative environment pushed now starts poisoned
    /// and whether it is inside a `with` environment.
    fn inherited_poisoned_and_with(&self) -> (bool, bool) {
        // Check if the outer environment is a declarative environment.
        let with = if let Some(env) = self.stack.last() {
            env.as_declarative().is_none()
        } else {
            false
        };

        let environment = self
            .stack
            .iter()
            .rev()
            .find_map(Environment::as_declarative)
            .unwrap_or(self.global());
        (environment.poisoned(), with || environment.with())
    }

    /// Push a lexical environment on the environments stack and return it's index.
    pub(crate) fn push_lexical(&mut self, bindings_count: u32) -> u32 {
        let (poisoned, with) = self.inherited_poisoned_and_with();

        let index = self.stack.len() as u32;

        self.stack.push(Environment::Declarative(Gc::new(
            DeclarativeEnvironment::new(DeclarativeEnvironmentKind::Lexical(
                LexicalEnvironment::new(bindings_count, poisoned, with),
            )),
        )));

        index
    }

    /// Push the variable environment of a function body on the environments
    /// stack.
    ///
    /// The record keeps a runtime copy of its scope, as a function environment
    /// does, so that a direct eval can declare into it without changing the
    /// compiled scope that every call of the function shares.
    pub(crate) fn push_variable(&mut self, scope: &Scope) {
        let (poisoned, with) = self.inherited_poisoned_and_with();

        self.stack.push(Environment::Declarative(Gc::new(
            DeclarativeEnvironment::new(DeclarativeEnvironmentKind::Lexical(
                LexicalEnvironment::new_variable(poisoned, with, scope.clone_for_runtime()),
            )),
        )));
    }

    /// Push a function environment on the environments stack.
    pub(crate) fn push_function(&mut self, scope: Scope, function_slots: FunctionSlots) {
        let scope = scope.clone_for_runtime();
        let num_bindings = scope.num_bindings_non_local();

        let (poisoned, with) = {
            // Check if the outer environment is a declarative environment.
            let with = if let Some(env) = self.stack.last() {
                env.as_declarative().is_none()
            } else {
                false
            };

            let environment = self
                .stack
                .iter()
                .rev()
                .find_map(Environment::as_declarative)
                .unwrap_or(self.global());
            (environment.poisoned(), with || environment.with())
        };

        self.stack.push(Environment::Declarative(Gc::new(
            DeclarativeEnvironment::new(DeclarativeEnvironmentKind::Function(
                FunctionEnvironment::new(num_bindings, poisoned, with, function_slots, scope),
            )),
        )));
    }

    /// Push a module environment on the environments stack.
    pub(crate) fn push_module(&mut self, scope: Scope) {
        let num_bindings = scope.num_bindings_non_local();
        self.stack.push(Environment::Declarative(Gc::new(
            DeclarativeEnvironment::new(DeclarativeEnvironmentKind::Module(
                ModuleEnvironment::new(num_bindings, scope),
            )),
        )));
    }

    /// Pop environment from the environments stack.
    #[track_caller]
    pub(crate) fn pop(&mut self) {
        debug_assert!(!self.stack.is_empty());
        self.stack.pop();
    }

    /// Get the most outer environment.
    pub(crate) fn current_declarative_ref(&self) -> Option<&Gc<DeclarativeEnvironment>> {
        if let Some(env) = self.stack.last() {
            env.as_declarative()
        } else {
            Some(self.global())
        }
    }

    /// Get nearest declarative environment walking outwards from current stack top.
    pub(crate) fn nearest_declarative_ref(&self) -> Option<&Gc<DeclarativeEnvironment>> {
        for env in self.stack.iter().rev() {
            if let Some(env) = env.as_declarative() {
                return Some(env);
            }
        }

        Some(self.global())
    }

    /// Mark that there may be added bindings from the current environment to the
    /// VariableEnvironment of the running code.
    pub(crate) fn poison_until_variable_environment(&mut self) {
        for env in self
            .stack
            .iter()
            .rev()
            .filter_map(Environment::as_declarative)
        {
            env.poison();
            if env.variable_scope().is_some() {
                return;
            }
        }
        self.global().poison();
    }

    /// Set the value of a lexical binding.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    #[track_caller]
    pub(crate) fn put_lexical_value(
        &mut self,
        environment: BindingLocatorScope,
        binding_index: u32,
        value: JsValue,
    ) {
        let env = match environment {
            BindingLocatorScope::GlobalObject | BindingLocatorScope::GlobalDeclarative => {
                self.global()
            }
            BindingLocatorScope::Stack(index) => self
                .stack
                .get(index as usize)
                .and_then(Environment::as_declarative)
                .expect("must be declarative environment"),
        };
        if env.has_binding_index(binding_index) {
            env.set(binding_index, value);
            return;
        }

        if let Some(fallback) = self
            .stack
            .iter()
            .rev()
            .find_map(Environment::as_declarative)
            .filter(|env| env.has_binding_index(binding_index))
        {
            fallback.set(binding_index, value);
            return;
        }

        env.set(binding_index, value);
    }

    /// Set the value of a binding if it is uninitialized.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    #[track_caller]
    pub(crate) fn put_value_if_uninitialized(
        &mut self,
        environment: BindingLocatorScope,
        binding_index: u32,
        value: JsValue,
    ) {
        let env = match environment {
            BindingLocatorScope::GlobalObject | BindingLocatorScope::GlobalDeclarative => {
                self.global()
            }
            BindingLocatorScope::Stack(index) => self
                .stack
                .get(index as usize)
                .and_then(Environment::as_declarative)
                .expect("must be declarative environment"),
        };
        if env.is_uninitialized(binding_index) {
            env.set(binding_index, value);
        }
    }

    /// Push a private environment to the private environment stack.
    pub(crate) fn push_private(&mut self, environment: Gc<PrivateEnvironment>) {
        self.private_stack.push(environment);
    }

    /// Pop a private environment from the private environment stack.
    pub(crate) fn pop_private(&mut self) {
        self.private_stack.pop();
    }

    /// `ResolvePrivateIdentifier ( privEnv, identifier )`
    ///
    /// More information:
    ///  - [ECMAScript specification][spec]
    ///
    /// [spec]: https://tc39.es/ecma262/#sec-resolve-private-identifier
    pub(crate) fn resolve_private_identifier(&self, identifier: JsString) -> Option<PrivateName> {
        // 1. Let names be privEnv.[[Names]].
        // 2. For each Private Name pn of names, do
        //     a. If pn.[[Description]] is identifier, then
        //         i. Return pn.
        // 3. Let outerPrivEnv be privEnv.[[OuterPrivateEnvironment]].
        // 4. Assert: outerPrivEnv is not null.
        // 5. Return ResolvePrivateIdentifier(outerPrivEnv, identifier).
        for environment in self.private_stack.iter().rev() {
            if environment.descriptions().contains(&identifier) {
                return Some(PrivateName::new(identifier, environment.id()));
            }
        }
        None
    }

    /// Return all private name descriptions in all private environments.
    pub(crate) fn private_name_descriptions(&self) -> Vec<&JsString> {
        let mut names = Vec::new();
        for environment in self.private_stack.iter().rev() {
            for name in environment.descriptions() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// Indicate if the current environment stack has an object environment.
    pub(crate) fn has_object_environment(&self) -> bool {
        self.stack
            .iter()
            .any(|env| matches!(env, Environment::Object(_)))
    }
}

impl Context {
    pub(crate) const UNRESOLVABLE_GLOBAL_REFERENCE_MARKER: u32 = u32::MAX;

    fn explicit_resource_symbol(&self, r#async: bool) -> JsSymbol {
        if r#async {
            JsSymbol::async_dispose()
        } else {
            JsSymbol::dispose()
        }
    }

    fn is_binding_unscopable(&mut self, object: &JsObject, key: &JsString) -> JsResult<bool> {
        let Some(unscopables) = object.get(JsSymbol::unscopables(), self)?.as_object() else {
            return Ok(false);
        };

        Ok(unscopables.get(key.clone(), self)?.to_boolean())
    }

    pub(crate) fn get_dispose_method(
        &mut self,
        value: &JsValue,
        r#async: bool,
        nullish_message: &'static str,
        missing_message: &'static str,
    ) -> JsResult<Option<JsObject>> {
        if value.is_null() || value.is_undefined() {
            return Ok(None);
        }

        let Some(object) = value.as_object() else {
            return Err(JsNativeError::typ().with_message(nullish_message).into());
        };

        let (method, sync_fallback) = if r#async {
            let async_method = object.get(self.explicit_resource_symbol(true), self)?;
            if async_method.is_null() || async_method.is_undefined() {
                (
                    object.get(self.explicit_resource_symbol(false), self)?,
                    true,
                )
            } else {
                (async_method, false)
            }
        } else {
            (
                object.get(self.explicit_resource_symbol(false), self)?,
                false,
            )
        };

        if method.is_null() || method.is_undefined() {
            return Err(JsNativeError::typ().with_message(missing_message).into());
        }

        let method = method
            .as_callable()
            .ok_or_else(|| JsNativeError::typ().with_message(missing_message))?;
        if !sync_fallback {
            return Ok(Some(method));
        }

        // GetDisposeMethod's sync fallback ignores the method's return value
        // and converts its synchronous Throw into a rejected intrinsic Promise.
        let wrapper = FunctionObjectBuilder::new(
            self.realm(),
            NativeFunction::from_copy_closure_with_captures(
                |this, _args, method, context| {
                    let capability = PromiseCapability::new(
                        &context.intrinsics().constructors().promise().constructor(),
                        context,
                    )
                    .expect("intrinsic Promise creates a capability");
                    match method.call(this, &[], context) {
                        Ok(_) => {
                            capability
                                .resolve()
                                .call(&JsValue::undefined(), &[JsValue::undefined()], context)
                                .expect("resolving an intrinsic capability cannot fail");
                        }
                        Err(error) => {
                            let reason = error.to_opaque(context);
                            capability
                                .reject()
                                .call(&JsValue::undefined(), &[reason], context)
                                .expect("rejecting an intrinsic capability cannot fail");
                        }
                    }
                    Ok(capability.promise().clone().into())
                },
                method,
            ),
        )
        .name("")
        .length(0)
        .build();
        Ok(Some(wrapper.into()))
    }

    pub(crate) fn append_disposal_error_value(
        &mut self,
        current: Option<JsValue>,
        error: JsValue,
    ) -> JsValue {
        if let Some(suppressed) = current {
            self.construct_suppressed_error_value(error, suppressed)
        } else {
            error
        }
    }

    pub(crate) fn invoke_disposable_resource(
        &mut self,
        resource: &DisposableResource,
    ) -> JsResult<JsValue> {
        match resource.method() {
            Some(method) => match resource.argument() {
                Some(argument) => {
                    method.call(resource.this_value(), std::slice::from_ref(argument), self)
                }
                None => method.call(resource.this_value(), &[], self),
            },
            None => Ok(JsValue::undefined()),
        }
    }

    fn construct_suppressed_error_value(&mut self, error: JsValue, suppressed: JsValue) -> JsValue {
        let ctor = self
            .intrinsics()
            .constructors()
            .suppressed_error()
            .constructor();
        match ctor.construct(&[error, suppressed, JsValue::undefined()], None, self) {
            Ok(object) => object.into(),
            Err(err) => err.to_opaque(self),
        }
    }

    pub(crate) fn append_disposal_error(
        &mut self,
        current: Option<JsValue>,
        error: JsError,
    ) -> JsValue {
        let error = error.to_opaque(self);
        self.append_disposal_error_value(current, error)
    }

    fn await_disposal_result_blocking(&mut self, value: JsValue) -> JsResult<()> {
        let promise = Promise::promise_resolve(
            &self.intrinsics().constructors().promise().constructor(),
            value,
            self,
        )?;
        let promise = JsPromise::from_object(promise)?;
        if matches!(promise.state(), PromiseState::Pending) {
            self.run_jobs()?;
        }
        match promise.state() {
            PromiseState::Fulfilled(_) => Ok(()),
            PromiseState::Rejected(reason) => Err(JsError::from_opaque(reason)),
            PromiseState::Pending => Err(JsNativeError::error()
                .with_message("async disposal promise remained pending after draining jobs")
                .into()),
        }
    }

    pub(crate) fn add_disposable_resource_to_current_environment(
        &mut self,
        value: JsValue,
        r#async: bool,
    ) -> JsResult<()> {
        let method = self.get_dispose_method(
            &value,
            r#async,
            "using declarations require an object, null, or undefined",
            "using declaration resource is missing a callable dispose method",
        )?;

        if value.is_null() || value.is_undefined() {
            if !r#async {
                return Ok(());
            }
        }

        self.vm
            .environments
            .current_declarative_ref()
            .expect("using declarations require a declarative environment")
            .push_disposable_resource(DisposableResource::from_value(value, method, r#async));
        Ok(())
    }

    fn dispose_environment_resources(
        &mut self,
        env: &Gc<DeclarativeEnvironment>,
        current: Option<JsValue>,
    ) -> Option<JsValue> {
        let mut current = current;
        let resources = env.take_disposable_resources();
        for resource in resources.into_iter().rev() {
            let outcome = self
                .invoke_disposable_resource(&resource)
                .and_then(|value| {
                    if resource.r#async() {
                        self.await_disposal_result_blocking(value)
                    } else {
                        Ok(())
                    }
                });
            if let Err(err) = outcome {
                current = Some(self.append_disposal_error(current, err));
            }
        }
        current
    }

    pub(crate) fn pop_environment_with_dispose(&mut self) -> JsResult<()> {
        let mut current = self
            .vm
            .pending_exception
            .take()
            .map(|error| error.to_opaque(self));
        if let Some(env) = self
            .vm
            .environments
            .top()
            .and_then(Environment::as_declarative)
            .cloned()
        {
            current = self.dispose_environment_resources(&env, current);
        }
        self.vm.environments.pop();
        if let Some(error) = current {
            return Err(JsError::from_opaque(error));
        }
        Ok(())
    }

    pub(crate) fn unwind_environments_to(&mut self, len: usize) {
        let mut current = self
            .vm
            .pending_exception
            .take()
            .map(|error| error.to_opaque(self));

        while self.vm.environments.len() > len {
            if let Some(env) = self
                .vm
                .environments
                .top()
                .and_then(Environment::as_declarative)
                .cloned()
            {
                current = self.dispose_environment_resources(&env, current);
            }
            self.vm.environments.pop();
        }

        self.vm.pending_exception = current.map(JsError::from_opaque);
    }

    /// Gets the corresponding runtime binding of the provided `BindingLocator`, modifying
    /// its indexes in place.
    ///
    /// This readjusts a `BindingLocator` to the correct binding if a `with` environment or
    /// `eval` call modified the compile-time bindings.
    ///
    /// Only use if the binding origin is unknown or comes from a `var` declaration. Lexical bindings
    /// are completely removed of runtime checks because the specification guarantees that runtime
    /// semantics cannot add or remove lexical bindings.
    pub(crate) fn find_runtime_binding(&mut self, locator: &mut BindingLocator) -> JsResult<()> {
        if let Some(env) = self.vm.environments.current_declarative_ref()
            && !env.with()
            && !env.poisoned()
        {
            let top_index = self
                .vm
                .environments
                .stack
                .len()
                .checked_sub(1)
                .map(|index| index as u32);
            let matches_current_env = match locator.scope() {
                BindingLocatorScope::GlobalObject | BindingLocatorScope::GlobalDeclarative => {
                    top_index.is_none()
                }
                BindingLocatorScope::Stack(index) => top_index == Some(index),
            };

            if matches_current_env {
                return Ok(());
            }
        }

        let (global, min_index) = match locator.scope() {
            BindingLocatorScope::GlobalObject | BindingLocatorScope::GlobalDeclarative => (true, 0),
            BindingLocatorScope::Stack(index) => (false, index),
        };
        let max_index = self.vm.environments.stack.len() as u32;

        for index in (min_index..max_index).rev() {
            match self.environment_expect(index) {
                Environment::Declarative(env) => {
                    if env.poisoned() {
                        if let Some(b) = env.runtime_binding(locator.name()) {
                            locator.set_scope(b.scope());
                            locator.set_binding_index(b.binding_index());
                            return Ok(());
                        }
                    } else if !env.with() {
                        return Ok(());
                    }
                }
                Environment::Object(o) => {
                    let o = o.clone();
                    let key = locator.name().clone();
                    if o.has_property(key.clone(), self)? {
                        if self.is_binding_unscopable(&o, &key)? {
                            continue;
                        }
                        locator.set_scope(BindingLocatorScope::Stack(index));
                        return Ok(());
                    }
                }
            }
        }

        if global
            && self.realm().environment().poisoned()
            && let Some(b) = self.realm().scope().get_binding(locator.name())
        {
            locator.set_scope(b.scope());
            locator.set_binding_index(b.binding_index());
        }

        Ok(())
    }

    /// Finds the object environment that contains the binding and returns the `this` value of the object environment.
    pub(crate) fn this_from_object_environment_binding(
        &mut self,
        locator: &BindingLocator,
    ) -> JsResult<Option<JsObject>> {
        if matches!(locator.scope(), BindingLocatorScope::GlobalObject) {
            return Ok(None);
        }

        if let BindingLocatorScope::Stack(index) = locator.scope()
            && let Environment::Object(obj) = self.environment_expect(index)
        {
            return Ok(Some(obj.clone()));
        }

        if let Some(env) = self.vm.environments.current_declarative_ref()
            && !env.with()
        {
            return Ok(None);
        }

        let min_index = match locator.scope() {
            BindingLocatorScope::GlobalObject | BindingLocatorScope::GlobalDeclarative => 0,
            BindingLocatorScope::Stack(index) => index,
        };
        let max_index = self.vm.environments.stack.len() as u32;

        for index in (min_index..max_index).rev() {
            match self.environment_expect(index) {
                Environment::Declarative(env) => {
                    if env.poisoned() {
                        if env.runtime_binding(locator.name()).is_some() {
                            break;
                        }
                    } else if !env.with() {
                        break;
                    }
                }
                Environment::Object(o) => {
                    let o = o.clone();
                    let key = locator.name().clone();
                    if o.has_property(key.clone(), self)? {
                        if self.is_binding_unscopable(&o, &key)? {
                            continue;
                        }
                        return Ok(Some(o));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Checks if the binding pointed by `locator` is initialized.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    pub(crate) fn is_initialized_binding(&mut self, locator: &BindingLocator) -> JsResult<bool> {
        match locator.scope() {
            BindingLocatorScope::GlobalObject => {
                let key = locator.name().clone();
                let obj = self.global_object();
                obj.has_property(key, self)
            }
            BindingLocatorScope::GlobalDeclarative => {
                let env = self.vm.environments.global();
                Ok(env.get(locator.binding_index()).is_some())
            }
            BindingLocatorScope::Stack(index) => match self.environment_expect(index) {
                Environment::Declarative(env) => Ok(env.get(locator.binding_index()).is_some()),
                Environment::Object(obj) => {
                    let key = locator.name().clone();
                    let obj = obj.clone();
                    obj.has_property(key, self)
                }
            },
        }
    }

    /// Get the value of a binding.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    #[track_caller]
    pub(crate) fn get_binding(&mut self, locator: &BindingLocator) -> JsResult<Option<JsValue>> {
        match locator.scope() {
            BindingLocatorScope::GlobalObject => {
                let key = locator.name().clone();
                let obj = self.global_object();
                obj.try_get(key, self)
            }
            BindingLocatorScope::GlobalDeclarative => {
                let env = self.vm.environments.global();
                Ok(env.get(locator.binding_index()))
            }
            BindingLocatorScope::Stack(index) => match self.environment_expect(index) {
                Environment::Declarative(env) => Ok(env.get(locator.binding_index())),
                Environment::Object(obj) => {
                    let key = locator.name().clone();
                    let obj = obj.clone();
                    if obj.has_property(key.clone(), self)? {
                        obj.get(key, self).map(Some)
                    } else {
                        Ok(None)
                    }
                }
            },
        }
    }

    /// Sets the value of a binding.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    #[track_caller]
    pub(crate) fn set_binding(
        &mut self,
        locator: &BindingLocator,
        value: JsValue,
        strict: bool,
    ) -> JsResult<()> {
        match locator.scope() {
            BindingLocatorScope::GlobalObject => {
                if locator.binding_index() == Self::UNRESOLVABLE_GLOBAL_REFERENCE_MARKER {
                    return Ok(());
                }
                let key = locator.name().clone();
                let obj = self.global_object();
                obj.set(key, value, strict, self)?;
            }
            BindingLocatorScope::GlobalDeclarative => {
                let global_scope = self.realm().scope();
                if global_scope
                    .binding_throws_on_mutation(locator.binding_index())
                    .unwrap_or(false)
                {
                    return Err(JsNativeError::typ()
                        .with_message(format!(
                            "cannot mutate an immutable binding '{}'",
                            locator.name().to_std_string_escaped()
                        ))
                        .into());
                }
                if !global_scope
                    .binding_is_mutable(locator.binding_index())
                    .unwrap_or(true)
                {
                    return Ok(());
                }
                match global_scope.set_mutable_binding(locator.name().clone()) {
                    Ok(_) => {}
                    Err(BindingLocatorError::MutateImmutable) => {
                        return Err(JsNativeError::typ()
                            .with_message(format!(
                                "cannot mutate an immutable binding '{}'",
                                locator.name().to_std_string_escaped()
                            ))
                            .into());
                    }
                    Err(BindingLocatorError::Silent) => return Ok(()),
                }
                let env = self.vm.environments.global();
                env.set(locator.binding_index(), value);
            }
            BindingLocatorScope::Stack(index) => match self.environment_expect(index) {
                Environment::Declarative(decl) => {
                    decl.set(locator.binding_index(), value);
                }
                Environment::Object(obj) => {
                    let key = locator.name().clone();
                    let obj = obj.clone();
                    let still_exists = obj.has_property(key.clone(), self)?;
                    if !still_exists && strict {
                        return Err(JsNativeError::reference()
                            .with_message(format!(
                                "{} is not defined",
                                locator.name().to_std_string_escaped()
                            ))
                            .into());
                    }
                    obj.set(key, value, strict, self)?;
                }
            },
        }
        Ok(())
    }

    /// Marks a mutable binding as deletable.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    pub(crate) fn set_mutable_binding_deletable(&mut self, locator: &BindingLocator) {
        match locator.scope() {
            BindingLocatorScope::GlobalObject | BindingLocatorScope::GlobalDeclarative => {}
            BindingLocatorScope::Stack(index) => {
                if let Environment::Declarative(env) = self.environment_expect(index) {
                    env.set_mutable_binding_deletable(locator.binding_index());
                }
            }
        }
    }

    /// Deletes a binding if it exists.
    ///
    /// Returns `true` if the binding was deleted.
    ///
    /// # Panics
    ///
    /// Panics if the environment or binding index are out of range.
    pub(crate) fn delete_binding(&mut self, locator: &BindingLocator) -> JsResult<bool> {
        match locator.scope() {
            BindingLocatorScope::GlobalObject => {
                let key = locator.name().clone();
                let obj = self.global_object();
                obj.__delete__(&key.into(), &mut self.into())
            }
            BindingLocatorScope::GlobalDeclarative => Ok(false),
            BindingLocatorScope::Stack(index) => match self.environment_expect(index) {
                Environment::Declarative(env) => Ok(env.delete(locator.binding_index())),
                Environment::Object(obj) => {
                    let key = locator.name().clone();
                    let obj = obj.clone();
                    obj.__delete__(&key.into(), &mut self.into())
                }
            },
        }
    }

    /// Return the environment at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the `index` is out of range.
    pub(crate) fn environment_expect(&self, index: u32) -> &Environment {
        self.vm
            .environments
            .stack
            .get(index as usize)
            .expect("environment index must be in range")
    }
}
