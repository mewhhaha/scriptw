//! One canonical body receives fixed argument lanes and optional projection inputs.

use super::*;
use crate::arguments_protocol::{StrictArgumentsProjection, UnobservedStrictArguments};
use crate::function_layout::LocalArgumentsFunctionIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalArgumentsArity {
    Narrow,
    Wide,
}

impl LocalArgumentsArity {
    pub(crate) const fn slot_count(self) -> usize {
        match self {
            Self::Narrow => 2,
            Self::Wide => 6,
        }
    }

    pub(crate) fn for_actuals(count: usize) -> Option<Self> {
        [Self::Narrow, Self::Wide]
            .into_iter()
            .find(|arity| count <= arity.slot_count())
    }

    /// The wide ABI includes the actual count and a nullable original argv.
    /// A local call passes null and has at most six actuals. The adapter
    /// passes its original vector so it can forward any actual count.
    pub(super) const fn projection_parameters(self) -> Option<(u32, u32)> {
        match self {
            Self::Narrow => None,
            Self::Wide => {
                let argc = self.argument_parameters_end();
                Some((argc, argc + 1))
            }
        }
    }

    const fn argument_parameters_end(self) -> u32 {
        7 + 3 * self.slot_count() as u32
    }

    pub(super) const fn caller_realm_parameter(self) -> u32 {
        match self.projection_parameters() {
            Some((_, argv)) => argv + 1,
            None => self.argument_parameters_end(),
        }
    }

    fn signature(self) -> StaticSignature {
        match self {
            Self::Narrow => StaticSignature::LocalArgumentsFunction,
            Self::Wide => StaticSignature::WideLocalArgumentsFunction,
        }
    }
}

/// The unobserved proof permits the small ABI to omit the actual count and
/// extra values. A projected body always uses the wide ABI so even a call
/// supplying more actuals than formals can carry six arguments without argv.
#[derive(Debug, Clone)]
pub(crate) struct LocalArgumentsPlan {
    source: FunctionId,
    parameter_count: usize,
    arity: LocalArgumentsArity,
    observation: LocalArgumentsObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalArgumentsObservation {
    Unobserved,
    Projected,
}

impl LocalArgumentsObservation {
    fn admit(source: &FunctionIr) -> Option<Self> {
        if UnobservedStrictArguments::admit(source).is_some() {
            Some(Self::Unobserved)
        } else {
            StrictArgumentsProjection::admit_local(source).map(|_| Self::Projected)
        }
    }
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
                true,
            ),
            (
                "export function make(a,b,c,d,e,f) { return arguments[5]; }",
                true,
            ),
            (
                "export function make(a,b,c,d,e,f,g) { return arguments[6]; }",
                false,
            ),
            (
                "export function make(a,b,c,d,e) { return () => arguments[4]; }",
                false,
            ),
            ("export function make(a,b,c,d,e=1) { return e; }", false),
            ("export function make(a) { return arguments.length; }", true),
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
    fn object_pattern_formals_take_one_local_slot_each_under_the_same_proof() {
        for (source, expected) in [
            (
                "export function make({id}) { return id; }",
                Some(LocalArgumentsArity::Narrow),
            ),
            (
                "export function make({id}, x) { return id + x; }",
                Some(LocalArgumentsArity::Narrow),
            ),
            // A body `var` of a bound name is the parameter's binding, not
            // an observer of the argument list.
            (
                "export function make({id}, x) { var id, x; return id + x; }",
                Some(LocalArgumentsArity::Narrow),
            ),
            (
                "export function make({a: {b}, ...others}, {c = 1}) { return b + c; }",
                Some(LocalArgumentsArity::Narrow),
            ),
            (
                "export function make({a}, b, {c}) { return a + b + c; }",
                Some(LocalArgumentsArity::Wide),
            ),
            (
                "export function make({a}, {b}, {c}, {d}, {e}) { return a + e; }",
                Some(LocalArgumentsArity::Wide),
            ),
            // The bound counts formals, not the names they bind.
            (
                "export function make({a, b, c, d, e, f}) { return a + f; }",
                Some(LocalArgumentsArity::Narrow),
            ),
            (
                "export function make({a}, {b}, {c}, {d}, {e}, {f}) { return a + f; }",
                None,
            ),
            (
                "export function make({id}, x) { return arguments[1]; }",
                None,
            ),
            ("export function make({id}, x) { return () => id; }", None),
            ("export function make({id} = {}, x) { return id; }", None),
            ("export function make({id}, ...x) { return id; }", None),
            ("export function make([id], x) { return id; }", None),
            ("export function make({id}, [x]) { return id; }", None),
        ] {
            let parsed = parse(source, ParseOptions::module()).unwrap();
            let lowered = scriptw_ir::lower(&parsed);
            let script = lowered.script.as_ref().unwrap();
            let function = script
                .functions
                .iter()
                .find(|function| function.name == "make")
                .unwrap();
            let plan = LocalArgumentsPlan::for_function(function);
            assert_eq!(plan.as_ref().map(|plan| plan.arity), expected, "{source}");
            if let Some(plan) = plan {
                assert_eq!(plan.parameter_count, function.params.len(), "{source}");
            }
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
        let parameter_count = source.params.len();
        let formal_arity = LocalArgumentsArity::for_actuals(parameter_count)?;
        let observation = LocalArgumentsObservation::admit(source)?;
        // A builtin entry guard tail-calls its fallback with the parameters
        // it received: both bodies of the pair take one argument layout.
        let partner_observation = match &source.builtin_entry {
            Some(pair) => LocalArgumentsObservation::admit(pair.partner())?,
            None => observation,
        };
        let arity =
            if [observation, partner_observation].contains(&LocalArgumentsObservation::Projected) {
                LocalArgumentsArity::Wide
            } else {
                // Keep the existing formal admission for unobserved bodies.
                if parameter_count > 5 {
                    return None;
                }
                formal_arity
            };
        Some(Self {
            source: source.id.clone(),
            parameter_count,
            arity,
            observation,
        })
    }

    pub(crate) fn projects_arguments(&self) -> bool {
        self.observation == LocalArgumentsObservation::Projected
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

/// Native algorithms that read at most their typed local argument slots
/// (2 for Narrow, 6 for Wide) and never observe the argument count or any
/// later argument. An absent argument is undefined in both ABIs, so the vector
/// adapter and a direct local call reach the same algorithm. A body that reads
/// the count or the vector cannot be compiled for this ABI (`BodyEntryLocals`
/// rejects it at emission).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeLocalArgumentsBuiltin {
    FunctionPrototypeSymbolHasInstance,
    ObjectGetPrototypeOf,
    ObjectHasOwn,
    ObjectKeys,
    ObjectValues,
    ObjectEntries,
    ObjectFromEntries,
    ObjectPrototypeHasOwnProperty,
    ArrayIsArray,
    ArrayPrototypeMap,
    ArrayPrototypeFilter,
    ArrayPrototypeEvery,
    ArrayPrototypeSome,
    ArrayPrototypeFind,
    ArrayPrototypeFindIndex,
    ArrayPrototypeForEach,
    ArrayPrototypeSlice,
    ArrayPrototypeSort,
    NumberIsFinite,
    NumberIsInteger,
    NumberIsNaN,
    NumberIsSafeInteger,
    NumberPrototypeToString,
    MathAbs,
    MathCeil,
    MathFloor,
    MathRound,
    MathSqrt,
    MathTrunc,
    StringPrototypeCharCodeAt,
    StringPrototypeIndexOf,
    StringPrototypeIncludes,
    StringPrototypeStartsWith,
    StringPrototypeEndsWith,
    StringPrototypeSlice,
    StringPrototypeSplit,
    StringPrototypeTrim,
    StringPrototypeToLowerCase,
    StringPrototypeToUpperCase,
    StringPrototypeReplace,
    StringPrototypeReplaceAll,
    DecodeUriComponent,
    EncodeUriComponent,
    JsonParse,
    JsonStringify,
    MapPrototypeGet,
    MapPrototypeHas,
    MapPrototypeSet,
    MapPrototypeDelete,
    SetPrototypeHas,
    SetPrototypeAdd,
    SetPrototypeDelete,
    WeakMapPrototypeGet,
    WeakMapPrototypeHas,
    WeakMapPrototypeSet,
}

impl NativeLocalArgumentsBuiltin {
    /// Declaration order is the order of the common R suffix of local bodies.
    pub(crate) const ALL: [Self; 55] = [
        Self::FunctionPrototypeSymbolHasInstance,
        Self::ObjectGetPrototypeOf,
        Self::ObjectHasOwn,
        Self::ObjectKeys,
        Self::ObjectValues,
        Self::ObjectEntries,
        Self::ObjectFromEntries,
        Self::ObjectPrototypeHasOwnProperty,
        Self::ArrayIsArray,
        Self::ArrayPrototypeMap,
        Self::ArrayPrototypeFilter,
        Self::ArrayPrototypeEvery,
        Self::ArrayPrototypeSome,
        Self::ArrayPrototypeFind,
        Self::ArrayPrototypeFindIndex,
        Self::ArrayPrototypeForEach,
        Self::ArrayPrototypeSlice,
        Self::ArrayPrototypeSort,
        Self::NumberIsFinite,
        Self::NumberIsInteger,
        Self::NumberIsNaN,
        Self::NumberIsSafeInteger,
        Self::NumberPrototypeToString,
        Self::MathAbs,
        Self::MathCeil,
        Self::MathFloor,
        Self::MathRound,
        Self::MathSqrt,
        Self::MathTrunc,
        Self::StringPrototypeCharCodeAt,
        Self::StringPrototypeIndexOf,
        Self::StringPrototypeIncludes,
        Self::StringPrototypeStartsWith,
        Self::StringPrototypeEndsWith,
        Self::StringPrototypeSlice,
        Self::StringPrototypeSplit,
        Self::StringPrototypeTrim,
        Self::StringPrototypeToLowerCase,
        Self::StringPrototypeToUpperCase,
        Self::StringPrototypeReplace,
        Self::StringPrototypeReplaceAll,
        Self::DecodeUriComponent,
        Self::EncodeUriComponent,
        Self::JsonParse,
        Self::JsonStringify,
        Self::MapPrototypeGet,
        Self::MapPrototypeHas,
        Self::MapPrototypeSet,
        Self::MapPrototypeDelete,
        Self::SetPrototypeHas,
        Self::SetPrototypeAdd,
        Self::SetPrototypeDelete,
        Self::WeakMapPrototypeGet,
        Self::WeakMapPrototypeHas,
        Self::WeakMapPrototypeSet,
    ];

    pub(crate) const fn arity(self) -> LocalArgumentsArity {
        match self {
            Self::JsonStringify => LocalArgumentsArity::Wide,
            _ => LocalArgumentsArity::Narrow,
        }
    }

    pub(crate) const fn builtin(self) -> StandardBuiltinId {
        match self {
            Self::FunctionPrototypeSymbolHasInstance => {
                StandardBuiltinId::FunctionPrototypeSymbolHasInstance
            }
            Self::ObjectGetPrototypeOf => StandardBuiltinId::ObjectGetPrototypeOf,
            Self::ObjectHasOwn => StandardBuiltinId::ObjectHasOwn,
            Self::ObjectKeys => StandardBuiltinId::ObjectKeys,
            Self::ObjectValues => StandardBuiltinId::ObjectValues,
            Self::ObjectEntries => StandardBuiltinId::ObjectEntries,
            Self::ObjectFromEntries => StandardBuiltinId::ObjectFromEntries,
            Self::ObjectPrototypeHasOwnProperty => StandardBuiltinId::ObjectPrototypeHasOwnProperty,
            Self::ArrayIsArray => StandardBuiltinId::ArrayIsArray,
            Self::ArrayPrototypeMap => StandardBuiltinId::ArrayPrototypeMap,
            Self::ArrayPrototypeFilter => StandardBuiltinId::ArrayPrototypeFilter,
            Self::ArrayPrototypeEvery => StandardBuiltinId::ArrayPrototypeEvery,
            Self::ArrayPrototypeSome => StandardBuiltinId::ArrayPrototypeSome,
            Self::ArrayPrototypeFind => StandardBuiltinId::ArrayPrototypeFind,
            Self::ArrayPrototypeFindIndex => StandardBuiltinId::ArrayPrototypeFindIndex,
            Self::ArrayPrototypeForEach => StandardBuiltinId::ArrayPrototypeForEach,
            Self::ArrayPrototypeSlice => StandardBuiltinId::ArrayPrototypeSlice,
            Self::ArrayPrototypeSort => StandardBuiltinId::ArrayPrototypeSort,
            Self::NumberIsFinite => StandardBuiltinId::NumberIsFinite,
            Self::NumberIsInteger => StandardBuiltinId::NumberIsInteger,
            Self::NumberIsNaN => StandardBuiltinId::NumberIsNaN,
            Self::NumberIsSafeInteger => StandardBuiltinId::NumberIsSafeInteger,
            Self::NumberPrototypeToString => StandardBuiltinId::NumberPrototypeToString,
            Self::MathAbs => StandardBuiltinId::MathAbs,
            Self::MathCeil => StandardBuiltinId::MathCeil,
            Self::MathFloor => StandardBuiltinId::MathFloor,
            Self::MathRound => StandardBuiltinId::MathRound,
            Self::MathSqrt => StandardBuiltinId::MathSqrt,
            Self::MathTrunc => StandardBuiltinId::MathTrunc,
            Self::StringPrototypeCharCodeAt => StandardBuiltinId::StringPrototypeCharCodeAt,
            Self::StringPrototypeIndexOf => StandardBuiltinId::StringPrototypeIndexOf,
            Self::StringPrototypeIncludes => StandardBuiltinId::StringPrototypeIncludes,
            Self::StringPrototypeStartsWith => StandardBuiltinId::StringPrototypeStartsWith,
            Self::StringPrototypeEndsWith => StandardBuiltinId::StringPrototypeEndsWith,
            Self::StringPrototypeSlice => StandardBuiltinId::StringPrototypeSlice,
            Self::StringPrototypeSplit => StandardBuiltinId::StringPrototypeSplit,
            Self::StringPrototypeTrim => StandardBuiltinId::StringPrototypeTrim,
            Self::StringPrototypeToLowerCase => StandardBuiltinId::StringPrototypeToLowerCase,
            Self::StringPrototypeToUpperCase => StandardBuiltinId::StringPrototypeToUpperCase,
            Self::StringPrototypeReplace => StandardBuiltinId::StringPrototypeReplace,
            Self::StringPrototypeReplaceAll => StandardBuiltinId::StringPrototypeReplaceAll,
            Self::DecodeUriComponent => StandardBuiltinId::DecodeUriComponent,
            Self::EncodeUriComponent => StandardBuiltinId::EncodeUriComponent,
            Self::JsonParse => StandardBuiltinId::JsonParse,
            Self::JsonStringify => StandardBuiltinId::JsonStringify,
            Self::MapPrototypeGet => StandardBuiltinId::MapPrototypeGet,
            Self::MapPrototypeHas => StandardBuiltinId::MapPrototypeHas,
            Self::MapPrototypeSet => StandardBuiltinId::MapPrototypeSet,
            Self::MapPrototypeDelete => StandardBuiltinId::MapPrototypeDelete,
            Self::SetPrototypeHas => StandardBuiltinId::SetPrototypeHas,
            Self::SetPrototypeAdd => StandardBuiltinId::SetPrototypeAdd,
            Self::SetPrototypeDelete => StandardBuiltinId::SetPrototypeDelete,
            Self::WeakMapPrototypeGet => StandardBuiltinId::WeakMapPrototypeGet,
            Self::WeakMapPrototypeHas => StandardBuiltinId::WeakMapPrototypeHas,
            Self::WeakMapPrototypeSet => StandardBuiltinId::WeakMapPrototypeSet,
        }
    }

    pub(crate) fn admits(builtin: Option<StandardBuiltinId>) -> bool {
        builtin.and_then(Self::from_builtin).is_some()
    }

    pub(crate) fn from_builtin_option(builtin: Option<StandardBuiltinId>) -> Option<Self> {
        builtin.and_then(Self::from_builtin)
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
            LocalArgumentsOwner::Native(native) => native.arity(),
        }
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.arity().slot_count()
    }

    pub(super) fn projects_arguments(&self) -> bool {
        match &self.owner {
            LocalArgumentsOwner::Source { proof, .. } => proof.projects_arguments(),
            LocalArgumentsOwner::Native(_) => false,
        }
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
