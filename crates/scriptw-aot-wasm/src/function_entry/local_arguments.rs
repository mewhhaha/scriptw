//! One canonical source body receives unobservable arguments in Wasm locals.

use super::*;
use crate::arguments_protocol::UnobservedStrictArguments;
use crate::function_layout::LocalArgumentsFunctionIndex;

/// The existing arguments-observer proof also justifies dropping the actual
/// argument count and all values beyond the bounded simple formal list.
#[derive(Debug, Clone)]
pub(crate) struct LocalArgumentsPlan {
    source: FunctionId,
    parameter_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    #[test]
    fn local_transport_keeps_the_existing_observer_proof_and_bounds_formals() {
        for (source, expected) in [
            (
                "let state=0; export function make(a,b) { state+=a.x; return b(state); }",
                true,
            ),
            ("export function make() { return this; }", true),
            ("export function make(a,b,c) { return a+b+c; }", false),
            (
                "export function make(a) { return arguments.length; }",
                false,
            ),
            (
                "export function make(a) { return () => arguments[0]; }",
                false,
            ),
            (
                "export function make(a) { return eval('arguments[0]'); }",
                false,
            ),
            ("export function make(...a) { return a[0]; }", false),
            ("export function make(a=1) { return a; }", false),
            ("export async function make(a) { return a; }", false),
        ] {
            let parsed = parse(source, ParseOptions::module()).unwrap();
            let lowered = scriptw_ir::lower(&parsed);
            let script = lowered.script.as_ref().unwrap();
            let function = script
                .functions
                .iter()
                .find(|function| function.name == "make")
                .unwrap();
            assert_eq!(
                LocalArgumentsPlan::for_function(function).is_some(),
                expected,
                "{source}"
            );
        }
    }
}

impl LocalArgumentsPlan {
    pub(crate) fn for_function(source: &FunctionIr) -> Option<Self> {
        if source.params.len() > 2 {
            return None;
        }
        UnobservedStrictArguments::admit(source)?;
        Some(Self {
            source: source.id.clone(),
            parameter_count: source.params.len(),
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedLocalArgumentsEntry {
    proof: LocalArgumentsPlan,
    name: String,
    index: LocalArgumentsFunctionIndex,
}

impl PlannedLocalArgumentsEntry {
    pub(crate) fn new(
        source: &FunctionIr,
        proof: LocalArgumentsPlan,
        index: LocalArgumentsFunctionIndex,
    ) -> Self {
        let entry = Self {
            proof,
            name: source.name.clone(),
            index,
        };
        entry.require_source(source);
        entry
    }

    pub(super) fn require_source(&self, source: &FunctionIr) {
        assert_eq!(
            self.proof.source, source.id,
            "local arguments proof belongs to its source"
        );
        assert_eq!(
            self.proof.parameter_count,
            source.params.len(),
            "local arguments proof retains its formal list"
        );
    }

    pub(crate) fn signature(&self) -> StaticSignature {
        StaticSignature::LocalArgumentsFunction
    }

    pub(crate) fn identity(&self) -> FunctionIdentity {
        FunctionIdentity::Script {
            id: self.proof.source.clone(),
            name: self.name.clone(),
        }
    }

    pub(super) fn adapter_identity(&self) -> FunctionIdentity {
        FunctionIdentity::LocalArgumentsAdapter {
            id: self.proof.source.clone(),
            name: self.name.clone(),
        }
    }

    pub(crate) fn source_site_owner(&self) -> Result<u32, EmitError> {
        self.index.source_site_owner()
    }

    pub(crate) fn assert_body_index(&self, actual: u32) {
        self.index.assert_body_index(actual);
    }

    pub(crate) fn emit_ref_func(&self, function: &mut Function) {
        self.index.emit_ref_func(function);
    }

    pub(crate) fn emit_direct_call_instruction(&self, function: &mut Function) {
        self.index.emit_call(function);
    }

    pub(crate) fn emit_return_call_instruction(&self, function: &mut Function) {
        self.index.emit_return_call(function);
    }
}
