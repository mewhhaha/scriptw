//! Resolved immutable global DATA reads need no runtime lookup.
use super::*;
use scriptw_ir::{GlobalBindingPlan, GlobalPropertyInitializerIr};

enum ImmutableGlobalValue {
    Undefined,
    NaN,
    Infinity,
}

impl ImmutableGlobalValue {
    // Bootstrap publishes these exact own DATA initializers before source runs.
    // Their attributes cannot change, and RealmBuilder cannot replace the global
    // object. Global declaration instantiation rejects lexical declarations of
    // restricted global names before publishing any corresponding binding.
    fn new(bindings: &GlobalBindingPlan, name: &str) -> Option<Self> {
        let binding = bindings.get(name)?;
        if binding.initializer.writable() || binding.initializer.configurable() {
            return None;
        }
        match (name, &binding.initializer) {
            ("undefined", GlobalPropertyInitializerIr::Undefined) => Some(Self::Undefined),
            ("NaN", GlobalPropertyInitializerIr::NaN) => Some(Self::NaN),
            ("Infinity", GlobalPropertyInitializerIr::Infinity) => Some(Self::Infinity),
            _ => None,
        }
    }

    fn emit(self, output: &ValueLocals, function: &mut Function) {
        match self {
            Self::Undefined => output.set_undefined(function),
            Self::NaN => {
                output.set_scalar(ScalarValue::NumberBits(f64::NAN.to_bits() as i64), function)
            }
            Self::Infinity => output.set_scalar(
                ScalarValue::NumberBits(f64::INFINITY.to_bits() as i64),
                function,
            ),
        }
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_immutable_global_read(
        &mut self,
        name: &str,
        output: &ValueLocals,
        function: &mut Function,
    ) -> bool {
        if !self.has_source_execution_environment() {
            return false;
        }
        let Some(bindings) = self.script_global_bindings else {
            return false;
        };
        let Some(value) = ImmutableGlobalValue::new(bindings, name) else {
            return false;
        };
        // Only the resolved GlobalPropertyRead owner may consume this proof.
        // Locals, with scopes and eval-visible names use binding reads or
        // EnvironmentIdentifier instead, including their shadowing and TDZ.
        value.emit(output, function);
        true
    }
}
