//! One canonical body receives arguments proven unobservable in Wasm locals.

use super::*;
use crate::arguments_protocol::UnobservedStrictArguments;
use crate::function_layout::LocalArgumentsFunctionIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalArgumentsArity {
    Narrow,
    Wide,
}

impl LocalArgumentsArity {
    pub(crate) fn slot_count(self) -> usize {
        match self {
            Self::Narrow => 2,
            Self::Wide => 5,
        }
    }

    fn signature(self) -> StaticSignature {
        match self {
            Self::Narrow => StaticSignature::LocalArgumentsFunction,
            Self::Wide => StaticSignature::WideLocalArgumentsFunction,
        }
    }
}

/// The existing arguments-observer proof also justifies dropping the actual
/// argument count and all values beyond the bounded simple formal list.
#[derive(Debug, Clone)]
pub(crate) struct LocalArgumentsPlan {
    source: FunctionId,
    parameter_count: usize,
    arity: LocalArgumentsArity,
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
            ("export function make(a,b,c) { return a+b+c; }", true),
            ("export function make(a,b,c,d,e) { return a+e; }", true),
            ("export function make(a,b,c,d,e,f) { return a+f; }", false),
            (
                "export function make(a,b,c,d,e) { return arguments[4]; }",
                false,
            ),
            (
                "export function make(a,b,c,d,e) { return () => arguments[4]; }",
                false,
            ),
            ("export function make(a,b,c,d,e=1) { return e; }", false),
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

    #[test]
    fn native_local_entry_is_closed_and_keeps_the_existing_runtime_prefix() {
        use crate::function_layout::{FunctionIndexLayout, ModuleEntryRole};
        use crate::runtime_artifact::ModuleKind;
        let original = FunctionIndexLayout::new(
            2,
            1,
            3,
            4,
            5,
            0,
            0,
            0,
            0,
            ModuleKind::Runtime(crate::RuntimeSourceCompilationProfile::Portable),
            ModuleEntryRole::RuntimeOnly,
        );
        let builtin = StandardBuiltinId::FunctionPrototypeSymbolHasInstance;
        assert!(original.native_local_arguments_index(builtin).is_none());
        let layout = original.with_standard_builtin_local_arguments(&[builtin]);
        assert_eq!(
            layout.first_program_index(),
            original.first_program_index() + 1
        );
        assert_eq!(layout.standard_index(0), original.standard_index(0));
        assert_eq!(layout.host_index(0), original.host_index(0));
        for candidate in StandardBuiltinId::all_functions() {
            assert_eq!(
                layout.native_local_arguments_index(*candidate).is_some(),
                *candidate == builtin
            );
        }
        let index = layout.native_local_arguments_index(builtin).unwrap();
        index.assert_body_index(original.first_program_index());
        let entry = PlannedLocalArgumentsEntry::native_builtin(builtin, index);
        assert!(!entry.is_source());
        assert_eq!(entry.arity(), LocalArgumentsArity::Narrow);
        assert!(entry.source_site_owner().is_err());
        let planned = PlannedFunctionEntry::standard_with_local_arguments(builtin, 3, Some(entry));
        assert_eq!(planned.protocol(), FunctionProtocolIr::OrdinaryCallOnly);
        assert!(!planned.has_source_activation());
    }
}

impl LocalArgumentsPlan {
    pub(crate) fn for_function(source: &FunctionIr) -> Option<Self> {
        let arity = match source.params.len() {
            0..=2 => LocalArgumentsArity::Narrow,
            3..=5 => LocalArgumentsArity::Wide,
            _ => return None,
        };
        UnobservedStrictArguments::admit(source)?;
        Some(Self {
            source: source.id.clone(),
            parameter_count: source.params.len(),
            arity,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedLocalArgumentsEntry {
    owner: LocalArgumentsOwner,
    index: LocalArgumentsFunctionIndex,
}

#[derive(Debug, Clone)]
enum LocalArgumentsOwner {
    Source {
        proof: LocalArgumentsPlan,
        name: String,
    },
    Native(NativeLocalArgumentsBuiltin),
}

/// Native algorithms that read at most the two narrow argument slots and
/// never observe the argument count or any later argument. An absent argument
/// is undefined in both ABIs, so the vector adapter and a direct local call
/// reach the same algorithm. A body that reads the count or the vector cannot
/// be compiled for this ABI (`BodyEntryLocals` rejects it at emission).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeLocalArgumentsBuiltin {
    FunctionPrototypeSymbolHasInstance,
    ObjectGetPrototypeOf,
    ObjectPrototypeHasOwnProperty,
    ArrayIsArray,
    NumberPrototypeToString,
    StringPrototypeCharCodeAt,
    StringPrototypeIndexOf,
    StringPrototypeSlice,
    MapPrototypeGet,
    MapPrototypeHas,
    MapPrototypeSet,
    WeakMapPrototypeGet,
    WeakMapPrototypeHas,
    WeakMapPrototypeSet,
}

impl NativeLocalArgumentsBuiltin {
    /// Declaration order is the order of the common R suffix of local bodies.
    pub(crate) const ALL: [Self; 14] = [
        Self::FunctionPrototypeSymbolHasInstance,
        Self::ObjectGetPrototypeOf,
        Self::ObjectPrototypeHasOwnProperty,
        Self::ArrayIsArray,
        Self::NumberPrototypeToString,
        Self::StringPrototypeCharCodeAt,
        Self::StringPrototypeIndexOf,
        Self::StringPrototypeSlice,
        Self::MapPrototypeGet,
        Self::MapPrototypeHas,
        Self::MapPrototypeSet,
        Self::WeakMapPrototypeGet,
        Self::WeakMapPrototypeHas,
        Self::WeakMapPrototypeSet,
    ];

    pub(crate) const fn builtin(self) -> StandardBuiltinId {
        match self {
            Self::FunctionPrototypeSymbolHasInstance => {
                StandardBuiltinId::FunctionPrototypeSymbolHasInstance
            }
            Self::ObjectGetPrototypeOf => StandardBuiltinId::ObjectGetPrototypeOf,
            Self::ObjectPrototypeHasOwnProperty => StandardBuiltinId::ObjectPrototypeHasOwnProperty,
            Self::ArrayIsArray => StandardBuiltinId::ArrayIsArray,
            Self::NumberPrototypeToString => StandardBuiltinId::NumberPrototypeToString,
            Self::StringPrototypeCharCodeAt => StandardBuiltinId::StringPrototypeCharCodeAt,
            Self::StringPrototypeIndexOf => StandardBuiltinId::StringPrototypeIndexOf,
            Self::StringPrototypeSlice => StandardBuiltinId::StringPrototypeSlice,
            Self::MapPrototypeGet => StandardBuiltinId::MapPrototypeGet,
            Self::MapPrototypeHas => StandardBuiltinId::MapPrototypeHas,
            Self::MapPrototypeSet => StandardBuiltinId::MapPrototypeSet,
            Self::WeakMapPrototypeGet => StandardBuiltinId::WeakMapPrototypeGet,
            Self::WeakMapPrototypeHas => StandardBuiltinId::WeakMapPrototypeHas,
            Self::WeakMapPrototypeSet => StandardBuiltinId::WeakMapPrototypeSet,
        }
    }

    pub(crate) fn admits(builtin: Option<StandardBuiltinId>) -> bool {
        builtin.and_then(Self::from_builtin).is_some()
    }

    pub(crate) fn from_builtin(builtin: StandardBuiltinId) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.builtin() == builtin)
    }

    pub(crate) const fn position(self) -> u32 {
        self as u32
    }
}

impl PlannedLocalArgumentsEntry {
    pub(crate) fn new(
        source: &FunctionIr,
        proof: LocalArgumentsPlan,
        index: LocalArgumentsFunctionIndex,
    ) -> Self {
        let entry = Self {
            owner: LocalArgumentsOwner::Source {
                proof,
                name: source.name.clone(),
            },
            index,
        };
        entry.require_source(source);
        entry
    }

    pub(crate) fn native_builtin(
        builtin: StandardBuiltinId,
        index: LocalArgumentsFunctionIndex,
    ) -> Self {
        let native = NativeLocalArgumentsBuiltin::from_builtin(builtin)
            .expect("native local entry belongs to the closed admitted set");
        Self {
            owner: LocalArgumentsOwner::Native(native),
            index,
        }
    }

    pub(super) fn require_builtin(&self, builtin: StandardBuiltinId) {
        let LocalArgumentsOwner::Native(native) = self.owner else {
            panic!("source local arguments proof cannot authorize a native body");
        };
        assert_eq!(native.builtin(), builtin);
    }

    pub(crate) fn is_source(&self) -> bool {
        matches!(self.owner, LocalArgumentsOwner::Source { .. })
    }

    pub(super) fn require_source(&self, source: &FunctionIr) {
        let LocalArgumentsOwner::Source { proof, .. } = &self.owner else {
            panic!("native local arguments proof cannot authorize a source body");
        };
        assert_eq!(
            proof.source, source.id,
            "local arguments proof belongs to its source"
        );
        assert_eq!(
            proof.parameter_count,
            source.params.len(),
            "local arguments proof retains its formal list"
        );
    }

    pub(crate) fn arity(&self) -> LocalArgumentsArity {
        match &self.owner {
            LocalArgumentsOwner::Source { proof, .. } => proof.arity,
            LocalArgumentsOwner::Native(_) => LocalArgumentsArity::Narrow,
        }
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.arity().slot_count()
    }

    pub(crate) fn signature(&self) -> StaticSignature {
        self.arity().signature()
    }

    pub(crate) fn identity(&self) -> FunctionIdentity {
        match &self.owner {
            LocalArgumentsOwner::Source { proof, name } => FunctionIdentity::Script {
                id: proof.source.clone(),
                name: name.clone(),
            },
            LocalArgumentsOwner::Native(native) => {
                FunctionIdentity::StandardBuiltin(native.builtin())
            }
        }
    }

    pub(super) fn adapter_identity(&self) -> FunctionIdentity {
        match &self.owner {
            LocalArgumentsOwner::Source { proof, name } => {
                FunctionIdentity::LocalArgumentsAdapter {
                    id: proof.source.clone(),
                    name: name.clone(),
                }
            }
            LocalArgumentsOwner::Native(native) => {
                FunctionIdentity::NativeLocalArgumentsAdapter(native.builtin())
            }
        }
    }

    pub(crate) fn source_site_owner(&self) -> Result<u32, EmitError> {
        if !self.is_source() {
            return Err(EmitError::unsupported(
                "native local entry cannot own a program source site",
            ));
        }
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
