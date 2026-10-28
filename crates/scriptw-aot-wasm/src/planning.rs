use super::*;
mod module_graph;
use crate::control_flow::FOR_IN_INTRINSICS;
pub(crate) use module_graph::ModuleGraphPlan;
use scriptw_ir::{ArrayAccumulationElementIr, ArrayAccumulationIr};
use scriptw_ir::{
    ArrayDestructuringEvaluationIr, AsyncDisposableResourcesIr, AsyncResumeModeIr,
    ObjectDestructuringPatternIr, OptionalChainOperationIr, SyncDisposableScopeExecutionIr,
};
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone)]
pub(crate) struct WasmFunctionMeta {
    pub(crate) constant_leaf: Option<crate::functions::ConstantLeafPlan>,
    pub(crate) numeric_leaf: Option<crate::functions::NumericLeafPlan>,
    pub(crate) async_generator_resume_environment_plan:
        Option<scriptw_ir::AsyncGeneratorResumeEnvironmentPlanIr>,
    pub(crate) template_source: Option<scriptw_ir::TemplateSourceIr>,
    pub(crate) name: String,
    pub(crate) to_string_value: String,
    /// `Some` when this meta belongs to a standard builtin. Used to record
    /// which builtins get their function values materialized (or their bodies
    /// direct-called) during emission — see [`FunctionMetaRegistry`] — and for
    /// precise meta-to-builtin reverse lookups.
    pub(crate) standard_builtin: Option<StandardBuiltinId>,
    /// `Some` when this meta belongs to a host builtin. Same role as
    /// `standard_builtin`: host builtin bodies are stubbed unless the script
    /// references them, but they can also be reached dynamically (installed on
    /// a realm global by `__scriptwCreateRealm`, or direct-called from another
    /// builtin's body like `JSON.parse` -> `parseFloat`), so materializations
    /// and direct calls are recorded and force their real bodies.
    pub(crate) host_builtin: Option<HostBuiltinId>,
    pub(crate) length: u64,
    pub(crate) length_name_configurable: bool,
    pub(crate) entry: crate::function_entry::PlannedFunctionEntry,
    pub(crate) strict: bool,
    pub(crate) is_named_expression: bool,
    pub(crate) class_element_execution_kind: ClassElementExecutionKind,
    pub(crate) class_heritage_kind: ClassHeritageKind,
    pub(crate) is_static_class_member: bool,
    pub(crate) is_derived_constructor: bool,
    pub(crate) is_synthetic_default_derived_constructor: bool,
    pub(crate) class_instance_element_plan: Option<ClassInstanceElementPlanIr>,
    pub(crate) uses_super: bool,
    pub(crate) this_before_super: bool,
    pub(crate) captures_private_environment: bool,
    pub(crate) needs_active_function_identity: bool,
}

impl WasmFunctionMeta {
    pub(crate) fn protocol(&self) -> FunctionProtocolIr {
        self.entry.protocol()
    }

    pub(crate) fn runtime_name(&self) -> &str {
        if self.protocol().class_kind() != ClassFunctionKind::Method {
            return self.name.as_str();
        }
        self.name
            .split_once('.')
            .map_or(self.name.as_str(), |(_, method_name)| method_name)
    }

    pub(crate) fn has_class_execution_context(&self) -> bool {
        !matches!(self.protocol().class_kind(), ClassFunctionKind::None)
            || !matches!(
                self.class_element_execution_kind,
                ClassElementExecutionKind::None
            )
    }

    pub(crate) fn has_home_object_execution_context(&self) -> bool {
        self.has_class_execution_context() || self.protocol().is_object_literal_method()
    }
}

#[cfg(test)]
mod tests;

/// Every builtin the `Intl` namespace object depends on, declared exactly once.
///
/// Two independent obligations force a single list:
///
/// - `ScriptLowerer::intl_object_value_info` puts every
///   `INTL_NAMESPACE_CONSTRUCTORS` member on the `Intl` shape
///   *unconditionally*. An `Intl` object missing one of them contradicts the
///   shape the same program was compiled against, which is invisible to any
///   test that only reaches the member through a folded member expression and
///   visible to every test that reads it reflectively — that is
///   `intl402/DateTimeFormat/prop-desc.js`.
/// - `Intl.DateTimeFormat` is not a usable constructor without
///   `supportedLocalesOf`, its prototype accessors and its bound-format body,
///   and the five `Temporal.Plain*.prototype.toLocaleString` emitters look the
///   constructor and the format getter up by function id and return
///   `EmitError::unsupported` when either is missing.
///
/// `require_standard_builtin` does not recurse through its own match, so a
/// caller that needs the formatter must seed every id itself rather than
/// relying on one id dragging in the rest.
const INTL_NAMESPACE_ROOTS: [StandardBuiltinId; 77] = [
    StandardBuiltinId::IntlGetCanonicalLocales,
    StandardBuiltinId::IntlSupportedValuesOf,
    StandardBuiltinId::IntlLocaleConstructor,
    StandardBuiltinId::IntlLocalePrototypeLanguageGetter,
    StandardBuiltinId::IntlLocalePrototypeScriptGetter,
    StandardBuiltinId::IntlLocalePrototypeRegionGetter,
    StandardBuiltinId::IntlLocalePrototypeBaseNameGetter,
    StandardBuiltinId::IntlLocalePrototypeCalendarGetter,
    StandardBuiltinId::IntlLocalePrototypeCollationGetter,
    StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter,
    StandardBuiltinId::IntlLocalePrototypeHourCycleGetter,
    StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter,
    StandardBuiltinId::IntlLocalePrototypeNumericGetter,
    StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter,
    StandardBuiltinId::IntlLocalePrototypeVariantsGetter,
    StandardBuiltinId::IntlLocalePrototypeToString,
    StandardBuiltinId::IntlLocalePrototypeMaximize,
    StandardBuiltinId::IntlLocalePrototypeMinimize,
    StandardBuiltinId::IntlLocalePrototypeGetWeekInfo,
    StandardBuiltinId::IntlLocalePrototypeGetCalendars,
    StandardBuiltinId::IntlLocalePrototypeGetCollations,
    StandardBuiltinId::IntlLocalePrototypeGetTimeZones,
    StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems,
    StandardBuiltinId::IntlLocalePrototypeGetHourCycles,
    StandardBuiltinId::IntlLocalePrototypeGetTextInfo,
    StandardBuiltinId::IntlDateTimeFormatConstructor,
    StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf,
    StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions,
    StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter,
    StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts,
    StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange,
    StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts,
    StandardBuiltinId::IntlDateTimeFormatBoundFormat,
    StandardBuiltinId::IntlNumberFormatConstructor,
    StandardBuiltinId::IntlNumberFormatSupportedLocalesOf,
    StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions,
    StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter,
    StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts,
    StandardBuiltinId::IntlNumberFormatPrototypeFormatRange,
    StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts,
    StandardBuiltinId::IntlNumberFormatBoundFormat,
    StandardBuiltinId::IntlPluralRulesConstructor,
    StandardBuiltinId::IntlPluralRulesSupportedLocalesOf,
    StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions,
    StandardBuiltinId::IntlPluralRulesPrototypeSelect,
    StandardBuiltinId::IntlPluralRulesPrototypeSelectRange,
    StandardBuiltinId::IntlListFormatConstructor,
    StandardBuiltinId::IntlListFormatSupportedLocalesOf,
    StandardBuiltinId::IntlListFormatPrototypeResolvedOptions,
    StandardBuiltinId::IntlListFormatPrototypeFormat,
    StandardBuiltinId::IntlListFormatPrototypeFormatToParts,
    StandardBuiltinId::IntlCollatorConstructor,
    StandardBuiltinId::IntlCollatorSupportedLocalesOf,
    StandardBuiltinId::IntlCollatorPrototypeResolvedOptions,
    StandardBuiltinId::IntlCollatorPrototypeCompareGetter,
    StandardBuiltinId::IntlCollatorBoundCompare,
    StandardBuiltinId::IntlDisplayNamesConstructor,
    StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf,
    StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions,
    StandardBuiltinId::IntlDisplayNamesPrototypeOf,
    StandardBuiltinId::IntlRelativeTimeFormatConstructor,
    StandardBuiltinId::IntlDurationFormatConstructor,
    StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf,
    StandardBuiltinId::IntlDurationFormatSupportedLocalesOf,
    StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions,
    StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions,
    StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat,
    StandardBuiltinId::IntlDurationFormatPrototypeFormat,
    StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts,
    StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts,
    StandardBuiltinId::IntlSegmenterConstructor,
    StandardBuiltinId::IntlSegmenterSupportedLocalesOf,
    StandardBuiltinId::IntlSegmenterPrototypeSegment,
    StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions,
    StandardBuiltinId::IntlSegmentsPrototypeContaining,
    StandardBuiltinId::IntlSegmentsPrototypeIterator,
    StandardBuiltinId::IntlSegmentIteratorPrototypeNext,
];

/// `INTL_NAMESPACE_CONSTRUCTORS` ⊆ [`INTL_NAMESPACE_ROOTS`], checked by the
/// compiler rather than by a test.
///
/// This is the one link the two types cannot carry. [`IntlNamespacePlan::rooted`]
/// seeds `INTL_NAMESPACE_ROOTS` and [`IntlNamespacePlan::members`] hands out
/// `INTL_NAMESPACE_CONSTRUCTORS`, which lives in `scriptw-ir`; nothing relates
/// them, so [`IntlNamespaceMembers`]'s "every member is rooted" claim rests on
/// this containment alone. Adding `Intl.NumberFormat` to the shape list and
/// forgetting the root list is the plausible mistake, and it is not a missing
/// property: `init_intl_object` walks the member list with no per-member guard,
/// so an unseeded member becomes a `GlobalGet` on a global that is never
/// `GlobalSet`, paired with an unconditional `Function` tag — a callable with a
/// zero payload.
///
/// `StandardBuiltinId` is fieldless, so `as u32` is a total, `const`-evaluable
/// identity for it and the whole check runs at compile time.
const _: () = {
    let mut member = 0;
    while member < INTL_NAMESPACE_CONSTRUCTORS.len() {
        let needle = INTL_NAMESPACE_CONSTRUCTORS[member].1 as u32;
        let mut root = 0;
        let mut found = false;
        while root < INTL_NAMESPACE_ROOTS.len() {
            if INTL_NAMESPACE_ROOTS[root] as u32 == needle {
                found = true;
            }
            root += 1;
        }
        assert!(
            found,
            "an `Intl` namespace member declared on the IR shape is missing from \
             `INTL_NAMESPACE_ROOTS`, so a bare `Intl` reference would install it as a \
             `Function`-tagged value with a never-written payload"
        );
        member += 1;
    }
    let mut method = 0;
    while method < INTL_NAMESPACE_METHODS.len() {
        let needle = INTL_NAMESPACE_METHODS[method].1 as u32;
        let mut root = 0;
        let mut found = false;
        while root < INTL_NAMESPACE_ROOTS.len() {
            if INTL_NAMESPACE_ROOTS[root] as u32 == needle {
                found = true;
            }
            root += 1;
        }
        assert!(
            found,
            "an Intl namespace method is missing from its required roots"
        );
        method += 1;
    }
};

pub(crate) use intl_namespace::{IntlNamespaceMembers, IntlNamespacePlan};
pub(crate) use temporal_namespace::{TemporalNamespaceMembers, TemporalNamespacePlan};

/// The `Intl` namespace plan and its member-list witness, in a module of their
/// own.
///
/// The module boundary is the enforcement. Both types were previously declared
/// beside [`RuntimeBootstrapPlan`], where their doc comments claimed the
/// installed variant could only come from the seeding constructor — true for
/// every module *except* the 6,000-line one where the next namespace-rooting
/// arm actually gets written, and in which
/// `self.intl = IntlNamespacePlan::RootedWithDateTimeFormatFamily;` compiled
/// fine. Here the installed variant carries a payload whose field is private to
/// this module and [`IntlNamespaceMembers`] has no constructor at all, so
/// neither can be built anywhere else in the crate, `planning` included.
///
/// The failure that shape prevents is not a missing property. `init_intl_object`
/// walks the member list with no per-member guard (deliberately — the guard it
/// used to have silently `continue`d past unrooted members), emitting
/// `GlobalGet` for each constructor's global paired with an unconditional
/// `Function` tag. A global that is never `GlobalSet` still resolves, so an
/// unseeded member becomes a `Function`-tagged value with a never-written
/// payload: a bogus callable, which is worse than the absent property the old
/// guard produced.
mod intl_namespace;
mod temporal_namespace;

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeBootstrapPlan {
    shared: Arc<RuntimeBootstrapPlanData>,
}

impl std::ops::Deref for RuntimeBootstrapPlan {
    type Target = RuntimeBootstrapPlanData;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl std::ops::DerefMut for RuntimeBootstrapPlan {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.shared)
    }
}

#[cfg(test)]
#[path = "../tests/unit/bootstrap_plan_sharing.rs"]
mod bootstrap_plan_sharing_tests;

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeBootstrapPlanData {
    pub(crate) full_standard_globals: bool,
    pub(crate) standard_roots: BTreeSet<StandardBuiltinId>,
    pub(crate) reflect_object: bool,
    pub(crate) math_object: bool,
    pub(crate) json_object: bool,
    pub(crate) atomics_object: bool,
    /// Private because installing `Temporal` carries the obligation represented
    /// by [`RuntimeBootstrapPlan::temporal_namespace_members`].
    temporal: TemporalNamespacePlan,
    /// Deliberately private for the same reason as `temporal`: namespace
    /// installation carries a rooting obligation, and `planning` is the only
    /// module allowed to discharge it. Read it through
    /// [`RuntimeBootstrapPlan::intl_namespace_members`].
    intl: IntlNamespacePlan,
    /// Which builtins [`RuntimeBootstrapPlan::require_standard_builtin`] has
    /// already *walked*, as opposed to which ones are rooted.
    ///
    /// The dependency graph these arms describe has cycles in it, and one of
    /// them is reachable from a one-line program. `Temporal.PlainDateTime`'s
    /// arm requires `TemporalZonedDateTimeConstructor` (for `toZonedDateTime`)
    /// and `Temporal.ZonedDateTime`'s arm requires
    /// `TemporalPlainDateTimeConstructor` (for `toPlainDateTime`), so
    /// `print(typeof Temporal.ZonedDateTime)` recursed until it exhausted the
    /// engine's 64 MiB worker stack and aborted the process with SIGABRT. Top
    /// level `this` and `globalThis` reach it too, because they root every
    /// global; that is how a `var global = this;` case in
    /// `built-ins/Array/prototype/map` came to kill a full-suite sweep.
    ///
    /// This must NOT be replaced by guarding on `standard_roots` itself, however
    /// tempting the one-liner is. Several arms below add their dependencies with
    /// a bare `standard_roots.insert(dep)` rather than a recursive `require`, so
    /// a builtin can already be *rooted* without ever having been *walked*.
    /// Guarding on `standard_roots` would then skip that builtin's arm the first
    /// time anything genuinely required it, silently dropping every root the arm
    /// would have added — trading an abort for a wrong answer.
    ///
    /// Walking each builtin exactly once reaches the same fixpoint as walking it
    /// repeatedly: every effect in these arms is a set insertion or a `= true`
    /// flag, so they are idempotent and order-independent.
    walked: BTreeSet<StandardBuiltinId>,
}

impl RuntimeBootstrapPlan {
    /// The realm every heap-backed module bootstraps: every standard global,
    /// namespace and intrinsic. Nothing here reads the script, so the
    /// `RealmInitializeIntrinsics` helper is the same for every program; the
    /// script's own global bindings are installed by main.
    pub(crate) fn full() -> Self {
        // The closed catalogue plan contains no Script, Realm or Store state.
        // Builders share it immutably; partial-plan mutation remains copy-on-write.
        static FULL: OnceLock<RuntimeBootstrapPlan> = OnceLock::new();
        FULL.get_or_init(Self::build_full).clone()
    }

    fn build_full() -> Self {
        let mut plan = Self::default();
        plan.full_standard_globals = true;
        for builtin in StandardBuiltinId::all_functions() {
            plan.require_standard_builtin(*builtin);
        }
        plan.require_standard_builtin(StandardBuiltinId::AbstractModuleSourceConstructor);
        plan.require_standard_builtin(StandardBuiltinId::PromiseConstructor);
        plan.require_foundational_roots();
        plan
    }

    pub(crate) fn should_initialize_standard_builtin(&self, builtin: StandardBuiltinId) -> bool {
        self.full_standard_globals || self.standard_roots.contains(&builtin)
    }

    pub(crate) fn should_install_script_global_binding(
        &self,
        initializer: &GlobalPropertyInitializerIr,
    ) -> bool {
        match initializer {
            GlobalPropertyInitializerIr::Intrinsic
            | GlobalPropertyInitializerIr::Infinity
            | GlobalPropertyInitializerIr::NaN
            | GlobalPropertyInitializerIr::Undefined
            | GlobalPropertyInitializerIr::FreshUndefined
            | GlobalPropertyInitializerIr::SourceFunction(_)
            | GlobalPropertyInitializerIr::HostFunction(_) => true,
            GlobalPropertyInitializerIr::ReflectObject => {
                self.full_standard_globals || self.reflect_object
            }
            GlobalPropertyInitializerIr::MathObject => {
                self.full_standard_globals || self.math_object
            }
            GlobalPropertyInitializerIr::JsonObject => {
                self.full_standard_globals || self.json_object
            }
            GlobalPropertyInitializerIr::AtomicsObject => {
                self.full_standard_globals || self.atomics_object
            }
            GlobalPropertyInitializerIr::TemporalObject => {
                self.temporal_namespace_members().is_some()
            }
            GlobalPropertyInitializerIr::IntlObject => self.intl_namespace_members().is_some(),
            GlobalPropertyInitializerIr::BuiltinFunction(builtin) => {
                self.should_initialize_standard_builtin(*builtin)
            }
        }
    }

    /// The `Intl` namespace members to install, or `None` when this program
    /// never gets an `Intl` object.
    ///
    /// This is the single gate `builtins::bootstrap` consults: it calls
    /// `init_intl_object` only while holding the returned witness, and the
    /// witness is the only source of the member list. There is deliberately no
    /// bool accessor beside it — a caller that can ask "is `Intl` installed?"
    /// without receiving the proof is a caller that can install a partial one.
    pub(crate) fn intl_namespace_members(&self) -> Option<IntlNamespaceMembers> {
        self.intl.members(self.full_standard_globals)
    }

    /// The complete `Temporal` namespace member witness, or `None` when this
    /// plan emits no `Temporal` object.
    pub(crate) fn temporal_namespace_members(&self) -> Option<TemporalNamespaceMembers> {
        self.temporal.members(self.full_standard_globals)
    }

    pub(crate) fn needs_typed_array_intrinsic(&self) -> bool {
        self.full_standard_globals
            || self.standard_roots.iter().any(|builtin| {
                *builtin == StandardBuiltinId::TypedArrayConstructor
                    || is_typed_array_constructor(*builtin)
            })
    }

    fn require_foundational_roots(&mut self) {
        for builtin in [
            // Every ordinary callable creates an Arguments object whose own
            // @@iterator is the realm's original %Array.prototype.values%.
            StandardBuiltinId::ArrayPrototypeValues,
            StandardBuiltinId::FunctionConstructor,
            StandardBuiltinId::EvalFunction,
            StandardBuiltinId::ObjectConstructor,
            // Global declaration facts and the completed ObjectRead/
            // ObjectReadProxy Get core use the actual descriptor facade for
            // ordinary, exotic and Proxy receivers. Retain its real body and
            // installer before pooling or helper emission, even without a
            // source-level descriptor reference.
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
            // Receiver Set delegates to its complete native [[DefineOwnProperty]]
            // owner, including Proxy and indexed exotic receivers.
            StandardBuiltinId::ReflectDefineProperty,
            // CopyDataProperties and Proxy own-key validation call the recorded
            // native OwnKeys boundary before publishing the checked key list.
            StandardBuiltinId::ReflectOwnKeys,
            StandardBuiltinId::ErrorConstructor,
            StandardBuiltinId::EvalErrorConstructor,
            StandardBuiltinId::AggregateErrorConstructor,
            StandardBuiltinId::SuppressedErrorConstructor,
            StandardBuiltinId::RangeErrorConstructor,
            StandardBuiltinId::SyntaxErrorConstructor,
            StandardBuiltinId::TypeErrorConstructor,
            StandardBuiltinId::URIErrorConstructor,
            StandardBuiltinId::ReferenceErrorConstructor,
        ] {
            self.require_standard_builtin(builtin);
        }
    }

    /// Root every represented Intl family and mark the namespace object
    /// as installed.
    ///
    /// Both effects come from one assignment because [`IntlNamespacePlan`] has
    /// no other way to reach its installed state: the seeding happens inside the
    /// only constructor of that variant. See [`INTL_NAMESPACE_ROOTS`] for why
    /// the family is all-or-nothing.
    fn require_intl_namespace(&mut self) {
        self.intl = IntlNamespacePlan::rooted(&mut self.standard_roots);
    }

    /// Root both IR-advertised Temporal namespace levels before making the
    /// namespace available to bootstrap.
    fn require_temporal_namespace(&mut self) {
        TemporalNamespacePlan::root(self);
    }

    fn require_standard_builtin(&mut self, builtin: StandardBuiltinId) {
        self.standard_roots.insert(builtin);
        // Rooting is unconditional above; *walking* happens at most once. See
        // the `walked` field's documentation for why the guard cannot live on
        // `standard_roots`, and for the cycle that makes it necessary at all.
        if !self.walked.insert(builtin) {
            return;
        }
        if builtin == StandardBuiltinId::AbstractModuleSourceConstructor {
            self.require_standard_builtin(
                StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter,
            );
        }
        if matches!(
            builtin,
            StandardBuiltinId::ShadowRealmConstructor
                | StandardBuiltinId::ShadowRealmPrototypeEvaluate
                | StandardBuiltinId::ShadowRealmPrototypeImportValue
                | StandardBuiltinId::ShadowRealmWrappedFunctionCall
                | StandardBuiltinId::ShadowRealmImportFulfilled
                | StandardBuiltinId::ShadowRealmImportRejected
        ) {
            for dependency in [
                StandardBuiltinId::ShadowRealmConstructor,
                StandardBuiltinId::ShadowRealmPrototypeEvaluate,
                StandardBuiltinId::ShadowRealmPrototypeImportValue,
                StandardBuiltinId::ShadowRealmWrappedFunctionCall,
                StandardBuiltinId::ShadowRealmImportFulfilled,
                StandardBuiltinId::ShadowRealmImportRejected,
                StandardBuiltinId::ObjectGetOwnPropertyDescriptor,
                StandardBuiltinId::PromiseConstructor,
            ] {
                self.require_standard_builtin(dependency);
            }
        }
        if matches!(
            builtin,
            StandardBuiltinId::TypedArrayPrototypeMap
                | StandardBuiltinId::TypedArrayPrototypeFilter
                | StandardBuiltinId::TypedArrayPrototypeSlice
                | StandardBuiltinId::TypedArrayPrototypeSubarray
                | StandardBuiltinId::TypedArrayPrototypeToReversed
                | StandardBuiltinId::TypedArrayPrototypeToSorted
                | StandardBuiltinId::TypedArrayPrototypeWith
        ) {
            // A borrowed species or SameType method may receive any kind from
            // another Realm, even when its own globals never name that kind.
            // Its defining-Realm default therefore requires every immutable
            // constructor slot and the installer that publishes it.
            for kind in TypedArrayElementKind::ALL {
                self.require_standard_builtin(kind.constructor());
            }
        }
        if builtin == StandardBuiltinId::IntlLocalePrototypeGetWeekInfo {
            self.require_standard_builtin(StandardBuiltinId::ArrayConstructor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::StringPrototypeLocaleCompare
                | StandardBuiltinId::NumberPrototypeToLocaleString
                | StandardBuiltinId::BigIntPrototypeToLocaleString
        ) {
            self.require_intl_namespace();
        }
        if builtin == StandardBuiltinId::Uint8ArrayConstructor
            || scriptw_ir::UINT8_ARRAY_CODEC_STATIC_MEMBERS
                .iter()
                .chain(scriptw_ir::UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS.iter())
                .any(|&(_, codec)| codec == builtin)
        {
            self.require_standard_builtin(StandardBuiltinId::Uint8ArrayConstructor);
            for &(_, codec) in scriptw_ir::UINT8_ARRAY_CODEC_STATIC_MEMBERS
                .iter()
                .chain(scriptw_ir::UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS.iter())
            {
                self.require_standard_builtin(codec);
            }
        }
        if builtin == StandardBuiltinId::FunctionConstructor {
            // `%Function.prototype%` is a callable intrinsic, not an Object
            // shell, and its non-configurable `@@hasInstance` property publishes
            // another exact function value. Root both bodies whenever the
            // foundational Function family is present so bootstrap cannot
            // publish either a stubbed call target or a half-installed
            // prototype.
            for dependency in [
                StandardBuiltinId::FunctionPrototype,
                StandardBuiltinId::FunctionPrototypeSymbolHasInstance,
            ] {
                self.require_standard_builtin(dependency);
            }
        }
        if builtin == StandardBuiltinId::DisposableStackConstructor {
            // The constructor installer publishes the complete prototype as one
            // intrinsic unit. Root every function value it reads so an emitted
            // constructor can never observe a half-installed prototype.
            for dependency in [
                StandardBuiltinId::DisposableStackPrototypeUse,
                StandardBuiltinId::DisposableStackPrototypeAdopt,
                StandardBuiltinId::DisposableStackPrototypeDefer,
                StandardBuiltinId::DisposableStackPrototypeMove,
                StandardBuiltinId::DisposableStackPrototypeDispose,
                StandardBuiltinId::DisposableStackPrototypeDisposedGetter,
            ] {
                self.require_standard_builtin(dependency);
            }
        }
        if builtin == StandardBuiltinId::ErrorConstructor {
            self.require_standard_builtin(StandardBuiltinId::ErrorPrototypeStackGetter);
            self.require_standard_builtin(StandardBuiltinId::ErrorPrototypeStackSetter);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ErrorPrototypeStackGetter
                | StandardBuiltinId::ErrorPrototypeStackSetter
        ) {
            self.require_standard_builtin(StandardBuiltinId::ErrorConstructor);
        }
        if builtin == StandardBuiltinId::DisposableStackPrototypeDispose {
            // DisposeResources constructs this intrinsic when a later disposer
            // suppresses an earlier abrupt completion.
            self.require_standard_builtin(StandardBuiltinId::SuppressedErrorConstructor);
        }
        if builtin == StandardBuiltinId::ArrayFromAsync {
            self.standard_roots
                .insert(StandardBuiltinId::ArrayConstructor);
            for dependency in [
                StandardBuiltinId::ArrayFromAsyncFulfilled,
                StandardBuiltinId::ArrayFromAsyncRejected,
                StandardBuiltinId::PromiseResolve,
            ] {
                self.require_standard_builtin(dependency);
            }
        }
        if matches!(
            builtin,
            StandardBuiltinId::RegExpConstructor
                | StandardBuiltinId::StringPrototypeMatchAll
                | StandardBuiltinId::RegExpPrototypeSymbolMatchAll
        ) {
            self.require_standard_builtin(StandardBuiltinId::RegExpStringIteratorNext);
        }
        if builtin == StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose {
            self.require_standard_builtin(
                StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled,
            );
        }
        if is_typed_array_constructor(builtin) {
            self.require_standard_builtin(StandardBuiltinId::TypedArrayConstructor);
            // Every TypedArray owns an observable ArrayBuffer: AllocateTypedArray
            // reaches AllocateArrayBuffer(%ArrayBuffer%, ...), whose
            // OrdinaryCreateFromConstructor gives the buffer
            // %ArrayBuffer.prototype% (23.2.5.1.1, 25.1.3.1). The ArrayBuffer
            // constructor's installer is the only writer of that prototype's
            // members, so without it `view.buffer.detached` read through
            // ordinary [[Get]] finds no accessor and answers `undefined`.
            self.require_standard_builtin(StandardBuiltinId::ArrayBufferConstructor);
            for iterator_builtin in [
                StandardBuiltinId::ArrayPrototypeValues,
                StandardBuiltinId::TypedArrayPrototypeIncludes,
                StandardBuiltinId::TypedArrayPrototypeIndexOf,
                StandardBuiltinId::TypedArrayPrototypeLastIndexOf,
                StandardBuiltinId::TypedArrayPrototypeFind,
                StandardBuiltinId::TypedArrayPrototypeFindIndex,
                StandardBuiltinId::TypedArrayPrototypeFindLast,
                StandardBuiltinId::TypedArrayPrototypeFindLastIndex,
                StandardBuiltinId::TypedArrayPrototypeEvery,
                StandardBuiltinId::TypedArrayPrototypeSome,
                StandardBuiltinId::TypedArrayPrototypeMap,
                StandardBuiltinId::TypedArrayPrototypeFilter,
                StandardBuiltinId::TypedArrayPrototypeForEach,
                StandardBuiltinId::TypedArrayPrototypeReduce,
                StandardBuiltinId::TypedArrayPrototypeReduceRight,
                StandardBuiltinId::TypedArrayPrototypeValues,
                StandardBuiltinId::TypedArrayPrototypeKeys,
                StandardBuiltinId::TypedArrayPrototypeEntries,
                StandardBuiltinId::ArrayIteratorNext,
                StandardBuiltinId::ArrayIteratorIdentity,
            ] {
                self.require_standard_builtin(iterator_builtin);
            }
        }
        if builtin == StandardBuiltinId::ObjectConstructor {
            self.require_standard_builtin(StandardBuiltinId::ObjectGroupBy);
            self.require_standard_builtin(StandardBuiltinId::ObjectFromEntries);
            self.require_standard_builtin(StandardBuiltinId::ObjectAssign);
            self.require_standard_builtin(StandardBuiltinId::ObjectPrototypeDefineGetter);
            self.require_standard_builtin(StandardBuiltinId::ObjectPrototypeDefineSetter);
            self.require_standard_builtin(StandardBuiltinId::ObjectGetOwnPropertyDescriptors);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ObjectPrototypeDefineGetter
                | StandardBuiltinId::ObjectPrototypeDefineSetter
        ) {
            self.require_standard_builtin(StandardBuiltinId::ObjectDefineProperty);
        }
        if matches!(
            builtin,
            StandardBuiltinId::MapConstructor
                | StandardBuiltinId::WeakMapConstructor
                | StandardBuiltinId::WeakSetConstructor
                | StandardBuiltinId::SetConstructor
                | StandardBuiltinId::ObjectGroupBy
                | StandardBuiltinId::ObjectFromEntries
                | StandardBuiltinId::MathSumPrecise
        ) {
            if builtin == StandardBuiltinId::MapConstructor {
                self.require_standard_builtin(StandardBuiltinId::MapGroupBy);
                self.require_standard_builtin(StandardBuiltinId::MapSpeciesGetter);
            }
            if builtin == StandardBuiltinId::SetConstructor {
                self.require_standard_builtin(StandardBuiltinId::SetSpeciesGetter);
            }
            for iterator_builtin in [
                StandardBuiltinId::ArrayPrototypeValues,
                StandardBuiltinId::ArrayIteratorNext,
                StandardBuiltinId::ArrayIteratorIdentity,
                StandardBuiltinId::StringPrototypeIterator,
                StandardBuiltinId::StringIteratorNext,
            ] {
                self.require_standard_builtin(iterator_builtin);
            }
        }
        if builtin == StandardBuiltinId::TypedArrayPrototypeToString {
            // The generic shared Array/TypedArray toString entry calls the
            // actual Object.prototype.toString native when Get(join) is noncallable.
            self.require_standard_builtin(StandardBuiltinId::ObjectPrototypeToString);
        }
        if builtin.string_prototype_method_name().is_some()
            || matches!(
                builtin,
                StandardBuiltinId::StringFromCharCode
                    | StandardBuiltinId::StringFromCodePoint
                    | StandardBuiltinId::StringRaw
            )
        {
            // String.prototype methods are installed by the String
            // constructor bootstrap block. Dynamic method calls can root the
            // method body without otherwise rooting that installer.
            self.standard_roots
                .insert(StandardBuiltinId::StringConstructor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ArrayPrototypeConcat
                | StandardBuiltinId::ArrayPrototypeJoin
                | StandardBuiltinId::ArrayPrototypeSlice
                | StandardBuiltinId::ArrayPrototypeSplice
                | StandardBuiltinId::ArrayPrototypeFill
                | StandardBuiltinId::ArrayPrototypeSort
                | StandardBuiltinId::TypedArrayPrototypeToString
                | StandardBuiltinId::ArrayPrototypeToLocaleString
                | StandardBuiltinId::ArrayPrototypeFlat
                | StandardBuiltinId::ArrayPrototypeFlatMap
                | StandardBuiltinId::ArrayPrototypeAt
                | StandardBuiltinId::TypedArrayPrototypeAt
                | StandardBuiltinId::TypedArrayPrototypeCopyWithin
                | StandardBuiltinId::TypedArrayPrototypeIncludes
                | StandardBuiltinId::TypedArrayPrototypeIndexOf
                | StandardBuiltinId::TypedArrayPrototypeLastIndexOf
                | StandardBuiltinId::TypedArrayPrototypeFind
                | StandardBuiltinId::TypedArrayPrototypeFindIndex
                | StandardBuiltinId::TypedArrayPrototypeFindLast
                | StandardBuiltinId::TypedArrayPrototypeFindLastIndex
                | StandardBuiltinId::TypedArrayPrototypeEvery
                | StandardBuiltinId::TypedArrayPrototypeSome
                | StandardBuiltinId::TypedArrayPrototypeMap
                | StandardBuiltinId::TypedArrayPrototypeFilter
                | StandardBuiltinId::TypedArrayPrototypeForEach
                | StandardBuiltinId::TypedArrayPrototypeReduce
                | StandardBuiltinId::TypedArrayPrototypeReduceRight
                | StandardBuiltinId::TypedArrayPrototypeValues
                | StandardBuiltinId::TypedArrayPrototypeKeys
                | StandardBuiltinId::TypedArrayPrototypeEntries
                | StandardBuiltinId::ArrayPrototypeToReversed
                | StandardBuiltinId::ArrayPrototypeToSpliced
                | StandardBuiltinId::ArrayPrototypeToSorted
                | StandardBuiltinId::ArrayPrototypeWith
                | StandardBuiltinId::ArrayPrototypeReverse
                | StandardBuiltinId::ArrayPrototypeCopyWithin
                | StandardBuiltinId::ArrayPrototypeIncludes
                | StandardBuiltinId::ArrayPrototypeIndexOf
                | StandardBuiltinId::ArrayPrototypeLastIndexOf
                | StandardBuiltinId::ArrayPrototypeFind
                | StandardBuiltinId::ArrayPrototypeFindIndex
                | StandardBuiltinId::ArrayPrototypeFindLast
                | StandardBuiltinId::ArrayPrototypeFindLastIndex
                | StandardBuiltinId::ArrayPrototypeEvery
                | StandardBuiltinId::ArrayPrototypeSome
                | StandardBuiltinId::ArrayPrototypeForEach
                | StandardBuiltinId::ArrayPrototypeFilter
                | StandardBuiltinId::ArrayPrototypeMap
                | StandardBuiltinId::ArrayPrototypeReduce
                | StandardBuiltinId::ArrayPrototypeReduceRight
                | StandardBuiltinId::ArrayPrototypePop
                | StandardBuiltinId::ArrayPrototypePush
                | StandardBuiltinId::ArrayPrototypeShift
                | StandardBuiltinId::ArrayPrototypeUnshift
                | StandardBuiltinId::ArrayPrototypeKeys
                | StandardBuiltinId::ArrayPrototypeEntries
                | StandardBuiltinId::ArrayPrototypeValues
        ) {
            // These properties are all installed by the Array constructor's
            // bootstrap block. A dynamic GetV can make a method body reachable
            // without otherwise referencing `Array`, so root the installer as
            // well as the body.
            self.standard_roots
                .insert(StandardBuiltinId::ArrayConstructor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ArrayPrototypeFlatMap
                | StandardBuiltinId::ArrayPrototypeMap
                | StandardBuiltinId::ArrayPrototypeFilter
                | StandardBuiltinId::ArrayPrototypeSlice
                | StandardBuiltinId::ArrayPrototypeSplice
        ) {
            // These algorithms create result properties through the shared
            // descriptor path, even when user code never references Object.
            self.require_standard_builtin(StandardBuiltinId::ObjectDefineProperty);
        }
        if builtin == StandardBuiltinId::IteratorZipKeyed {
            self.require_standard_builtin(StandardBuiltinId::ReflectOwnKeys);
            self.require_standard_builtin(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ObjectAssign
                | StandardBuiltinId::ObjectKeys
                | StandardBuiltinId::ObjectEntries
                | StandardBuiltinId::ObjectGetOwnPropertyDescriptors
                | StandardBuiltinId::ObjectValues
        ) {
            self.require_standard_builtin(StandardBuiltinId::ReflectOwnKeys);
            self.require_standard_builtin(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
        }
        if builtin == StandardBuiltinId::ObjectAssign {
            self.require_standard_builtin(StandardBuiltinId::ReflectSet);
        }
        if builtin == StandardBuiltinId::ObjectHasOwn {
            self.require_standard_builtin(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ReflectApply
                | StandardBuiltinId::ReflectConstruct
                | StandardBuiltinId::FunctionPrototypeApply
        ) {
            self.require_standard_builtin(StandardBuiltinId::ProxyConstructor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::JsonParse | StandardBuiltinId::JsonStringify
        ) {
            // Reviver/serialization snapshot own keys and descriptors; fresh
            // parser/root publications use the same real definition owner.
            self.require_standard_builtin(StandardBuiltinId::ReflectOwnKeys);
            self.require_standard_builtin(StandardBuiltinId::ObjectGetOwnPropertyDescriptor);
            self.require_standard_builtin(StandardBuiltinId::ReflectDefineProperty);
        }
        if builtin == StandardBuiltinId::IteratorZipKeyed {
            // Keyed acquisition snapshots enumerable own descriptors before
            // Get; resume publishes each result through real DefineOwnProperty.
            self.require_standard_builtin(StandardBuiltinId::ReflectOwnKeys);
            self.require_standard_builtin(StandardBuiltinId::ObjectGetOwnPropertyDescriptor);
            self.require_standard_builtin(StandardBuiltinId::ReflectDefineProperty);
        }
        if builtin == StandardBuiltinId::ReflectDefineProperty {
            self.require_standard_builtin(StandardBuiltinId::ObjectDefineProperty);
        }
        if builtin == StandardBuiltinId::ObjectCreate {
            self.require_standard_builtin(StandardBuiltinId::ObjectDefineProperties);
        }
        if builtin == StandardBuiltinId::ObjectDefineProperties {
            self.require_standard_builtin(StandardBuiltinId::ObjectDefineProperty);
            self.require_standard_builtin(StandardBuiltinId::ReflectOwnKeys);
            self.require_standard_builtin(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
            self.require_standard_builtin(StandardBuiltinId::ReflectDefineProperty);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ObjectSeal
                | StandardBuiltinId::ObjectFreeze
                | StandardBuiltinId::ObjectIsSealed
                | StandardBuiltinId::ObjectIsFrozen
        ) {
            self.require_standard_builtin(StandardBuiltinId::ReflectOwnKeys);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ObjectSeal | StandardBuiltinId::ObjectFreeze
        ) {
            self.require_standard_builtin(StandardBuiltinId::ReflectDefineProperty);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ObjectFreeze
                | StandardBuiltinId::ObjectIsSealed
                | StandardBuiltinId::ObjectIsFrozen
        ) {
            self.require_standard_builtin(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
        }
        if builtin == StandardBuiltinId::TemporalDurationPrototypeToLocaleString {
            // The private initializer shares the actual DurationFormat option
            // and locale algorithm, including its canonical-locale dependencies.
            self.require_standard_builtin(StandardBuiltinId::IntlDurationFormatConstructor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::IntlDurationFormatPrototypeFormat
                | StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts
        ) {
            // String duration input delegates to the actual saved Temporal
            // conversion. The existing visited-entry guard closes the reverse
            // Duration.toLocaleString -> DurationFormat dependency cycle.
            self.require_standard_builtin(StandardBuiltinId::TemporalDurationFrom);
        }
        if builtin == StandardBuiltinId::DatePrototypeToTemporalInstant {
            self.require_standard_builtin(StandardBuiltinId::TemporalInstantConstructor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::ProxyConstructor
                | StandardBuiltinId::FunctionPrototypeBind
                | StandardBuiltinId::ReflectGet
                | StandardBuiltinId::ReflectSet
                | StandardBuiltinId::ReflectHas
                | StandardBuiltinId::ReflectDeleteProperty
                | StandardBuiltinId::ObjectDefineProperty
                | StandardBuiltinId::ReflectDefineProperty
                | StandardBuiltinId::ReflectGetOwnPropertyDescriptor
                | StandardBuiltinId::ReflectOwnKeys
                | StandardBuiltinId::ObjectGetOwnPropertyNames
                | StandardBuiltinId::ObjectGetOwnPropertySymbols
                | StandardBuiltinId::ObjectPrototypeHasOwnProperty
                | StandardBuiltinId::ObjectPrototypeLookupGetter
                | StandardBuiltinId::ObjectPrototypeLookupSetter
                | StandardBuiltinId::ObjectPrototypePropertyIsEnumerable
        ) {
            // These entry points dispatch through the generic descriptor
            // builtin so exotic and nested-Proxy [[GetOwnProperty]] semantics
            // are observed exactly once. Keep its real body rooted rather than
            // calling a lazy stub.
            self.require_standard_builtin(StandardBuiltinId::ObjectGetOwnPropertyDescriptor);
        }
        if matches!(
            builtin,
            StandardBuiltinId::IteratorPrototypeConstructorSetter
                | StandardBuiltinId::IteratorPrototypeToStringTagSetter
                | StandardBuiltinId::ErrorPrototypeStackSetter
        ) {
            // SetterThatIgnoresPrototypeProperties performs
            // [[GetOwnProperty]] through the descriptor builtin and
            // CreateDataPropertyOrThrow through Object.defineProperty, the
            // generic bodies that own every exotic and Proxy receiver.
            self.require_standard_builtin(StandardBuiltinId::ObjectGetOwnPropertyDescriptor);
            self.require_standard_builtin(StandardBuiltinId::ObjectDefineProperty);
        }
        match builtin {
            StandardBuiltinId::FunctionPrototypeSymbolHasInstance => {
                // This body is installed only by the Function constructor
                // intrinsic family. Walking back to its owner closes direct IR
                // references without leaving property installation optional.
                self.require_standard_builtin(StandardBuiltinId::FunctionConstructor);
            }
            StandardBuiltinId::ObjectGroupBy | StandardBuiltinId::ObjectFromEntries => {
                self.standard_roots
                    .insert(StandardBuiltinId::ObjectConstructor);
            }
            StandardBuiltinId::ObjectConstructor => {
                // Object boxing can expose a Symbol wrapper to ToPrimitive.
                // Keep the real Symbol.prototype @@toPrimitive installed so
                // the default hook (and any own-property override) remains
                // observable through dynamic lookup.
                self.require_standard_builtin(StandardBuiltinId::SymbolPrototypeToPrimitive);
                self.require_standard_builtin(StandardBuiltinId::ObjectPrototypeProtoGetter);
                self.require_standard_builtin(StandardBuiltinId::ObjectPrototypeProtoSetter);
            }
            StandardBuiltinId::ObjectPrototypeToLocaleString => {
                // Invoke(this, "toString") can resolve through any primitive
                // prototype without a statically visible method call.
                for builtin in [
                    StandardBuiltinId::StringPrototypeToString,
                    StandardBuiltinId::NumberPrototypeToString,
                    StandardBuiltinId::BooleanPrototypeToString,
                    StandardBuiltinId::SymbolPrototypeToString,
                    StandardBuiltinId::BigIntPrototypeToString,
                ] {
                    self.require_standard_builtin(builtin);
                }
            }
            StandardBuiltinId::ReflectConstruct
            | StandardBuiltinId::ReflectApply
            | StandardBuiltinId::ReflectGet
            | StandardBuiltinId::ReflectGetPrototypeOf
            | StandardBuiltinId::ReflectGetOwnPropertyDescriptor
            | StandardBuiltinId::ReflectSet
            | StandardBuiltinId::ReflectHas
            | StandardBuiltinId::ReflectDefineProperty
            | StandardBuiltinId::ReflectDeleteProperty
            | StandardBuiltinId::ReflectIsExtensible
            | StandardBuiltinId::ReflectPreventExtensions
            | StandardBuiltinId::ReflectSetPrototypeOf
            | StandardBuiltinId::ReflectOwnKeys => self.reflect_object = true,
            StandardBuiltinId::MathAbs
            | StandardBuiltinId::MathAcos
            | StandardBuiltinId::MathAcosh
            | StandardBuiltinId::MathAsin
            | StandardBuiltinId::MathAsinh
            | StandardBuiltinId::MathAtan
            | StandardBuiltinId::MathAtan2
            | StandardBuiltinId::MathAtanh
            | StandardBuiltinId::MathCbrt
            | StandardBuiltinId::MathCeil
            | StandardBuiltinId::MathClz32
            | StandardBuiltinId::MathCos
            | StandardBuiltinId::MathCosh
            | StandardBuiltinId::MathExp
            | StandardBuiltinId::MathExpm1
            | StandardBuiltinId::MathF16Round
            | StandardBuiltinId::MathFloor
            | StandardBuiltinId::MathFround
            | StandardBuiltinId::MathHypot
            | StandardBuiltinId::MathImul
            | StandardBuiltinId::MathLog
            | StandardBuiltinId::MathLog10
            | StandardBuiltinId::MathLog1p
            | StandardBuiltinId::MathLog2
            | StandardBuiltinId::MathMax
            | StandardBuiltinId::MathMin
            | StandardBuiltinId::MathPow
            | StandardBuiltinId::MathRandom
            | StandardBuiltinId::MathRound
            | StandardBuiltinId::MathSign
            | StandardBuiltinId::MathSin
            | StandardBuiltinId::MathSinh
            | StandardBuiltinId::MathSqrt
            | StandardBuiltinId::MathSumPrecise
            | StandardBuiltinId::MathTan
            | StandardBuiltinId::MathTanh
            | StandardBuiltinId::MathTrunc => self.math_object = true,
            StandardBuiltinId::JsonParse
            | StandardBuiltinId::JsonStringify
            | StandardBuiltinId::JsonRawJson
            | StandardBuiltinId::JsonIsRawJson => self.json_object = true,
            StandardBuiltinId::AtomicsAdd
            | StandardBuiltinId::AtomicsAnd
            | StandardBuiltinId::AtomicsCompareExchange
            | StandardBuiltinId::AtomicsExchange
            | StandardBuiltinId::AtomicsLoad
            | StandardBuiltinId::AtomicsNotify
            | StandardBuiltinId::AtomicsOr
            | StandardBuiltinId::AtomicsPause
            | StandardBuiltinId::AtomicsStore
            | StandardBuiltinId::AtomicsSub
            | StandardBuiltinId::AtomicsWait
            | StandardBuiltinId::AtomicsXor
            | StandardBuiltinId::AtomicsIsLockFree => self.atomics_object = true,
            StandardBuiltinId::AtomicsWaitAsync => {
                self.atomics_object = true;
                self.standard_roots
                    .insert(StandardBuiltinId::PromiseConstructor);
                self.standard_roots
                    .insert(StandardBuiltinId::PromiseSpeciesGetter);
            }
            StandardBuiltinId::IntlSupportedValuesOf
            | StandardBuiltinId::IntlGetCanonicalLocales
            | StandardBuiltinId::IntlLocaleConstructor
            | StandardBuiltinId::IntlLocalePrototypeLanguageGetter
            | StandardBuiltinId::IntlLocalePrototypeScriptGetter
            | StandardBuiltinId::IntlLocalePrototypeRegionGetter
            | StandardBuiltinId::IntlLocalePrototypeBaseNameGetter
            | StandardBuiltinId::IntlLocalePrototypeCalendarGetter
            | StandardBuiltinId::IntlLocalePrototypeCollationGetter
            | StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter
            | StandardBuiltinId::IntlLocalePrototypeHourCycleGetter
            | StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter
            | StandardBuiltinId::IntlLocalePrototypeNumericGetter
            | StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter
            | StandardBuiltinId::IntlLocalePrototypeVariantsGetter
            | StandardBuiltinId::IntlLocalePrototypeToString
            | StandardBuiltinId::IntlLocalePrototypeMaximize
            | StandardBuiltinId::IntlLocalePrototypeMinimize
            | StandardBuiltinId::IntlLocalePrototypeGetWeekInfo
            | StandardBuiltinId::IntlLocalePrototypeGetCalendars
            | StandardBuiltinId::IntlLocalePrototypeGetCollations
            | StandardBuiltinId::IntlLocalePrototypeGetTimeZones
            | StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems
            | StandardBuiltinId::IntlLocalePrototypeGetHourCycles
            | StandardBuiltinId::IntlLocalePrototypeGetTextInfo
            | StandardBuiltinId::IntlDateTimeFormatConstructor
            | StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf
            | StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts
            | StandardBuiltinId::IntlDateTimeFormatBoundFormat
            | StandardBuiltinId::IntlNumberFormatConstructor
            | StandardBuiltinId::IntlNumberFormatSupportedLocalesOf
            | StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatRange
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts
            | StandardBuiltinId::IntlNumberFormatBoundFormat
            | StandardBuiltinId::IntlPluralRulesConstructor
            | StandardBuiltinId::IntlPluralRulesSupportedLocalesOf
            | StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions
            | StandardBuiltinId::IntlListFormatConstructor
            | StandardBuiltinId::IntlListFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlCollatorConstructor
            | StandardBuiltinId::IntlCollatorPrototypeResolvedOptions
            | StandardBuiltinId::IntlCollatorPrototypeCompareGetter
            | StandardBuiltinId::IntlPluralRulesPrototypeSelect
            | StandardBuiltinId::IntlListFormatSupportedLocalesOf
            | StandardBuiltinId::IntlCollatorSupportedLocalesOf
            | StandardBuiltinId::IntlListFormatPrototypeFormat
            | StandardBuiltinId::IntlListFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlCollatorBoundCompare
            | StandardBuiltinId::IntlDisplayNamesConstructor
            | StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf
            | StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions
            | StandardBuiltinId::IntlDisplayNamesPrototypeOf
            | StandardBuiltinId::IntlRelativeTimeFormatConstructor
            | StandardBuiltinId::IntlDurationFormatConstructor
            | StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf
            | StandardBuiltinId::IntlDurationFormatSupportedLocalesOf
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat
            | StandardBuiltinId::IntlDurationFormatPrototypeFormat
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlSegmenterConstructor
            | StandardBuiltinId::IntlSegmenterSupportedLocalesOf
            | StandardBuiltinId::IntlSegmenterPrototypeSegment
            | StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions
            | StandardBuiltinId::IntlSegmentsPrototypeContaining
            | StandardBuiltinId::IntlSegmentsPrototypeIterator
            | StandardBuiltinId::IntlSegmentIteratorPrototypeNext
            | StandardBuiltinId::IntlPluralRulesPrototypeSelectRange => {
                // This or-pattern and `INTL_NAMESPACE_ROOTS` are two spellings
                // of the same set, and only one of them can be a `match`
                // pattern. The assertion pins the direction the types cannot:
                // an id matched here but absent from the const would be rooted
                // without its namespace being installable.
                //
                // It is a backstop, not the primary check, and it is compiled
                // out of release builds: `every_intl_standard_builtin_is_in_the_
                // namespace_root_list` covers the same drift at rung 1 by
                // walking `all_functions()`, which no arm can hide from.
                debug_assert!(
                    INTL_NAMESPACE_ROOTS.contains(&builtin),
                    "`{}` is matched as an `Intl` namespace builtin but is not in \
                     `INTL_NAMESPACE_ROOTS`",
                    builtin.debug_name()
                );
                self.require_intl_namespace();
            }
            // Any Temporal entry first closes the namespace contract. The
            // private Rooting state makes these recursive calls no-ops until
            // all advertised constructors and `Now` members are rooted.
            StandardBuiltinId::TemporalNowTimeZoneId => {
                self.require_temporal_namespace();
            }
            StandardBuiltinId::TemporalNowInstant => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalInstantConstructor);
            }
            StandardBuiltinId::TemporalNowZonedDateTimeIso => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimeConstructor);
            }
            StandardBuiltinId::TemporalNowPlainDateTimeIso => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainDateTimeConstructor);
            }
            StandardBuiltinId::TemporalNowPlainDateIso => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainDateConstructor);
            }
            StandardBuiltinId::TemporalNowPlainTimeIso => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainTimeConstructor);
            }
            // The whole `Temporal.PlainDate` family installs together: the
            // prototype is built once, so rooting one member without the rest
            // would leave the object half-populated.
            StandardBuiltinId::TemporalPlainDateConstructor
            | StandardBuiltinId::TemporalPlainDateFrom
            | StandardBuiltinId::TemporalPlainDateCompare
            | StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeEraGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeWith
            | StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar
            | StandardBuiltinId::TemporalPlainDatePrototypeEquals
            | StandardBuiltinId::TemporalPlainDatePrototypeToString
            | StandardBuiltinId::TemporalPlainDatePrototypeToJson
            | StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainDatePrototypeAdd
            | StandardBuiltinId::TemporalPlainDatePrototypeSubtract
            | StandardBuiltinId::TemporalPlainDatePrototypeUntil
            | StandardBuiltinId::TemporalPlainDatePrototypeSince
            | StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime
            | StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime
            | StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth
            | StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay
            | StandardBuiltinId::TemporalPlainDatePrototypeValueOf => {
                self.require_temporal_namespace();
                for dependency in [
                    StandardBuiltinId::TemporalPlainDateConstructor,
                    StandardBuiltinId::TemporalPlainDateFrom,
                    StandardBuiltinId::TemporalPlainDateCompare,
                    StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeEraGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeDayGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter,
                    StandardBuiltinId::TemporalPlainDatePrototypeWith,
                    StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar,
                    StandardBuiltinId::TemporalPlainDatePrototypeEquals,
                    StandardBuiltinId::TemporalPlainDatePrototypeToString,
                    StandardBuiltinId::TemporalPlainDatePrototypeToJson,
                    StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString,
                    StandardBuiltinId::TemporalPlainDatePrototypeValueOf,
                    StandardBuiltinId::TemporalPlainDatePrototypeAdd,
                    StandardBuiltinId::TemporalPlainDatePrototypeSubtract,
                    StandardBuiltinId::TemporalPlainDatePrototypeUntil,
                    StandardBuiltinId::TemporalPlainDatePrototypeSince,
                    StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime,
                    StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime,
                    StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth,
                    StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay,
                ] {
                    self.standard_roots.insert(dependency);
                }
                // `Temporal.PlainDate.prototype.toLocaleString` builds an
                // `Intl.DateTimeFormat` and calls its bound format function.
                self.require_intl_namespace();
            }
            // The whole `Temporal.PlainYearMonth` family installs together: one shared prototype, and `until`/`since`/`toPlainDate` hand back sibling types.
            StandardBuiltinId::TemporalPlainYearMonthConstructor
            | StandardBuiltinId::TemporalPlainYearMonthFrom
            | StandardBuiltinId::TemporalPlainYearMonthCompare
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeWith
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeSince
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToString
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalDurationConstructor);
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainDateConstructor);
                for dependency in [
                    StandardBuiltinId::TemporalPlainYearMonthConstructor,
                    StandardBuiltinId::TemporalPlainYearMonthFrom,
                    StandardBuiltinId::TemporalPlainYearMonthCompare,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeWith,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeSince,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeToString,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf,
                    StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate,
                ] {
                    self.standard_roots.insert(dependency);
                }
                // `Temporal.PlainYearMonth.prototype.toLocaleString` builds an
                // `Intl.DateTimeFormat` and calls its bound format function.
                self.require_intl_namespace();
            }
            // The whole `Temporal.PlainMonthDay` family installs together; `toPlainDate` hands back a `Temporal.PlainDate`.
            StandardBuiltinId::TemporalPlainMonthDayConstructor
            | StandardBuiltinId::TemporalPlainMonthDayFrom
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeWith
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToString
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainDateConstructor);
                for dependency in [
                    StandardBuiltinId::TemporalPlainMonthDayConstructor,
                    StandardBuiltinId::TemporalPlainMonthDayFrom,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeWith,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeToString,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf,
                    StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate,
                ] {
                    self.standard_roots.insert(dependency);
                }
                // `Temporal.PlainMonthDay.prototype.toLocaleString` builds an
                // `Intl.DateTimeFormat` and calls its bound format function.
                self.require_intl_namespace();
            }
            // The whole `Temporal.PlainTime` family installs together, for the
            // same reason `Temporal.PlainDate` does: one shared prototype.
            StandardBuiltinId::TemporalPlainTimeConstructor
            | StandardBuiltinId::TemporalPlainTimeFrom
            | StandardBuiltinId::TemporalPlainTimeCompare
            | StandardBuiltinId::TemporalPlainTimePrototypeHourGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeWith
            | StandardBuiltinId::TemporalPlainTimePrototypeAdd
            | StandardBuiltinId::TemporalPlainTimePrototypeSubtract
            | StandardBuiltinId::TemporalPlainTimePrototypeUntil
            | StandardBuiltinId::TemporalPlainTimePrototypeSince
            | StandardBuiltinId::TemporalPlainTimePrototypeRound
            | StandardBuiltinId::TemporalPlainTimePrototypeEquals
            | StandardBuiltinId::TemporalPlainTimePrototypeToString
            | StandardBuiltinId::TemporalPlainTimePrototypeToJson
            | StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainTimePrototypeValueOf => {
                self.require_temporal_namespace();
                // `until`/`since` hand back a `Temporal.Duration`, and `add`
                // and `subtract` read one, so the Duration family has to come
                // along whenever any PlainTime member does.
                self.require_standard_builtin(StandardBuiltinId::TemporalDurationConstructor);
                for dependency in [
                    StandardBuiltinId::TemporalPlainTimeConstructor,
                    StandardBuiltinId::TemporalPlainTimeFrom,
                    StandardBuiltinId::TemporalPlainTimeCompare,
                    StandardBuiltinId::TemporalPlainTimePrototypeHourGetter,
                    StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter,
                    StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter,
                    StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter,
                    StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter,
                    StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter,
                    StandardBuiltinId::TemporalPlainTimePrototypeWith,
                    StandardBuiltinId::TemporalPlainTimePrototypeAdd,
                    StandardBuiltinId::TemporalPlainTimePrototypeSubtract,
                    StandardBuiltinId::TemporalPlainTimePrototypeUntil,
                    StandardBuiltinId::TemporalPlainTimePrototypeSince,
                    StandardBuiltinId::TemporalPlainTimePrototypeRound,
                    StandardBuiltinId::TemporalPlainTimePrototypeEquals,
                    StandardBuiltinId::TemporalPlainTimePrototypeToString,
                    StandardBuiltinId::TemporalPlainTimePrototypeToJson,
                    StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString,
                    StandardBuiltinId::TemporalPlainTimePrototypeValueOf,
                ] {
                    self.standard_roots.insert(dependency);
                }
                // `Temporal.PlainTime.prototype.toLocaleString` builds an
                // `Intl.DateTimeFormat` and calls its bound format function.
                self.require_intl_namespace();
            }
            // The whole `Temporal.PlainDateTime` family installs together, for the
            // same reason `Temporal.PlainDate` does: one shared prototype.
            StandardBuiltinId::TemporalPlainDateTimeConstructor
            | StandardBuiltinId::TemporalPlainDateTimeFrom
            | StandardBuiltinId::TemporalPlainDateTimeCompare
            | StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWith
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar
            | StandardBuiltinId::TemporalPlainDateTimePrototypeAdd
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract
            | StandardBuiltinId::TemporalPlainDateTimePrototypeUntil
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSince
            | StandardBuiltinId::TemporalPlainDateTimePrototypeRound
            | StandardBuiltinId::TemporalPlainDateTimePrototypeEquals
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToString
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToJson
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime => {
                self.require_temporal_namespace();
                // `until`/`since` hand back a `Temporal.Duration` and `add`/`subtract`
                // read one; `toPlainDate`, `toPlainTime` and `toZonedDateTime` hand back
                // the three sibling types. All four families come along.
                self.require_standard_builtin(StandardBuiltinId::TemporalDurationConstructor);
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainDateConstructor);
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainTimeConstructor);
                self.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimeConstructor);
                for dependency in [
                    StandardBuiltinId::TemporalPlainDateTimeConstructor,
                    StandardBuiltinId::TemporalPlainDateTimeFrom,
                    StandardBuiltinId::TemporalPlainDateTimeCompare,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeWith,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeAdd,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeUntil,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeSince,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeRound,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeEquals,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeToString,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeToJson,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime,
                    StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime,
                ] {
                    self.standard_roots.insert(dependency);
                }
                // `Temporal.PlainDateTime.prototype.toLocaleString` builds an
                // `Intl.DateTimeFormat` and calls its bound format function.
                self.require_intl_namespace();
            }
            // The whole `Temporal.Duration` family installs together, for the
            // same reason `Temporal.PlainDate` does: one shared prototype.
            StandardBuiltinId::TemporalDurationConstructor
            | StandardBuiltinId::TemporalDurationFrom
            | StandardBuiltinId::TemporalDurationCompare
            | StandardBuiltinId::TemporalDurationPrototypeYearsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMonthsGetter
            | StandardBuiltinId::TemporalDurationPrototypeWeeksGetter
            | StandardBuiltinId::TemporalDurationPrototypeDaysGetter
            | StandardBuiltinId::TemporalDurationPrototypeHoursGetter
            | StandardBuiltinId::TemporalDurationPrototypeMinutesGetter
            | StandardBuiltinId::TemporalDurationPrototypeSecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeSignGetter
            | StandardBuiltinId::TemporalDurationPrototypeBlankGetter
            | StandardBuiltinId::TemporalDurationPrototypeWith
            | StandardBuiltinId::TemporalDurationPrototypeNegated
            | StandardBuiltinId::TemporalDurationPrototypeAbs
            | StandardBuiltinId::TemporalDurationPrototypeAdd
            | StandardBuiltinId::TemporalDurationPrototypeSubtract
            | StandardBuiltinId::TemporalDurationPrototypeRound
            | StandardBuiltinId::TemporalDurationPrototypeTotal
            | StandardBuiltinId::TemporalDurationPrototypeToString
            | StandardBuiltinId::TemporalDurationPrototypeToJson
            | StandardBuiltinId::TemporalDurationPrototypeToLocaleString
            | StandardBuiltinId::TemporalDurationPrototypeValueOf => {
                self.require_temporal_namespace();
                for dependency in [
                    StandardBuiltinId::TemporalDurationConstructor,
                    StandardBuiltinId::TemporalDurationFrom,
                    StandardBuiltinId::TemporalDurationCompare,
                    StandardBuiltinId::TemporalDurationPrototypeYearsGetter,
                    StandardBuiltinId::TemporalDurationPrototypeMonthsGetter,
                    StandardBuiltinId::TemporalDurationPrototypeWeeksGetter,
                    StandardBuiltinId::TemporalDurationPrototypeDaysGetter,
                    StandardBuiltinId::TemporalDurationPrototypeHoursGetter,
                    StandardBuiltinId::TemporalDurationPrototypeMinutesGetter,
                    StandardBuiltinId::TemporalDurationPrototypeSecondsGetter,
                    StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter,
                    StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter,
                    StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter,
                    StandardBuiltinId::TemporalDurationPrototypeSignGetter,
                    StandardBuiltinId::TemporalDurationPrototypeBlankGetter,
                    StandardBuiltinId::TemporalDurationPrototypeWith,
                    StandardBuiltinId::TemporalDurationPrototypeNegated,
                    StandardBuiltinId::TemporalDurationPrototypeAbs,
                    StandardBuiltinId::TemporalDurationPrototypeAdd,
                    StandardBuiltinId::TemporalDurationPrototypeSubtract,
                    StandardBuiltinId::TemporalDurationPrototypeRound,
                    StandardBuiltinId::TemporalDurationPrototypeTotal,
                    StandardBuiltinId::TemporalDurationPrototypeToString,
                    StandardBuiltinId::TemporalDurationPrototypeToJson,
                    StandardBuiltinId::TemporalDurationPrototypeToLocaleString,
                    StandardBuiltinId::TemporalDurationPrototypeValueOf,
                ] {
                    self.standard_roots.insert(dependency);
                }
            }
            // The whole `Temporal.Instant` family installs together: the
            // prototype is built once, so rooting one member without the rest
            // would leave the object half-populated.
            StandardBuiltinId::TemporalInstantConstructor
            | StandardBuiltinId::TemporalInstantFrom
            | StandardBuiltinId::TemporalInstantCompare
            | StandardBuiltinId::TemporalInstantFromEpochMilliseconds
            | StandardBuiltinId::TemporalInstantFromEpochNanoseconds
            | StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter
            | StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter
            | StandardBuiltinId::TemporalInstantPrototypeAdd
            | StandardBuiltinId::TemporalInstantPrototypeSubtract
            | StandardBuiltinId::TemporalInstantPrototypeRound
            | StandardBuiltinId::TemporalInstantPrototypeUntil
            | StandardBuiltinId::TemporalInstantPrototypeSince
            | StandardBuiltinId::TemporalInstantPrototypeEquals
            | StandardBuiltinId::TemporalInstantPrototypeToString
            | StandardBuiltinId::TemporalInstantPrototypeToLocaleString
            | StandardBuiltinId::TemporalInstantPrototypeToJson
            | StandardBuiltinId::TemporalInstantPrototypeValueOf => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalDurationConstructor);
                self.require_intl_namespace();
                for dependency in [
                    StandardBuiltinId::TemporalInstantConstructor,
                    StandardBuiltinId::TemporalInstantFrom,
                    StandardBuiltinId::TemporalInstantCompare,
                    StandardBuiltinId::TemporalInstantFromEpochMilliseconds,
                    StandardBuiltinId::TemporalInstantFromEpochNanoseconds,
                    StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter,
                    StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter,
                    StandardBuiltinId::TemporalInstantPrototypeAdd,
                    StandardBuiltinId::TemporalInstantPrototypeSubtract,
                    StandardBuiltinId::TemporalInstantPrototypeRound,
                    StandardBuiltinId::TemporalInstantPrototypeUntil,
                    StandardBuiltinId::TemporalInstantPrototypeSince,
                    StandardBuiltinId::TemporalInstantPrototypeEquals,
                    StandardBuiltinId::TemporalInstantPrototypeToString,
                    StandardBuiltinId::TemporalInstantPrototypeToLocaleString,
                    StandardBuiltinId::TemporalInstantPrototypeToJson,
                    StandardBuiltinId::TemporalInstantPrototypeValueOf,
                ] {
                    self.standard_roots.insert(dependency);
                }
            }
            // Split from the Instant arm above: `toZonedDateTimeISO` allocates
            // a `ZonedDateTime` and reads its prototype global, so it roots
            // the zoned constructor — the rest of the Instant family must not
            // drag that in.
            StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso => {
                self.require_temporal_namespace();
                self.require_standard_builtin(StandardBuiltinId::TemporalZonedDateTimeConstructor);
            }
            StandardBuiltinId::TemporalZonedDateTimeConstructor
            | StandardBuiltinId::TemporalZonedDateTimeFrom
            | StandardBuiltinId::TemporalZonedDateTimeCompare
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToString
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToJson
            | StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWith
            | StandardBuiltinId::TemporalZonedDateTimePrototypeRound
            | StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition
            | StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEquals
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar
            | StandardBuiltinId::TemporalZonedDateTimePrototypeAdd
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract
            | StandardBuiltinId::TemporalZonedDateTimePrototypeUntil
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSince => {
                self.require_temporal_namespace();
                // `toPlainDateTime` hands back a `Temporal.PlainDateTime`, the
                // mirror of the `TemporalZonedDateTimeConstructor` requirement
                // the PlainDateTime arm above carries for `toZonedDateTime`.
                // Without it `emit_alloc_temporal_plain_date_time` reads a
                // prototype global nothing has bootstrapped.
                //
                // SIZE CONSEQUENCE, recorded deliberately. This arm is shared
                // by all 24 ZonedDateTime members, so a program that touches
                // *any* of them — `zdt.hour`, say — now roots the whole
                // PlainDateTime constructor family. It cannot be narrowed in
                // place: the same arm unconditionally inserts
                // `...PrototypeToPlainDateTime` into `standard_roots` below,
                // and that emitter reads
                // `TEMPORAL_PLAIN_DATE_TIME_PROTOTYPE_GLOBAL_INDEX`
                // (`intrinsics/temporal_plain_date_time.rs`). So the growth is
                // inherent to the existing root-them-all shape rather than a
                // placement mistake, and it is unmeasured against batch 3's
                // emit-size budget. The real fix, if it ever matters, is to
                // split this arm so the accessors do not drag in
                // `toPlainDateTime`.
                self.require_standard_builtin(StandardBuiltinId::TemporalPlainDateTimeConstructor);
                // `until`/`since` hand back a `Temporal.Duration` and
                // `add`/`subtract` read one, so this arm states that edge
                // directly — the same sentence the PlainTime (`:1923`) and
                // PlainDateTime (`:1999`) arms already carry.
                //
                // IT IS REDUNDANT TODAY, and batch 6 first shipped it with a
                // comment claiming the opposite ("THE LINE THIS ARM WAS MISSING
                // WHEN IT ONLY HELD 22 MEMBERS"). Traced instead of assumed:
                // `require_standard_builtin` roots unconditionally and walks
                // once (`:1408`), the line directly above enters the
                // PlainDateTime arm, and that arm's FIRST statement (`:2079`)
                // is `require_standard_builtin(TemporalDurationConstructor)`.
                // So `standard_roots` contains the whole Duration family with
                // this line deleted — for `zdt.add`, and equally for `zdt.hour`,
                // before batch 6 as well as after. The emitter-reads-an-
                // unbootstrapped-global failure mode was never reachable
                // through this path, and no test can witness this line.
                //
                // Keep it anyway, but for the honest reason: it says what this
                // arm depends on rather than what the arm above happens to drag
                // in, and it becomes load-bearing the moment the arm is split
                // as the comment above proposes.
                //
                // TERMINATION, checked against the Duration arm rather than
                // assumed. This is a new edge out of an arm that already sits
                // on the PlainDateTime <-> ZonedDateTime cycle contained by
                // `RuntimeBootstrapPlan::walked` (`:1161`, `:1334`). It does
                // *not* widen that cycle today: the `Temporal.Duration` arm
                // below inserts its whole family straight into `standard_roots`
                // and calls `require_standard_builtin` for nothing, so it is a
                // leaf in the walk. The edge that would close a second cycle is
                // `Duration.prototype.round`/`total` taking a `relativeTo` that
                // may be a ZonedDateTime — the day that arm starts requiring
                // `TemporalZonedDateTimeConstructor`, this line is what turns it
                // into a real cycle, and `walked` is what keeps it terminating.
                //
                // SIZE, corrected. The unmeasured growth batch 6 added to this
                // arm is NOT the Duration root (which changed nothing, above):
                // it is the five new members in the unconditional
                // `standard_roots` list below, so every program touching any
                // ZonedDateTime member — `zdt.hour` included — now emits
                // `withCalendar`, `add`, `subtract`, `until` and `since` as
                // well. `add`/`subtract` and `until`/`since` each inline the
                // whole `emit_temporal_zoned_date_time_to_plain` body
                // on top of two or three `emit_direct_js_call` sequences, so
                // this is five function bodies, two of them large. No budget
                // test reddens on it (`SCRIPTW_EMIT_SIZE_REPORT[_PATH]` is
                // opt-in); it shows up as slower cold compiles in a family b5
                // already measured at ~300 s/case. Recorded as owned debt in
                // `target/lane-notes/zdt-arithmetic-surface-b6-integration.md`;
                // the fix is the arm split the comment above names.
                self.require_standard_builtin(StandardBuiltinId::TemporalDurationConstructor);
                for dependency in [
                    StandardBuiltinId::TemporalInstantConstructor,
                    StandardBuiltinId::TemporalZonedDateTimeConstructor,
                    StandardBuiltinId::TemporalZonedDateTimeFrom,
                    StandardBuiltinId::TemporalZonedDateTimeCompare,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToString,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToJson,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeWith,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeRound,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeEquals,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeAdd,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
                    StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
                ] {
                    self.standard_roots.insert(dependency);
                }
            }
            StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter
            | StandardBuiltinId::ArrayBufferPrototypeDetachedGetter
            | StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter
            | StandardBuiltinId::ArrayBufferPrototypeResizableGetter
            | StandardBuiltinId::ArrayBufferPrototypeImmutableGetter
            | StandardBuiltinId::ArrayBufferPrototypeResize
            | StandardBuiltinId::ArrayBufferPrototypeSlice
            | StandardBuiltinId::ArrayBufferPrototypeTransfer
            | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
            | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable
            | StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable
            | StandardBuiltinId::ArrayBufferSpeciesGetter => {
                self.standard_roots
                    .insert(StandardBuiltinId::ArrayBufferConstructor);
            }
            StandardBuiltinId::NumberPrototypeToFixed
            | StandardBuiltinId::NumberPrototypeToExponential
            | StandardBuiltinId::NumberPrototypeToPrecision
            | StandardBuiltinId::NumberPrototypeToString
            | StandardBuiltinId::NumberPrototypeToLocaleString
            | StandardBuiltinId::NumberPrototypeValueOf => {
                // These bodies can be reached purely via dynamic property
                // dispatch on a Number-typed value (no direct call-site
                // FunctionId reference), so `should_stub_standard_builtin`
                // alone isn't enough to guarantee the property gets installed:
                // `Number.prototype`'s own-properties are only written by the
                // `NumberConstructor` bootstrap block, which is separately
                // gated on this same root set. Without forcing the
                // constructor in here too, the method body compiles but its
                // `Number.prototype` property is silently never defined, so
                // the runtime property read at the call site resolves to
                // `undefined` and traps instead of throwing/working.
                self.standard_roots
                    .insert(StandardBuiltinId::NumberConstructor);
            }
            StandardBuiltinId::BooleanPrototypeToString
            | StandardBuiltinId::BooleanPrototypeValueOf => {
                self.standard_roots
                    .insert(StandardBuiltinId::BooleanConstructor);
            }
            StandardBuiltinId::PromiseConstructor
            | StandardBuiltinId::PromisePrototypeThen
            | StandardBuiltinId::PromisePrototypeCatch
            | StandardBuiltinId::PromisePrototypeFinally
            | StandardBuiltinId::PromiseThenFinally
            | StandardBuiltinId::PromiseCatchFinally
            | StandardBuiltinId::PromiseValueThunk
            | StandardBuiltinId::PromiseThrower
            | StandardBuiltinId::PromiseSpeciesGetter
            | StandardBuiltinId::PromiseResolve
            | StandardBuiltinId::PromiseWithResolvers
            | StandardBuiltinId::PromiseTry
            | StandardBuiltinId::PromiseReject
            | StandardBuiltinId::PromiseAll
            | StandardBuiltinId::PromiseAllSettled
            | StandardBuiltinId::PromiseAllKeyed
            | StandardBuiltinId::PromiseAllSettledKeyed
            | StandardBuiltinId::PromiseAny
            | StandardBuiltinId::PromiseRace
            | StandardBuiltinId::ArrayFromAsync
            | StandardBuiltinId::AsyncGeneratorPrototypeNext
            | StandardBuiltinId::AsyncGeneratorPrototypeReturn
            | StandardBuiltinId::AsyncGeneratorPrototypeThrow
            | StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose
            | StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync
            | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
            | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected => {
                self.standard_roots
                    .insert(StandardBuiltinId::PromiseConstructor);
                self.standard_roots
                    .insert(StandardBuiltinId::PromiseSpeciesGetter);
                if matches!(
                    builtin,
                    StandardBuiltinId::PromisePrototypeThen
                        | StandardBuiltinId::PromisePrototypeFinally
                        | StandardBuiltinId::PromiseThenFinally
                        | StandardBuiltinId::PromiseCatchFinally
                        | StandardBuiltinId::PromiseResolve
                        | StandardBuiltinId::PromiseWithResolvers
                        | StandardBuiltinId::PromiseTry
                        | StandardBuiltinId::PromiseReject
                        | StandardBuiltinId::PromiseAll
                        | StandardBuiltinId::PromiseAllSettled
                        | StandardBuiltinId::PromiseAllKeyed
                        | StandardBuiltinId::PromiseAllSettledKeyed
                        | StandardBuiltinId::PromiseAny
                        | StandardBuiltinId::PromiseRace
                        | StandardBuiltinId::ArrayFromAsync
                        | StandardBuiltinId::AsyncGeneratorPrototypeNext
                        | StandardBuiltinId::AsyncGeneratorPrototypeReturn
                        | StandardBuiltinId::AsyncGeneratorPrototypeThrow
                        | StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose
                        | StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync
                        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
                        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected
                ) {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseCapabilityExecutor);
                }
                // `disposeAsync` reaches its two settlement callbacks only as
                // function *values* parked in a promise reaction, never by a
                // direct call, and all three need the constructor's intrinsic
                // installer to have run for the prototype to exist at all.
                if matches!(
                    builtin,
                    StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync
                        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
                        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected
                ) {
                    self.standard_roots
                        .insert(StandardBuiltinId::AsyncDisposableStackConstructor);
                    self.standard_roots
                        .insert(StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled);
                    self.standard_roots
                        .insert(StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected);
                    self.standard_roots
                        .insert(StandardBuiltinId::SuppressedErrorConstructor);
                }
                if builtin == StandardBuiltinId::PromiseAll {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAllResolveElement);
                }
                if builtin == StandardBuiltinId::PromiseAllSettled {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAllSettledResolveElement);
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAllSettledRejectElement);
                }
                if builtin == StandardBuiltinId::PromiseAny {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAnyRejectElement);
                    self.standard_roots
                        .insert(StandardBuiltinId::AggregateErrorConstructor);
                }
                if builtin == StandardBuiltinId::PromiseAllKeyed {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAllKeyedResolveElement);
                    self.standard_roots
                        .insert(StandardBuiltinId::ReflectOwnKeys);
                    self.standard_roots
                        .insert(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
                }
                if builtin == StandardBuiltinId::PromiseAllSettledKeyed {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAllSettledKeyedResolveElement);
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseAllSettledKeyedRejectElement);
                    self.standard_roots
                        .insert(StandardBuiltinId::ReflectOwnKeys);
                    self.standard_roots
                        .insert(StandardBuiltinId::ReflectGetOwnPropertyDescriptor);
                }
                if matches!(
                    builtin,
                    StandardBuiltinId::PromisePrototypeFinally
                        | StandardBuiltinId::PromiseThenFinally
                        | StandardBuiltinId::PromiseCatchFinally
                ) {
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseThenFinally);
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseCatchFinally);
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseValueThunk);
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseThrower);
                    self.standard_roots
                        .insert(StandardBuiltinId::PromiseResolve);
                }
            }
            StandardBuiltinId::MapSpeciesGetter
            | StandardBuiltinId::MapGroupBy
            | StandardBuiltinId::MapPrototypeClear
            | StandardBuiltinId::MapPrototypeDelete
            | StandardBuiltinId::MapPrototypeForEach
            | StandardBuiltinId::MapPrototypeKeys
            | StandardBuiltinId::MapPrototypeValues
            | StandardBuiltinId::MapPrototypeEntries
            | StandardBuiltinId::MapIteratorNext
            | StandardBuiltinId::MapPrototypeGet
            | StandardBuiltinId::MapPrototypeGetOrInsert
            | StandardBuiltinId::MapPrototypeGetOrInsertComputed
            | StandardBuiltinId::MapPrototypeHas
            | StandardBuiltinId::MapPrototypeSet
            | StandardBuiltinId::MapPrototypeSizeGetter => {
                self.standard_roots
                    .insert(StandardBuiltinId::MapConstructor);
            }
            StandardBuiltinId::WeakMapPrototypeDelete
            | StandardBuiltinId::WeakMapPrototypeGet
            | StandardBuiltinId::WeakMapPrototypeGetOrInsert
            | StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed
            | StandardBuiltinId::WeakMapPrototypeHas
            | StandardBuiltinId::WeakMapPrototypeSet => {
                self.standard_roots
                    .insert(StandardBuiltinId::WeakMapConstructor);
            }
            StandardBuiltinId::WeakSetPrototypeAdd
            | StandardBuiltinId::WeakSetPrototypeDelete
            | StandardBuiltinId::WeakSetPrototypeHas => {
                self.standard_roots
                    .insert(StandardBuiltinId::WeakSetConstructor);
            }
            StandardBuiltinId::WeakRefPrototypeDeref => {
                self.standard_roots
                    .insert(StandardBuiltinId::WeakRefConstructor);
            }
            StandardBuiltinId::FinalizationRegistryPrototypeRegister
            | StandardBuiltinId::FinalizationRegistryPrototypeUnregister => {
                self.standard_roots
                    .insert(StandardBuiltinId::FinalizationRegistryConstructor);
            }
            // Every prototype member roots the constructor, because the
            // constructor's intrinsic installer is what puts the member on
            // `AsyncDisposableStack.prototype` in the first place. `disposeAsync`
            // and its two settlement callbacks are matched by the Promise arm
            // above -- one arm per id, so they root the constructor there.
            StandardBuiltinId::AsyncDisposableStackPrototypeUse
            | StandardBuiltinId::AsyncDisposableStackPrototypeAdopt
            | StandardBuiltinId::AsyncDisposableStackPrototypeDefer
            | StandardBuiltinId::AsyncDisposableStackPrototypeMove
            | StandardBuiltinId::AsyncDisposableStackPrototypeDisposedGetter => {
                self.standard_roots
                    .insert(StandardBuiltinId::AsyncDisposableStackConstructor);
            }
            StandardBuiltinId::DisposableStackPrototypeUse
            | StandardBuiltinId::DisposableStackPrototypeAdopt
            | StandardBuiltinId::DisposableStackPrototypeDefer
            | StandardBuiltinId::DisposableStackPrototypeMove
            | StandardBuiltinId::DisposableStackPrototypeDispose
            | StandardBuiltinId::DisposableStackPrototypeDisposedGetter => {
                self.require_standard_builtin(StandardBuiltinId::DisposableStackConstructor);
            }
            StandardBuiltinId::SetSpeciesGetter
            | StandardBuiltinId::SetPrototypeAdd
            | StandardBuiltinId::SetPrototypeClear
            | StandardBuiltinId::SetPrototypeDelete
            | StandardBuiltinId::SetPrototypeDifference
            | StandardBuiltinId::SetPrototypeForEach
            | StandardBuiltinId::SetPrototypeIntersection
            | StandardBuiltinId::SetPrototypeIsDisjointFrom
            | StandardBuiltinId::SetPrototypeIsSubsetOf
            | StandardBuiltinId::SetPrototypeIsSupersetOf
            | StandardBuiltinId::SetPrototypeSymmetricDifference
            | StandardBuiltinId::SetPrototypeUnion
            | StandardBuiltinId::SetPrototypeValues
            | StandardBuiltinId::SetPrototypeEntries
            | StandardBuiltinId::SetIteratorNext
            | StandardBuiltinId::SetPrototypeHas
            | StandardBuiltinId::SetPrototypeSizeGetter => {
                self.standard_roots
                    .insert(StandardBuiltinId::SetConstructor);
            }
            StandardBuiltinId::SymbolFor
            | StandardBuiltinId::SymbolKeyFor
            | StandardBuiltinId::SymbolPrototypeDescriptionGetter
            | StandardBuiltinId::SymbolPrototypeToString
            | StandardBuiltinId::SymbolPrototypeValueOf
            | StandardBuiltinId::SymbolPrototypeToPrimitive => {
                // `Symbol.for` / `Symbol.keyFor` live on the `Symbol`
                // constructor object and share its runtime registry, which is
                // only allocated by the `SymbolConstructor` bootstrap block.
                // The `Symbol.prototype` methods are likewise only installed
                // as properties by that same bootstrap block, and (like
                // `Number.prototype.valueOf`) their bodies can be reached
                // purely via dynamic property dispatch on a Symbol-typed
                // value with no direct call-site `FunctionId` reference.
                self.standard_roots
                    .insert(StandardBuiltinId::SymbolConstructor);
            }
            StandardBuiltinId::BigIntPrototypeToString
            | StandardBuiltinId::BigIntPrototypeToLocaleString
            | StandardBuiltinId::BigIntPrototypeValueOf => {
                self.standard_roots
                    .insert(StandardBuiltinId::BigIntConstructor);
            }
            StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter
            | StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter
            | StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter
            | StandardBuiltinId::SharedArrayBufferPrototypeGrow
            | StandardBuiltinId::SharedArrayBufferPrototypeSlice => {
                self.standard_roots
                    .insert(StandardBuiltinId::SharedArrayBufferConstructor);
            }
            StandardBuiltinId::DataViewPrototypeBufferGetter
            | StandardBuiltinId::DataViewPrototypeByteLengthGetter
            | StandardBuiltinId::DataViewPrototypeByteOffsetGetter
            | StandardBuiltinId::DataViewPrototypeGetUint8
            | StandardBuiltinId::DataViewPrototypeSetUint8
            | StandardBuiltinId::DataViewPrototypeGetInt8
            | StandardBuiltinId::DataViewPrototypeSetInt8
            | StandardBuiltinId::DataViewPrototypeGetUint16
            | StandardBuiltinId::DataViewPrototypeSetUint16
            | StandardBuiltinId::DataViewPrototypeGetInt16
            | StandardBuiltinId::DataViewPrototypeSetInt16
            | StandardBuiltinId::DataViewPrototypeGetUint32
            | StandardBuiltinId::DataViewPrototypeSetUint32
            | StandardBuiltinId::DataViewPrototypeGetInt32
            | StandardBuiltinId::DataViewPrototypeSetInt32
            | StandardBuiltinId::DataViewPrototypeGetFloat16
            | StandardBuiltinId::DataViewPrototypeSetFloat16
            | StandardBuiltinId::DataViewPrototypeGetFloat32
            | StandardBuiltinId::DataViewPrototypeSetFloat32
            | StandardBuiltinId::DataViewPrototypeGetFloat64
            | StandardBuiltinId::DataViewPrototypeSetFloat64
            | StandardBuiltinId::DataViewPrototypeGetBigInt64
            | StandardBuiltinId::DataViewPrototypeSetBigInt64
            | StandardBuiltinId::DataViewPrototypeGetBigUint64
            | StandardBuiltinId::DataViewPrototypeSetBigUint64 => {
                self.standard_roots
                    .insert(StandardBuiltinId::DataViewConstructor);
            }
            StandardBuiltinId::DatePrototypeToJson => {
                self.standard_roots
                    .insert(StandardBuiltinId::DateConstructor);
                self.standard_roots
                    .insert(StandardBuiltinId::DatePrototypeToIsoString);
            }
            StandardBuiltinId::DateNow
            | StandardBuiltinId::DateParse
            | StandardBuiltinId::DateUtc
            | StandardBuiltinId::DatePrototypeGetTime
            | StandardBuiltinId::DatePrototypeSetTime
            | StandardBuiltinId::DatePrototypeValueOf
            | StandardBuiltinId::DatePrototypeGetFullYear
            | StandardBuiltinId::DatePrototypeGetUtcFullYear
            | StandardBuiltinId::DatePrototypeGetMonth
            | StandardBuiltinId::DatePrototypeGetUtcMonth
            | StandardBuiltinId::DatePrototypeGetDate
            | StandardBuiltinId::DatePrototypeGetUtcDate
            | StandardBuiltinId::DatePrototypeGetDay
            | StandardBuiltinId::DatePrototypeGetUtcDay
            | StandardBuiltinId::DatePrototypeGetHours
            | StandardBuiltinId::DatePrototypeGetUtcHours
            | StandardBuiltinId::DatePrototypeGetMinutes
            | StandardBuiltinId::DatePrototypeGetUtcMinutes
            | StandardBuiltinId::DatePrototypeGetSeconds
            | StandardBuiltinId::DatePrototypeGetUtcSeconds
            | StandardBuiltinId::DatePrototypeGetMilliseconds
            | StandardBuiltinId::DatePrototypeGetUtcMilliseconds
            | StandardBuiltinId::DatePrototypeGetTimezoneOffset
            | StandardBuiltinId::DatePrototypeGetYear
            | StandardBuiltinId::DatePrototypeSetYear
            | StandardBuiltinId::DatePrototypeSetFullYear
            | StandardBuiltinId::DatePrototypeSetUtcFullYear
            | StandardBuiltinId::DatePrototypeSetMonth
            | StandardBuiltinId::DatePrototypeSetUtcMonth
            | StandardBuiltinId::DatePrototypeSetDate
            | StandardBuiltinId::DatePrototypeSetUtcDate
            | StandardBuiltinId::DatePrototypeSetHours
            | StandardBuiltinId::DatePrototypeSetUtcHours
            | StandardBuiltinId::DatePrototypeSetMinutes
            | StandardBuiltinId::DatePrototypeSetUtcMinutes
            | StandardBuiltinId::DatePrototypeSetSeconds
            | StandardBuiltinId::DatePrototypeSetUtcSeconds
            | StandardBuiltinId::DatePrototypeSetMilliseconds
            | StandardBuiltinId::DatePrototypeSetUtcMilliseconds
            | StandardBuiltinId::DatePrototypeToIsoString
            | StandardBuiltinId::DatePrototypeToPrimitive
            | StandardBuiltinId::DatePrototypeToDateString
            | StandardBuiltinId::DatePrototypeToTemporalInstant
            | StandardBuiltinId::DatePrototypeToTimeString
            | StandardBuiltinId::DatePrototypeToString
            | StandardBuiltinId::DatePrototypeToUtcString => {
                self.standard_roots
                    .insert(StandardBuiltinId::DateConstructor);
            }
            StandardBuiltinId::DatePrototypeToLocaleDateString
            | StandardBuiltinId::DatePrototypeToLocaleString
            | StandardBuiltinId::DatePrototypeToLocaleTimeString => {
                self.standard_roots
                    .insert(StandardBuiltinId::DateConstructor);
                self.require_intl_namespace();
            }
            StandardBuiltinId::RegExpEscape
            | StandardBuiltinId::RegExpSpeciesGetter
            | StandardBuiltinId::RegExpPrototypeFlagsGetter
            | StandardBuiltinId::RegExpPrototypeSourceGetter
            | StandardBuiltinId::RegExpPrototypeHasIndicesGetter
            | StandardBuiltinId::RegExpPrototypeGlobalGetter
            | StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter
            | StandardBuiltinId::RegExpPrototypeMultilineGetter
            | StandardBuiltinId::RegExpPrototypeDotAllGetter
            | StandardBuiltinId::RegExpPrototypeUnicodeGetter
            | StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter
            | StandardBuiltinId::RegExpPrototypeStickyGetter
            | StandardBuiltinId::RegExpLegacyStaticGetter
            | StandardBuiltinId::RegExpLegacyStaticSetter
            | StandardBuiltinId::RegExpPrototypeCompile
            | StandardBuiltinId::RegExpPrototypeExec
            | StandardBuiltinId::RegExpPrototypeTest
            | StandardBuiltinId::RegExpPrototypeToString
            | StandardBuiltinId::RegExpPrototypeSymbolMatch
            | StandardBuiltinId::RegExpPrototypeSymbolMatchAll
            | StandardBuiltinId::RegExpStringIteratorNext
            | StandardBuiltinId::RegExpPrototypeSymbolReplace
            | StandardBuiltinId::RegExpPrototypeSymbolSearch
            | StandardBuiltinId::RegExpPrototypeSymbolSplit => {
                self.standard_roots
                    .insert(StandardBuiltinId::RegExpConstructor);
            }
            StandardBuiltinId::ArrayIteratorIdentity
            | StandardBuiltinId::IteratorFrom
            | StandardBuiltinId::IteratorConcat
            | StandardBuiltinId::IteratorZip
            | StandardBuiltinId::IteratorZipKeyed
            | StandardBuiltinId::IteratorHelperNext
            | StandardBuiltinId::IteratorHelperReturn
            | StandardBuiltinId::IteratorPrototypeToArray
            | StandardBuiltinId::IteratorPrototypeIncludes
            | StandardBuiltinId::IteratorPrototypeJoin
            | StandardBuiltinId::IteratorPrototypeForEach
            | StandardBuiltinId::IteratorPrototypeEvery
            | StandardBuiltinId::IteratorPrototypeSome
            | StandardBuiltinId::IteratorPrototypeFind
            | StandardBuiltinId::IteratorPrototypeReduce
            | StandardBuiltinId::IteratorPrototypeMap
            | StandardBuiltinId::IteratorPrototypeFilter
            | StandardBuiltinId::IteratorPrototypeFlatMap
            | StandardBuiltinId::IteratorPrototypeTake
            | StandardBuiltinId::IteratorPrototypeChunks
            | StandardBuiltinId::IteratorPrototypeWindows
            | StandardBuiltinId::IteratorPrototypeDrop
            | StandardBuiltinId::IteratorPrototypeConstructorGetter
            | StandardBuiltinId::IteratorPrototypeConstructorSetter
            | StandardBuiltinId::IteratorPrototypeSymbolDispose
            | StandardBuiltinId::IteratorPrototypeToStringTagGetter
            | StandardBuiltinId::IteratorPrototypeToStringTagSetter => {
                // All public Iterator helpers are installed by the Iterator
                // constructor bootstrap block. A method body can be retained
                // through an iterator instance without the global `Iterator`
                // binding otherwise being referenced.
                self.standard_roots
                    .insert(StandardBuiltinId::IteratorConstructor);
            }
            StandardBuiltinId::TypedArrayPrototypeBufferGetter
            | StandardBuiltinId::TypedArrayPrototypeByteLengthGetter
            | StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter
            | StandardBuiltinId::TypedArrayPrototypeLengthGetter
            | StandardBuiltinId::TypedArrayPrototypeToStringTagGetter
            | StandardBuiltinId::TypedArrayPrototypeToString
            | StandardBuiltinId::TypedArrayPrototypeToLocaleString
            | StandardBuiltinId::TypedArrayPrototypeFill
            | StandardBuiltinId::TypedArrayPrototypeSubarray
            | StandardBuiltinId::TypedArrayPrototypeSlice
            | StandardBuiltinId::TypedArrayPrototypeSet
            | StandardBuiltinId::TypedArrayPrototypeReverse
            | StandardBuiltinId::TypedArrayPrototypeCopyWithin
            | StandardBuiltinId::TypedArrayPrototypeSort
            | StandardBuiltinId::TypedArrayPrototypeToReversed
            | StandardBuiltinId::TypedArrayPrototypeToSorted
            | StandardBuiltinId::TypedArrayPrototypeWith
            | StandardBuiltinId::TypedArrayFrom
            | StandardBuiltinId::TypedArrayOf => {
                self.standard_roots
                    .insert(StandardBuiltinId::Int32ArrayConstructor);
            }
            _ => {}
        }
    }
}

fn block_exposes_global_object(block: &BlockIr) -> bool {
    block.statements.iter().any(statement_exposes_global_object)
}

/// Visits the evaluated operands stored by a suspended property Reference.
/// `[[Strict]]` has no planning dependency, while every future base/receiver
/// shape must extend the exhaustive use-view match here.
fn for_each_suspended_property_reference_operand(
    reference: &SuspendedPropertyReferenceIr,
    mut visit: impl FnMut(&TypedExpr),
) {
    match reference.use_view() {
        SuspendedPropertyReferenceUse::Ordinary {
            base_and_receiver,
            key,
            strictness: _,
        } => {
            visit(base_and_receiver);
            if let PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) = key {
                visit(expr);
            }
        }
    }
}

fn suspended_property_reference_operand_matches(
    reference: &SuspendedPropertyReferenceIr,
    mut predicate: impl FnMut(&TypedExpr) -> bool,
) -> bool {
    let mut matched = false;
    for_each_suspended_property_reference_operand(reference, |expr| {
        matched |= predicate(expr);
    });
    matched
}

fn statement_exposes_global_object(statement: &StatementIr) -> bool {
    match statement {
        StatementIr::ResumableClassDefinition(plan) => {
            expr_exposes_global_object(plan.expression())
                || plan
                    .prefixes()
                    .flat_map(|prefix| prefix.statements())
                    .any(statement_exposes_global_object)
        }
        StatementIr::ModuleImportBinding(_) => false,
        StatementIr::ModuleUnitOnce { block, .. } => block_exposes_global_object(block),
        StatementIr::AsyncModuleInstantiation
        | StatementIr::Empty
        | StatementIr::AnnexBFunctionCopy { .. }
        | StatementIr::Debugger
        | StatementIr::Break { .. }
        | StatementIr::Continue { .. } => false,
        StatementIr::Lexical { init, .. }
        | StatementIr::DeclarationEvaluation(init)
        | StatementIr::Expression(init)
        | StatementIr::Throw(init)
        | StatementIr::Return(init) => expr_exposes_global_object(init),
        StatementIr::GeneratorYield {
            value, resume_mode, ..
        } => {
            expr_exposes_global_object(value)
                || match resume_mode {
                    GeneratorResumeModeIr::AssignProperty(reference) => {
                        suspended_property_reference_operand_matches(
                            reference,
                            expr_exposes_global_object,
                        )
                    }
                    GeneratorResumeModeIr::Ignore
                    | GeneratorResumeModeIr::Return
                    | GeneratorResumeModeIr::AssignIdentifier(_)
                    | GeneratorResumeModeIr::AssignGlobal { .. } => false,
                }
        }
        StatementIr::AsyncAwait { value, .. } => expr_exposes_global_object(value),
        StatementIr::EmptyStatementCompletion(item) => {
            statement_exposes_global_object(item.statement())
        }
        StatementIr::LexicalBlock(statements)
        | StatementIr::ParameterInitialization { statements, .. } => {
            statements.iter().any(statement_exposes_global_object)
        }
        StatementIr::Var(declarators) => declarators.iter().any(|declarator| {
            declarator
                .init
                .as_ref()
                .is_some_and(expr_exposes_global_object)
        }),
        StatementIr::Block(block) => block_exposes_global_object(block),
        StatementIr::If {
            condition,
            then_branch,
            else_branch,
        }
        | StatementIr::AsyncFunctionIf {
            condition,
            then_branch,
            else_branch,
            plan: _,
        } => {
            expr_exposes_global_object(condition)
                || statement_exposes_global_object(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(statement_exposes_global_object)
        }
        StatementIr::AsyncFunctionWhile(plan) => {
            plan.condition_prefix()
                .iter()
                .any(statement_exposes_global_object)
                || expr_exposes_global_object(plan.condition())
                || statement_exposes_global_object(plan.body())
        }
        StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
            expr_exposes_global_object(condition) || statement_exposes_global_object(body)
        }
        StatementIr::For {
            init,
            test,
            update,
            body,
            ..
        } => {
            init.as_ref().is_some_and(for_init_exposes_global_object)
                || test.as_ref().is_some_and(expr_exposes_global_object)
                || update.as_ref().is_some_and(expr_exposes_global_object)
                || statement_exposes_global_object(body)
        }
        StatementIr::OrdinaryGeneratorLoop(plan) => {
            plan.initialization()
                .is_some_and(|region| block_exposes_global_object(region.block()))
                || block_exposes_global_object(plan.test().region().block())
                || expr_exposes_global_object(plan.test().value())
                || block_exposes_global_object(plan.body().block())
                || plan.update().is_some_and(|update| {
                    block_exposes_global_object(update.region().block())
                        || expr_exposes_global_object(update.value())
                })
        }
        StatementIr::AsyncGeneratorLoop(plan) => {
            plan.initialization()
                .is_some_and(|region| block_exposes_global_object(region.block()))
                || block_exposes_global_object(plan.test().region().block())
                || expr_exposes_global_object(plan.test().value())
                || block_exposes_global_object(plan.body().block())
                || plan.update().is_some_and(|update| {
                    block_exposes_global_object(update.region().block())
                        || expr_exposes_global_object(update.value())
                })
        }
        StatementIr::OrdinaryGeneratorIf(plan) => {
            expr_exposes_global_object(plan.condition())
                || block_exposes_global_object(plan.then_branch().block())
                || block_exposes_global_object(plan.else_branch().block())
        }
        StatementIr::AsyncGeneratorIf(plan) => {
            block_exposes_global_object(plan.condition().region().block())
                || expr_exposes_global_object(plan.condition().value())
                || block_exposes_global_object(plan.then_branch().block())
                || block_exposes_global_object(plan.else_branch().block())
        }
        StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
            expr_exposes_global_object(plan.raw_source())
                || block_exposes_global_object(plan.body().block())
        }
        StatementIr::AsyncGeneratorResourceScope(plan) => {
            block_exposes_global_object(plan.body().block())
        }
        StatementIr::AsyncGeneratorResourceRegistration(operation) => {
            expr_exposes_global_object(operation.initializer())
        }
        StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
            expr_exposes_global_object(plan.raw_source())
                || block_exposes_global_object(plan.body().block())
        }
        StatementIr::AsyncFunctionArrayDestructuring(plan) => {
            expr_exposes_global_object(plan.raw_source())
                || block_exposes_global_object(plan.body())
        }
        StatementIr::OrdinaryGeneratorWith(plan) => {
            block_exposes_global_object(plan.head().region().block())
                || expr_exposes_global_object(plan.head().value())
                || block_exposes_global_object(plan.body().block())
        }
        StatementIr::AsyncGeneratorWith(plan) => {
            block_exposes_global_object(plan.head().region().block())
                || expr_exposes_global_object(plan.head().value())
                || block_exposes_global_object(plan.body().block())
        }
        StatementIr::AsyncGeneratorSwitch(plan) => {
            plan.regions()
                .any(|region| block_exposes_global_object(region.block()))
                || plan.expressions().any(expr_exposes_global_object)
                || plan
                    .lexical_declarations()
                    .iter()
                    .any(statement_exposes_global_object)
        }
        StatementIr::AsyncFunctionWith(plan) => {
            block_exposes_global_object(plan.head())
                || expr_exposes_global_object(plan.head_value())
                || block_exposes_global_object(plan.body())
        }
        StatementIr::AsyncGeneratorForIn(plan) => {
            block_exposes_global_object(plan.head().region().block())
                || expr_exposes_global_object(plan.head().value())
                || block_exposes_global_object(plan.initialization())
                || block_exposes_global_object(plan.body().block())
        }
        StatementIr::AsyncGeneratorForOf(plan) => {
            block_exposes_global_object(plan.head().region().block())
                || expr_exposes_global_object(plan.head().value())
                || block_exposes_global_object(plan.initialization().block())
                || block_exposes_global_object(plan.body().block())
        }
        StatementIr::ArrayDestructuringOperation(_) => false,
        StatementIr::OrdinaryGeneratorSwitch(plan) => {
            block_exposes_global_object(plan.discriminant().region().block())
                || expr_exposes_global_object(plan.discriminant().value())
                || plan
                    .lexical_declarations()
                    .iter()
                    .any(statement_exposes_global_object)
                || plan.cases().iter().any(|case| {
                    case.selector().is_some_and(|selector| {
                        block_exposes_global_object(selector.region().block())
                            || expr_exposes_global_object(selector.value())
                    }) || block_exposes_global_object(case.body().block())
                })
        }
        StatementIr::GeneratorLoop {
            init,
            test,
            update,
            before_suspension,
            suspension_statement,
            after_suspension,
            ..
        } => {
            init.as_ref().is_some_and(for_init_exposes_global_object)
                || test.as_ref().is_some_and(expr_exposes_global_object)
                || update.as_ref().is_some_and(expr_exposes_global_object)
                || before_suspension
                    .iter()
                    .any(statement_exposes_global_object)
                || statement_exposes_global_object(suspension_statement)
                || after_suspension.iter().any(statement_exposes_global_object)
        }
        StatementIr::AsyncFunctionForOfIterator { iterable, plan } => {
            expr_exposes_global_object(iterable)
                || plan
                    .body()
                    .statements()
                    .iter()
                    .any(statement_exposes_global_object)
        }
        StatementIr::GeneratorForOfIterator { iterable, plan } => {
            expr_exposes_global_object(iterable)
                || plan
                    .body()
                    .statements()
                    .iter()
                    .any(statement_exposes_global_object)
        }
        StatementIr::GeneratorIf {
            condition,
            then_before_yield,
            then_yield_statement,
            then_after_yield,
            else_before_yield,
            else_yield_statement,
            else_after_yield,
            ..
        } => {
            expr_exposes_global_object(condition)
                || then_before_yield
                    .iter()
                    .chain(then_yield_statement.as_deref())
                    .chain(then_after_yield)
                    .chain(else_before_yield)
                    .chain(else_yield_statement.as_deref())
                    .chain(else_after_yield)
                    .any(statement_exposes_global_object)
        }
        StatementIr::ForOfIterator { iterable, body, .. } => {
            expr_exposes_global_object(iterable) || statement_exposes_global_object(body)
        }
        StatementIr::ForInArray {
            target: iterable,
            body,
            ..
        }
        | StatementIr::ForInString {
            target: iterable,
            body,
            ..
        }
        | StatementIr::ForInObject {
            target: iterable,
            body,
            ..
        } => expr_exposes_global_object(iterable) || statement_exposes_global_object(body),
        StatementIr::AsyncFunctionSwitch(plan) => {
            expr_exposes_global_object(plan.discriminant())
                || plan
                    .lexical_declarations()
                    .iter()
                    .any(statement_exposes_global_object)
                || plan.cases().iter().any(|case| {
                    case.condition_prefix()
                        .iter()
                        .any(statement_exposes_global_object)
                        || case.condition().is_some_and(expr_exposes_global_object)
                        || block_exposes_global_object(case.body())
                })
        }
        StatementIr::Switch {
            discriminant,
            lexical_declarations,
            cases,
            ..
        } => {
            expr_exposes_global_object(discriminant)
                || lexical_declarations
                    .iter()
                    .any(statement_exposes_global_object)
                || cases.iter().any(|case| {
                    case.condition
                        .as_ref()
                        .is_some_and(expr_exposes_global_object)
                        || block_exposes_global_object(&case.body)
                })
        }
        StatementIr::Labelled { statement, .. } => statement_exposes_global_object(statement),
        StatementIr::SyncDisposableScope {
            resources, body, ..
        } => {
            resources
                .iter()
                .any(|resource| expr_exposes_global_object(&resource.initializer))
                || block_exposes_global_object(body)
        }
        StatementIr::AsyncDisposableScope {
            resources, body, ..
        } => {
            resources
                .iter()
                .any(|resource| expr_exposes_global_object(resource.initializer()))
                || block_exposes_global_object(body)
        }
        StatementIr::TryCatch {
            try_block,
            catch_block,
            ..
        } => block_exposes_global_object(try_block) || block_exposes_global_object(catch_block),
        StatementIr::TryFinally {
            try_block,
            finally_block,
            ..
        } => block_exposes_global_object(try_block) || block_exposes_global_object(finally_block),
        StatementIr::TryCatchFinally {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            block_exposes_global_object(try_block)
                || block_exposes_global_object(catch_block)
                || block_exposes_global_object(finally_block)
        }
    }
}

fn for_init_exposes_global_object(init: &ForInitIr) -> bool {
    match init {
        ForInitIr::Lexical { init, .. } | ForInitIr::Expression(init) => {
            expr_exposes_global_object(init)
        }
        ForInitIr::LexicalBlock(bindings) => bindings
            .iter()
            .any(|binding| expr_exposes_global_object(&binding.init)),
        ForInitIr::Var(declarators) => declarators.iter().any(|declarator| {
            declarator
                .init
                .as_ref()
                .is_some_and(expr_exposes_global_object)
        }),
        ForInitIr::Statements(statements) => statements.iter().any(statement_exposes_global_object),
        ForInitIr::SyncDisposable(resources) => resources
            .iter()
            .any(|resource| expr_exposes_global_object(&resource.initializer)),
        ForInitIr::AsyncDisposable(init) => init
            .resources()
            .iter()
            .any(|resource| expr_exposes_global_object(resource.initializer())),
    }
}

fn property_key_exposes_global_object(key: &PropertyKeyIr) -> bool {
    match key {
        PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => false,
        PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
            expr_exposes_global_object(expr)
        }
    }
}

fn expr_is_global_object(expr: &TypedExpr) -> bool {
    matches!(&expr.expr, ExprIr::ExecutionGlobalObject)
}

fn property_access_exposes_global_object(target: &TypedExpr, key: &PropertyKeyIr) -> bool {
    if expr_is_global_object(target) {
        !matches!(
            key,
            PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength
        )
    } else {
        expr_exposes_global_object(target)
    }
}

fn object_property_exposes_global_object(property: &ObjectPropertyIr) -> bool {
    match property {
        ObjectPropertyIr::PrototypeSetter { value }
        | ObjectPropertyIr::Spread { source: value }
        | ObjectPropertyIr::Data { value, .. }
        | ObjectPropertyIr::NonEnumerableData { value, .. } => expr_exposes_global_object(value),
        ObjectPropertyIr::Method { .. }
        | ObjectPropertyIr::Getter { .. }
        | ObjectPropertyIr::Setter { .. } => false,
        ObjectPropertyIr::ComputedData { key, value, .. } => {
            expr_exposes_global_object(key) || expr_exposes_global_object(value)
        }
        ObjectPropertyIr::ComputedMethod { key, .. }
        | ObjectPropertyIr::ComputedGetter { key, .. }
        | ObjectPropertyIr::ComputedSetter { key, .. } => expr_exposes_global_object(key),
    }
}

fn array_destructuring_pattern_any_expression(
    pattern: &ArrayDestructuringPatternIr,
    mut predicate: impl FnMut(&TypedExpr) -> bool,
) -> bool {
    let mut found = false;
    pattern.visit_expressions(&mut |expr| found |= predicate(expr));
    found
}

fn object_destructuring_pattern_any_expression(
    pattern: &ObjectDestructuringPatternIr,
    mut predicate: impl FnMut(&TypedExpr) -> bool,
) -> bool {
    let mut found = false;
    pattern.visit_expressions(&mut |expr| found |= predicate(expr));
    found
}

fn array_accumulation_expressions(
    accumulation: &ArrayAccumulationIr,
) -> impl Iterator<Item = &TypedExpr> {
    accumulation
        .elements()
        .iter()
        .filter_map(|element| match element {
            ArrayAccumulationElementIr::Elision => None,
            ArrayAccumulationElementIr::Value(value) => Some(value),
            ArrayAccumulationElementIr::Spread(spread) => Some(spread.value.as_ref()),
        })
}

fn array_accumulation_has_spread(accumulation: &ArrayAccumulationIr) -> bool {
    accumulation
        .elements()
        .iter()
        .any(|element| matches!(element, ArrayAccumulationElementIr::Spread(_)))
}

fn expr_exposes_global_object(expr: &TypedExpr) -> bool {
    match &expr.expr {
        ExprIr::ForeignCall(call) => call.operands().iter().any(expr_exposes_global_object),
        ExprIr::ModuleEntryEvaluation(entry) => expr_exposes_global_object(entry.evaluation()),
        ExprIr::EnvironmentIdentifier(_) => true,
        // Module top-level `this` is `undefined`, and neither a namespace
        // object nor `import.meta` can reach the global object.
        ExprIr::ModuleExecutionGraph(_)
        | ExprIr::ModuleBindingRead(_)
        | ExprIr::JsonModuleValue(_)
        | ExprIr::BytesModuleValue(_)
        | ExprIr::ModuleEvaluate(_)
        | ExprIr::DeferredModuleEvaluate(_)
        | ExprIr::ModuleHasAsyncDependencies(_)
        | ExprIr::ModuleDeferredImportEvaluate(_) => false,
        ExprIr::ModuleNamespacePublish { namespace, .. } => expr_exposes_global_object(namespace),
        ExprIr::ImportMeta { .. } => false,
        ExprIr::ModuleNamespace { exports, .. } => expr_exposes_global_object(exports),
        ExprIr::DynamicImport {
            specifier, options, ..
        } => {
            expr_exposes_global_object(specifier)
                || options.as_deref().is_some_and(expr_exposes_global_object)
        }
        ExprIr::ExecutionGlobalObject => true,
        ExprIr::GlobalIdentifierRead { name } | ExprIr::GlobalPropertyRead { name }
            if name == GLOBAL_THIS_NAME =>
        {
            true
        }
        ExprIr::Identifier(_) => false,
        ExprIr::ObjectLiteral(properties) => {
            properties.iter().any(object_property_exposes_global_object)
        }
        ExprIr::ObjectPropertyDefinition(definition) => {
            expr_exposes_global_object(definition.target())
                || object_property_exposes_global_object(definition.property())
        }
        ExprIr::CaptureOptionalCallReference(capture) => capture
            .operands()
            .iter()
            .any(|operand| expr_exposes_global_object(operand)),
        ExprIr::CaptureArgumentList(capture) => {
            capture.arguments().iter().any(expr_exposes_global_object)
        }
        ExprIr::CapturedArgumentList(list) => expr_exposes_global_object(list.binding()),
        ExprIr::ArrayLiteral(elements) => elements.iter().any(expr_exposes_global_object),
        ExprIr::ArrayAccumulation(accumulation) => {
            array_accumulation_expressions(accumulation).any(expr_exposes_global_object)
        }
        ExprIr::AssignIdentifier { value, .. }
        | ExprIr::GlobalPropertyWrite { value, .. }
        | ExprIr::CompoundAssignIdentifier { value, .. }
        | ExprIr::UnaryPlus { expr: value }
        | ExprIr::UnaryMinusNumeric { expr: value }
        | ExprIr::UnaryBitwiseNumeric { expr: value, .. }
        | ExprIr::Void { expr: value }
        | ExprIr::DeleteValue { expr: value }
        | ExprIr::TypeOf { expr: value }
        | ExprIr::LogicalNot { expr: value }
        | ExprIr::StringFromCharCode { code: value }
        | ExprIr::SpreadArgument(SpreadArgumentIr { value, .. })
        | ExprIr::PrivateIn { rhs: value, .. } => expr_exposes_global_object(value),
        ExprIr::SpecOperation { operands, .. } => operands.iter().any(expr_exposes_global_object),
        ExprIr::PropertyRead { target, key } | ExprIr::DeleteProperty { target, key, .. } => {
            property_access_exposes_global_object(target, key)
                || property_key_exposes_global_object(key)
        }
        ExprIr::OrdinaryPropertyAssignment(assignment) => {
            property_access_exposes_global_object(
                assignment.base_and_receiver(),
                assignment.referenced_name(),
            ) || property_key_exposes_global_object(assignment.referenced_name())
                || expr_exposes_global_object(assignment.rhs())
        }
        ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {
            property_access_exposes_global_object(
                assignment.base_and_receiver(),
                assignment.referenced_name(),
            ) || property_key_exposes_global_object(assignment.referenced_name())
                || expr_exposes_global_object(assignment.rhs())
        }
        ExprIr::OrdinaryPropertyGetCapture(capture) => {
            property_access_exposes_global_object(
                capture.base_and_receiver(),
                capture.referenced_name(),
            ) || property_key_exposes_global_object(capture.referenced_name())
        }
        ExprIr::CapturedOrdinaryPropertyWrite(write) => expr_exposes_global_object(write.rhs()),
        ExprIr::OrdinaryPropertyNumericUpdate(update) => {
            property_access_exposes_global_object(
                update.base_and_receiver(),
                update.referenced_name(),
            ) || property_key_exposes_global_object(update.referenced_name())
        }
        ExprIr::OrdinaryPropertyEagerCompoundAssignment(mutation) => {
            property_access_exposes_global_object(
                mutation.base_and_receiver(),
                mutation.referenced_name(),
            ) || property_key_exposes_global_object(mutation.referenced_name())
                || expr_exposes_global_object(mutation.result())
        }
        ExprIr::OptionalPropertyChain { target, chain } => {
            expr_exposes_global_object(target)
                || chain.iter().any(|operation| match operation {
                    OptionalChainOperationIr::Property { key, .. } => {
                        property_key_exposes_global_object(key)
                            || property_access_exposes_global_object(target, key)
                    }
                    OptionalChainOperationIr::PrivateProperty { .. } => false,
                    OptionalChainOperationIr::Call { args, .. } => {
                        args.iter().any(expr_exposes_global_object)
                    }
                })
        }
        ExprIr::DeleteOptionalPropertyChain(deletion) => {
            let target = deletion.target();
            let chain = deletion.prefix();
            property_key_exposes_global_object(deletion.key())
                || property_access_exposes_global_object(target, deletion.key())
                || expr_exposes_global_object(target)
                || chain.iter().any(|operation| match operation {
                    OptionalChainOperationIr::Property { key, .. } => {
                        property_key_exposes_global_object(key)
                            || property_access_exposes_global_object(target, key)
                    }
                    OptionalChainOperationIr::PrivateProperty { .. } => false,
                    OptionalChainOperationIr::Call { args, .. } => {
                        args.iter().any(expr_exposes_global_object)
                    }
                })
        }
        ExprIr::PropertyWrite {
            target, key, value, ..
        } => {
            property_access_exposes_global_object(target, key)
                || property_key_exposes_global_object(key)
                || expr_exposes_global_object(value)
        }
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. }
        | ExprIr::StringConcat { lhs, rhs }
        | ExprIr::CompareNumber { lhs, rhs, .. }
        | ExprIr::CompareValue { lhs, rhs, .. }
        | ExprIr::StrictEquality { lhs, rhs, .. }
        | ExprIr::LooseEquality { lhs, rhs, .. }
        | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
        | ExprIr::Comma { lhs, rhs }
        | ExprIr::InstanceOf { lhs, rhs }
        | ExprIr::In { lhs, rhs } => {
            expr_exposes_global_object(lhs) || expr_exposes_global_object(rhs)
        }
        ExprIr::MaterializeBinding { value, body, .. } => {
            expr_exposes_global_object(value) || expr_exposes_global_object(body)
        }
        ExprIr::ArrayDestructure { value, pattern, .. } => {
            expr_exposes_global_object(value)
                || array_destructuring_pattern_any_expression(pattern, expr_exposes_global_object)
        }
        ExprIr::ObjectDestructure { value, pattern } => {
            expr_exposes_global_object(value)
                || object_destructuring_pattern_any_expression(pattern, expr_exposes_global_object)
        }
        ExprIr::ObjectDestructuringOperation(operation) => {
            let mut exposes = false;
            operation.visit_expressions(&mut |expr| exposes |= expr_exposes_global_object(expr));
            exposes
        }
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_exposes_global_object(condition)
                || expr_exposes_global_object(then_expr)
                || expr_exposes_global_object(else_expr)
        }
        ExprIr::CallNamed { args, .. } | ExprIr::SuperConstruct { args } => {
            args.iter().any(expr_exposes_global_object)
        }
        ExprIr::SuperNewTarget | ExprIr::SuperConstructor => true,
        ExprIr::PreparedSuperConstruct(prepared) => {
            prepared.operands().any(expr_exposes_global_object)
        }
        ExprIr::CallIndirect {
            callee,
            this_arg,
            args,
            ..
        } => {
            expr_exposes_global_object(callee)
                || this_arg.as_deref().is_some_and(expr_exposes_global_object)
                || args.iter().any(expr_exposes_global_object)
        }
        ExprIr::Construct { callee, args, .. } => {
            expr_exposes_global_object(callee) || args.iter().any(expr_exposes_global_object)
        }
        ExprIr::CallMethod {
            receiver,
            key,
            args,
        } => {
            property_access_exposes_global_object(receiver, key)
                || property_key_exposes_global_object(key)
                || args.iter().any(expr_exposes_global_object)
        }
        ExprIr::SuperPropertyRead { key, receiver } => {
            property_key_exposes_global_object(key) || expr_exposes_global_object(receiver)
        }
        ExprIr::SuperPropertyWrite {
            key,
            receiver,
            value,
            ..
        } => {
            property_key_exposes_global_object(key)
                || expr_exposes_global_object(receiver)
                || expr_exposes_global_object(value)
        }
        ExprIr::SuperPropertyMutation(mutation) => {
            property_key_exposes_global_object(mutation.referenced_name())
                || expr_exposes_global_object(mutation.receiver())
                || match mutation.operation() {
                    SuperPropertyMutationOperationIr::NumericUpdate { .. }
                    | SuperPropertyMutationOperationIr::Capture(_) => false,
                    SuperPropertyMutationOperationIr::EagerCompound { result, .. }
                    | SuperPropertyMutationOperationIr::PutCaptured { value: result, .. } => {
                        expr_exposes_global_object(result)
                    }
                }
        }
        ExprIr::PrivateRead { target, .. } => expr_exposes_global_object(target),
        ExprIr::PrivateWrite { target, value, .. } => {
            expr_exposes_global_object(target) || expr_exposes_global_object(value)
        }
        ExprIr::ClassDefinition(class) => class
            .heritage
            .as_deref()
            .is_some_and(expr_exposes_global_object),
        ExprIr::AssertSameValue {
            actual, expected, ..
        } => expr_exposes_global_object(actual) || expr_exposes_global_object(expected),
        ExprIr::Undefined
        | ExprIr::ArrayHole
        | ExprIr::Null
        | ExprIr::Boolean(_)
        | ExprIr::Number(_)
        | ExprIr::BigInt(_)
        | ExprIr::Symbol { .. }
        | ExprIr::WellKnownSymbol(_)
        | ExprIr::String(_)
        | ExprIr::TemplateObject(_)
        | ExprIr::RegExpLiteral { .. }
        | ExprIr::FunctionValue(_)
        | ExprIr::ClassFieldNamedFunctionValue { .. }
        | ExprIr::This
        | ExprIr::Arguments
        | ExprIr::GlobalPropertyRead { .. }
        | ExprIr::GlobalIdentifierRead { .. }
        | ExprIr::UpdateIdentifier { .. }
        | ExprIr::DeleteIdentifier { .. }
        | ExprIr::DeleteGlobalProperty { .. }
        | ExprIr::NewTarget
        | ExprIr::TypeOfUnresolvedIdentifier { .. }
        | ExprIr::RuntimeThrow { .. } => false,
    }
}

fn collect_block_global_property_names(block: &BlockIr, names: &mut BTreeSet<String>) {
    for statement in &block.statements {
        collect_statement_global_property_names(statement, names);
    }
}

fn collect_statement_global_property_names(statement: &StatementIr, names: &mut BTreeSet<String>) {
    match statement {
        StatementIr::ModuleImportBinding(_) => {}
        StatementIr::ResumableClassDefinition(plan) => {
            collect_expr_global_property_names(plan.expression(), names);
            for statement in plan.prefixes().flat_map(|prefix| prefix.statements()) {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::ModuleUnitOnce { block, .. } => {
            collect_block_global_property_names(block, names);
        }
        StatementIr::AsyncModuleInstantiation
        | StatementIr::Empty
        | StatementIr::AnnexBFunctionCopy { .. }
        | StatementIr::Debugger
        | StatementIr::Break { .. }
        | StatementIr::Continue { .. } => {}
        StatementIr::Lexical { init, .. }
        | StatementIr::DeclarationEvaluation(init)
        | StatementIr::Expression(init)
        | StatementIr::Throw(init)
        | StatementIr::Return(init) => collect_expr_global_property_names(init, names),
        StatementIr::GeneratorYield {
            value, resume_mode, ..
        } => {
            if let GeneratorResumeModeIr::AssignProperty(reference) = resume_mode {
                for_each_suspended_property_reference_operand(reference, |expr| {
                    collect_expr_global_property_names(expr, names);
                });
            }
            collect_expr_global_property_names(value, names)
        }
        StatementIr::AsyncAwait { value, .. } => collect_expr_global_property_names(value, names),
        StatementIr::EmptyStatementCompletion(item) => {
            collect_statement_global_property_names(item.statement(), names)
        }
        StatementIr::LexicalBlock(statements)
        | StatementIr::ParameterInitialization { statements, .. } => {
            for statement in statements {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::Var(declarators) => {
            for declarator in declarators {
                if let Some(init) = &declarator.init {
                    collect_expr_global_property_names(init, names);
                }
            }
        }
        StatementIr::Block(block) => collect_block_global_property_names(block, names),
        StatementIr::If {
            condition,
            then_branch,
            else_branch,
        }
        | StatementIr::AsyncFunctionIf {
            condition,
            then_branch,
            else_branch,
            plan: _,
        } => {
            collect_expr_global_property_names(condition, names);
            collect_statement_global_property_names(then_branch, names);
            if let Some(else_branch) = else_branch {
                collect_statement_global_property_names(else_branch, names);
            }
        }
        StatementIr::AsyncFunctionWhile(plan) => {
            for statement in plan.condition_prefix() {
                collect_statement_global_property_names(statement, names);
            }
            collect_expr_global_property_names(plan.condition(), names);
            collect_statement_global_property_names(plan.body(), names);
        }
        StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
            collect_expr_global_property_names(condition, names);
            collect_statement_global_property_names(body, names);
        }
        StatementIr::For {
            init,
            test,
            update,
            body,
            ..
        } => {
            if let Some(init) = init {
                collect_for_init_global_property_names(init, names);
            }
            if let Some(test) = test {
                collect_expr_global_property_names(test, names);
            }
            if let Some(update) = update {
                collect_expr_global_property_names(update, names);
            }
            collect_statement_global_property_names(body, names);
        }
        StatementIr::OrdinaryGeneratorLoop(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
            collect_expr_global_property_names(plan.test().value(), names);
            if let Some(update) = plan.update() {
                collect_expr_global_property_names(update.value(), names);
            }
        }
        StatementIr::AsyncGeneratorLoop(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
            collect_expr_global_property_names(plan.test().value(), names);
            if let Some(update) = plan.update() {
                collect_expr_global_property_names(update.value(), names);
            }
        }
        StatementIr::OrdinaryGeneratorIf(plan) => {
            collect_expr_global_property_names(plan.condition(), names);
            for region in [plan.then_branch(), plan.else_branch()] {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
        }
        StatementIr::AsyncGeneratorIf(plan) => {
            collect_expr_global_property_names(plan.condition().value(), names);
            for region in [
                plan.condition().region(),
                plan.then_branch(),
                plan.else_branch(),
            ] {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
        }
        StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
            collect_expr_global_property_names(plan.raw_source(), names);
            for statement in &plan.body().block().statements {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::AsyncGeneratorResourceScope(plan) => {
            for statement in &plan.body().block().statements {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::AsyncGeneratorResourceRegistration(operation) => {
            collect_expr_global_property_names(operation.initializer(), names)
        }
        StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
            collect_expr_global_property_names(plan.raw_source(), names);
            for statement in &plan.body().block().statements {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::AsyncFunctionArrayDestructuring(plan) => {
            collect_expr_global_property_names(plan.raw_source(), names);
            for statement in &plan.body().statements {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::OrdinaryGeneratorWith(plan) => {
            for region in [plan.head().region(), plan.body()] {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
            collect_expr_global_property_names(plan.head().value(), names);
        }
        StatementIr::AsyncGeneratorWith(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
            collect_expr_global_property_names(plan.head().value(), names);
        }
        StatementIr::AsyncGeneratorSwitch(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
            for expression in plan.expressions() {
                collect_expr_global_property_names(expression, names);
            }
            for declaration in plan.lexical_declarations() {
                collect_statement_global_property_names(declaration, names);
            }
        }
        StatementIr::AsyncFunctionWith(plan) => {
            for block in [plan.head(), plan.body()] {
                collect_block_global_property_names(block, names);
            }
            collect_expr_global_property_names(plan.head_value(), names);
        }
        StatementIr::AsyncGeneratorForIn(plan) => {
            for block in [
                plan.head().region().block(),
                plan.initialization(),
                plan.body().block(),
            ] {
                collect_block_global_property_names(block, names);
            }
            collect_expr_global_property_names(plan.head().value(), names);
        }
        StatementIr::AsyncGeneratorForOf(plan) => {
            for block in [
                plan.head().region().block(),
                plan.initialization().block(),
                plan.body().block(),
            ] {
                collect_block_global_property_names(block, names);
            }
            collect_expr_global_property_names(plan.head().value(), names);
        }
        StatementIr::ArrayDestructuringOperation(_) => {}
        StatementIr::OrdinaryGeneratorSwitch(plan) => {
            for statement in &plan.discriminant().region().block().statements {
                collect_statement_global_property_names(statement, names);
            }
            collect_expr_global_property_names(plan.discriminant().value(), names);
            for declaration in plan.lexical_declarations() {
                collect_statement_global_property_names(declaration, names);
            }
            for case in plan.cases() {
                if let Some(selector) = case.selector() {
                    for statement in &selector.region().block().statements {
                        collect_statement_global_property_names(statement, names);
                    }
                    collect_expr_global_property_names(selector.value(), names);
                }
                for statement in &case.body().block().statements {
                    collect_statement_global_property_names(statement, names);
                }
            }
        }
        StatementIr::GeneratorLoop {
            init,
            test,
            update,
            before_suspension,
            suspension_statement,
            after_suspension,
            ..
        } => {
            if let Some(init) = init {
                collect_for_init_global_property_names(init, names);
            }
            if let Some(test) = test {
                collect_expr_global_property_names(test, names);
            }
            if let Some(update) = update {
                collect_expr_global_property_names(update, names);
            }
            for statement in before_suspension {
                collect_statement_global_property_names(statement, names);
            }
            collect_statement_global_property_names(suspension_statement, names);
            for statement in after_suspension {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::AsyncFunctionForOfIterator { iterable, plan } => {
            collect_expr_global_property_names(iterable, names);
            for statement in plan.body().statements() {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::GeneratorForOfIterator { iterable, plan } => {
            collect_expr_global_property_names(iterable, names);
            for statement in plan.body().statements() {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::GeneratorIf {
            condition,
            then_before_yield,
            then_yield_statement,
            then_after_yield,
            else_before_yield,
            else_yield_statement,
            else_after_yield,
            ..
        } => {
            collect_expr_global_property_names(condition, names);
            for statement in then_before_yield
                .iter()
                .chain(then_yield_statement.as_deref())
                .chain(then_after_yield)
                .chain(else_before_yield)
                .chain(else_yield_statement.as_deref())
                .chain(else_after_yield)
            {
                collect_statement_global_property_names(statement, names);
            }
        }
        StatementIr::ForOfIterator { iterable, body, .. } => {
            collect_expr_global_property_names(iterable, names);
            collect_statement_global_property_names(body, names);
        }
        StatementIr::ForInArray {
            target: iterable,
            body,
            ..
        }
        | StatementIr::ForInString {
            target: iterable,
            body,
            ..
        }
        | StatementIr::ForInObject {
            target: iterable,
            body,
            ..
        } => {
            collect_expr_global_property_names(iterable, names);
            collect_statement_global_property_names(body, names);
        }
        StatementIr::AsyncFunctionSwitch(plan) => {
            collect_expr_global_property_names(plan.discriminant(), names);
            for declaration in plan.lexical_declarations() {
                collect_statement_global_property_names(declaration, names);
            }
            for case in plan.cases() {
                for statement in case.condition_prefix() {
                    collect_statement_global_property_names(statement, names);
                }
                if let Some(condition) = case.condition() {
                    collect_expr_global_property_names(condition, names);
                }
                collect_block_global_property_names(case.body(), names);
            }
        }
        StatementIr::Switch {
            discriminant,
            lexical_declarations,
            cases,
            ..
        } => {
            collect_expr_global_property_names(discriminant, names);
            for declaration in lexical_declarations {
                collect_statement_global_property_names(declaration, names);
            }
            for case in cases {
                if let Some(condition) = &case.condition {
                    collect_expr_global_property_names(condition, names);
                }
                collect_block_global_property_names(&case.body, names);
            }
        }
        StatementIr::Labelled { statement, .. } => {
            collect_statement_global_property_names(statement, names);
        }
        StatementIr::SyncDisposableScope {
            resources, body, ..
        } => {
            for resource in resources.iter() {
                collect_expr_global_property_names(&resource.initializer, names);
            }
            collect_block_global_property_names(body, names);
        }
        StatementIr::AsyncDisposableScope {
            resources, body, ..
        } => {
            for resource in resources.iter() {
                collect_expr_global_property_names(resource.initializer(), names);
            }
            collect_block_global_property_names(body, names);
        }
        StatementIr::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_block_global_property_names(try_block, names);
            collect_block_global_property_names(catch_block, names);
        }
        StatementIr::TryFinally {
            try_block,
            finally_block,
            ..
        } => {
            collect_block_global_property_names(try_block, names);
            collect_block_global_property_names(finally_block, names);
        }
        StatementIr::TryCatchFinally {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            collect_block_global_property_names(try_block, names);
            collect_block_global_property_names(catch_block, names);
            collect_block_global_property_names(finally_block, names);
        }
    }
}

fn collect_for_init_global_property_names(init: &ForInitIr, names: &mut BTreeSet<String>) {
    match init {
        ForInitIr::Lexical { init, .. } | ForInitIr::Expression(init) => {
            collect_expr_global_property_names(init, names);
        }
        ForInitIr::LexicalBlock(bindings) => {
            for binding in bindings {
                collect_expr_global_property_names(&binding.init, names);
            }
        }
        ForInitIr::Var(declarators) => {
            for declarator in declarators {
                if let Some(init) = &declarator.init {
                    collect_expr_global_property_names(init, names);
                }
            }
        }
        ForInitIr::Statements(statements) => {
            for statement in statements {
                collect_statement_global_property_names(statement, names);
            }
        }
        ForInitIr::SyncDisposable(resources) => {
            for resource in resources.iter() {
                collect_expr_global_property_names(&resource.initializer, names);
            }
        }
        ForInitIr::AsyncDisposable(init) => {
            for resource in init.resources().iter() {
                collect_expr_global_property_names(resource.initializer(), names);
            }
        }
    }
}

fn collect_property_key_global_property_names(key: &PropertyKeyIr, names: &mut BTreeSet<String>) {
    match key {
        PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => {}
        PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
            collect_expr_global_property_names(expr, names);
        }
    }
}

fn collect_object_property_global_property_names(
    property: &ObjectPropertyIr,
    names: &mut BTreeSet<String>,
) {
    match property {
        ObjectPropertyIr::PrototypeSetter { value }
        | ObjectPropertyIr::Spread { source: value }
        | ObjectPropertyIr::Data { value, .. }
        | ObjectPropertyIr::NonEnumerableData { value, .. } => {
            collect_expr_global_property_names(value, names);
        }
        ObjectPropertyIr::Method { .. }
        | ObjectPropertyIr::Getter { .. }
        | ObjectPropertyIr::Setter { .. } => {}
        ObjectPropertyIr::ComputedData { key, value, .. } => {
            collect_expr_global_property_names(key, names);
            collect_expr_global_property_names(value, names);
        }
        ObjectPropertyIr::ComputedMethod { key, .. }
        | ObjectPropertyIr::ComputedGetter { key, .. }
        | ObjectPropertyIr::ComputedSetter { key, .. } => {
            collect_expr_global_property_names(key, names);
        }
    }
}

fn collect_destructuring_target_global_names(
    target: &DestructuringTargetIr,
    names: &mut BTreeSet<String>,
) {
    match target {
        DestructuringTargetIr::ResolvedVarBinding { reference, .. }
        | DestructuringTargetIr::AssignmentIdentifier(reference) => {
            if let IdentifierWriteDisposition::Global {
                referenced_name, ..
            }
            | IdentifierWriteDisposition::WithObject {
                referenced_name, ..
            }
            | IdentifierWriteDisposition::Environment {
                referenced_name, ..
            } = reference.write_disposition()
            {
                names.insert(referenced_name.to_string());
            }
        }
        DestructuringTargetIr::NestedArray(pattern) => {
            for element in &pattern.elements {
                match element {
                    ArrayDestructuringElementIr::Elision => {}
                    ArrayDestructuringElementIr::Target { target, .. }
                    | ArrayDestructuringElementIr::Rest { target } => {
                        collect_destructuring_target_global_names(target, names);
                    }
                }
            }
        }
        DestructuringTargetIr::NestedObject(pattern) => {
            for property in &pattern.properties {
                collect_destructuring_target_global_names(&property.target, names);
            }
            if let Some(rest) = &pattern.rest {
                collect_destructuring_target_global_names(rest, names);
            }
        }
        DestructuringTargetIr::Binding { .. }
        | DestructuringTargetIr::AssignmentProperty { .. }
        | DestructuringTargetIr::AssignmentPrivate { .. } => {}
        DestructuringTargetIr::AssignmentSuper { capture, put, .. } => {
            collect_expr_global_property_names(capture, names);
            collect_expr_global_property_names(put, names);
        }
    }
}

fn collect_expr_global_property_names(expr: &TypedExpr, names: &mut BTreeSet<String>) {
    match &expr.expr {
        ExprIr::ForeignCall(call) => {
            for operand in call.operands() {
                collect_expr_global_property_names(operand, names);
            }
        }
        ExprIr::ModuleEntryEvaluation(entry) => {
            collect_expr_global_property_names(entry.evaluation(), names)
        }
        ExprIr::EnvironmentIdentifier(identifier) => {
            names.insert(identifier.name.clone());
            for operand in identifier.operation.operands() {
                collect_expr_global_property_names(operand, names);
            }
        }
        ExprIr::ModuleExecutionGraph(_)
        | ExprIr::ModuleBindingRead(_)
        | ExprIr::JsonModuleValue(_)
        | ExprIr::BytesModuleValue(_)
        | ExprIr::ModuleEvaluate(_)
        | ExprIr::DeferredModuleEvaluate(_)
        | ExprIr::ModuleHasAsyncDependencies(_)
        | ExprIr::ModuleDeferredImportEvaluate(_) => {}
        ExprIr::ModuleNamespacePublish { namespace, .. } => {
            collect_expr_global_property_names(namespace, names)
        }
        ExprIr::ImportMeta { .. } => {}
        ExprIr::ModuleNamespace { exports, .. } => {
            collect_expr_global_property_names(exports, names)
        }
        ExprIr::DynamicImport {
            specifier, options, ..
        } => {
            collect_expr_global_property_names(specifier, names);
            if let Some(options) = options {
                collect_expr_global_property_names(options, names);
            }
        }
        ExprIr::Identifier(name)
        | ExprIr::GlobalPropertyRead { name }
        | ExprIr::GlobalIdentifierRead { name }
        | ExprIr::TypeOfUnresolvedIdentifier { name } => {
            names.insert(name.clone());
        }
        ExprIr::GlobalPropertyWrite { name, value, .. } => {
            names.insert(name.clone());
            collect_expr_global_property_names(value, names);
        }
        ExprIr::DeleteGlobalProperty { name, .. } => {
            names.insert(name.clone());
        }
        ExprIr::ObjectLiteral(properties) => {
            for property in properties {
                collect_object_property_global_property_names(property, names);
            }
        }
        ExprIr::ObjectPropertyDefinition(definition) => {
            collect_expr_global_property_names(definition.target(), names);
            collect_object_property_global_property_names(definition.property(), names);
        }
        ExprIr::CaptureOptionalCallReference(capture) => {
            for operand in capture.operands() {
                collect_expr_global_property_names(operand, names);
            }
        }
        ExprIr::CaptureArgumentList(capture) => {
            for argument in capture.arguments() {
                collect_expr_global_property_names(argument, names);
            }
        }
        ExprIr::CapturedArgumentList(list) => {
            collect_expr_global_property_names(list.binding(), names)
        }
        ExprIr::ArrayLiteral(elements) => {
            for element in elements {
                collect_expr_global_property_names(element, names);
            }
        }
        ExprIr::ArrayAccumulation(accumulation) => {
            for element in array_accumulation_expressions(accumulation) {
                collect_expr_global_property_names(element, names);
            }
        }
        ExprIr::AssignIdentifier { name, value }
        | ExprIr::CompoundAssignIdentifier { name, value, .. } => {
            names.insert(name.clone());
            collect_expr_global_property_names(value, names);
        }
        ExprIr::UnaryPlus { expr: value }
        | ExprIr::UnaryMinusNumeric { expr: value }
        | ExprIr::UnaryBitwiseNumeric { expr: value, .. }
        | ExprIr::Void { expr: value }
        | ExprIr::DeleteValue { expr: value }
        | ExprIr::TypeOf { expr: value }
        | ExprIr::LogicalNot { expr: value }
        | ExprIr::StringFromCharCode { code: value }
        | ExprIr::SpreadArgument(SpreadArgumentIr { value, .. })
        | ExprIr::PrivateIn { rhs: value, .. } => collect_expr_global_property_names(value, names),
        ExprIr::SpecOperation { operands, .. } => {
            for operand in operands {
                collect_expr_global_property_names(operand, names);
            }
        }
        ExprIr::PropertyRead { target, key } | ExprIr::DeleteProperty { target, key, .. } => {
            collect_expr_global_property_names(target, names);
            collect_property_key_global_property_names(key, names);
        }
        ExprIr::OrdinaryPropertyAssignment(assignment) => {
            collect_expr_global_property_names(assignment.base_and_receiver(), names);
            collect_property_key_global_property_names(assignment.referenced_name(), names);
            collect_expr_global_property_names(assignment.rhs(), names);
        }
        ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {
            collect_expr_global_property_names(assignment.base_and_receiver(), names);
            collect_property_key_global_property_names(assignment.referenced_name(), names);
            collect_expr_global_property_names(assignment.rhs(), names);
        }
        ExprIr::OrdinaryPropertyGetCapture(capture) => {
            collect_expr_global_property_names(capture.base_and_receiver(), names);
            collect_property_key_global_property_names(capture.referenced_name(), names);
        }
        ExprIr::CapturedOrdinaryPropertyWrite(write) => {
            collect_expr_global_property_names(write.rhs(), names)
        }
        ExprIr::OrdinaryPropertyNumericUpdate(update) => {
            collect_expr_global_property_names(update.base_and_receiver(), names);
            collect_property_key_global_property_names(update.referenced_name(), names);
        }
        ExprIr::OrdinaryPropertyEagerCompoundAssignment(mutation) => {
            collect_expr_global_property_names(mutation.base_and_receiver(), names);
            collect_property_key_global_property_names(mutation.referenced_name(), names);
            collect_expr_global_property_names(mutation.result(), names);
        }
        ExprIr::OptionalPropertyChain { target, chain } => {
            collect_expr_global_property_names(target, names);
            for operation in chain {
                match operation {
                    OptionalChainOperationIr::Property { key, .. } => {
                        collect_property_key_global_property_names(key, names);
                    }
                    OptionalChainOperationIr::PrivateProperty { .. } => {}
                    OptionalChainOperationIr::Call { args, .. } => {
                        for arg in args {
                            collect_expr_global_property_names(arg, names);
                        }
                    }
                }
            }
        }
        ExprIr::DeleteOptionalPropertyChain(deletion) => {
            let target = deletion.target();
            let chain = deletion.prefix();
            collect_expr_global_property_names(target, names);
            for operation in chain {
                match operation {
                    OptionalChainOperationIr::Property { key, .. } => {
                        collect_property_key_global_property_names(key, names);
                    }
                    OptionalChainOperationIr::PrivateProperty { .. } => {}
                    OptionalChainOperationIr::Call { args, .. } => {
                        for arg in args {
                            collect_expr_global_property_names(arg, names);
                        }
                    }
                }
            }

            collect_property_key_global_property_names(deletion.key(), names);
        }
        ExprIr::PropertyWrite {
            target, key, value, ..
        } => {
            collect_expr_global_property_names(target, names);
            collect_property_key_global_property_names(key, names);
            collect_expr_global_property_names(value, names);
        }
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. }
        | ExprIr::StringConcat { lhs, rhs }
        | ExprIr::CompareNumber { lhs, rhs, .. }
        | ExprIr::CompareValue { lhs, rhs, .. }
        | ExprIr::StrictEquality { lhs, rhs, .. }
        | ExprIr::LooseEquality { lhs, rhs, .. }
        | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
        | ExprIr::Comma { lhs, rhs }
        | ExprIr::InstanceOf { lhs, rhs }
        | ExprIr::In { lhs, rhs } => {
            collect_expr_global_property_names(lhs, names);
            collect_expr_global_property_names(rhs, names);
        }
        ExprIr::MaterializeBinding { value, body, .. } => {
            collect_expr_global_property_names(value, names);
            collect_expr_global_property_names(body, names);
        }
        ExprIr::ArrayDestructure { value, pattern, .. } => {
            collect_expr_global_property_names(value, names);
            pattern.visit_expressions(&mut |expr| collect_expr_global_property_names(expr, names));
            for element in &pattern.elements {
                match element {
                    ArrayDestructuringElementIr::Elision => {}
                    ArrayDestructuringElementIr::Target { target, .. }
                    | ArrayDestructuringElementIr::Rest { target } => {
                        collect_destructuring_target_global_names(target, names);
                    }
                }
            }
        }
        ExprIr::ObjectDestructure { value, pattern } => {
            collect_expr_global_property_names(value, names);
            pattern.visit_expressions(&mut |expr| collect_expr_global_property_names(expr, names));
            for property in &pattern.properties {
                collect_destructuring_target_global_names(&property.target, names);
            }
            if let Some(rest) = &pattern.rest {
                collect_destructuring_target_global_names(rest, names);
            }
        }
        ExprIr::ObjectDestructuringOperation(operation) => {
            operation
                .visit_expressions(&mut |expr| collect_expr_global_property_names(expr, names));
            if let scriptw_ir::ObjectDestructuringOperationView::PutTarget { target, .. } =
                operation.use_view()
            {
                collect_destructuring_target_global_names(target, names);
            }
        }
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr_global_property_names(condition, names);
            collect_expr_global_property_names(then_expr, names);
            collect_expr_global_property_names(else_expr, names);
        }
        ExprIr::CallNamed { args, .. } | ExprIr::SuperConstruct { args } => {
            for arg in args {
                collect_expr_global_property_names(arg, names);
            }
        }
        ExprIr::SuperNewTarget | ExprIr::SuperConstructor => {}
        ExprIr::PreparedSuperConstruct(prepared) => {
            for operand in prepared.operands() {
                collect_expr_global_property_names(operand, names);
            }
        }
        ExprIr::CallIndirect {
            callee,
            this_arg,
            args,
            ..
        } => {
            collect_expr_global_property_names(callee, names);
            if let Some(this_arg) = this_arg {
                collect_expr_global_property_names(this_arg, names);
            }
            for arg in args {
                collect_expr_global_property_names(arg, names);
            }
        }
        ExprIr::Construct { callee, args, .. } => {
            collect_expr_global_property_names(callee, names);
            for arg in args {
                collect_expr_global_property_names(arg, names);
            }
        }
        ExprIr::CallMethod {
            receiver,
            key,
            args,
        } => {
            collect_expr_global_property_names(receiver, names);
            collect_property_key_global_property_names(key, names);
            for arg in args {
                collect_expr_global_property_names(arg, names);
            }
        }
        ExprIr::SuperPropertyRead { key, receiver } => {
            collect_property_key_global_property_names(key, names);
            collect_expr_global_property_names(receiver, names);
        }
        ExprIr::SuperPropertyWrite {
            key,
            receiver,
            value,
            ..
        } => {
            collect_property_key_global_property_names(key, names);
            collect_expr_global_property_names(receiver, names);
            collect_expr_global_property_names(value, names);
        }
        ExprIr::SuperPropertyMutation(mutation) => {
            collect_property_key_global_property_names(mutation.referenced_name(), names);
            collect_expr_global_property_names(mutation.receiver(), names);
            match mutation.operation() {
                SuperPropertyMutationOperationIr::NumericUpdate { .. }
                | SuperPropertyMutationOperationIr::Capture(_) => {}
                SuperPropertyMutationOperationIr::EagerCompound { result, .. }
                | SuperPropertyMutationOperationIr::PutCaptured { value: result, .. } => {
                    collect_expr_global_property_names(result, names);
                }
            }
        }
        ExprIr::PrivateRead { target, .. } => collect_expr_global_property_names(target, names),
        ExprIr::PrivateWrite { target, value, .. } => {
            collect_expr_global_property_names(target, names);
            collect_expr_global_property_names(value, names);
        }
        ExprIr::ClassDefinition(class) => {
            if let Some(heritage) = &class.heritage {
                collect_expr_global_property_names(heritage, names);
            }
        }
        ExprIr::AssertSameValue {
            actual, expected, ..
        } => {
            collect_expr_global_property_names(actual, names);
            collect_expr_global_property_names(expected, names);
        }
        ExprIr::Undefined
        | ExprIr::ArrayHole
        | ExprIr::Null
        | ExprIr::Boolean(_)
        | ExprIr::Number(_)
        | ExprIr::BigInt(_)
        | ExprIr::Symbol { .. }
        | ExprIr::WellKnownSymbol(_)
        | ExprIr::String(_)
        | ExprIr::TemplateObject(_)
        | ExprIr::RegExpLiteral { .. }
        | ExprIr::FunctionValue(_)
        | ExprIr::ClassFieldNamedFunctionValue { .. }
        | ExprIr::This
        | ExprIr::ExecutionGlobalObject
        | ExprIr::Arguments
        | ExprIr::UpdateIdentifier { .. }
        | ExprIr::DeleteIdentifier { .. }
        | ExprIr::NewTarget
        | ExprIr::RuntimeThrow { .. } => {}
    }
}

pub(crate) fn script_references_standard_builtin(
    script: &ScriptIr,
    builtin: StandardBuiltinId,
) -> bool {
    let target = builtin.function_id();
    script
        .executable_script_bodies()
        .any(|body| block_references_function(body, &target))
        || script.functions.iter().any(|function| {
            function.params.iter().any(|param| {
                param
                    .default_init
                    .as_ref()
                    .is_some_and(|init| expr_references_function(init, &target))
            }) || block_references_function(&function.body, &target)
        })
}

/// Shared resource imports are selected from actual reachable SAB producers and
/// operations, independently of whether the script performs an atomic access.
pub(crate) fn standard_builtin_uses_shared_buffer_resource(builtin: StandardBuiltinId) -> bool {
    matches!(
        builtin,
        StandardBuiltinId::SharedArrayBufferConstructor
            | StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter
            | StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter
            | StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter
            | StandardBuiltinId::SharedArrayBufferPrototypeGrow
            | StandardBuiltinId::SharedArrayBufferPrototypeSlice
    )
}

pub(crate) fn standard_builtin_uses_memory_atomics(builtin: StandardBuiltinId) -> bool {
    matches!(
        builtin,
        StandardBuiltinId::AtomicsAdd
            | StandardBuiltinId::AtomicsAnd
            | StandardBuiltinId::AtomicsCompareExchange
            | StandardBuiltinId::AtomicsExchange
            | StandardBuiltinId::AtomicsLoad
            | StandardBuiltinId::AtomicsNotify
            | StandardBuiltinId::AtomicsOr
            | StandardBuiltinId::AtomicsStore
            | StandardBuiltinId::AtomicsSub
            | StandardBuiltinId::AtomicsWait
            | StandardBuiltinId::AtomicsWaitAsync
            | StandardBuiltinId::AtomicsXor
    )
}

pub(crate) fn script_references_memory_atomics(script: &ScriptIr) -> bool {
    StandardBuiltinId::all_functions()
        .iter()
        .copied()
        .filter(|builtin| standard_builtin_uses_memory_atomics(*builtin))
        .any(|builtin| script_references_standard_builtin(script, builtin))
}

pub(crate) fn block_references_function(block: &BlockIr, target: &FunctionId) -> bool {
    block
        .statements
        .iter()
        .any(|statement| statement_references_function(statement, target))
}

pub(crate) fn statement_references_function(statement: &StatementIr, target: &FunctionId) -> bool {
    match statement {
        StatementIr::ResumableClassDefinition(plan) => {
            expr_references_function(plan.expression(), target)
                || plan
                    .prefixes()
                    .flat_map(|prefix| prefix.statements())
                    .any(|statement| statement_references_function(statement, target))
        }
        StatementIr::ModuleImportBinding(_) => false,
        StatementIr::ModuleUnitOnce { block, .. } => block_references_function(block, target),
        StatementIr::AsyncModuleInstantiation
        | StatementIr::Empty
        | StatementIr::AnnexBFunctionCopy { .. }
        | StatementIr::Debugger
        | StatementIr::Break { .. }
        | StatementIr::Continue { .. } => false,
        StatementIr::Lexical { init, .. }
        | StatementIr::DeclarationEvaluation(init)
        | StatementIr::Expression(init)
        | StatementIr::Throw(init)
        | StatementIr::Return(init) => expr_references_function(init, target),
        StatementIr::GeneratorYield {
            value,
            form,
            resume_mode,
            ..
        } => {
            expr_references_function(value, target)
                || match resume_mode {
                    GeneratorResumeModeIr::AssignProperty(reference) => {
                        suspended_property_reference_operand_matches(reference, |expr| {
                            expr_references_function(expr, target)
                        })
                    }
                    GeneratorResumeModeIr::Ignore
                    | GeneratorResumeModeIr::Return
                    | GeneratorResumeModeIr::AssignIdentifier(_)
                    | GeneratorResumeModeIr::AssignGlobal { .. } => false,
                }
                || match form {
                    YieldForm::Plain => false,
                    YieldForm::Delegate(_) => [
                        StandardBuiltinId::ArrayPrototypeValues,
                        StandardBuiltinId::ArrayIteratorNext,
                        StandardBuiltinId::ArrayIteratorIdentity,
                        StandardBuiltinId::StringPrototypeIterator,
                        StandardBuiltinId::StringIteratorNext,
                    ]
                    .into_iter()
                    .any(|builtin| builtin.function_id() == target.as_str()),
                }
        }
        StatementIr::AsyncAwait { value, .. } => expr_references_function(value, target),
        StatementIr::EmptyStatementCompletion(item) => {
            statement_references_function(item.statement(), target)
        }
        StatementIr::LexicalBlock(statements)
        | StatementIr::ParameterInitialization { statements, .. } => statements
            .iter()
            .any(|statement| statement_references_function(statement, target)),
        StatementIr::Var(declarators) => declarators.iter().any(|declarator| {
            declarator
                .init
                .as_ref()
                .is_some_and(|init| expr_references_function(init, target))
        }),
        StatementIr::Block(block) => block_references_function(block, target),
        StatementIr::If {
            condition,
            then_branch,
            else_branch,
        }
        | StatementIr::AsyncFunctionIf {
            condition,
            then_branch,
            else_branch,
            plan: _,
        } => {
            expr_references_function(condition, target)
                || statement_references_function(then_branch, target)
                || else_branch
                    .as_deref()
                    .is_some_and(|branch| statement_references_function(branch, target))
        }
        StatementIr::AsyncFunctionWhile(plan) => {
            plan.condition_prefix()
                .iter()
                .any(|statement| statement_references_function(statement, target))
                || expr_references_function(plan.condition(), target)
                || statement_references_function(plan.body(), target)
        }
        StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
            expr_references_function(condition, target)
                || statement_references_function(body, target)
        }
        StatementIr::For {
            init,
            test,
            update,
            body,
            ..
        } => {
            init.as_ref()
                .is_some_and(|init| for_init_references_function(init, target))
                || test
                    .as_ref()
                    .is_some_and(|test| expr_references_function(test, target))
                || update
                    .as_ref()
                    .is_some_and(|update| expr_references_function(update, target))
                || statement_references_function(body, target)
        }
        StatementIr::OrdinaryGeneratorLoop(plan) => {
            plan.initialization()
                .is_some_and(|region| block_references_function(region.block(), target))
                || block_references_function(plan.test().region().block(), target)
                || expr_references_function(plan.test().value(), target)
                || block_references_function(plan.body().block(), target)
                || plan.update().is_some_and(|update| {
                    block_references_function(update.region().block(), target)
                        || expr_references_function(update.value(), target)
                })
        }
        StatementIr::AsyncGeneratorLoop(plan) => {
            plan.initialization()
                .is_some_and(|region| block_references_function(region.block(), target))
                || block_references_function(plan.test().region().block(), target)
                || expr_references_function(plan.test().value(), target)
                || block_references_function(plan.body().block(), target)
                || plan.update().is_some_and(|update| {
                    block_references_function(update.region().block(), target)
                        || expr_references_function(update.value(), target)
                })
        }
        StatementIr::OrdinaryGeneratorIf(plan) => {
            expr_references_function(plan.condition(), target)
                || block_references_function(plan.then_branch().block(), target)
                || block_references_function(plan.else_branch().block(), target)
        }
        StatementIr::AsyncGeneratorIf(plan) => {
            block_references_function(plan.condition().region().block(), target)
                || expr_references_function(plan.condition().value(), target)
                || block_references_function(plan.then_branch().block(), target)
                || block_references_function(plan.else_branch().block(), target)
        }
        StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
            *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                || *target == StandardBuiltinId::StringConstructor.function_id()
                || expr_references_function(plan.raw_source(), target)
                || block_references_function(plan.body().block(), target)
        }
        StatementIr::AsyncGeneratorResourceScope(plan) => {
            block_references_function(plan.body().block(), target)
        }
        StatementIr::AsyncGeneratorResourceRegistration(operation) => {
            expr_references_function(operation.initializer(), target)
        }
        StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
            *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                || *target == StandardBuiltinId::StringConstructor.function_id()
                || expr_references_function(plan.raw_source(), target)
                || block_references_function(plan.body().block(), target)
        }
        StatementIr::AsyncFunctionArrayDestructuring(plan) => {
            *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                || *target == StandardBuiltinId::StringConstructor.function_id()
                || expr_references_function(plan.raw_source(), target)
                || block_references_function(plan.body(), target)
        }
        StatementIr::OrdinaryGeneratorWith(plan) => {
            block_references_function(plan.head().region().block(), target)
                || expr_references_function(plan.head().value(), target)
                || block_references_function(plan.body().block(), target)
        }
        StatementIr::AsyncGeneratorWith(plan) => {
            block_references_function(plan.head().region().block(), target)
                || expr_references_function(plan.head().value(), target)
                || block_references_function(plan.body().block(), target)
        }
        StatementIr::AsyncGeneratorSwitch(plan) => {
            plan.regions()
                .any(|region| block_references_function(region.block(), target))
                || plan
                    .expressions()
                    .any(|expression| expr_references_function(expression, target))
                || plan
                    .lexical_declarations()
                    .iter()
                    .any(|declaration| statement_references_function(declaration, target))
        }
        StatementIr::AsyncFunctionWith(plan) => {
            block_references_function(plan.head(), target)
                || expr_references_function(plan.head_value(), target)
                || block_references_function(plan.body(), target)
        }
        StatementIr::AsyncGeneratorForIn(plan) => {
            block_references_function(plan.head().region().block(), target)
                || expr_references_function(plan.head().value(), target)
                || block_references_function(plan.initialization(), target)
                || block_references_function(plan.body().block(), target)
                || for_in_head_intrinsic_references_function(plan.head().value(), target)
        }
        StatementIr::AsyncGeneratorForOf(plan) => {
            block_references_function(plan.head().region().block(), target)
                || expr_references_function(plan.head().value(), target)
                || block_references_function(plan.initialization().block(), target)
                || block_references_function(plan.body().block(), target)
                || *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                || *target == StandardBuiltinId::StringConstructor.function_id()
        }
        StatementIr::ArrayDestructuringOperation(_) => false,
        StatementIr::OrdinaryGeneratorSwitch(plan) => {
            block_references_function(plan.discriminant().region().block(), target)
                || expr_references_function(plan.discriminant().value(), target)
                || plan
                    .lexical_declarations()
                    .iter()
                    .any(|statement| statement_references_function(statement, target))
                || plan.cases().iter().any(|case| {
                    case.selector().is_some_and(|selector| {
                        block_references_function(selector.region().block(), target)
                            || expr_references_function(selector.value(), target)
                    }) || block_references_function(case.body().block(), target)
                })
        }
        StatementIr::GeneratorLoop {
            init,
            test,
            update,
            before_suspension,
            suspension_statement,
            after_suspension,
            ..
        } => {
            init.as_ref()
                .is_some_and(|init| for_init_references_function(init, target))
                || test
                    .as_ref()
                    .is_some_and(|test| expr_references_function(test, target))
                || update
                    .as_ref()
                    .is_some_and(|update| expr_references_function(update, target))
                || before_suspension
                    .iter()
                    .any(|statement| statement_references_function(statement, target))
                || statement_references_function(suspension_statement, target)
                || after_suspension
                    .iter()
                    .any(|statement| statement_references_function(statement, target))
        }
        StatementIr::AsyncFunctionForOfIterator { iterable, plan } => {
            expr_references_function(iterable, target)
                || plan
                    .body()
                    .statements()
                    .iter()
                    .any(|statement| statement_references_function(statement, target))
        }
        StatementIr::GeneratorForOfIterator { iterable, plan } => {
            expr_references_function(iterable, target)
                || plan
                    .body()
                    .statements()
                    .iter()
                    .any(|statement| statement_references_function(statement, target))
        }
        StatementIr::GeneratorIf {
            condition,
            then_before_yield,
            then_yield_statement,
            then_after_yield,
            else_before_yield,
            else_yield_statement,
            else_after_yield,
            ..
        } => {
            expr_references_function(condition, target)
                || then_before_yield
                    .iter()
                    .chain(then_yield_statement.as_deref())
                    .chain(then_after_yield)
                    .chain(else_before_yield)
                    .chain(else_yield_statement.as_deref())
                    .chain(else_after_yield)
                    .any(|statement| statement_references_function(statement, target))
        }
        StatementIr::ForOfIterator { iterable, body, .. } => {
            expr_references_function(iterable, target)
                || statement_references_function(body, target)
        }
        StatementIr::ForInArray {
            target: iterable,
            body,
            ..
        }
        | StatementIr::ForInString {
            target: iterable,
            body,
            ..
        }
        | StatementIr::ForInObject {
            target: iterable,
            body,
            ..
        } => {
            expr_references_function(iterable, target)
                || statement_references_function(body, target)
                || for_in_head_intrinsic_references_function(iterable, target)
        }
        StatementIr::AsyncFunctionSwitch(plan) => {
            expr_references_function(plan.discriminant(), target)
                || plan
                    .lexical_declarations()
                    .iter()
                    .any(|declaration| statement_references_function(declaration, target))
                || plan.cases().iter().any(|case| {
                    case.condition_prefix()
                        .iter()
                        .any(|statement| statement_references_function(statement, target))
                        || case
                            .condition()
                            .is_some_and(|condition| expr_references_function(condition, target))
                        || block_references_function(case.body(), target)
                })
        }
        StatementIr::Switch {
            discriminant,
            lexical_declarations,
            cases,
            ..
        } => {
            expr_references_function(discriminant, target)
                || lexical_declarations
                    .iter()
                    .any(|declaration| statement_references_function(declaration, target))
                || cases.iter().any(|case| {
                    case.condition
                        .as_ref()
                        .is_some_and(|condition| expr_references_function(condition, target))
                        || block_references_function(&case.body, target)
                })
        }
        StatementIr::Labelled { statement, .. } => statement_references_function(statement, target),
        StatementIr::SyncDisposableScope {
            resources, body, ..
        } => {
            resources
                .iter()
                .any(|resource| expr_references_function(&resource.initializer, target))
                || block_references_function(body, target)
        }
        StatementIr::AsyncDisposableScope {
            resources, body, ..
        } => {
            resources
                .iter()
                .any(|resource| expr_references_function(resource.initializer(), target))
                || block_references_function(body, target)
        }
        StatementIr::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            block_references_function(try_block, target)
                || block_references_function(catch_block, target)
        }
        StatementIr::TryFinally {
            try_block,
            finally_block,
            ..
        } => {
            block_references_function(try_block, target)
                || block_references_function(finally_block, target)
        }
        StatementIr::TryCatchFinally {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            block_references_function(try_block, target)
                || block_references_function(catch_block, target)
                || block_references_function(finally_block, target)
        }
    }
}

fn for_in_head_intrinsic_references_function(head: &TypedExpr, target: &FunctionId) -> bool {
    FOR_IN_INTRINSICS
        .iter()
        .any(|builtin| builtin.function_id() == *target)
        || [
            (ValueKind::Number, StandardBuiltinId::NumberConstructor),
            (ValueKind::String, StandardBuiltinId::StringConstructor),
            (ValueKind::Boolean, StandardBuiltinId::BooleanConstructor),
            (ValueKind::Symbol, StandardBuiltinId::SymbolConstructor),
            (ValueKind::BigInt, StandardBuiltinId::BigIntConstructor),
        ]
        .iter()
        .any(|(kind, builtin)| {
            head.possible_kinds.contains(*kind) && builtin.function_id() == *target
        })
}

pub(crate) fn for_init_references_function(init: &ForInitIr, target: &FunctionId) -> bool {
    match init {
        ForInitIr::Lexical { init, .. } | ForInitIr::Expression(init) => {
            expr_references_function(init, target)
        }
        ForInitIr::LexicalBlock(bindings) => bindings
            .iter()
            .any(|binding| expr_references_function(&binding.init, target)),
        ForInitIr::Var(declarators) => declarators.iter().any(|declarator| {
            declarator
                .init
                .as_ref()
                .is_some_and(|init| expr_references_function(init, target))
        }),
        ForInitIr::Statements(statements) => statements
            .iter()
            .any(|statement| statement_references_function(statement, target)),
        ForInitIr::SyncDisposable(resources) => resources
            .iter()
            .any(|resource| expr_references_function(&resource.initializer, target)),
        ForInitIr::AsyncDisposable(init) => init
            .resources()
            .iter()
            .any(|resource| expr_references_function(resource.initializer(), target)),
    }
}

pub(crate) fn property_key_references_function(key: &PropertyKeyIr, target: &FunctionId) -> bool {
    match key {
        PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => false,
        PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
            expr_references_function(expr, target)
        }
    }
}

pub(crate) fn static_property_key_name(key: &PropertyKeyIr) -> Option<&str> {
    match key {
        PropertyKeyIr::StaticString(name) => Some(name),
        PropertyKeyIr::ArrayLength => Some("length"),
        PropertyKeyIr::StringExpr(_) | PropertyKeyIr::ArrayIndex(_) => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapeAccessorReferenceSelection {
    Getter,
    Setter,
    GetterOrSetter,
}

fn shape_accessor_references_function(
    shape: Option<&HeapShape>,
    key: &PropertyKeyIr,
    target: &FunctionId,
    selection: ShapeAccessorReferenceSelection,
) -> bool {
    let Some(shape) = shape else {
        return false;
    };
    if let Some(name) = static_property_key_name(key) {
        let Some(ObjectShapeProperty::Accessor { getter, setter }) =
            read_static_heap_shape_property(shape, name)
        else {
            return false;
        };
        return match selection {
            ShapeAccessorReferenceSelection::Getter => {
                getter.is_some_and(|getter| getter.function_id == *target)
            }
            ShapeAccessorReferenceSelection::Setter => {
                setter.is_some_and(|setter| setter.function_id == *target)
            }
            ShapeAccessorReferenceSelection::GetterOrSetter => {
                getter.is_some_and(|getter| getter.function_id == *target)
                    || setter.is_some_and(|setter| setter.function_id == *target)
            }
        };
    }

    fn any_accessor(
        shape: &HeapShape,
        target: &FunctionId,
        selection: ShapeAccessorReferenceSelection,
    ) -> bool {
        let (properties, prototype) = match shape {
            HeapShape::Object(shape) => (&shape.properties, shape.prototype.as_deref()),
            HeapShape::Array(shape) => (&shape.properties, shape.prototype.as_deref()),
        };
        properties.values().any(|property| {
            let ObjectShapeProperty::Accessor { getter, setter } = property else {
                return false;
            };
            match selection {
                ShapeAccessorReferenceSelection::Getter => getter
                    .as_ref()
                    .is_some_and(|getter| getter.function_id == *target),
                ShapeAccessorReferenceSelection::Setter => setter
                    .as_ref()
                    .is_some_and(|setter| setter.function_id == *target),
                ShapeAccessorReferenceSelection::GetterOrSetter => {
                    getter
                        .as_ref()
                        .is_some_and(|getter| getter.function_id == *target)
                        || setter
                            .as_ref()
                            .is_some_and(|setter| setter.function_id == *target)
                }
            }
        }) || prototype.is_some_and(|prototype| any_accessor(prototype, target, selection))
    }

    any_accessor(shape, target, selection)
}

pub(crate) fn shape_data_references_function(
    shape: Option<&HeapShape>,
    key: &PropertyKeyIr,
    target: &FunctionId,
) -> bool {
    let Some(name) = static_property_key_name(key) else {
        return false;
    };
    let Some(ObjectShapeProperty::Data(info)) =
        shape.and_then(|shape| read_static_heap_shape_property(shape, name))
    else {
        return false;
    };

    info.function_targets.known_targets().contains(target)
}

pub(crate) fn object_property_references_function(
    property: &ObjectPropertyIr,
    target: &FunctionId,
) -> bool {
    match property {
        ObjectPropertyIr::PrototypeSetter { value }
        | ObjectPropertyIr::Data { value, .. }
        | ObjectPropertyIr::NonEnumerableData { value, .. } => {
            expr_references_function(value, target)
        }
        ObjectPropertyIr::Method { function, .. }
        | ObjectPropertyIr::Getter { function, .. }
        | ObjectPropertyIr::Setter { function, .. } => function.function_id() == target,
        ObjectPropertyIr::Spread { source } => {
            expr_references_function(source, target)
                || *target == StandardBuiltinId::ReflectOwnKeys.function_id()
                || *target == StandardBuiltinId::ReflectGetOwnPropertyDescriptor.function_id()
        }
        ObjectPropertyIr::ComputedData { key, value, .. } => {
            expr_references_function(key, target) || expr_references_function(value, target)
        }
        ObjectPropertyIr::ComputedMethod { key, function }
        | ObjectPropertyIr::ComputedGetter { key, function }
        | ObjectPropertyIr::ComputedSetter { key, function } => {
            expr_references_function(key, target) || function.function_id() == target
        }
    }
}

pub(crate) fn optimized_call_method_references_function(
    key: &PropertyKeyIr,
    target: &FunctionId,
) -> bool {
    let PropertyKeyIr::StaticString(name) = key else {
        return false;
    };
    if name == "toString" {
        // Dynamically dispatched (receiver type not statically known to be a
        // literal), so no single call site pins one FunctionId. Without these
        // arms, `should_stub_standard_builtin` treats every primitive-wrapper
        // `toString` as unreferenced whenever it's only reached via runtime
        // property lookup (e.g. `computedNumber.toString(16)`), so the
        // builtin body is stubbed AND its Number.prototype/String.prototype/
        // etc. property is never installed, leaving the runtime property
        // read to resolve to `undefined` and trap on the "callee must be a
        // function" check instead of throwing.
        return StandardBuiltinId::NumberPrototypeToString.function_id() == *target
            || StandardBuiltinId::StringPrototypeToString.function_id() == *target
            || StandardBuiltinId::BooleanPrototypeToString.function_id() == *target
            || StandardBuiltinId::BigIntPrototypeToString.function_id() == *target
            || StandardBuiltinId::SymbolPrototypeToString.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeToString.function_id() == *target;
    }
    if name == "join" {
        return StandardBuiltinId::ArrayPrototypeJoin.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeJoin.function_id() == *target;
    }
    if name == "splice" {
        return StandardBuiltinId::ArrayPrototypeSplice.function_id() == *target;
    }
    if name == "fill" {
        return StandardBuiltinId::ArrayPrototypeFill.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeFill.function_id() == *target;
    }
    if name == "sort" {
        return StandardBuiltinId::ArrayPrototypeSort.function_id() == *target;
    }
    if name == "valueOf" {
        return StandardBuiltinId::NumberPrototypeValueOf.function_id() == *target
            || StandardBuiltinId::StringPrototypeValueOf.function_id() == *target
            || StandardBuiltinId::BooleanPrototypeValueOf.function_id() == *target
            || StandardBuiltinId::BigIntPrototypeValueOf.function_id() == *target
            || StandardBuiltinId::SymbolPrototypeValueOf.function_id() == *target;
    }
    if name == "toFixed" {
        return StandardBuiltinId::NumberPrototypeToFixed.function_id() == *target;
    }
    if name == "toPrecision" {
        return StandardBuiltinId::NumberPrototypeToPrecision.function_id() == *target;
    }
    if name == "toExponential" {
        return StandardBuiltinId::NumberPrototypeToExponential.function_id() == *target;
    }
    if name == "includes" {
        return StandardBuiltinId::ArrayPrototypeIncludes.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeIncludes.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeIncludes.function_id() == *target
            || StandardBuiltinId::StringPrototypeIncludes.function_id() == *target;
    }
    if name == "indexOf" {
        return StandardBuiltinId::ArrayPrototypeIndexOf.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeIndexOf.function_id() == *target
            || StandardBuiltinId::StringPrototypeIndexOf.function_id() == *target;
    }
    if name == "lastIndexOf" {
        return StandardBuiltinId::ArrayPrototypeLastIndexOf.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeLastIndexOf.function_id() == *target
            || StandardBuiltinId::StringPrototypeLastIndexOf.function_id() == *target;
    }
    if name == "find" {
        return StandardBuiltinId::ArrayPrototypeFind.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeFind.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeFind.function_id() == *target;
    }
    if name == "reduce" {
        return StandardBuiltinId::ArrayPrototypeReduce.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeReduce.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeReduce.function_id() == *target;
    }
    if name == "reduceRight" {
        return StandardBuiltinId::ArrayPrototypeReduceRight.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeReduceRight.function_id() == *target;
    }
    if name == "map" {
        return StandardBuiltinId::ArrayPrototypeMap.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeMap.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeMap.function_id() == *target;
    }
    if name == "filter" {
        return StandardBuiltinId::ArrayPrototypeFilter.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeFilter.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeFilter.function_id() == *target;
    }
    if name == "flatMap" {
        return StandardBuiltinId::ArrayPrototypeFlatMap.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeFlatMap.function_id() == *target;
    }
    if name == "take" {
        return StandardBuiltinId::IteratorPrototypeTake.function_id() == *target;
    }
    if name == "chunks" {
        return StandardBuiltinId::IteratorPrototypeChunks.function_id() == *target;
    }
    if name == "windows" {
        return StandardBuiltinId::IteratorPrototypeWindows.function_id() == *target;
    }
    if name == "drop" {
        return StandardBuiltinId::IteratorPrototypeDrop.function_id() == *target;
    }
    if name == "zip" {
        return StandardBuiltinId::IteratorZip.function_id() == *target;
    }
    if name == "zipKeyed" {
        return StandardBuiltinId::IteratorZipKeyed.function_id() == *target;
    }
    if name == "concat" {
        return StandardBuiltinId::ArrayPrototypeConcat.function_id() == *target
            || StandardBuiltinId::StringPrototypeConcat.function_id() == *target
            || StandardBuiltinId::IteratorConcat.function_id() == *target;
    }
    if name == "findIndex" {
        return StandardBuiltinId::ArrayPrototypeFindIndex.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeFindIndex.function_id() == *target;
    }
    if name == "findLast" {
        return StandardBuiltinId::ArrayPrototypeFindLast.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeFindLast.function_id() == *target;
    }
    if name == "findLastIndex" {
        return StandardBuiltinId::ArrayPrototypeFindLastIndex.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeFindLastIndex.function_id() == *target;
    }
    if name == "every" {
        return StandardBuiltinId::ArrayPrototypeEvery.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeEvery.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeEvery.function_id() == *target;
    }
    if name == "some" {
        return StandardBuiltinId::ArrayPrototypeSome.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeSome.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeSome.function_id() == *target;
    }
    if name == "forEach" {
        return StandardBuiltinId::ArrayPrototypeForEach.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeForEach.function_id() == *target
            || StandardBuiltinId::IteratorPrototypeForEach.function_id() == *target;
    }
    if name == "at" {
        return StandardBuiltinId::ArrayPrototypeAt.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeAt.function_id() == *target
            || StandardBuiltinId::StringPrototypeAt.function_id() == *target;
    }
    if name == "toReversed" {
        return StandardBuiltinId::ArrayPrototypeToReversed.function_id() == *target;
    }
    if name == "with" {
        return StandardBuiltinId::ArrayPrototypeWith.function_id() == *target;
    }
    if name == "toSpliced" {
        return StandardBuiltinId::ArrayPrototypeToSpliced.function_id() == *target;
    }
    if name == "toSorted" {
        return StandardBuiltinId::ArrayPrototypeToSorted.function_id() == *target;
    }
    if name == "reverse" {
        return StandardBuiltinId::ArrayPrototypeReverse.function_id() == *target;
    }
    if name == "copyWithin" {
        return StandardBuiltinId::ArrayPrototypeCopyWithin.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeCopyWithin.function_id() == *target;
    }
    if name == "slice" {
        return StandardBuiltinId::ArrayPrototypeSlice.function_id() == *target
            || StandardBuiltinId::StringPrototypeSlice.function_id() == *target;
    }
    if name == "toLocaleString" {
        return StandardBuiltinId::ArrayPrototypeToLocaleString.function_id() == *target
            || StandardBuiltinId::NumberPrototypeToLocaleString.function_id() == *target
            || StandardBuiltinId::BigIntPrototypeToLocaleString.function_id() == *target;
    }
    if name == "Symbol.iterator" {
        return StandardBuiltinId::ArrayPrototypeValues.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeValues.function_id() == *target
            || StandardBuiltinId::StringPrototypeIterator.function_id() == *target;
    }
    if name == "values" {
        return StandardBuiltinId::ArrayPrototypeValues.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeValues.function_id() == *target;
    }
    if name == "keys" {
        return StandardBuiltinId::ArrayPrototypeKeys.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeKeys.function_id() == *target;
    }
    if name == "entries" {
        return StandardBuiltinId::ArrayPrototypeEntries.function_id() == *target
            || StandardBuiltinId::TypedArrayPrototypeEntries.function_id() == *target;
    }
    if name == "next" {
        return StandardBuiltinId::ArrayIteratorNext.function_id() == *target
            || StandardBuiltinId::StringIteratorNext.function_id() == *target
            || StandardBuiltinId::RegExpStringIteratorNext.function_id() == *target;
    }
    let builtin = match name.as_str() {
        "join" => StandardBuiltinId::ArrayPrototypeJoin,
        "slice" => StandardBuiltinId::ArrayPrototypeSlice,
        "splice" => StandardBuiltinId::ArrayPrototypeSplice,
        "fill" => StandardBuiltinId::ArrayPrototypeFill,
        "sort" => StandardBuiltinId::ArrayPrototypeSort,
        "flat" => StandardBuiltinId::ArrayPrototypeFlat,
        "flatMap" => StandardBuiltinId::ArrayPrototypeFlatMap,
        "reduce" => StandardBuiltinId::ArrayPrototypeReduce,
        "reduceRight" => StandardBuiltinId::ArrayPrototypeReduceRight,
        "pop" => StandardBuiltinId::ArrayPrototypePop,
        "push" => StandardBuiltinId::ArrayPrototypePush,
        "shift" => StandardBuiltinId::ArrayPrototypeShift,
        "unshift" => StandardBuiltinId::ArrayPrototypeUnshift,
        "from" => StandardBuiltinId::ArrayFrom,
        "fromAsync" => StandardBuiltinId::ArrayFromAsync,
        "of" => StandardBuiltinId::ArrayOf,
        "at" => StandardBuiltinId::ArrayPrototypeAt,
        "toReversed" => StandardBuiltinId::ArrayPrototypeToReversed,
        "toSpliced" => StandardBuiltinId::ArrayPrototypeToSpliced,
        "toSorted" => StandardBuiltinId::ArrayPrototypeToSorted,
        "with" => StandardBuiltinId::ArrayPrototypeWith,
        "reverse" => StandardBuiltinId::ArrayPrototypeReverse,
        "copyWithin" => StandardBuiltinId::ArrayPrototypeCopyWithin,
        "keys" => StandardBuiltinId::ArrayPrototypeKeys,
        "entries" => StandardBuiltinId::ArrayPrototypeEntries,
        "values" => StandardBuiltinId::ArrayPrototypeValues,
        "charAt" => StandardBuiltinId::StringPrototypeCharAt,
        "charCodeAt" => StandardBuiltinId::StringPrototypeCharCodeAt,
        "codePointAt" => StandardBuiltinId::StringPrototypeCodePointAt,
        "endsWith" => StandardBuiltinId::StringPrototypeEndsWith,
        "match" => StandardBuiltinId::StringPrototypeMatch,
        "matchAll" => StandardBuiltinId::StringPrototypeMatchAll,
        "padStart" => StandardBuiltinId::StringPrototypePadStart,
        "padEnd" => StandardBuiltinId::StringPrototypePadEnd,
        "repeat" => StandardBuiltinId::StringPrototypeRepeat,
        "normalize" => StandardBuiltinId::StringPrototypeNormalize,
        "localeCompare" => StandardBuiltinId::StringPrototypeLocaleCompare,
        "isWellFormed" => StandardBuiltinId::StringPrototypeIsWellFormed,
        "toWellFormed" => StandardBuiltinId::StringPrototypeToWellFormed,
        "search" => StandardBuiltinId::StringPrototypeSearch,
        "compile" => StandardBuiltinId::RegExpPrototypeCompile,
        "exec" => StandardBuiltinId::RegExpPrototypeExec,
        "test" => StandardBuiltinId::RegExpPrototypeTest,
        "Symbol.match" => StandardBuiltinId::RegExpPrototypeSymbolMatch,
        "Symbol.matchAll" => StandardBuiltinId::RegExpPrototypeSymbolMatchAll,
        "Symbol.replace" => StandardBuiltinId::RegExpPrototypeSymbolReplace,
        "Symbol.search" => StandardBuiltinId::RegExpPrototypeSymbolSearch,
        "Symbol.split" => StandardBuiltinId::RegExpPrototypeSymbolSplit,
        "startsWith" => StandardBuiltinId::StringPrototypeStartsWith,
        "toLocaleLowerCase" => StandardBuiltinId::StringPrototypeToLocaleLowerCase,
        "toLocaleUpperCase" => StandardBuiltinId::StringPrototypeToLocaleUpperCase,
        "toLowerCase" => StandardBuiltinId::StringPrototypeToLowerCase,
        "toUpperCase" => StandardBuiltinId::StringPrototypeToUpperCase,
        _ => return false,
    };
    builtin.function_id() == *target
}

pub(crate) fn expr_references_function(expr: &TypedExpr, target: &FunctionId) -> bool {
    if expr.function_targets.known_targets().contains(target) {
        return true;
    }
    match &expr.expr {
        ExprIr::ForeignCall(call) => call
            .operands()
            .iter()
            .any(|operand| expr_references_function(operand, target)),
        ExprIr::EnvironmentIdentifier(identifier) => {
            StandardBuiltinId::from_function_id(target)
                .is_some_and(|builtin| builtin.global_name() == Some(identifier.name.as_str()))
                || identifier
                    .operation
                    .operands()
                    .any(|operand| expr_references_function(operand, target))
        }
        ExprIr::ModuleEntryEvaluation(entry) => {
            expr_references_function(entry.evaluation(), target)
        }
        ExprIr::ModuleExecutionGraph(graph) => {
            graph.initializer() == Some(target)
                || graph.realm_import_dispatcher() == Some(target)
                || graph
                    .script_import_dispatchers()
                    .values()
                    .any(|dispatcher| dispatcher == target)
                || graph
                    .activations()
                    .iter()
                    .any(|activation| activation.function() == target)
                || *target == StandardBuiltinId::GeneratorPrototypeNext.function_id()
        }
        ExprIr::ModuleEvaluate(_) => {
            *target == StandardBuiltinId::GeneratorPrototypeNext.function_id()
        }
        ExprIr::ModuleBindingRead(_)
        | ExprIr::JsonModuleValue(_)
        | ExprIr::BytesModuleValue(_)
        | ExprIr::DeferredModuleEvaluate(_)
        | ExprIr::ModuleHasAsyncDependencies(_)
        | ExprIr::ModuleDeferredImportEvaluate(_) => false,
        ExprIr::ModuleNamespacePublish { namespace, .. } => {
            expr_references_function(namespace, target)
        }
        ExprIr::ImportMeta { .. } => false,
        ExprIr::ModuleNamespace { exports, .. } => expr_references_function(exports, target),
        ExprIr::DynamicImport {
            specifier, options, ..
        } => {
            expr_references_function(specifier, target)
                || options
                    .as_deref()
                    .is_some_and(|options| expr_references_function(options, target))
        }
        ExprIr::RegExpLiteral { .. } => {
            // A literal allocates directly and never calls the mutable global
            // RegExp binding. It does, however, require the intrinsic
            // constructor's bootstrap to initialize RegExp.prototype.
            *target == StandardBuiltinId::RegExpConstructor.function_id()
        }
        ExprIr::FunctionValue(function_id)
        | ExprIr::ClassFieldNamedFunctionValue { function_id, .. } => function_id == target,
        ExprIr::ObjectLiteral(properties) => properties
            .iter()
            .any(|property| object_property_references_function(property, target)),
        ExprIr::ObjectPropertyDefinition(definition) => {
            expr_references_function(definition.target(), target)
                || object_property_references_function(definition.property(), target)
        }
        ExprIr::ArrayLiteral(elements) => elements
            .iter()
            .any(|element| expr_references_function(element, target)),
        ExprIr::ArrayAccumulation(accumulation) => {
            (array_accumulation_has_spread(accumulation)
                && (*target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                    || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                    || *target == StandardBuiltinId::StringConstructor.function_id()))
                || array_accumulation_expressions(accumulation)
                    .any(|element| expr_references_function(element, target))
        }
        ExprIr::AssignIdentifier { value, .. }
        | ExprIr::GlobalPropertyWrite { value, .. }
        | ExprIr::CompoundAssignIdentifier { value, .. }
        | ExprIr::UnaryPlus { expr: value }
        | ExprIr::UnaryMinusNumeric { expr: value }
        | ExprIr::UnaryBitwiseNumeric { expr: value, .. }
        | ExprIr::Void { expr: value }
        | ExprIr::DeleteValue { expr: value }
        | ExprIr::TypeOf { expr: value }
        | ExprIr::LogicalNot { expr: value }
        | ExprIr::StringFromCharCode { code: value }
        | ExprIr::PrivateIn { rhs: value, .. } => expr_references_function(value, target),
        ExprIr::CaptureOptionalCallReference(capture) => capture
            .operands()
            .iter()
            .any(|operand| expr_references_function(operand, target)),
        ExprIr::CaptureArgumentList(capture) => capture
            .arguments()
            .iter()
            .any(|argument| expr_references_function(argument, target)),
        ExprIr::CapturedArgumentList(list) => expr_references_function(list.binding(), target),
        ExprIr::SpreadArgument(spread) => {
            *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                || *target == StandardBuiltinId::StringConstructor.function_id()
                || expr_references_function(&spread.value, target)
        }
        ExprIr::SpecOperation {
            operation,
            operands,
        } => {
            operands
                .iter()
                .any(|operand| expr_references_function(operand, target))
                || matches!(operation, SpecOperationIr::ToObject)
                    && operands.first().is_some_and(|operand| {
                        [
                            (ValueKind::Number, StandardBuiltinId::NumberConstructor),
                            (ValueKind::String, StandardBuiltinId::StringConstructor),
                            (ValueKind::Boolean, StandardBuiltinId::BooleanConstructor),
                            (ValueKind::Symbol, StandardBuiltinId::SymbolConstructor),
                            (ValueKind::BigInt, StandardBuiltinId::BigIntConstructor),
                        ]
                        .iter()
                        .any(|(kind, builtin)| {
                            operand.possible_kinds.contains(*kind)
                                && builtin.function_id() == *target
                        })
                    })
                || matches!(operation, SpecOperationIr::CopyDataProperties)
                    && (*target == StandardBuiltinId::ReflectOwnKeys.function_id()
                        || *target
                            == StandardBuiltinId::ReflectGetOwnPropertyDescriptor.function_id())
                || matches!(
                    operation,
                    SpecOperationIr::Get | SpecOperationIr::GetV | SpecOperationIr::GetMethod
                ) && operands.get(1).is_some_and(|key| {
                    let ExprIr::String(name) = &key.expr else {
                        return false;
                    };
                    optimized_call_method_references_function(
                        &PropertyKeyIr::StaticString(name.clone()),
                        target,
                    )
                })
        }
        ExprIr::OptionalPropertyChain {
            target: object,
            chain,
        } => {
            expr_references_function(object, target)
                || chain.iter().any(|operation| match operation {
                    OptionalChainOperationIr::Property { key, .. } => {
                        property_key_references_function(key, target)
                            || optimized_call_method_references_function(key, target)
                            || shape_data_references_function(
                                object.heap_shape.as_deref(),
                                key,
                                target,
                            )
                            || shape_accessor_references_function(
                                object.heap_shape.as_deref(),
                                key,
                                target,
                                ShapeAccessorReferenceSelection::Getter,
                            )
                    }
                    OptionalChainOperationIr::PrivateProperty { .. } => false,
                    OptionalChainOperationIr::Call { args, .. } => {
                        args.iter().any(|arg| expr_references_function(arg, target))
                    }
                })
        }
        ExprIr::DeleteOptionalPropertyChain(deletion) => {
            let object = deletion.target();
            let chain = deletion.prefix();
            property_key_references_function(deletion.key(), target)
                || expr_references_function(object, target)
                || chain.iter().any(|operation| match operation {
                    OptionalChainOperationIr::Property { key, .. } => {
                        property_key_references_function(key, target)
                            || optimized_call_method_references_function(key, target)
                            || shape_data_references_function(
                                object.heap_shape.as_deref(),
                                key,
                                target,
                            )
                            || shape_accessor_references_function(
                                object.heap_shape.as_deref(),
                                key,
                                target,
                                ShapeAccessorReferenceSelection::Getter,
                            )
                    }
                    OptionalChainOperationIr::PrivateProperty { .. } => false,
                    OptionalChainOperationIr::Call { args, .. } => {
                        args.iter().any(|arg| expr_references_function(arg, target))
                    }
                })
        }
        ExprIr::PropertyRead {
            target: object,
            key,
        } => {
            expr_references_function(object, target)
                || property_key_references_function(key, target)
                || optimized_call_method_references_function(key, target)
                || shape_data_references_function(object.heap_shape.as_deref(), key, target)
                || shape_accessor_references_function(
                    object.heap_shape.as_deref(),
                    key,
                    target,
                    ShapeAccessorReferenceSelection::Getter,
                )
        }
        ExprIr::DeleteProperty {
            target: object,
            key,
            ..
        } => {
            expr_references_function(object, target)
                || property_key_references_function(key, target)
        }
        ExprIr::OrdinaryPropertyAssignment(assignment) => {
            expr_references_function(assignment.base_and_receiver(), target)
                || property_key_references_function(assignment.referenced_name(), target)
                || expr_references_function(assignment.rhs(), target)
                || assignment.possible_setters().contains(target)
                || shape_accessor_references_function(
                    assignment.base_and_receiver().heap_shape.as_deref(),
                    assignment.referenced_name(),
                    target,
                    ShapeAccessorReferenceSelection::Setter,
                )
        }
        ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {
            expr_references_function(assignment.base_and_receiver(), target)
                || property_key_references_function(assignment.referenced_name(), target)
                || expr_references_function(assignment.rhs(), target)
                || assignment.possible_getters().contains(target)
                || assignment.possible_setters().contains(target)
                || shape_accessor_references_function(
                    assignment.base_and_receiver().heap_shape.as_deref(),
                    assignment.referenced_name(),
                    target,
                    ShapeAccessorReferenceSelection::GetterOrSetter,
                )
        }
        ExprIr::OrdinaryPropertyGetCapture(capture) => {
            expr_references_function(capture.base_and_receiver(), target)
                || property_key_references_function(capture.referenced_name(), target)
                || capture.possible_getters().contains(target)
                || shape_accessor_references_function(
                    capture.base_and_receiver().heap_shape.as_deref(),
                    capture.referenced_name(),
                    target,
                    ShapeAccessorReferenceSelection::Getter,
                )
        }
        ExprIr::CapturedOrdinaryPropertyWrite(write) => {
            expr_references_function(write.rhs(), target)
                || write.possible_setters().contains(target)
        }
        ExprIr::OrdinaryPropertyNumericUpdate(update) => {
            expr_references_function(update.base_and_receiver(), target)
                || property_key_references_function(update.referenced_name(), target)
                || update.possible_getters().contains(target)
                || update.possible_setters().contains(target)
                || shape_accessor_references_function(
                    update.base_and_receiver().heap_shape.as_deref(),
                    update.referenced_name(),
                    target,
                    ShapeAccessorReferenceSelection::GetterOrSetter,
                )
        }
        ExprIr::OrdinaryPropertyEagerCompoundAssignment(mutation) => {
            expr_references_function(mutation.base_and_receiver(), target)
                || property_key_references_function(mutation.referenced_name(), target)
                || expr_references_function(mutation.result(), target)
                || mutation.possible_getters().contains(target)
                || mutation.possible_setters().contains(target)
                || shape_accessor_references_function(
                    mutation.base_and_receiver().heap_shape.as_deref(),
                    mutation.referenced_name(),
                    target,
                    ShapeAccessorReferenceSelection::GetterOrSetter,
                )
        }
        ExprIr::PropertyWrite {
            target: object,
            key,
            value,
            ..
        } => {
            expr_references_function(object, target)
                || property_key_references_function(key, target)
                || expr_references_function(value, target)
                || shape_accessor_references_function(
                    object.heap_shape.as_deref(),
                    key,
                    target,
                    ShapeAccessorReferenceSelection::Setter,
                )
        }
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. }
        | ExprIr::StringConcat { lhs, rhs }
        | ExprIr::CompareNumber { lhs, rhs, .. }
        | ExprIr::CompareValue { lhs, rhs, .. }
        | ExprIr::StrictEquality { lhs, rhs, .. }
        | ExprIr::LooseEquality { lhs, rhs, .. }
        | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
        | ExprIr::Comma { lhs, rhs }
        | ExprIr::InstanceOf { lhs, rhs }
        | ExprIr::In { lhs, rhs } => {
            expr_references_function(lhs, target) || expr_references_function(rhs, target)
        }
        ExprIr::MaterializeBinding { value, body, .. } => {
            expr_references_function(value, target) || expr_references_function(body, target)
        }
        ExprIr::ArrayDestructure { value, pattern, .. } => {
            *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                || *target == StandardBuiltinId::StringConstructor.function_id()
                || expr_references_function(value, target)
                || array_destructuring_pattern_any_expression(pattern, |expr| {
                    expr_references_function(expr, target)
                })
        }
        ExprIr::ObjectDestructure { value, pattern } => {
            (pattern.rest.is_some()
                && (*target == StandardBuiltinId::ReflectOwnKeys.function_id()
                    || *target == StandardBuiltinId::ReflectGetOwnPropertyDescriptor.function_id()))
                || expr_references_function(value, target)
                || object_destructuring_pattern_any_expression(pattern, |expr| {
                    expr_references_function(expr, target)
                })
        }
        ExprIr::ObjectDestructuringOperation(operation) => {
            use scriptw_ir::ObjectDestructuringOperationView;
            let mut referenced = match operation.use_view() {
                ObjectDestructuringOperationView::Rest { .. } => {
                    *target == StandardBuiltinId::ReflectOwnKeys.function_id()
                        || *target
                            == StandardBuiltinId::ReflectGetOwnPropertyDescriptor.function_id()
                }
                ObjectDestructuringOperationView::PutTarget {
                    target: DestructuringTargetIr::NestedArray(_),
                    ..
                } => {
                    *target == StandardBuiltinId::ArrayPrototypeValues.function_id()
                        || *target == StandardBuiltinId::ArrayIteratorNext.function_id()
                        || *target == StandardBuiltinId::StringConstructor.function_id()
                        || *target == StandardBuiltinId::ReflectOwnKeys.function_id()
                        || *target
                            == StandardBuiltinId::ReflectGetOwnPropertyDescriptor.function_id()
                }
                ObjectDestructuringOperationView::GetV { .. }
                | ObjectDestructuringOperationView::PutTarget { .. } => false,
            };
            operation.visit_expressions(&mut |expr| {
                referenced |= expr_references_function(expr, target)
            });
            referenced
        }
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_references_function(condition, target)
                || expr_references_function(then_expr, target)
                || expr_references_function(else_expr, target)
        }
        ExprIr::CallNamed { name, args } => {
            name == target || args.iter().any(|arg| expr_references_function(arg, target))
        }
        ExprIr::SuperConstruct { args } => {
            args.iter().any(|arg| expr_references_function(arg, target))
        }
        ExprIr::SuperNewTarget | ExprIr::SuperConstructor => false,
        ExprIr::PreparedSuperConstruct(prepared) => prepared
            .operands()
            .any(|operand| expr_references_function(operand, target)),
        ExprIr::CallIndirect {
            callee,
            this_arg,
            args,
            ..
        } => {
            expr_references_function(callee, target)
                || this_arg
                    .as_deref()
                    .is_some_and(|this_arg| expr_references_function(this_arg, target))
                || args.iter().any(|arg| expr_references_function(arg, target))
        }
        ExprIr::Construct { callee, args, .. } => {
            expr_references_function(callee, target)
                || args.iter().any(|arg| expr_references_function(arg, target))
        }
        ExprIr::CallMethod {
            receiver,
            key,
            args,
        } => {
            expr_references_function(receiver, target)
                || property_key_references_function(key, target)
                || shape_data_references_function(receiver.heap_shape.as_deref(), key, target)
                || optimized_call_method_references_function(key, target)
                || args.iter().any(|arg| expr_references_function(arg, target))
        }
        ExprIr::SuperPropertyRead { key, receiver } => {
            property_key_references_function(key, target)
                || expr_references_function(receiver, target)
        }
        ExprIr::SuperPropertyWrite {
            key,
            receiver,
            value,
            ..
        } => {
            property_key_references_function(key, target)
                || expr_references_function(receiver, target)
                || expr_references_function(value, target)
        }
        ExprIr::SuperPropertyMutation(mutation) => {
            property_key_references_function(mutation.referenced_name(), target)
                || expr_references_function(mutation.receiver(), target)
                || match mutation.operation() {
                    SuperPropertyMutationOperationIr::NumericUpdate { .. }
                    | SuperPropertyMutationOperationIr::Capture(_) => false,
                    SuperPropertyMutationOperationIr::EagerCompound { result, .. }
                    | SuperPropertyMutationOperationIr::PutCaptured { value: result, .. } => {
                        expr_references_function(result, target)
                    }
                }
        }
        ExprIr::PrivateRead { target: object, .. } => expr_references_function(object, target),
        ExprIr::PrivateWrite {
            target: object,
            value,
            ..
        } => expr_references_function(object, target) || expr_references_function(value, target),
        ExprIr::ClassDefinition(class) => class
            .heritage
            .as_deref()
            .is_some_and(|heritage| expr_references_function(heritage, target)),
        ExprIr::AssertSameValue {
            actual, expected, ..
        } => expr_references_function(actual, target) || expr_references_function(expected, target),
        ExprIr::Undefined
        | ExprIr::ArrayHole
        | ExprIr::Null
        | ExprIr::Boolean(_)
        | ExprIr::Number(_)
        | ExprIr::BigInt(_)
        | ExprIr::Symbol { .. }
        | ExprIr::WellKnownSymbol(_)
        | ExprIr::String(_)
        | ExprIr::TemplateObject(_)
        | ExprIr::This
        | ExprIr::ExecutionGlobalObject
        | ExprIr::Arguments
        | ExprIr::Identifier(_)
        | ExprIr::GlobalPropertyRead { .. }
        | ExprIr::GlobalIdentifierRead { .. }
        | ExprIr::UpdateIdentifier { .. }
        | ExprIr::DeleteIdentifier { .. }
        | ExprIr::DeleteGlobalProperty { .. }
        | ExprIr::NewTarget
        | ExprIr::TypeOfUnresolvedIdentifier { .. }
        | ExprIr::RuntimeThrow { .. } => false,
    }
}

pub(crate) struct NumberPowImportFunctionIndex(u32);
pub(crate) struct WallClockMillisImportFunctionIndex(u32);
pub(crate) struct MonotonicClockNanosImportFunctionIndex(u32);
pub(crate) struct SleepNanosImportFunctionIndex(u32);
pub(crate) struct AgentCallImportFunctionIndex(u32);
pub(crate) struct RandomF64ImportFunctionIndex(u32);
pub(crate) struct MathAcosImportFunctionIndex(u32);
pub(crate) struct MathAcoshImportFunctionIndex(u32);
pub(crate) struct MathAsinImportFunctionIndex(u32);
pub(crate) struct MathAsinhImportFunctionIndex(u32);
pub(crate) struct MathAtanImportFunctionIndex(u32);
pub(crate) struct MathAtanhImportFunctionIndex(u32);
pub(crate) struct MathCbrtImportFunctionIndex(u32);
pub(crate) struct MathCosImportFunctionIndex(u32);
pub(crate) struct MathCoshImportFunctionIndex(u32);
pub(crate) struct MathExpImportFunctionIndex(u32);
pub(crate) struct MathExpm1ImportFunctionIndex(u32);
pub(crate) struct MathLogImportFunctionIndex(u32);
pub(crate) struct MathLog10ImportFunctionIndex(u32);
pub(crate) struct MathLog1pImportFunctionIndex(u32);
pub(crate) struct MathLog2ImportFunctionIndex(u32);
pub(crate) struct MathSinImportFunctionIndex(u32);
pub(crate) struct MathSinhImportFunctionIndex(u32);
pub(crate) struct MathTanImportFunctionIndex(u32);
pub(crate) struct MathTanhImportFunctionIndex(u32);
pub(crate) struct MathAtan2ImportFunctionIndex(u32);
pub(crate) struct RejectRuntimeSemanticsImportFunctionIndex(u32);

macro_rules! host_import_function_index_role {
    ($role:ident) => {
        impl $role {
            pub(crate) const fn new(index: u32) -> Self {
                Self(index)
            }
        }
    };
}

host_import_function_index_role!(NumberPowImportFunctionIndex);
host_import_function_index_role!(WallClockMillisImportFunctionIndex);
host_import_function_index_role!(MonotonicClockNanosImportFunctionIndex);
host_import_function_index_role!(SleepNanosImportFunctionIndex);
host_import_function_index_role!(AgentCallImportFunctionIndex);
host_import_function_index_role!(RandomF64ImportFunctionIndex);
host_import_function_index_role!(MathAcosImportFunctionIndex);
host_import_function_index_role!(MathAcoshImportFunctionIndex);
host_import_function_index_role!(MathAsinImportFunctionIndex);
host_import_function_index_role!(MathAsinhImportFunctionIndex);
host_import_function_index_role!(MathAtanImportFunctionIndex);
host_import_function_index_role!(MathAtanhImportFunctionIndex);
host_import_function_index_role!(MathCbrtImportFunctionIndex);
host_import_function_index_role!(MathCosImportFunctionIndex);
host_import_function_index_role!(MathCoshImportFunctionIndex);
host_import_function_index_role!(MathExpImportFunctionIndex);
host_import_function_index_role!(MathExpm1ImportFunctionIndex);
host_import_function_index_role!(MathLogImportFunctionIndex);
host_import_function_index_role!(MathLog10ImportFunctionIndex);
host_import_function_index_role!(MathLog1pImportFunctionIndex);
host_import_function_index_role!(MathLog2ImportFunctionIndex);
host_import_function_index_role!(MathSinImportFunctionIndex);
host_import_function_index_role!(MathSinhImportFunctionIndex);
host_import_function_index_role!(MathTanImportFunctionIndex);
host_import_function_index_role!(MathTanhImportFunctionIndex);
host_import_function_index_role!(MathAtan2ImportFunctionIndex);
host_import_function_index_role!(RejectRuntimeSemanticsImportFunctionIndex);

#[must_use]
pub(crate) struct HostImportFunctionIndices {
    gc_imports: crate::gc_types::GcHostImports,
    number_pow: Option<NumberPowImportFunctionIndex>,
    wall_clock_millis: Option<WallClockMillisImportFunctionIndex>,
    monotonic_clock_nanos: Option<MonotonicClockNanosImportFunctionIndex>,
    sleep_nanos: Option<SleepNanosImportFunctionIndex>,
    agent_call: Option<AgentCallImportFunctionIndex>,
    random_f64: Option<RandomF64ImportFunctionIndex>,
    math_acos: Option<MathAcosImportFunctionIndex>,
    math_acosh: Option<MathAcoshImportFunctionIndex>,
    math_asin: Option<MathAsinImportFunctionIndex>,
    math_asinh: Option<MathAsinhImportFunctionIndex>,
    math_atan: Option<MathAtanImportFunctionIndex>,
    math_atanh: Option<MathAtanhImportFunctionIndex>,
    math_cbrt: Option<MathCbrtImportFunctionIndex>,
    math_cos: Option<MathCosImportFunctionIndex>,
    math_cosh: Option<MathCoshImportFunctionIndex>,
    math_exp: Option<MathExpImportFunctionIndex>,
    math_expm1: Option<MathExpm1ImportFunctionIndex>,
    math_log: Option<MathLogImportFunctionIndex>,
    math_log10: Option<MathLog10ImportFunctionIndex>,
    math_log1p: Option<MathLog1pImportFunctionIndex>,
    math_log2: Option<MathLog2ImportFunctionIndex>,
    math_sin: Option<MathSinImportFunctionIndex>,
    math_sinh: Option<MathSinhImportFunctionIndex>,
    math_tan: Option<MathTanImportFunctionIndex>,
    math_tanh: Option<MathTanhImportFunctionIndex>,
    math_atan2: Option<MathAtan2ImportFunctionIndex>,
    reject_runtime_semantics: RejectRuntimeSemanticsImportFunctionIndex,
}

impl HostImportFunctionIndices {
    pub(crate) fn with_gc_imports(mut self, imports: crate::gc_types::GcHostImports) -> Self {
        self.gc_imports = imports;
        self
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        number_pow: Option<NumberPowImportFunctionIndex>,
        wall_clock_millis: Option<WallClockMillisImportFunctionIndex>,
        monotonic_clock_nanos: Option<MonotonicClockNanosImportFunctionIndex>,
        sleep_nanos: Option<SleepNanosImportFunctionIndex>,
        agent_call: Option<AgentCallImportFunctionIndex>,
        random_f64: Option<RandomF64ImportFunctionIndex>,
        math_acos: Option<MathAcosImportFunctionIndex>,
        math_acosh: Option<MathAcoshImportFunctionIndex>,
        math_asin: Option<MathAsinImportFunctionIndex>,
        math_asinh: Option<MathAsinhImportFunctionIndex>,
        math_atan: Option<MathAtanImportFunctionIndex>,
        math_atanh: Option<MathAtanhImportFunctionIndex>,
        math_cbrt: Option<MathCbrtImportFunctionIndex>,
        math_cos: Option<MathCosImportFunctionIndex>,
        math_cosh: Option<MathCoshImportFunctionIndex>,
        math_exp: Option<MathExpImportFunctionIndex>,
        math_expm1: Option<MathExpm1ImportFunctionIndex>,
        math_log: Option<MathLogImportFunctionIndex>,
        math_log10: Option<MathLog10ImportFunctionIndex>,
        math_log1p: Option<MathLog1pImportFunctionIndex>,
        math_log2: Option<MathLog2ImportFunctionIndex>,
        math_sin: Option<MathSinImportFunctionIndex>,
        math_sinh: Option<MathSinhImportFunctionIndex>,
        math_tan: Option<MathTanImportFunctionIndex>,
        math_tanh: Option<MathTanhImportFunctionIndex>,
        math_atan2: Option<MathAtan2ImportFunctionIndex>,
        reject_runtime_semantics: RejectRuntimeSemanticsImportFunctionIndex,
    ) -> Self {
        Self {
            gc_imports: crate::gc_types::GcHostImports::empty(),
            number_pow,
            wall_clock_millis,
            monotonic_clock_nanos,
            sleep_nanos,
            agent_call,
            random_f64,
            math_acos,
            math_acosh,
            math_asin,
            math_asinh,
            math_atan,
            math_atanh,
            math_cbrt,
            math_cos,
            math_cosh,
            math_exp,
            math_expm1,
            math_log,
            math_log10,
            math_log1p,
            math_log2,
            math_sin,
            math_sinh,
            math_tan,
            math_tanh,
            math_atan2,
            reject_runtime_semantics,
        }
    }
}

/// Only the runtime schema's checked unit projection mints a once guard.
/// The token denotes numeric process state, never a semantic reference.
#[derive(Clone, Copy)]
pub(crate) struct ModuleUnitGuard {
    global_index: u32,
}
impl ModuleUnitGuard {
    pub(crate) const fn new(global_index: u32) -> Self {
        Self { global_index }
    }
    pub(crate) fn load(self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.global_index));
    }
    pub(crate) fn mark_started(self, function: &mut Function) {
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::GlobalSet(self.global_index));
    }
}

pub(crate) struct FunctionMetaRegistry {
    /// Each cache kind has a dense namespace shared by all source bodies in P.
    next_compact_shape_site: std::cell::Cell<u32>,
    next_own_data_read_site: std::cell::Cell<u32>,
    foreign_imports: crate::foreign::ForeignImportPlan,
    module_graph: Option<ModuleGraphPlan>,
    prepared_dynamic_functions: Vec<scriptw_ir::PreparedDynamicFunction>,
    prepared_scripts: Vec<PreparedScript>,
    metas: BTreeMap<FunctionId, WasmFunctionMeta>,
    /// The host builtins the script's own host surface names. Every host
    /// builtin is compiled, but only these resolve from a bare identifier.
    host_surface: BTreeSet<HostBuiltinId>,
    host_import_function_indices: HostImportFunctionIndices,
}

impl FunctionMetaRegistry {
    pub(crate) fn next_program_site_index(
        &self,
        kind: crate::emit::ProgramSiteKind,
    ) -> Result<u32, EmitError> {
        let next = match kind {
            crate::emit::ProgramSiteKind::CompactShape => &self.next_compact_shape_site,
            crate::emit::ProgramSiteKind::OwnDataRead => &self.next_own_data_read_site,
        };
        let site = next.get();
        next.set(
            site.checked_add(1)
                .ok_or_else(|| EmitError::unsupported("program cache sites exceed u32"))?,
        );
        Ok(site)
    }

    pub(crate) fn with_foreign_imports(mut self, plan: crate::foreign::ForeignImportPlan) -> Self {
        self.foreign_imports = plan;
        self
    }

    pub(crate) fn foreign_imports(&self) -> &crate::foreign::ForeignImportPlan {
        &self.foreign_imports
    }
    pub(crate) fn gc_host_imports(&self) -> &crate::gc_types::GcHostImports {
        &self.host_import_function_indices.gc_imports
    }
    pub(crate) fn gc_host_import(
        &self,
        import: crate::gc_types::GcHostImport,
    ) -> Option<crate::gc_types::DeclaredGcHostImport> {
        self.gc_host_imports().get(import)
    }
    pub(crate) fn module_execution_record_count(&self) -> u32 {
        self.module_graph
            .as_ref()
            .map_or(0, ModuleGraphPlan::record_count)
    }
    pub(crate) fn module_graph(&self) -> Option<&ModuleGraphPlan> {
        self.module_graph.as_ref()
    }

    pub(crate) fn prepared_scripts(&self) -> &[PreparedScript] {
        &self.prepared_scripts
    }

    pub(crate) fn prepared_dynamic_functions(&self) -> &[scriptw_ir::PreparedDynamicFunction] {
        &self.prepared_dynamic_functions
    }

    pub(crate) fn new(
        metas: BTreeMap<FunctionId, WasmFunctionMeta>,
        host_surface: BTreeSet<HostBuiltinId>,
        host_import_function_indices: HostImportFunctionIndices,
        prepared_dynamic_functions: Vec<scriptw_ir::PreparedDynamicFunction>,
        prepared_scripts: Vec<PreparedScript>,
        module_graph: Option<ModuleGraphPlan>,
    ) -> Self {
        Self {
            next_compact_shape_site: std::cell::Cell::new(0),
            next_own_data_read_site: std::cell::Cell::new(0),
            foreign_imports: crate::foreign::ForeignImportPlan::default(),
            module_graph,
            prepared_dynamic_functions,
            prepared_scripts,
            metas,
            host_surface,
            host_import_function_indices,
        }
    }

    pub(crate) fn number_pow_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .number_pow
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn wall_clock_millis_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .wall_clock_millis
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn monotonic_clock_nanos_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .monotonic_clock_nanos
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn sleep_nanos_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .sleep_nanos
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn agent_call_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .agent_call
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn random_f64_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .random_f64
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_acos_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_acos
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_acosh_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_acosh
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_asin_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_asin
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_asinh_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_asinh
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_atan_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_atan
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_atanh_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_atanh
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_cbrt_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_cbrt
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_cos_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_cos
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_cosh_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_cosh
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_exp_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_exp
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_expm1_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_expm1
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_log_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_log
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_log10_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_log10
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_log1p_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_log1p
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_log2_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_log2
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_sin_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_sin
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_sinh_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_sinh
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_tan_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_tan
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_tanh_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_tanh
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn math_atan2_import_function_index(&self) -> Option<u32> {
        self.host_import_function_indices
            .math_atan2
            .as_ref()
            .map(|index| index.0)
    }

    pub(crate) fn reject_runtime_semantics_import_function_index(&self) -> u32 {
        self.host_import_function_indices.reject_runtime_semantics.0
    }

    pub(crate) fn get(&self, function_id: &str) -> Option<&WasmFunctionMeta> {
        self.metas.get(function_id)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &WasmFunctionMeta> {
        self.metas.values()
    }

    pub(crate) fn metas(&self) -> &BTreeMap<FunctionId, WasmFunctionMeta> {
        &self.metas
    }

    pub(crate) fn host_surface_contains(&self, builtin: HostBuiltinId) -> bool {
        self.host_surface.contains(&builtin)
    }
}

pub(crate) fn build_function_metas<'a>(
    source: &SourceFunctions<'_>,
    prepared_units: impl Iterator<Item = &'a PreparedScriptUnit>,
    standard_builtins: &[StandardBuiltinId],
    host_builtins: &[HostBuiltinId],
    layout: &FunctionIndexLayout,
) -> Result<BTreeMap<FunctionId, WasmFunctionMeta>, EmitError> {
    let mut metas = BTreeMap::new();
    let mut insert_meta = |id: FunctionId, meta: WasmFunctionMeta| match metas.entry(id) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(meta);
            Ok(())
        }
        std::collections::btree_map::Entry::Occupied(entry) => {
            Err(EmitError::unsupported(format!(
                "compiler invariant: duplicate function identity `{}`",
                entry.key()
            )))
        }
    };
    let source_functions = source
        .runtime_owned
        .iter()
        .enumerate()
        .map(|(position, function)| (*function, layout.runtime_owned_index(position)))
        .chain(
            source
                .program
                .iter()
                .enumerate()
                .map(|(position, function)| (*function, layout.program_callable_index(position))),
        );
    for (function, wasm_index) in source_functions {
        insert_meta(
            function.id.clone(),
            WasmFunctionMeta {
                constant_leaf: crate::functions::ConstantLeafPlan::for_function(function),
                numeric_leaf: source.numeric_leaf(function).cloned(),
                async_generator_resume_environment_plan: function
                    .resumable_plan
                    .as_ref()
                    .map(|plan| plan.resume_environment_plan().clone()),
                template_source: function.template_source,
                name: function.name.clone(),
                to_string_value: function.to_string_representation.materialize(),
                standard_builtin: None,
                host_builtin: None,
                length: function_length(&function.params),
                length_name_configurable: true,
                entry: crate::function_entry::PlannedFunctionEntry::user(
                    function,
                    wasm_index,
                    source.binary_numeric_entry(function, layout),
                ),
                strict: function.strict,
                is_named_expression: function.is_named_expression,
                class_element_execution_kind: function.class_element_execution_kind,
                class_heritage_kind: function.class_heritage_kind,
                is_static_class_member: function.is_static_class_member,
                is_derived_constructor: function.is_derived_constructor,
                is_synthetic_default_derived_constructor: function
                    .is_synthetic_default_derived_constructor,
                class_instance_element_plan: function.class_instance_element_plan.clone(),
                uses_super: function.uses_super,
                this_before_super: function.this_before_super,
                captures_private_environment: function.captures_private_environment,
                needs_active_function_identity: function.protocol.flavor()
                    == FunctionFlavor::Ordinary,
            },
        )?;
    }

    let prepared_base = source.program.len();
    for (position, unit) in prepared_units.enumerate() {
        insert_meta(
            unit.id.function_id(),
            WasmFunctionMeta {
                constant_leaf: None,
                numeric_leaf: None,
                // Prepared Script thunks receive a lexical environment directly;
                // their entry creates its own source instance.
                async_generator_resume_environment_plan: None,
                template_source: None,
                name: unit.id.function_id(),
                to_string_value: String::new(),
                standard_builtin: None,
                host_builtin: None,
                length: 0,
                length_name_configurable: false,
                entry: crate::function_entry::PlannedFunctionEntry::prepared(
                    unit,
                    layout.program_callable_index(prepared_base + position),
                ),
                strict: unit.strict,
                is_named_expression: false,
                class_element_execution_kind: ClassElementExecutionKind::None,
                class_heritage_kind: ClassHeritageKind::None,
                is_static_class_member: false,
                is_derived_constructor: false,
                is_synthetic_default_derived_constructor: false,
                class_instance_element_plan: None,
                uses_super: false,
                this_before_super: false,
                captures_private_environment: false,
                needs_active_function_identity: false,
            },
        )?;
    }

    let standard_builtin_meta = |builtin: StandardBuiltinId, wasm_index: u32| WasmFunctionMeta {
        constant_leaf: None,
        numeric_leaf: None,
        async_generator_resume_environment_plan: None,
        template_source: None,
        name: builtin
            .native_function_name()
            .unwrap_or_else(|| builtin.debug_name())
            .to_string(),
        to_string_value: builtin
            .native_function_name()
            .map(|name| CallableToStringRepresentation::NativeNamed(name.to_string()).materialize())
            .unwrap_or_else(|| CallableToStringRepresentation::NativeAnonymous.materialize()),
        standard_builtin: Some(builtin),
        host_builtin: None,
        length: standard_builtin_length(builtin),
        length_name_configurable: !matches!(builtin, StandardBuiltinId::ThrowTypeError),
        entry: crate::function_entry::PlannedFunctionEntry::standard(builtin, wasm_index),
        strict: true,
        is_named_expression: false,
        class_element_execution_kind: ClassElementExecutionKind::None,
        class_heritage_kind: ClassHeritageKind::None,
        is_static_class_member: false,
        is_derived_constructor: false,
        is_synthetic_default_derived_constructor: false,
        class_instance_element_plan: None,
        uses_super: false,
        this_before_super: false,
        captures_private_environment: false,
        needs_active_function_identity: false,
    };
    let host_builtin_meta = |builtin: HostBuiltinId, wasm_index: u32| WasmFunctionMeta {
        constant_leaf: None,
        numeric_leaf: None,
        async_generator_resume_environment_plan: None,
        template_source: None,
        name: builtin.as_str().to_string(),
        to_string_value: CallableToStringRepresentation::NativeNamed(builtin.as_str().to_string())
            .materialize(),
        standard_builtin: None,
        host_builtin: Some(builtin),
        length: host_builtin_length(builtin),
        length_name_configurable: true,
        entry: crate::function_entry::PlannedFunctionEntry::host(builtin, wasm_index),
        strict: true,
        is_named_expression: false,
        class_element_execution_kind: ClassElementExecutionKind::None,
        class_heritage_kind: ClassHeritageKind::None,
        is_static_class_member: false,
        is_derived_constructor: false,
        is_synthetic_default_derived_constructor: false,
        class_instance_element_plan: None,
        uses_super: false,
        this_before_super: false,
        captures_private_environment: false,
        needs_active_function_identity: false,
    };

    for (position, builtin) in standard_builtins.iter().enumerate() {
        // A compiled entry owns this builtin's specialized body. In particular,
        // concrete TypedArray constructors embed their own element kind; sharing
        // their entry would preserve the prototype but allocate the wrong storage.
        insert_meta(
            builtin.function_id(),
            standard_builtin_meta(*builtin, layout.standard_index(position)),
        )?;
    }
    for (position, builtin) in host_builtins.iter().enumerate() {
        insert_meta(
            builtin.function_id(),
            host_builtin_meta(*builtin, layout.host_index(position)),
        )?;
    }
    Ok(metas)
}

pub(crate) fn function_length(params: &[FunctionParamIr]) -> u64 {
    params
        .iter()
        .take_while(|param| !param.is_rest && param.default_init.is_none())
        .count() as u64
}

pub(crate) fn standard_builtin_length(builtin: StandardBuiltinId) -> u64 {
    match builtin {
        StandardBuiltinId::FunctionConstructor
        | StandardBuiltinId::FunctionPrototypeSymbolHasInstance
        | StandardBuiltinId::WeakRefConstructor
        | StandardBuiltinId::FinalizationRegistryConstructor
        | StandardBuiltinId::FinalizationRegistryPrototypeUnregister => 1,
        StandardBuiltinId::PromiseConstructor
        | StandardBuiltinId::PromiseResolve
        | StandardBuiltinId::PromiseTry
        | StandardBuiltinId::PromiseReject
        | StandardBuiltinId::PromiseAll
        | StandardBuiltinId::PromiseAllSettled
        | StandardBuiltinId::PromiseAllKeyed
        | StandardBuiltinId::PromiseAllSettledKeyed
        | StandardBuiltinId::PromiseAny
        | StandardBuiltinId::PromiseRace
        | StandardBuiltinId::PromiseAllResolveElement
        | StandardBuiltinId::PromiseAllSettledResolveElement
        | StandardBuiltinId::PromiseAllSettledRejectElement
        | StandardBuiltinId::PromiseAnyRejectElement
        | StandardBuiltinId::PromiseAllKeyedResolveElement
        | StandardBuiltinId::PromiseAllSettledKeyedResolveElement
        | StandardBuiltinId::PromiseAllSettledKeyedRejectElement
        | StandardBuiltinId::PromiseResolveFunction
        | StandardBuiltinId::PromiseRejectFunction
        | StandardBuiltinId::PromisePrototypeCatch
        | StandardBuiltinId::PromisePrototypeFinally
        | StandardBuiltinId::PromiseThenFinally
        | StandardBuiltinId::PromiseCatchFinally => 1,
        StandardBuiltinId::PromiseWithResolvers
        | StandardBuiltinId::PromiseValueThunk
        | StandardBuiltinId::PromiseThrower => 0,
        StandardBuiltinId::PromisePrototypeThen
        | StandardBuiltinId::FinalizationRegistryPrototypeRegister => 2,
        StandardBuiltinId::PromiseCapabilityExecutor => 2,
        StandardBuiltinId::MapConstructor
        | StandardBuiltinId::MapSpeciesGetter
        | StandardBuiltinId::MapPrototypeClear
        | StandardBuiltinId::MapPrototypeKeys
        | StandardBuiltinId::MapPrototypeValues
        | StandardBuiltinId::MapPrototypeEntries
        | StandardBuiltinId::MapIteratorNext
        | StandardBuiltinId::MapPrototypeSizeGetter
        | StandardBuiltinId::WeakMapConstructor
        | StandardBuiltinId::WeakSetConstructor
        | StandardBuiltinId::WeakRefPrototypeDeref => 0,
        StandardBuiltinId::MapPrototypeDelete
        | StandardBuiltinId::MapPrototypeForEach
        | StandardBuiltinId::MapPrototypeGet
        | StandardBuiltinId::MapPrototypeHas
        | StandardBuiltinId::WeakMapPrototypeDelete
        | StandardBuiltinId::WeakMapPrototypeGet
        | StandardBuiltinId::WeakMapPrototypeHas
        | StandardBuiltinId::WeakSetPrototypeAdd
        | StandardBuiltinId::WeakSetPrototypeDelete
        | StandardBuiltinId::WeakSetPrototypeHas => 1,
        StandardBuiltinId::ObjectFromEntries => 1,
        StandardBuiltinId::MapGroupBy
        | StandardBuiltinId::ObjectGroupBy
        | StandardBuiltinId::MapPrototypeGetOrInsert
        | StandardBuiltinId::MapPrototypeGetOrInsertComputed
        | StandardBuiltinId::MapPrototypeSet
        | StandardBuiltinId::WeakMapPrototypeGetOrInsert
        | StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed
        | StandardBuiltinId::WeakMapPrototypeSet => 2,
        StandardBuiltinId::SetConstructor
        | StandardBuiltinId::SetSpeciesGetter
        | StandardBuiltinId::SetPrototypeClear
        | StandardBuiltinId::SetPrototypeValues
        | StandardBuiltinId::SetPrototypeEntries
        | StandardBuiltinId::SetIteratorNext
        | StandardBuiltinId::SetPrototypeSizeGetter => 0,
        StandardBuiltinId::SetPrototypeAdd
        | StandardBuiltinId::SetPrototypeDelete
        | StandardBuiltinId::SetPrototypeDifference
        | StandardBuiltinId::SetPrototypeForEach
        | StandardBuiltinId::SetPrototypeHas
        | StandardBuiltinId::SetPrototypeIntersection
        | StandardBuiltinId::SetPrototypeIsDisjointFrom
        | StandardBuiltinId::SetPrototypeIsSubsetOf
        | StandardBuiltinId::SetPrototypeIsSupersetOf
        | StandardBuiltinId::SetPrototypeSymmetricDifference
        | StandardBuiltinId::SetPrototypeUnion => 1,
        StandardBuiltinId::Uint8ArrayFromBase64
        | StandardBuiltinId::Uint8ArrayFromHex
        | StandardBuiltinId::Uint8ArrayPrototypeSetFromBase64
        | StandardBuiltinId::Uint8ArrayPrototypeSetFromHex => 1,
        StandardBuiltinId::Uint8ArrayPrototypeToBase64
        | StandardBuiltinId::Uint8ArrayPrototypeToHex => 0,
        StandardBuiltinId::EvalFunction => 1,
        StandardBuiltinId::FunctionPrototypeCall => 1,
        StandardBuiltinId::FunctionPrototypeApply => 2,
        StandardBuiltinId::FunctionPrototypeBind => 1,
        StandardBuiltinId::ObjectConstructor => 1,
        StandardBuiltinId::ObjectAssign => 2,
        StandardBuiltinId::ObjectCreate => 2,
        StandardBuiltinId::ObjectGetPrototypeOf => 1,
        StandardBuiltinId::ObjectSetPrototypeOf => 2,
        StandardBuiltinId::ObjectDefineProperty => 3,
        StandardBuiltinId::ObjectDefineProperties => 2,
        StandardBuiltinId::ObjectGetOwnPropertyDescriptor => 2,
        StandardBuiltinId::ObjectGetOwnPropertyDescriptors => 1,
        StandardBuiltinId::ObjectGetOwnPropertyNames => 1,
        StandardBuiltinId::ObjectGetOwnPropertySymbols => 1,
        StandardBuiltinId::ObjectKeys => 1,
        StandardBuiltinId::ObjectValues => 1,
        StandardBuiltinId::ObjectEntries => 1,
        StandardBuiltinId::ObjectHasOwn => 2,
        StandardBuiltinId::ObjectIs => 2,
        StandardBuiltinId::ObjectIsSealed => 1,
        StandardBuiltinId::ObjectIsFrozen => 1,
        StandardBuiltinId::ObjectSeal => 1,
        StandardBuiltinId::ObjectFreeze => 1,
        StandardBuiltinId::ObjectIsExtensible => 1,
        StandardBuiltinId::ObjectPreventExtensions => 1,
        StandardBuiltinId::ObjectPrototypeHasOwnProperty => 1,
        StandardBuiltinId::ObjectPrototypeDefineGetter => 2,
        StandardBuiltinId::ObjectPrototypeDefineSetter => 2,
        StandardBuiltinId::ObjectPrototypeLookupGetter => 1,
        StandardBuiltinId::ObjectPrototypeLookupSetter => 1,
        StandardBuiltinId::ObjectPrototypeProtoGetter => 0,
        StandardBuiltinId::ObjectPrototypeProtoSetter => 1,
        StandardBuiltinId::ObjectPrototypePropertyIsEnumerable => 1,
        StandardBuiltinId::ObjectPrototypeIsPrototypeOf => 1,
        StandardBuiltinId::SymbolConstructor => 0,
        StandardBuiltinId::SymbolFor => 1,
        StandardBuiltinId::SymbolKeyFor => 1,
        StandardBuiltinId::SymbolPrototypeDescriptionGetter => 0,
        StandardBuiltinId::SymbolPrototypeToString => 0,
        StandardBuiltinId::SymbolPrototypeValueOf => 0,
        StandardBuiltinId::SymbolPrototypeToPrimitive => 1,
        StandardBuiltinId::ObjectPrototypeToString => 0,
        StandardBuiltinId::ObjectPrototypeToLocaleString => 0,
        StandardBuiltinId::ObjectPrototypeValueOf => 0,
        StandardBuiltinId::ProxyConstructor => 2,
        StandardBuiltinId::ProxyRevocable => 2,
        StandardBuiltinId::ProxyRevoke => 0,
        StandardBuiltinId::ReflectConstruct => 2,
        StandardBuiltinId::ReflectApply => 3,
        StandardBuiltinId::ReflectGet => 2,
        StandardBuiltinId::ReflectGetPrototypeOf => 1,
        StandardBuiltinId::ReflectGetOwnPropertyDescriptor => 2,
        StandardBuiltinId::ReflectSet => 3,
        StandardBuiltinId::ReflectHas => 2,
        StandardBuiltinId::ReflectDefineProperty => 3,
        StandardBuiltinId::ReflectDeleteProperty => 2,
        StandardBuiltinId::ReflectIsExtensible => 1,
        StandardBuiltinId::ReflectPreventExtensions => 1,
        StandardBuiltinId::ReflectSetPrototypeOf => 2,
        StandardBuiltinId::ReflectOwnKeys => 1,
        StandardBuiltinId::ArrayConstructor => 1,
        StandardBuiltinId::ArrayFrom => 1,
        StandardBuiltinId::ArrayFromAsync
        | StandardBuiltinId::ArrayFromAsyncFulfilled
        | StandardBuiltinId::ArrayFromAsyncRejected => 1,
        StandardBuiltinId::ArrayOf => 0,
        StandardBuiltinId::TypedArrayFrom => 1,
        StandardBuiltinId::TypedArrayOf => 0,
        StandardBuiltinId::ArrayIsArray => 1,
        StandardBuiltinId::ArrayPrototypeToLocaleString => 0,
        StandardBuiltinId::ArrayPrototypeFlat => 0,
        StandardBuiltinId::ArrayPrototypeFlatMap => 1,
        StandardBuiltinId::ArrayPrototypeAt | StandardBuiltinId::TypedArrayPrototypeAt => 1,
        StandardBuiltinId::TypedArrayPrototypeFind
        | StandardBuiltinId::TypedArrayPrototypeFindIndex
        | StandardBuiltinId::TypedArrayPrototypeFindLast
        | StandardBuiltinId::TypedArrayPrototypeFindLastIndex => 1,
        StandardBuiltinId::TypedArrayPrototypeValues
        | StandardBuiltinId::TypedArrayPrototypeKeys
        | StandardBuiltinId::TypedArrayPrototypeEntries => 0,
        StandardBuiltinId::ArrayPrototypeToReversed => 0,
        StandardBuiltinId::ArrayPrototypeToSpliced => 2,
        StandardBuiltinId::ArrayPrototypeToSorted => 1,
        StandardBuiltinId::ArrayPrototypeWith => 2,
        StandardBuiltinId::ArrayPrototypeReverse => 0,
        StandardBuiltinId::ArrayPrototypeCopyWithin
        | StandardBuiltinId::TypedArrayPrototypeCopyWithin => 2,
        StandardBuiltinId::ArrayPrototypeIncludes
        | StandardBuiltinId::TypedArrayPrototypeIncludes => 1,
        StandardBuiltinId::ArrayPrototypeIndexOf
        | StandardBuiltinId::TypedArrayPrototypeIndexOf => 1,
        StandardBuiltinId::ArrayPrototypeLastIndexOf
        | StandardBuiltinId::TypedArrayPrototypeLastIndexOf => 1,
        StandardBuiltinId::ArrayPrototypeFind => 1,
        StandardBuiltinId::ArrayPrototypeFindIndex => 1,
        StandardBuiltinId::ArrayPrototypeFindLast => 1,
        StandardBuiltinId::ArrayPrototypeFindLastIndex => 1,
        StandardBuiltinId::ArrayPrototypeEvery | StandardBuiltinId::TypedArrayPrototypeEvery => 1,
        StandardBuiltinId::ArrayPrototypeSome | StandardBuiltinId::TypedArrayPrototypeSome => 1,
        StandardBuiltinId::ArrayPrototypeForEach
        | StandardBuiltinId::TypedArrayPrototypeForEach => 1,
        StandardBuiltinId::ArrayPrototypeFilter | StandardBuiltinId::TypedArrayPrototypeFilter => 1,
        StandardBuiltinId::ArrayPrototypeMap | StandardBuiltinId::TypedArrayPrototypeMap => 1,
        StandardBuiltinId::ArrayPrototypeReduce | StandardBuiltinId::TypedArrayPrototypeReduce => 1,
        StandardBuiltinId::ArrayPrototypeReduceRight
        | StandardBuiltinId::TypedArrayPrototypeReduceRight => 1,
        StandardBuiltinId::ArrayPrototypeConcat => 1,
        StandardBuiltinId::StringPrototypeConcat => 1,
        StandardBuiltinId::StringPrototypeLocaleCompare => 1,
        StandardBuiltinId::ArrayPrototypeJoin | StandardBuiltinId::TypedArrayPrototypeJoin => 1,
        StandardBuiltinId::ArrayPrototypeSlice => 2,
        StandardBuiltinId::TypedArrayPrototypeSubarray => 2,
        StandardBuiltinId::TypedArrayPrototypeSlice => 2,
        StandardBuiltinId::TypedArrayPrototypeSet => 1,
        StandardBuiltinId::TypedArrayPrototypeReverse => 0,
        StandardBuiltinId::TypedArrayPrototypeSort => 1,
        StandardBuiltinId::TypedArrayPrototypeToReversed => 0,
        StandardBuiltinId::TypedArrayPrototypeToSorted => 1,
        StandardBuiltinId::TypedArrayPrototypeWith => 2,
        StandardBuiltinId::ArrayPrototypeSplice => 2,
        StandardBuiltinId::ArrayPrototypeFill | StandardBuiltinId::TypedArrayPrototypeFill => 1,
        StandardBuiltinId::ArrayPrototypeSort => 1,
        StandardBuiltinId::ArrayPrototypePop => 0,
        StandardBuiltinId::ArrayPrototypePush => 1,
        StandardBuiltinId::ArrayPrototypeShift => 0,
        StandardBuiltinId::ArrayPrototypeUnshift => 1,
        StandardBuiltinId::ArrayPrototypeKeys => 0,
        StandardBuiltinId::ArrayPrototypeEntries => 0,
        StandardBuiltinId::ArrayPrototypeValues => 0,
        StandardBuiltinId::ArrayIteratorNext => 0,
        StandardBuiltinId::ArrayIteratorIdentity => 0,
        StandardBuiltinId::StringIteratorNext => 0,
        StandardBuiltinId::RegExpStringIteratorNext => 0,
        StandardBuiltinId::GeneratorPrototypeNext
        | StandardBuiltinId::GeneratorPrototypeReturn
        | StandardBuiltinId::GeneratorPrototypeThrow
        | StandardBuiltinId::AsyncGeneratorPrototypeNext
        | StandardBuiltinId::AsyncGeneratorPrototypeReturn
        | StandardBuiltinId::AsyncGeneratorPrototypeThrow => 1,
        StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose => 0,
        StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled => 1,
        StandardBuiltinId::StringPrototypeIterator => 0,
        StandardBuiltinId::ShadowRealmConstructor | StandardBuiltinId::ShadowRealmWrappedFunctionCall => 0,
        StandardBuiltinId::ShadowRealmPrototypeEvaluate
        | StandardBuiltinId::ShadowRealmImportFulfilled
        | StandardBuiltinId::ShadowRealmImportRejected => 1,
        StandardBuiltinId::ShadowRealmPrototypeImportValue => 2,
        StandardBuiltinId::IteratorConstructor => 0,
        StandardBuiltinId::IteratorFrom => 1,
        StandardBuiltinId::IteratorConcat => 0,
        StandardBuiltinId::IteratorConcatNext => 0,
        StandardBuiltinId::IteratorConcatReturn => 0,
        StandardBuiltinId::IteratorZip => 1,
        StandardBuiltinId::IteratorZipKeyed => 1,
        StandardBuiltinId::IteratorZipNext => 0,
        StandardBuiltinId::IteratorZipReturn => 0,
        StandardBuiltinId::IteratorHelperNext => 0,
        StandardBuiltinId::IteratorHelperReturn => 0,
        StandardBuiltinId::IteratorPrototypeToArray => 0,
        StandardBuiltinId::IteratorPrototypeIncludes | StandardBuiltinId::IteratorPrototypeJoin => 1,
        StandardBuiltinId::IteratorPrototypeForEach => 1,
        StandardBuiltinId::IteratorPrototypeEvery => 1,
        StandardBuiltinId::IteratorPrototypeSome => 1,
        StandardBuiltinId::IteratorPrototypeFind => 1,
        StandardBuiltinId::IteratorPrototypeReduce => 1,
        StandardBuiltinId::IteratorPrototypeMap => 1,
        StandardBuiltinId::IteratorMapNext => 0,
        StandardBuiltinId::IteratorMapReturn => 0,
        StandardBuiltinId::IteratorPrototypeFilter => 1,
        StandardBuiltinId::IteratorFilterNext => 0,
        StandardBuiltinId::IteratorFilterReturn => 0,
        StandardBuiltinId::IteratorPrototypeFlatMap => 1,
        StandardBuiltinId::IteratorFlatMapNext => 0,
        StandardBuiltinId::IteratorFlatMapReturn => 0,
        StandardBuiltinId::IteratorPrototypeTake => 1,
        StandardBuiltinId::IteratorTakeNext => 0,
        StandardBuiltinId::IteratorTakeReturn => 0,
        StandardBuiltinId::IteratorPrototypeChunks => 1,
        StandardBuiltinId::IteratorPrototypeWindows => 1,
        StandardBuiltinId::IteratorBufferedNext => 0,
        StandardBuiltinId::IteratorBufferedReturn => 0,
        StandardBuiltinId::IteratorPrototypeDrop => 1,
        StandardBuiltinId::IteratorDropNext => 0,
        StandardBuiltinId::IteratorDropReturn => 0,
        StandardBuiltinId::IteratorPrototypeConstructorGetter => 0,
        StandardBuiltinId::IteratorPrototypeConstructorSetter => 1,
        StandardBuiltinId::IteratorPrototypeSymbolDispose => 0,
        StandardBuiltinId::IteratorPrototypeToStringTagGetter => 0,
        StandardBuiltinId::IteratorPrototypeToStringTagSetter => 1,
        StandardBuiltinId::IteratorFromWrapperNext => 0,
        StandardBuiltinId::IteratorFromWrapperReturn => 0,
        StandardBuiltinId::ArrayBufferConstructor
        | StandardBuiltinId::SharedArrayBufferConstructor => 1,
        StandardBuiltinId::ArrayBufferIsView => 1,
        StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter
        | StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter => 0,
        StandardBuiltinId::SharedArrayBufferPrototypeGrow => 1,
        StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter => 0,
        StandardBuiltinId::ArrayBufferPrototypeDetachedGetter => 0,
        StandardBuiltinId::ArrayBufferPrototypeResizableGetter
        | StandardBuiltinId::ArrayBufferPrototypeImmutableGetter => 0,
        StandardBuiltinId::ArrayBufferPrototypeResize => 1,
        StandardBuiltinId::ArrayBufferPrototypeSlice
        | StandardBuiltinId::SharedArrayBufferPrototypeSlice => 2,
        StandardBuiltinId::ArrayBufferPrototypeTransfer
        | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
        | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable => 0,
        StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable => 2,
        StandardBuiltinId::AtomicsAdd => 3,
        StandardBuiltinId::AtomicsAnd => 3,
        StandardBuiltinId::AtomicsCompareExchange => 4,
        StandardBuiltinId::AtomicsExchange => 3,
        StandardBuiltinId::AtomicsLoad => 2,
        StandardBuiltinId::AtomicsNotify => 3,
        StandardBuiltinId::AtomicsOr => 3,
        StandardBuiltinId::AtomicsPause => 0,
        StandardBuiltinId::AtomicsSub => 3,
        StandardBuiltinId::AtomicsStore => 3,
        StandardBuiltinId::AtomicsWait => 4,
        StandardBuiltinId::AtomicsWaitAsync => 4,
        StandardBuiltinId::AtomicsXor => 3,
        StandardBuiltinId::AtomicsIsLockFree => 1,
        StandardBuiltinId::DataViewConstructor => 1,
        StandardBuiltinId::DateConstructor => 7,
        StandardBuiltinId::RegExpConstructor => 2,
        StandardBuiltinId::RegExpPrototypeFlagsGetter => 0,
        StandardBuiltinId::RegExpPrototypeSourceGetter
        | StandardBuiltinId::RegExpPrototypeHasIndicesGetter
        | StandardBuiltinId::RegExpPrototypeGlobalGetter
        | StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter
        | StandardBuiltinId::RegExpPrototypeMultilineGetter
        | StandardBuiltinId::RegExpPrototypeDotAllGetter
        | StandardBuiltinId::RegExpPrototypeUnicodeGetter
        | StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter
        | StandardBuiltinId::RegExpPrototypeStickyGetter => 0,
        StandardBuiltinId::RegExpLegacyStaticGetter => 0,
        StandardBuiltinId::RegExpLegacyStaticSetter => 1,
        StandardBuiltinId::RegExpPrototypeCompile => 2,
        StandardBuiltinId::RegExpPrototypeToString => 0,
        StandardBuiltinId::RegExpPrototypeExec
        | StandardBuiltinId::RegExpPrototypeTest
        | StandardBuiltinId::RegExpPrototypeSymbolMatch
        | StandardBuiltinId::RegExpPrototypeSymbolMatchAll
        | StandardBuiltinId::RegExpPrototypeSymbolSearch => 1,
        StandardBuiltinId::RegExpPrototypeSymbolReplace
        | StandardBuiltinId::RegExpPrototypeSymbolSplit => 2,
        StandardBuiltinId::RegExpEscape => 1,
        StandardBuiltinId::JsonParse => 2,
        StandardBuiltinId::JsonStringify => 3,
        StandardBuiltinId::JsonRawJson => 1,
        StandardBuiltinId::JsonIsRawJson => 1,
        StandardBuiltinId::DateUtc => 7,
        StandardBuiltinId::DateParse => 1,
        StandardBuiltinId::DateNow
        | StandardBuiltinId::DatePrototypeGetTime
        | StandardBuiltinId::DatePrototypeValueOf
        | StandardBuiltinId::DatePrototypeGetFullYear
        | StandardBuiltinId::DatePrototypeGetUtcFullYear
        | StandardBuiltinId::DatePrototypeGetMonth
        | StandardBuiltinId::DatePrototypeGetUtcMonth
        | StandardBuiltinId::DatePrototypeGetDate
        | StandardBuiltinId::DatePrototypeGetUtcDate
        | StandardBuiltinId::DatePrototypeGetDay
        | StandardBuiltinId::DatePrototypeGetUtcDay
        | StandardBuiltinId::DatePrototypeGetHours
        | StandardBuiltinId::DatePrototypeGetUtcHours
        | StandardBuiltinId::DatePrototypeGetMinutes
        | StandardBuiltinId::DatePrototypeGetUtcMinutes
        | StandardBuiltinId::DatePrototypeGetSeconds
        | StandardBuiltinId::DatePrototypeGetUtcSeconds
        | StandardBuiltinId::DatePrototypeGetMilliseconds
        | StandardBuiltinId::DatePrototypeGetUtcMilliseconds
        | StandardBuiltinId::DatePrototypeGetTimezoneOffset
        | StandardBuiltinId::DatePrototypeGetYear
        | StandardBuiltinId::DatePrototypeToIsoString
        | StandardBuiltinId::DatePrototypeToDateString
        | StandardBuiltinId::DatePrototypeToLocaleDateString
        | StandardBuiltinId::DatePrototypeToLocaleString
        | StandardBuiltinId::DatePrototypeToLocaleTimeString
        | StandardBuiltinId::DatePrototypeToTemporalInstant
        | StandardBuiltinId::DatePrototypeToTimeString
        | StandardBuiltinId::DatePrototypeToString
        | StandardBuiltinId::DatePrototypeToUtcString => 0,
        StandardBuiltinId::DatePrototypeSetTime
        | StandardBuiltinId::DatePrototypeSetYear
        | StandardBuiltinId::DatePrototypeToJson
        | StandardBuiltinId::DatePrototypeToPrimitive => 1,
        StandardBuiltinId::DatePrototypeSetFullYear
        | StandardBuiltinId::DatePrototypeSetUtcFullYear
        | StandardBuiltinId::DatePrototypeSetMinutes
        | StandardBuiltinId::DatePrototypeSetUtcMinutes => 3,
        StandardBuiltinId::DatePrototypeSetMonth
        | StandardBuiltinId::DatePrototypeSetUtcMonth
        | StandardBuiltinId::DatePrototypeSetSeconds
        | StandardBuiltinId::DatePrototypeSetUtcSeconds => 2,
        StandardBuiltinId::DatePrototypeSetDate
        | StandardBuiltinId::DatePrototypeSetUtcDate
        | StandardBuiltinId::DatePrototypeSetMilliseconds
        | StandardBuiltinId::DatePrototypeSetUtcMilliseconds => 1,
        StandardBuiltinId::DatePrototypeSetHours | StandardBuiltinId::DatePrototypeSetUtcHours => 4,
        StandardBuiltinId::DataViewPrototypeBufferGetter
        | StandardBuiltinId::DataViewPrototypeByteLengthGetter
        | StandardBuiltinId::DataViewPrototypeByteOffsetGetter
        | StandardBuiltinId::TypedArrayPrototypeBufferGetter
        | StandardBuiltinId::TypedArrayPrototypeByteLengthGetter
        | StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter
        | StandardBuiltinId::TypedArrayPrototypeLengthGetter
        | StandardBuiltinId::TypedArrayPrototypeToStringTagGetter
        | StandardBuiltinId::TypedArrayPrototypeToString
        | StandardBuiltinId::TypedArrayPrototypeToLocaleString => 0,
        StandardBuiltinId::DataViewPrototypeGetUint8
        | StandardBuiltinId::DataViewPrototypeGetInt8
        | StandardBuiltinId::DataViewPrototypeGetUint16
        | StandardBuiltinId::DataViewPrototypeGetInt16
        | StandardBuiltinId::DataViewPrototypeGetUint32
        | StandardBuiltinId::DataViewPrototypeGetInt32
        | StandardBuiltinId::DataViewPrototypeGetFloat16
        | StandardBuiltinId::DataViewPrototypeGetFloat32
        | StandardBuiltinId::DataViewPrototypeGetFloat64
        | StandardBuiltinId::DataViewPrototypeGetBigInt64
        | StandardBuiltinId::DataViewPrototypeGetBigUint64 => 1,
        StandardBuiltinId::DataViewPrototypeSetUint8
        | StandardBuiltinId::DataViewPrototypeSetInt8
        | StandardBuiltinId::DataViewPrototypeSetUint16
        | StandardBuiltinId::DataViewPrototypeSetInt16
        | StandardBuiltinId::DataViewPrototypeSetUint32
        | StandardBuiltinId::DataViewPrototypeSetInt32
        | StandardBuiltinId::DataViewPrototypeSetFloat16
        | StandardBuiltinId::DataViewPrototypeSetFloat32
        | StandardBuiltinId::DataViewPrototypeSetFloat64
        | StandardBuiltinId::DataViewPrototypeSetBigInt64
        | StandardBuiltinId::DataViewPrototypeSetBigUint64 => 2,
        StandardBuiltinId::Float64ArrayConstructor
        | StandardBuiltinId::Float32ArrayConstructor
        | StandardBuiltinId::Float16ArrayConstructor
        | StandardBuiltinId::Int32ArrayConstructor
        | StandardBuiltinId::Int16ArrayConstructor
        | StandardBuiltinId::Int8ArrayConstructor
        | StandardBuiltinId::Uint32ArrayConstructor
        | StandardBuiltinId::Uint16ArrayConstructor
        | StandardBuiltinId::Uint8ArrayConstructor
        | StandardBuiltinId::Uint8ClampedArrayConstructor
        | StandardBuiltinId::BigInt64ArrayConstructor
        | StandardBuiltinId::BigUint64ArrayConstructor => 3,
        StandardBuiltinId::NumberConstructor
        | StandardBuiltinId::BigIntConstructor
        | StandardBuiltinId::NumberIsInteger
        | StandardBuiltinId::NumberIsSafeInteger
        | StandardBuiltinId::NumberIsFinite
        | StandardBuiltinId::NumberIsNaN
        | StandardBuiltinId::NumberPrototypeToExponential
        | StandardBuiltinId::NumberPrototypeToFixed
        | StandardBuiltinId::NumberPrototypeToPrecision
        | StandardBuiltinId::NumberPrototypeToString
        | StandardBuiltinId::GlobalIsFinite
        | StandardBuiltinId::GlobalIsNaN
        | StandardBuiltinId::MathAbs
        | StandardBuiltinId::MathAcos
        | StandardBuiltinId::MathAcosh
        | StandardBuiltinId::MathAsin
        | StandardBuiltinId::MathAsinh
        | StandardBuiltinId::MathAtan
        | StandardBuiltinId::MathCbrt
        | StandardBuiltinId::MathAtanh
        | StandardBuiltinId::MathCeil
        | StandardBuiltinId::MathClz32
        | StandardBuiltinId::MathCos
        | StandardBuiltinId::MathCosh
        | StandardBuiltinId::MathExp
        | StandardBuiltinId::MathExpm1
        | StandardBuiltinId::MathF16Round
        | StandardBuiltinId::MathFloor
        | StandardBuiltinId::MathFround
        | StandardBuiltinId::MathLog
        | StandardBuiltinId::MathLog10
        | StandardBuiltinId::MathLog1p
        | StandardBuiltinId::MathLog2
        | StandardBuiltinId::MathRound
        | StandardBuiltinId::MathSign
        | StandardBuiltinId::MathSin
        | StandardBuiltinId::MathSinh
        | StandardBuiltinId::MathSqrt
        | StandardBuiltinId::MathSumPrecise
        | StandardBuiltinId::MathTan
        | StandardBuiltinId::MathTanh
        | StandardBuiltinId::MathTrunc => 1,
        StandardBuiltinId::NumberPrototypeToLocaleString
        | StandardBuiltinId::NumberPrototypeValueOf
        | StandardBuiltinId::BigIntPrototypeToString
        | StandardBuiltinId::BigIntPrototypeToLocaleString
        | StandardBuiltinId::BigIntPrototypeValueOf
        | StandardBuiltinId::StringPrototypeToString
        | StandardBuiltinId::StringPrototypeValueOf
        | StandardBuiltinId::StringPrototypeToLocaleLowerCase
        | StandardBuiltinId::StringPrototypeToLocaleUpperCase
        | StandardBuiltinId::StringPrototypeToLowerCase
        | StandardBuiltinId::StringPrototypeToUpperCase
        | StandardBuiltinId::StringPrototypeNormalize
        | StandardBuiltinId::BooleanPrototypeToString
        | StandardBuiltinId::BooleanPrototypeValueOf => 0,
        StandardBuiltinId::BigIntAsIntN | StandardBuiltinId::BigIntAsUintN => 2,
        StandardBuiltinId::MathAtan2
        | StandardBuiltinId::MathHypot
        | StandardBuiltinId::MathImul
        | StandardBuiltinId::MathPow
        | StandardBuiltinId::MathMin
        | StandardBuiltinId::MathMax => 2,
        StandardBuiltinId::MathRandom => 0,
        StandardBuiltinId::StringConstructor
        | StandardBuiltinId::StringFromCharCode
        | StandardBuiltinId::StringFromCodePoint
        | StandardBuiltinId::StringRaw
        | StandardBuiltinId::StringPrototypeCharAt
        | StandardBuiltinId::StringPrototypeCharCodeAt
        | StandardBuiltinId::StringPrototypeCodePointAt
        | StandardBuiltinId::StringPrototypeAt
        | StandardBuiltinId::StringPrototypeAnchor
        | StandardBuiltinId::StringPrototypeFontcolor
        | StandardBuiltinId::StringPrototypeFontsize
        | StandardBuiltinId::StringPrototypeLink => 1,
        StandardBuiltinId::StringPrototypeSubstr
        | StandardBuiltinId::StringPrototypeSubstring
        | StandardBuiltinId::StringPrototypeSlice => 2,
        StandardBuiltinId::StringPrototypeMatch
        | StandardBuiltinId::StringPrototypeMatchAll
        | StandardBuiltinId::StringPrototypeSearch
        | StandardBuiltinId::StringPrototypeIndexOf
        | StandardBuiltinId::StringPrototypeLastIndexOf
        | StandardBuiltinId::StringPrototypePadStart
        | StandardBuiltinId::StringPrototypePadEnd
        | StandardBuiltinId::StringPrototypeRepeat
        | StandardBuiltinId::StringPrototypeEndsWith
        | StandardBuiltinId::StringPrototypeIncludes
        | StandardBuiltinId::StringPrototypeStartsWith => 1,
        StandardBuiltinId::StringPrototypeReplace
        | StandardBuiltinId::StringPrototypeReplaceAll
        | StandardBuiltinId::StringPrototypeSplit => 2,
        StandardBuiltinId::StringPrototypeBig
        | StandardBuiltinId::StringPrototypeBlink
        | StandardBuiltinId::StringPrototypeBold
        | StandardBuiltinId::StringPrototypeFixed
        | StandardBuiltinId::StringPrototypeItalics
        | StandardBuiltinId::StringPrototypeSmall
        | StandardBuiltinId::StringPrototypeStrike
        | StandardBuiltinId::StringPrototypeSub
        | StandardBuiltinId::StringPrototypeSup
        | StandardBuiltinId::StringPrototypeTrim
        | StandardBuiltinId::StringPrototypeTrimStart
        | StandardBuiltinId::StringPrototypeTrimEnd
        | StandardBuiltinId::StringPrototypeIsWellFormed
        | StandardBuiltinId::StringPrototypeToWellFormed => 0,
        StandardBuiltinId::BooleanConstructor
        | StandardBuiltinId::TemporalInstantConstructor
        | StandardBuiltinId::TemporalInstantFrom
        | StandardBuiltinId::TemporalInstantFromEpochMilliseconds
        | StandardBuiltinId::TemporalInstantFromEpochNanoseconds
        | StandardBuiltinId::TemporalInstantPrototypeAdd
        | StandardBuiltinId::TemporalInstantPrototypeSubtract
        | StandardBuiltinId::TemporalInstantPrototypeRound
        | StandardBuiltinId::TemporalInstantPrototypeUntil
        | StandardBuiltinId::TemporalInstantPrototypeSince
        | StandardBuiltinId::TemporalInstantPrototypeEquals
        | StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso
        | StandardBuiltinId::TemporalZonedDateTimeFrom
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEquals
        | StandardBuiltinId::TemporalPlainDateFrom
        | StandardBuiltinId::TemporalPlainDatePrototypeWith
        | StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar
        | StandardBuiltinId::TemporalPlainDatePrototypeAdd
        | StandardBuiltinId::TemporalPlainDatePrototypeSubtract
        | StandardBuiltinId::TemporalPlainDatePrototypeUntil
        | StandardBuiltinId::TemporalPlainDatePrototypeSince
        | StandardBuiltinId::TemporalPlainDatePrototypeEquals
        | StandardBuiltinId::TemporalPlainTimeFrom
        | StandardBuiltinId::TemporalPlainTimePrototypeWith
        | StandardBuiltinId::TemporalPlainTimePrototypeAdd
        | StandardBuiltinId::TemporalPlainTimePrototypeSubtract
        | StandardBuiltinId::TemporalPlainTimePrototypeUntil
        | StandardBuiltinId::TemporalPlainTimePrototypeSince
        | StandardBuiltinId::TemporalPlainTimePrototypeRound
        | StandardBuiltinId::TemporalPlainTimePrototypeEquals
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone
        // Each takes one required argument and an optional options bag, so
        // `length` is 1 — the same value the PlainDateTime namesakes two rows
        // below already carry, and what
        // `built-ins/Temporal/ZonedDateTime/prototype/*/length.js` asserts.
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar
        | StandardBuiltinId::TemporalZonedDateTimePrototypeAdd
        | StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract
        | StandardBuiltinId::TemporalZonedDateTimePrototypeUntil
        | StandardBuiltinId::TemporalZonedDateTimePrototypeSince
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWith
        | StandardBuiltinId::TemporalZonedDateTimePrototypeRound
        | StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition
        | StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime
        | StandardBuiltinId::TemporalPlainDateTimeFrom
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWith
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar
        | StandardBuiltinId::TemporalPlainDateTimePrototypeAdd
        | StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract
        | StandardBuiltinId::TemporalPlainDateTimePrototypeUntil
        | StandardBuiltinId::TemporalPlainDateTimePrototypeSince
        | StandardBuiltinId::TemporalPlainDateTimePrototypeRound
        | StandardBuiltinId::TemporalPlainDateTimePrototypeEquals
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime => 1,
        StandardBuiltinId::TemporalPlainYearMonthConstructor
        | StandardBuiltinId::TemporalPlainYearMonthCompare
        | StandardBuiltinId::TemporalPlainMonthDayConstructor => 2,
        StandardBuiltinId::TemporalPlainYearMonthFrom
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeWith
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeSince
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals
        | StandardBuiltinId::TemporalPlainMonthDayFrom
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeWith
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate => 1,
        StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToString
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToString
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf => 0,
        StandardBuiltinId::TemporalZonedDateTimeConstructor => 2,
        StandardBuiltinId::TemporalInstantCompare => 2,
        StandardBuiltinId::TemporalPlainDateCompare => 2,
        StandardBuiltinId::TemporalPlainTimeCompare => 2,
        StandardBuiltinId::TemporalPlainDateTimeCompare
        | StandardBuiltinId::TemporalZonedDateTimeCompare => 2,
        StandardBuiltinId::TemporalPlainDateTimeConstructor => 3,
        StandardBuiltinId::TemporalPlainDateConstructor => 3,
        StandardBuiltinId::TemporalDurationConstructor => 0,
        StandardBuiltinId::TemporalDurationCompare => 2,
        StandardBuiltinId::TemporalDurationFrom
        | StandardBuiltinId::TemporalDurationPrototypeWith
        | StandardBuiltinId::TemporalDurationPrototypeAdd
        | StandardBuiltinId::TemporalDurationPrototypeSubtract
        | StandardBuiltinId::TemporalDurationPrototypeRound
        | StandardBuiltinId::TemporalDurationPrototypeTotal => 1,
        StandardBuiltinId::TemporalDurationPrototypeYearsGetter
        | StandardBuiltinId::TemporalDurationPrototypeMonthsGetter
        | StandardBuiltinId::TemporalDurationPrototypeWeeksGetter
        | StandardBuiltinId::TemporalDurationPrototypeDaysGetter
        | StandardBuiltinId::TemporalDurationPrototypeHoursGetter
        | StandardBuiltinId::TemporalDurationPrototypeMinutesGetter
        | StandardBuiltinId::TemporalDurationPrototypeSecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter
        | StandardBuiltinId::TemporalDurationPrototypeSignGetter
        | StandardBuiltinId::TemporalDurationPrototypeBlankGetter
        | StandardBuiltinId::TemporalDurationPrototypeNegated
        | StandardBuiltinId::TemporalDurationPrototypeAbs
        | StandardBuiltinId::TemporalDurationPrototypeToString
        | StandardBuiltinId::TemporalDurationPrototypeToJson
        | StandardBuiltinId::TemporalDurationPrototypeToLocaleString
        | StandardBuiltinId::TemporalDurationPrototypeValueOf => 0,
        StandardBuiltinId::IntlSupportedValuesOf | StandardBuiltinId::IntlGetCanonicalLocales | StandardBuiltinId::IntlLocaleConstructor => 1,
        // ECMA-402 11.1.1/11.2.2/11.3.4/11.1.5: `Intl.DateTimeFormat` has
        // length 0, `supportedLocalesOf` and the format functions length 1.
        StandardBuiltinId::IntlDateTimeFormatConstructor
        | StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter => 0,
        StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlDateTimeFormatBoundFormat => 1,
        // ECMA-402 11.4.6/11.4.7: `formatRange` and `formatRangeToParts` each
        // take (startDate, endDate), so their `length` is 2, not 1.
        StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange
        | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts => 2,
        StandardBuiltinId::IntlSegmenterConstructor
        | StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions
        | StandardBuiltinId::IntlSegmentsPrototypeIterator
        | StandardBuiltinId::IntlSegmentIteratorPrototypeNext
        | StandardBuiltinId::IntlRelativeTimeFormatConstructor
        | StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlDurationFormatConstructor
        | StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions
        | StandardBuiltinId::IntlNumberFormatConstructor
        | StandardBuiltinId::IntlPluralRulesConstructor
        | StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions
        | StandardBuiltinId::IntlListFormatConstructor
        | StandardBuiltinId::IntlListFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlCollatorConstructor
        | StandardBuiltinId::IntlCollatorPrototypeResolvedOptions
        | StandardBuiltinId::IntlCollatorPrototypeCompareGetter
        | StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter => 0,
        StandardBuiltinId::IntlSegmenterSupportedLocalesOf
        | StandardBuiltinId::IntlDurationFormatSupportedLocalesOf
        | StandardBuiltinId::IntlDurationFormatPrototypeFormat
        | StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlSegmenterPrototypeSegment
        | StandardBuiltinId::IntlSegmentsPrototypeContaining
        | StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf
        | StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf
        | StandardBuiltinId::IntlDisplayNamesPrototypeOf
        | StandardBuiltinId::IntlNumberFormatSupportedLocalesOf
        | StandardBuiltinId::IntlPluralRulesSupportedLocalesOf
        | StandardBuiltinId::IntlPluralRulesPrototypeSelect
        | StandardBuiltinId::IntlListFormatSupportedLocalesOf
        | StandardBuiltinId::IntlCollatorSupportedLocalesOf
        | StandardBuiltinId::IntlListFormatPrototypeFormat
        | StandardBuiltinId::IntlListFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlNumberFormatBoundFormat => 1,
        StandardBuiltinId::IntlDisplayNamesConstructor
        | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat
        | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatRange
        | StandardBuiltinId::IntlPluralRulesPrototypeSelectRange
        | StandardBuiltinId::IntlCollatorBoundCompare
        | StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts => 2,
        StandardBuiltinId::IntlLocalePrototypeLanguageGetter
        | StandardBuiltinId::IntlLocalePrototypeScriptGetter
        | StandardBuiltinId::IntlLocalePrototypeRegionGetter
        | StandardBuiltinId::IntlLocalePrototypeBaseNameGetter
        | StandardBuiltinId::IntlLocalePrototypeCalendarGetter
        | StandardBuiltinId::IntlLocalePrototypeCollationGetter
        | StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter
        | StandardBuiltinId::IntlLocalePrototypeHourCycleGetter
        | StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter
        | StandardBuiltinId::IntlLocalePrototypeNumericGetter
        | StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter
        | StandardBuiltinId::IntlLocalePrototypeVariantsGetter
        | StandardBuiltinId::IntlLocalePrototypeToString
        | StandardBuiltinId::IntlLocalePrototypeMaximize
        | StandardBuiltinId::IntlLocalePrototypeMinimize
        | StandardBuiltinId::IntlLocalePrototypeGetWeekInfo
        | StandardBuiltinId::IntlLocalePrototypeGetCalendars
        | StandardBuiltinId::IntlLocalePrototypeGetCollations
        | StandardBuiltinId::IntlLocalePrototypeGetTimeZones
        | StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems
        | StandardBuiltinId::IntlLocalePrototypeGetHourCycles
        | StandardBuiltinId::IntlLocalePrototypeGetTextInfo => 0,
        StandardBuiltinId::ErrorIsError | StandardBuiltinId::ErrorPrototypeStackSetter => 1,
        StandardBuiltinId::SuppressedErrorConstructor => 3,
        StandardBuiltinId::AggregateErrorConstructor => 2,
        StandardBuiltinId::ErrorConstructor
        | StandardBuiltinId::EvalErrorConstructor
        | StandardBuiltinId::RangeErrorConstructor
        | StandardBuiltinId::SyntaxErrorConstructor
        | StandardBuiltinId::TypeErrorConstructor
        | StandardBuiltinId::URIErrorConstructor
        | StandardBuiltinId::ReferenceErrorConstructor => 1,
        StandardBuiltinId::ArraySpeciesGetter
        | StandardBuiltinId::TypedArraySpeciesGetter
        | StandardBuiltinId::TypedArrayConstructor
        | StandardBuiltinId::ArrayBufferSpeciesGetter
        | StandardBuiltinId::RegExpSpeciesGetter
        | StandardBuiltinId::PromiseSpeciesGetter
        | StandardBuiltinId::FunctionPrototype
        | StandardBuiltinId::FunctionPrototypeToString
        | StandardBuiltinId::ErrorPrototypeToString
        | StandardBuiltinId::ErrorPrototypeStackGetter
        | StandardBuiltinId::ThrowTypeError
        | StandardBuiltinId::AbstractModuleSourceConstructor
        | StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter
        | StandardBuiltinId::TemporalNowInstant
        | StandardBuiltinId::TemporalNowTimeZoneId
        | StandardBuiltinId::TemporalNowZonedDateTimeIso
        | StandardBuiltinId::TemporalNowPlainDateTimeIso
        | StandardBuiltinId::TemporalNowPlainDateIso
        | StandardBuiltinId::TemporalNowPlainTimeIso
        | StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter
        | StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeEraGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDayGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalPlainTimeConstructor
        | StandardBuiltinId::TemporalPlainTimePrototypeHourGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter
        | StandardBuiltinId::TemporalPlainTimePrototypeToString
        | StandardBuiltinId::TemporalPlainTimePrototypeToJson
        | StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainTimePrototypeValueOf
        // `toString ( [ options ] )` takes only an optional parameter, so its
        // `length` is 0 even though the emitter reads argument 0.
        | StandardBuiltinId::TemporalPlainDatePrototypeToString
        | StandardBuiltinId::TemporalPlainDatePrototypeToJson
        | StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainDatePrototypeValueOf
        | StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime
        | StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth
        | StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay
        | StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToString
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToJson
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString
        | StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate
        | StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime
        | StandardBuiltinId::TemporalInstantPrototypeToString
        | StandardBuiltinId::TemporalInstantPrototypeToLocaleString
        | StandardBuiltinId::TemporalInstantPrototypeToJson
        | StandardBuiltinId::TemporalInstantPrototypeValueOf => 0,
        StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToString
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToJson
        | StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime
        | StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime
        | StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter
        | StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate
        | StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime => 0,
        StandardBuiltinId::Escape
        | StandardBuiltinId::Unescape
        | StandardBuiltinId::EncodeUri
        | StandardBuiltinId::EncodeUriComponent
        | StandardBuiltinId::DecodeUri
        | StandardBuiltinId::DecodeUriComponent => 1,
        // Pinned by the two DisposableStack families' `length.js` files, one
        // file per row. The async settlement callbacks are anonymous reaction
        // handlers and take the settled value, like the `AsyncIterator`
        // `@@asyncDispose` pair.
        StandardBuiltinId::AsyncDisposableStackConstructor
        | StandardBuiltinId::DisposableStackConstructor
        | StandardBuiltinId::AsyncDisposableStackPrototypeMove
        | StandardBuiltinId::DisposableStackPrototypeMove
        | StandardBuiltinId::DisposableStackPrototypeDispose
        | StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync
        | StandardBuiltinId::AsyncDisposableStackPrototypeDisposedGetter
        | StandardBuiltinId::DisposableStackPrototypeDisposedGetter => 0,
        StandardBuiltinId::AsyncDisposableStackPrototypeUse
        | StandardBuiltinId::AsyncDisposableStackPrototypeDefer
        | StandardBuiltinId::DisposableStackPrototypeUse
        | StandardBuiltinId::DisposableStackPrototypeDefer
        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
        | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected => 1,
        StandardBuiltinId::AsyncDisposableStackPrototypeAdopt
        | StandardBuiltinId::DisposableStackPrototypeAdopt => 2,
    }
}

pub(crate) fn host_builtin_length(builtin: HostBuiltinId) -> u64 {
    match builtin {
        HostBuiltinId::Print => 1,
        HostBuiltinId::Gc => 0,
        HostBuiltinId::AssertThrows => 2,
        HostBuiltinId::IsConstructor => 1,
        HostBuiltinId::CreateRealm => 0,
        HostBuiltinId::RealmEvalScript
        | HostBuiltinId::GeneratorFunctionConstructor
        | HostBuiltinId::AsyncFunctionConstructor
        | HostBuiltinId::AsyncGeneratorFunctionConstructor => 1,
        HostBuiltinId::CreateHTMLDDA
        | HostBuiltinId::GetAbstractModuleSource
        | HostBuiltinId::HTMLDDA
        | HostBuiltinId::AsyncDisposableStackSyncDispose => 0,
        HostBuiltinId::ParseInt => 2,
        HostBuiltinId::ParseFloat => 1,
        HostBuiltinId::DetachArrayBuffer => 1,
        HostBuiltinId::AgentStart => 1,
        HostBuiltinId::AgentBroadcast => 2,
        HostBuiltinId::AgentReceiveBroadcast => 0,
        HostBuiltinId::AgentReport => 1,
        HostBuiltinId::AgentGetReport => 0,
        HostBuiltinId::AgentSleep => 1,
        HostBuiltinId::AgentMonotonicNow => 0,
        HostBuiltinId::AgentLeaving => 0,
    }
}

/// A singleton Number fact only owns the payload when emission cannot obtain
/// a different runtime tag from mutable storage, a call, or object coercion.
pub(crate) fn collect_hoisted_vars_block_root(block: &BlockIr) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_hoisted_vars_block(block, &mut names);
    names.into_iter().collect()
}

pub(crate) fn collect_hoisted_vars_block(block: &BlockIr, names: &mut BTreeSet<String>) {
    if block
        .lexical_environment
        .as_ref()
        .is_some_and(|environment| {
            matches!(
                environment.initialization,
                scriptw_ir::LexicalEnvironmentInitializationIr::FunctionBody { .. }
            )
        })
    {
        return;
    }
    for statement in &block.statements {
        collect_hoisted_vars_statement(statement, names);
    }
}

/// A loop head can declare `var`s in every form except `Expression`, so this
/// walks the init exhaustively. The `Statements` form matters most: a pattern
/// head (`for (var { x } = o; …)`) lowers to ordinary `StatementIr::Var`
/// declarators, and missing them here leaves `x` unallocated at emit time.
pub(crate) fn collect_hoisted_vars_for_init(init: &ForInitIr, names: &mut BTreeSet<String>) {
    match init {
        ForInitIr::Var(declarators) => {
            for declarator in declarators {
                names.insert(declarator.name.clone());
            }
        }
        ForInitIr::Statements(statements) => {
            for statement in statements {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        ForInitIr::Lexical { .. }
        | ForInitIr::LexicalBlock(_)
        | ForInitIr::Expression(_)
        | ForInitIr::SyncDisposable(_)
        | ForInitIr::AsyncDisposable(_) => {}
    }
}

pub(crate) fn collect_hoisted_vars_statement(
    statement: &StatementIr,
    names: &mut BTreeSet<String>,
) {
    match statement {
        StatementIr::ResumableClassDefinition(plan) => {
            for statement in plan.prefixes().flat_map(|prefix| prefix.statements()) {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        // Module top-level `var`s are environment bindings of the module they
        // are written in, not hoisted vars of the merged script body.
        StatementIr::ModuleImportBinding(_) => {}
        StatementIr::ModuleUnitOnce { .. } => {}
        StatementIr::AnnexBFunctionCopy { target, .. } => {
            let name = match target {
                AnnexBFunctionCopyTargetIr::OwnerBinding { storage_name } => storage_name,
                AnnexBFunctionCopyTargetIr::ScriptGlobal { name } => name,
                AnnexBFunctionCopyTargetIr::DirectEvalVariable { .. }
                | AnnexBFunctionCopyTargetIr::FunctionVariableEnvironment { .. } => return,
            };
            names.insert(name.clone());
        }
        StatementIr::Var(declarators) => {
            for declarator in declarators {
                names.insert(declarator.name.clone());
            }
        }
        StatementIr::LexicalBlock(statements)
        | StatementIr::ParameterInitialization { statements, .. } => {
            for statement in statements {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::EmptyStatementCompletion(item) => {
            collect_hoisted_vars_statement(item.statement(), names)
        }
        StatementIr::Block(block) => collect_hoisted_vars_block(block, names),
        StatementIr::If {
            then_branch,
            else_branch,
            ..
        }
        | StatementIr::AsyncFunctionIf {
            condition: _,
            then_branch,
            else_branch,
            plan: _,
        } => {
            collect_hoisted_vars_statement(then_branch, names);
            if let Some(else_branch) = else_branch {
                collect_hoisted_vars_statement(else_branch, names);
            }
        }
        StatementIr::AsyncFunctionWhile(plan) => {
            for statement in plan.condition_prefix() {
                collect_hoisted_vars_statement(statement, names);
            }
            collect_hoisted_vars_statement(plan.body(), names);
        }
        StatementIr::While { body, .. }
        | StatementIr::DoWhile { body, .. }
        | StatementIr::Labelled {
            statement: body, ..
        } => collect_hoisted_vars_statement(body, names),
        StatementIr::For { init, body, .. } => {
            if let Some(init) = init {
                collect_hoisted_vars_for_init(init, names);
            }
            collect_hoisted_vars_statement(body, names);
        }
        StatementIr::OrdinaryGeneratorLoop(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::AsyncGeneratorLoop(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::OrdinaryGeneratorIf(plan) => {
            for region in [plan.then_branch(), plan.else_branch()] {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::AsyncGeneratorIf(plan) => {
            for region in [
                plan.condition().region(),
                plan.then_branch(),
                plan.else_branch(),
            ] {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
            for statement in &plan.body().block().statements {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::AsyncGeneratorResourceScope(plan) => {
            collect_hoisted_vars_block(plan.body().block(), names)
        }
        StatementIr::AsyncGeneratorResourceRegistration(_) => {}
        StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
            for statement in &plan.body().block().statements {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::AsyncFunctionArrayDestructuring(plan) => {
            collect_hoisted_vars_block(plan.body(), names);
        }
        StatementIr::OrdinaryGeneratorWith(plan) => {
            for region in [plan.head().region(), plan.body()] {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::AsyncGeneratorWith(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::AsyncGeneratorSwitch(plan) => {
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
            for declaration in plan.lexical_declarations() {
                collect_hoisted_vars_statement(declaration, names);
            }
        }
        StatementIr::AsyncFunctionWith(plan) => {
            for block in [plan.head(), plan.body()] {
                collect_hoisted_vars_block(block, names);
            }
        }
        StatementIr::AsyncGeneratorForIn(plan) => {
            for block in [
                plan.head().region().block(),
                plan.initialization(),
                plan.body().block(),
            ] {
                collect_hoisted_vars_block(block, names);
            }
        }
        StatementIr::AsyncGeneratorForOf(plan) => {
            for block in [
                plan.head().region().block(),
                plan.initialization().block(),
                plan.body().block(),
            ] {
                collect_hoisted_vars_block(block, names);
            }
        }
        StatementIr::ArrayDestructuringOperation(_) => {}
        StatementIr::OrdinaryGeneratorSwitch(plan) => {
            for declaration in plan.lexical_declarations() {
                collect_hoisted_vars_statement(declaration, names);
            }
            for region in plan.regions() {
                for statement in &region.block().statements {
                    collect_hoisted_vars_statement(statement, names);
                }
            }
        }
        StatementIr::GeneratorLoop {
            init,
            before_suspension,
            suspension_statement,
            after_suspension,
            ..
        } => {
            if let Some(init) = init {
                collect_hoisted_vars_for_init(init, names);
            }
            for statement in before_suspension {
                collect_hoisted_vars_statement(statement, names);
            }
            collect_hoisted_vars_statement(suspension_statement, names);
            for statement in after_suspension {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::AsyncFunctionForOfIterator { plan, .. } => {
            if plan.value_mode() == BindingMode::Var {
                names.insert(plan.value_name().to_string());
            }
            for statement in plan.body().statements() {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::GeneratorForOfIterator { plan, .. } => {
            match plan.value_storage() {
                scriptw_ir::GeneratorForOfIteratorValueStorageIr::Activation(binding)
                | scriptw_ir::GeneratorForOfIteratorValueStorageIr::IterationEnvironment(binding) => {
                    if binding.mode == BindingMode::Var {
                        names.insert(binding.name.clone());
                    }
                }
                scriptw_ir::GeneratorForOfIteratorValueStorageIr::EntryLocal { .. } => {}
            }
            for statement in plan.body().statements() {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::GeneratorIf {
            then_before_yield,
            then_yield_statement,
            then_after_yield,
            else_before_yield,
            else_yield_statement,
            else_after_yield,
            ..
        } => {
            for statement in then_before_yield
                .iter()
                .chain(then_yield_statement.as_deref())
                .chain(then_after_yield)
                .chain(else_before_yield)
                .chain(else_yield_statement.as_deref())
                .chain(else_after_yield)
            {
                collect_hoisted_vars_statement(statement, names);
            }
        }
        StatementIr::ForOfIterator { head, body, .. } => {
            if let ForOfIteratorHeadIr::Assignment { binding, .. } = head {
                if binding.mode == BindingMode::Var {
                    names.insert(binding.name.clone());
                }
            }
            collect_hoisted_vars_statement(body, names);
        }
        StatementIr::ForInArray {
            mode, name, body, ..
        }
        | StatementIr::ForInString {
            mode, name, body, ..
        }
        | StatementIr::ForInObject {
            mode, name, body, ..
        } => {
            if *mode == BindingMode::Var {
                names.insert(name.clone());
            }
            collect_hoisted_vars_statement(body, names);
        }
        StatementIr::AsyncFunctionSwitch(plan) => {
            for declaration in plan.lexical_declarations() {
                collect_hoisted_vars_statement(declaration, names);
            }
            for case in plan.cases() {
                for statement in case.condition_prefix() {
                    collect_hoisted_vars_statement(statement, names);
                }
                collect_hoisted_vars_block(case.body(), names);
            }
        }
        StatementIr::Switch { cases, .. } => {
            for case in cases {
                collect_hoisted_vars_block(&case.body, names);
            }
        }
        StatementIr::TryCatch {
            try_block,
            catch_block,
            ..
        } => {
            collect_hoisted_vars_block(try_block, names);
            collect_hoisted_vars_block(catch_block, names);
        }
        StatementIr::TryFinally {
            try_block,
            finally_block,
            ..
        } => {
            collect_hoisted_vars_block(try_block, names);
            collect_hoisted_vars_block(finally_block, names);
        }
        StatementIr::TryCatchFinally {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            collect_hoisted_vars_block(try_block, names);
            collect_hoisted_vars_block(catch_block, names);
            collect_hoisted_vars_block(finally_block, names);
        }
        StatementIr::SyncDisposableScope { body, .. } => {
            collect_hoisted_vars_block(body, names);
        }
        StatementIr::AsyncDisposableScope { body, .. } => {
            collect_hoisted_vars_block(body, names);
        }
        StatementIr::DeclarationEvaluation(TypedExpr {
            expr:
                ExprIr::ArrayDestructure {
                    pattern,
                    evaluation,
                    ..
                },
            ..
        }) => match *evaluation {
            ArrayDestructuringEvaluationIr::BindingInitialization => {
                pattern.visit_bindings(&mut |mode, name| {
                    if mode == BindingMode::Var {
                        names.insert(name.to_string());
                    }
                });
            }
            ArrayDestructuringEvaluationIr::AssignmentEvaluation => {}
        },
        StatementIr::DeclarationEvaluation(TypedExpr {
            expr: ExprIr::ObjectDestructure { pattern, .. },
            ..
        }) => {
            pattern.visit_bindings(&mut |mode, name| {
                if mode == BindingMode::Var {
                    names.insert(name.to_string());
                }
            });
        }
        StatementIr::DeclarationEvaluation(TypedExpr {
            expr: ExprIr::ObjectDestructuringOperation(operation),
            ..
        }) => {
            operation.visit_bindings(&mut |mode, name| {
                if mode == BindingMode::Var {
                    names.insert(name.to_string());
                }
            });
        }
        StatementIr::AsyncModuleInstantiation
        | StatementIr::Empty
        | StatementIr::Lexical { .. }
        | StatementIr::DeclarationEvaluation(_)
        | StatementIr::Expression(_)
        | StatementIr::GeneratorYield { .. }
        | StatementIr::AsyncAwait { .. }
        | StatementIr::Debugger
        | StatementIr::Return(_)
        | StatementIr::Throw(_)
        | StatementIr::Break { .. }
        | StatementIr::Continue { .. } => {}
    }
}
