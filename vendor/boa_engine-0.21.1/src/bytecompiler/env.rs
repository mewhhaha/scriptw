use super::ByteCompiler;
use crate::{JsString, vm::Constant};
use boa_ast::scope::{BindingLocatorError, IdentifierReference, Scope};

impl ByteCompiler<'_> {
    /// Push either a new declarative or function scope on the environment stack.
    #[must_use]
    pub(crate) fn push_scope(&mut self, scope: &Scope) -> u32 {
        self.current_open_environments_count += 1;

        let index = self.constants.len() as u32;
        self.constants.push(Constant::Scope(scope.clone()));

        if scope.is_function() {
            self.variable_scope = scope.clone();
        }

        self.lexical_scope = scope.clone();

        index
    }

    /// Push a declarative scope.
    ///
    /// Returns the outer scope.
    #[must_use]
    pub(crate) fn push_declarative_scope(
        &mut self,
        scope: Option<&Scope>,
    ) -> Option<(Scope, bool)> {
        let mut scope = scope?.clone();
        let pushed = !scope.all_bindings_local();
        if pushed {
            self.current_open_environments_count += 1;
            let index = self.constants.len() as u32;
            self.constants.push(Constant::Scope(scope.clone()));
            self.bytecode.emit_push_scope(index.into());
        }
        std::mem::swap(&mut self.lexical_scope, &mut scope);
        Some((scope, pushed))
    }

    /// Push the scope of a function body's own variable environment record
    /// (`FunctionDeclarationInstantiation` step 28) and make it the variable
    /// scope of the body's code. The record stays open until the function
    /// returns.
    ///
    /// A sloppy body gets a record that a direct eval can declare into; a
    /// strict direct eval declares into its own record instead.
    pub(crate) fn push_body_variable_scope(&mut self, scope: &Scope, strict: bool) {
        if !scope.all_bindings_local() {
            self.current_open_environments_count += 1;
            let index = self.constants.len() as u32;
            self.constants.push(Constant::Scope(scope.clone()));
            if strict {
                self.bytecode.emit_push_scope(index.into());
            } else {
                self.bytecode.emit_push_variable_scope(index.into());
            }
        }
        self.lexical_scope = scope.clone();
        self.variable_scope = scope.clone();
    }

    /// Resolves the mutable binding `name` of the running code's
    /// VariableEnvironment for Annex B function hoisting.
    ///
    /// A function body's own record is not a function scope, so its binding is
    /// looked up in it directly instead of in the enclosing function scopes.
    pub(crate) fn variable_environment_binding(
        &self,
        name: JsString,
    ) -> Result<IdentifierReference, BindingLocatorError> {
        if self.variable_scope.is_function() {
            self.variable_scope.set_mutable_binding_var(name)
        } else {
            self.variable_scope.set_mutable_binding(name)
        }
    }

    /// Returns whether `scope`, a scope around the code being compiled, is
    /// where the walk from the LexicalEnvironment to the VariableEnvironment
    /// ends: a function scope, or the scope of a function body's own record.
    pub(crate) fn is_variable_scope(&self, scope: &Scope) -> bool {
        // Eval code holds a runtime copy of the body's scope, which keeps the
        // outer scope of the scope it was made from, and no other scope of
        // one chain has that outer scope. Scope indices cannot tell: a scope
        // without a runtime environment shares the index of the one around it.
        scope.is_function()
            || (!self.variable_scope.is_function() && scope.outer() == self.variable_scope.outer())
    }

    /// Pop a declarative scope.
    pub(crate) fn pop_declarative_scope(&mut self, scope: Option<(Scope, bool)>) {
        if let Some((mut scope, pushed)) = scope {
            std::mem::swap(&mut self.lexical_scope, &mut scope);
            if pushed {
                self.current_open_environments_count -= 1;
                self.bytecode.emit_pop_environment();
            }
        }
    }

    /// Pops the top scope.
    pub(crate) fn pop_scope(&mut self) {
        self.current_open_environments_count -= 1;
    }
}
