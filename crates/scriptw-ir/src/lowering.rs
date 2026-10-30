mod array_literal;
mod assignment;
mod async_disposable;
mod async_generator_assignment;
mod async_generator_for_in;
pub(crate) use async_generator_for_in::CheckedAsyncGeneratorForInInitializer;
mod async_generator_if;
mod async_generator_loop;
mod async_generator_pattern;
mod async_generator_resource;
mod async_generator_switch;
mod async_generator_value;
mod async_generator_with;
mod async_identifier_assignment;
mod async_pattern;
pub(crate) use async_generator_resource::CheckedAsyncGeneratorResourceRegistration;

#[derive(Clone, Copy)]
enum StatementListPlacement {
    Root,
    Block,
}
mod async_generator_for_of;
pub(crate) use async_generator_for_of::CheckedAsyncGeneratorForOfInitializer;
mod async_classic_loop;
mod async_with;
mod break_continue;
mod builtin_call_info;
#[cfg(test)]
mod builtin_call_info_tests;
mod builtin_catalog;
mod builtin_shapes;
mod catalog_overlay;
use catalog_overlay::CatalogOverlay;
type FunctionSignatures = CatalogOverlay<FunctionSignature>;
type GlobalProperties = CatalogOverlay<GlobalPropertyInfo>;
mod call_candidate_analysis;
mod call_expression;
#[cfg(test)]
mod capture_invalidation_tests;
mod class_definition;
mod class_suspension;
mod conditional_flow;
mod define_property_call;
mod delete_expression;
mod direct_eval;
mod dynamic_source;
mod environment_identifier;
mod finite_function_body_hints;
mod finite_function_global_syntax;
mod finite_function_globals;
mod finite_function_source;
mod finite_iterator_source;
mod finite_regexp_source;
mod finite_returned_callable_source;
mod finite_source_binding;
mod finite_string_source;
mod for_lexical_environment;
mod function_source_candidates;
mod generator_array_pattern;
mod generator_call;
mod generator_compound_assignment;
mod generator_eager_value;
mod generator_identifier_reference;
mod generator_logical_assignment;
mod generator_loop;
mod generator_object_literal;
mod generator_object_pattern;
mod generator_pattern_assignment;
mod generator_pattern_initializer;
mod generator_pattern_target;
mod generator_plain_assignment;
mod generator_switch;
mod generator_value_branch;
mod generator_with;
mod pattern_target;
mod resumable_for_in;
mod resumable_for_initializer;
mod resumable_operand;
mod resumable_pattern;
mod resumable_switch;
mod super_construct;
mod suspended_call;
use finite_function_source::FiniteSourceValue;
use finite_source_binding::FiniteSourceBindingKey;
mod prepared_function;
mod prepared_script;
use prepared_function::{
    append_prepared_unit, compile_dynamic_function_sources, record_prepared_compilation_stage,
};
use prepared_script::compile_dynamic_script_sources;
pub(crate) use prepared_script::lower_runtime_direct_eval_program;
pub(crate) use prepared_script::lower_runtime_global_script_program;
mod for_in;
mod for_loop;
mod for_of;
mod function_declaration;
mod function_definition;
mod function_environment;
mod function_inference;
mod global_script_prelude;
mod if_statement;
mod intrinsic_method;
mod invocation_effects;
mod labelled_statement;
mod lexical_declaration;
mod module_graph;
mod signature_evidence;
mod signature_lookup;
mod var_declaration;
pub use module_graph::{
    lower_complete_module_catalog, lower_module_graph, lower_module_graph_with_host_surface_policy,
    lower_module_graph_with_prelude, lower_script_graph,
    lower_script_graph_with_host_surface_policy,
};
mod async_expression_prefix;
mod async_switch;
use crate::ir::reference::{
    CapturedPropertyKeySlot, CapturedPropertyReceiverSlot, CapturedPropertyTargetSlot,
};
use async_expression_prefix::{AwaitedLogicalAssignmentSource, CheckedAsyncPrefixSource};
mod awaited_while_condition;
use awaited_while_condition::AsyncValueBranchContext;
mod eager_async_while_body;
mod module_execution;
mod named_evaluation;
mod new_expression;
mod object_environment_logical;
mod object_literal;
mod operator_values;
mod optional_chain;
mod ordinary_property_compound;
mod ordinary_property_logical;
mod ordinary_property_update;
mod private_numeric_update;
mod promise_caller_flow;
mod property_access;
mod proxy_traps;
mod statement;
mod static_literals;
mod static_string_binding_facts;
mod super_property_mutation;
mod switch_statement;
pub(crate) mod synchronous_resource_loop;
mod throw_inference;
mod try_statement;
mod while_loop;
mod with_environment_call;
mod with_environment_compound;
mod with_statement;

use super::*;
use crate::source_call_flow_proof::SourceCallFlowEffects;
use async_disposable::LoweredStatementListItemIr;
use call_candidate_analysis::{CallCandidateAnalysis, CallCandidateSource};
use define_property_call::InvocationTargetProvenance;
use dynamic_source::{
    BuiltinCallContext, OptionalCallSource, ResolvedDynamicSourceCall,
    already_accounted_optional_calls,
};
use generator_identifier_reference::RetainedGeneratorIdentifierReference;
use intrinsic_method::{IntrinsicMethodLookup, IntrinsicPrototype};
use invocation_effects::{AnalyzedInvocationEffects, InvocationCallerFlowEffects};
use object_environment_logical::LogicalAssignmentReachability;
use ordinary_property_compound::BuiltinGetterReceiverProvenance;
use proxy_traps::{ProxyTrap, ProxyTrapSignature};
use static_string_binding_facts::StaticStringBindingFacts;

/// Reference Records (6.2.5). `Strictness` and `carried_strictness` arrive
/// through the crate-root glob above; the rest of the module is `pub(crate)`
/// and is named explicitly so every use of the Reference typestate in this
/// file is traceable to one import. See
/// `docs/rust-rewrite/contracts/reference-records.md`.
use crate::ir::reference::{
    CapturedBindingPosition, CapturedCursorDepth, CapturedObjectPosition, Composition,
    CurrentScopeDepth, DeclarativeEnvironmentPosition, DeleteSuperReferencePlan,
    EagerCompoundAssignmentBindings, EagerCompoundAssignmentOp, NumericUpdateBindings,
    ObjectEnvironmentBindingObject, OrderedWithEnvironmentChain, OrdinaryPropertyReferencePlan,
    PositionedWithEnvironment, PutValueBindings, ReferenceBase, ReferenceOperand, ReferencePins,
    ReferenceRecord, SelectedWithEnvironmentObjects, SuperPropertyReferencePlan,
    WithEnvironmentReferencePlan, reference_base_of_lowered_read,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BindingInfo {
    pub(crate) mode: BindingMode,
    pub(crate) storage_name: String,
    pub(crate) kind: ValueKind,
    pub(crate) possible_kinds: KindSet,
    pub(crate) heap_shape: Option<Box<HeapShape>>,
    pub(crate) function_targets: FunctionTargetKnowledge,
    /// ECMA-262 9.1.1.1: whether InitializeBinding (9.1.1.1.4) has run for this
    /// binding. Mandatory and undefaulted — see `binding_lifecycle::Initialization`.
    pub(crate) initialization: Initialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VarBindingInfo {
    pub(crate) kind: ValueKind,
    pub(crate) possible_kinds: KindSet,
    pub(crate) heap_shape: Option<Box<HeapShape>>,
    pub(crate) function_targets: FunctionTargetKnowledge,
    pub(crate) is_script_global: bool,
    pub(crate) is_lexical_metadata: bool,
}

impl VarBindingInfo {
    fn to_binding_info(&self, name: &str) -> BindingInfo {
        BindingInfo {
            mode: BindingMode::Var,
            storage_name: name.to_string(),
            kind: self.kind,
            possible_kinds: self.possible_kinds,
            heap_shape: self.heap_shape.clone(),
            function_targets: self.function_targets.clone(),
            initialization: Initialization::Initialized,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalPropertySource {
    Builtin,
    HostBuiltin,
    GlobalWrite,
    ImplicitGlobalWrite,
    DefinitelyDeleted,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlobalPropertyInfo {
    pub(crate) value_info: ValueInfo,
    pub(crate) proven_present: bool,
    pub(crate) configurable: bool,
    pub(crate) source: GlobalPropertySource,
}

/// The joined global-property fact immediately before a logical assignment's
/// possible outer write.
#[derive(Debug, Clone)]
#[must_use = "a pre-write global-property fact must be consumed by the possible write"]
struct PreWriteGlobalPropertyValue(Option<ValueInfo>);

/// The only builtin-prototype `toString` identities that static call lowering
/// may rely on after observable prototype writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrototypeToStringState {
    Intrinsic,
    ObjectPrototype,
    Unknown,
}

impl PrototypeToStringState {
    fn join(self, other: Self) -> Self {
        if self == other { self } else { Self::Unknown }
    }
}

/// Whether a function body's current execution path may still use heap shapes
/// stored in whole-program function signatures.
///
/// Function signatures are analysis summaries shared by every nested lowerer.
/// A property effect inside one function must invalidate later calls in that
/// function, but the effect has not run merely because the body was lowered.
/// Keeping that loss of evidence in the body-local flow state prevents an
/// uncalled function from erasing unrelated summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionSignatureShapeEvidence {
    Available,
    Invalidated,
}

impl FunctionSignatureShapeEvidence {
    fn join(self, other: Self) -> Self {
        if self == Self::Available && other == Self::Available {
            Self::Available
        } else {
            Self::Invalidated
        }
    }

    fn apply(self, mut info: ValueInfo) -> ValueInfo {
        if self == Self::Invalidated {
            info.heap_shape = None;
        }
        info
    }
}

/// Whether a function's result shape is a mutable heap summary or a recipe for
/// a new result created by each invocation.
///
/// An arbitrary call can mutate an object returned by a source function, so a
/// cached alias shape is only valid until the next unknown effect. A host such
/// as CreateRealm instead creates a new realm record after that effect; losing
/// its shape would confuse an earlier mutation with a future allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FunctionReturnShape {
    Unobserved,
    Absent,
    FlowSensitive(Box<HeapShape>),
    RecreatedPerCall {
        shape: Box<HeapShape>,
        dependencies: FunctionReturnDependencies,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FunctionReturnDependencies {
    global_function_targets: BTreeMap<String, FunctionId>,
}

impl FunctionReturnDependencies {
    fn with_global_function_target(
        mut self,
        name: String,
        function_id: FunctionId,
    ) -> Option<Self> {
        if self
            .global_function_targets
            .get(&name)
            .is_some_and(|existing| existing != &function_id)
        {
            return None;
        }
        self.global_function_targets.insert(name, function_id);
        Some(self)
    }

    fn merged(&self, other: &Self) -> Option<Self> {
        let mut merged = self.clone();
        for (name, function_id) in &other.global_function_targets {
            merged = merged.with_global_function_target(name.clone(), function_id.clone())?;
        }
        Some(merged)
    }
}

impl FunctionReturnShape {
    fn flow_sensitive(shape: Option<Box<HeapShape>>) -> Self {
        shape.map_or(Self::Absent, Self::FlowSensitive)
    }

    fn recreated_per_call(shape: Box<HeapShape>) -> Self {
        Self::RecreatedPerCall {
            shape,
            dependencies: FunctionReturnDependencies::default(),
        }
    }

    fn recreated_per_call_with_dependencies(
        shape: Box<HeapShape>,
        dependencies: FunctionReturnDependencies,
    ) -> Self {
        Self::RecreatedPerCall {
            shape,
            dependencies,
        }
    }

    fn cloned_shape(&self) -> Option<Box<HeapShape>> {
        match self {
            Self::Unobserved | Self::Absent => None,
            Self::FlowSensitive(shape) | Self::RecreatedPerCall { shape, .. } => {
                Some(shape.clone())
            }
        }
    }

    fn with_shape(&self, shape: Option<Box<HeapShape>>) -> Self {
        let Some(shape) = shape else {
            return Self::Absent;
        };
        match self {
            Self::RecreatedPerCall { dependencies, .. } => {
                Self::recreated_per_call_with_dependencies(shape, dependencies.clone())
            }
            Self::Unobserved | Self::Absent | Self::FlowSensitive(_) => Self::FlowSensitive(shape),
        }
    }

    fn merged(left: &Self, right: &Self, shape: Option<Box<HeapShape>>) -> Self {
        if matches!((left, right), (Self::Unobserved, Self::Unobserved)) {
            return Self::Unobserved;
        }
        if matches!(left, Self::Unobserved) {
            return right.with_shape(shape);
        }
        if matches!(right, Self::Unobserved) {
            return left.with_shape(shape);
        }
        let Some(shape) = shape else {
            return Self::Absent;
        };
        let dependencies = match (left, right) {
            (
                Self::RecreatedPerCall {
                    dependencies: left, ..
                },
                Self::RecreatedPerCall {
                    dependencies: right,
                    ..
                },
            ) => left.merged(right),
            (Self::Absent | Self::FlowSensitive(_), Self::Absent | Self::FlowSensitive(_))
            | (Self::RecreatedPerCall { .. }, Self::FlowSensitive(_))
            | (Self::FlowSensitive(_), Self::RecreatedPerCall { .. })
            | (Self::RecreatedPerCall { .. }, Self::Absent)
            | (Self::Absent, Self::RecreatedPerCall { .. }) => None,
            (Self::Unobserved, _) | (_, Self::Unobserved) => {
                unreachable!("unobserved return shapes were handled before merging")
            }
        };
        match dependencies {
            Some(dependencies) => Self::recreated_per_call_with_dependencies(shape, dependencies),
            None => Self::FlowSensitive(shape),
        }
    }

    fn invalidate_flow_sensitive(&mut self) {
        match self {
            Self::FlowSensitive(_) => *self = Self::Absent,
            Self::Unobserved | Self::Absent | Self::RecreatedPerCall { .. } => {}
        }
    }

    fn visit_flow_sensitive(
        &mut self,
        possible_kinds: KindSet,
        function_targets: &FunctionTargetKnowledge,
        visit: &mut impl FnMut(KindSet, &FunctionTargetKnowledge, &mut HeapShape) -> bool,
    ) {
        match self {
            Self::FlowSensitive(shape) => {
                if !visit(possible_kinds, function_targets, shape) {
                    *self = Self::Absent;
                }
            }
            Self::Unobserved | Self::Absent | Self::RecreatedPerCall { .. } => {}
        }
    }

    fn recreated_dependencies(&self) -> Option<&FunctionReturnDependencies> {
        match self {
            Self::RecreatedPerCall { dependencies, .. } => Some(dependencies),
            Self::Unobserved | Self::Absent | Self::FlowSensitive(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
struct FunctionReturnObservation {
    info: ValueInfo,
    shape: FunctionReturnShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LabelTargetKind {
    Breakable,
    Loop,
}

/// One entry of the 7.1.1 ToPrimitive lookup order.
///
/// OrdinaryToPrimitive genuinely mixes a symbol key with two ordinary string
/// method names, so the list cannot be all-`WellKnownSymbol`. It must not be
/// all-`&str` either: the two kinds resolve against different shape-map
/// namespaces, and collapsing them is how a bare description ends up being
/// looked up as if it were a string key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToPrimitiveLookupKey {
    Symbol(WellKnownSymbol),
    Method(&'static str),
}

/// Builtin methods whose inferred target proves the acquired function's
/// identity, but does not prove that the property base has the receiver brand
/// required by that function.
///
/// This is deliberately the complete Boolean/Number/BigInt/String
/// primitive-brand family, not a list of destination property names.
/// Possessing one of these values is the proof that lowering must retain the
/// acquired callee and `this` base as separate operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NonGenericBuiltinMethod {
    BooleanToString,
    BooleanValueOf,
    NumberToExponential,
    NumberToFixed,
    NumberToLocaleString,
    NumberToPrecision,
    NumberToString,
    NumberValueOf,
    BigIntToString,
    BigIntToLocaleString,
    BigIntValueOf,
    StringToString,
    StringValueOf,
}

impl NonGenericBuiltinMethod {
    fn from_function_id(function_id: &str) -> Option<Self> {
        match StandardBuiltinId::from_function_id(function_id)? {
            StandardBuiltinId::BooleanPrototypeToString => Some(Self::BooleanToString),
            StandardBuiltinId::BooleanPrototypeValueOf => Some(Self::BooleanValueOf),
            StandardBuiltinId::NumberPrototypeToExponential => Some(Self::NumberToExponential),
            StandardBuiltinId::NumberPrototypeToFixed => Some(Self::NumberToFixed),
            StandardBuiltinId::NumberPrototypeToLocaleString => Some(Self::NumberToLocaleString),
            StandardBuiltinId::NumberPrototypeToPrecision => Some(Self::NumberToPrecision),
            StandardBuiltinId::NumberPrototypeToString => Some(Self::NumberToString),
            StandardBuiltinId::NumberPrototypeValueOf => Some(Self::NumberValueOf),
            StandardBuiltinId::BigIntPrototypeToString => Some(Self::BigIntToString),
            StandardBuiltinId::BigIntPrototypeToLocaleString => Some(Self::BigIntToLocaleString),
            StandardBuiltinId::BigIntPrototypeValueOf => Some(Self::BigIntValueOf),
            StandardBuiltinId::StringPrototypeToString => Some(Self::StringToString),
            StandardBuiltinId::StringPrototypeValueOf => Some(Self::StringValueOf),
            _ => None,
        }
    }
}

/// The operator a compound or logical assignment applies to a property
/// Reference.
enum PropertyUpdateOp {
    Arithmetic(ArithmeticOp),
    Bitwise(BitwiseOp),
    Logical(LogicalBinaryOp),
}

/// The two lowering outcomes for a computed property key on a String exotic.
///
/// `CanonicalIndex` is available only after static proof. Every other key is
/// preserved for the ordinary `ToPropertyKey` path; failure to prove an index
/// is not a reason to reject the program.
enum StringExoticComputedKey {
    CanonicalIndex(Box<TypedExpr>),
    OrdinaryPropertyKey(PropertyKeyIr),
}

impl StringExoticComputedKey {
    fn into_property_key(self) -> PropertyKeyIr {
        match self {
            Self::CanonicalIndex(index) => PropertyKeyIr::ArrayIndex(index),
            Self::OrdinaryPropertyKey(key) => key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveLabel {
    pub(crate) name: String,
    pub(crate) kind: LabelTargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingLookupLocation {
    Scope(usize),
    VariableEnvironment,
}

/// ResolveBinding's declarative/global fallback, located exactly once before
/// Object Environment selection and consumed by the final write.
enum LocatedIdentifierReference {
    Declarative {
        resolution: BindingResolution,
        position: DeclarativeEnvironmentPosition,
    },
    Unresolvable,
}

/// Whether an identifier operation is definitely reached or is the fallback
/// of a run-time Object Environment Record selection. The latter invalidates
/// prior static binding shape because observable HasBinding may mutate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentifierUpdateReachability {
    Definite,
    WithEnvironmentFallback,
}

fn unknown_runtime_value_info() -> ValueInfo {
    ValueInfo {
        kind: ValueKind::Dynamic,
        possible_kinds: KindSet::all_runtime_tags(),
        heap_shape: None,
        function_targets: FunctionTargetKnowledge::unknown(),
    }
}

fn generated_capture_can_use_definition_time_facts(
    mode: BindingMode,
    execution_kind: ClassElementExecutionKind,
) -> bool {
    match (mode, execution_kind) {
        (BindingMode::Const, _) => true,
        (
            BindingMode::Let | BindingMode::Var,
            ClassElementExecutionKind::StaticFieldInitializer
            | ClassElementExecutionKind::StaticBlock,
        ) => true,
        (
            BindingMode::Let | BindingMode::Var,
            ClassElementExecutionKind::None | ClassElementExecutionKind::InstanceFieldInitializer,
        ) => false,
    }
}

impl LocatedIdentifierReference {
    fn declarative_position(&self) -> Option<DeclarativeEnvironmentPosition> {
        match self {
            Self::Declarative { position, .. } => Some(*position),
            Self::Unresolvable => None,
        }
    }
}

fn captured_environment_positions(
    analysis: &Analysis<'_>,
    function: &FunctionPlan<'_>,
) -> (
    BTreeMap<String, CapturedBindingPosition>,
    Vec<(WithObjectBindingName, CapturedObjectPosition)>,
) {
    let mut cursor = Some(
        analysis
            .owner_plans
            .get(&function.id)
            .expect("function owner must be planned")
            .definition_environment_cursor
            .clone(),
    );
    let mut binding_positions = BTreeMap::new();
    let mut with_positions = Vec::new();
    let mut depth = 0;
    while let Some(current) = cursor {
        let environment = analysis
            .environment_plans
            .get(&current.environment_id)
            .expect("definition cursor must name a planned environment");
        let cursor_depth = CapturedCursorDepth::at(depth);
        for (storage_name, _) in function
            .captures
            .iter()
            .filter(|(_, capture)| capture.environment_id == environment.id)
        {
            binding_positions.insert(
                storage_name.clone(),
                CapturedBindingPosition::at(cursor_depth),
            );
        }
        if environment.kind == EnvironmentKind::WithObject {
            let with_plan = analysis
                .with_object_environment_plans
                .get(&environment.id)
                .expect("WithObject environment must carry a hidden binding plan");
            assert!(
                function
                    .captures
                    .contains_key(with_plan.binding_name.as_str()),
                "surrounding WithObject environment must be captured"
            );
            with_positions.push((
                with_plan.binding_name.clone(),
                CapturedObjectPosition::at(cursor_depth),
            ));
        }
        cursor = environment.parent_cursor.clone();
        depth += 1;
    }
    (binding_positions, with_positions)
}

pub(crate) struct LoweredScript {
    pub(crate) function_global_hints: crate::prepared_source_cache::FunctionGlobalConstructorHints,
    pub(crate) dynamic_script_sources: Vec<DynamicScriptSource>,
    pub(crate) runtime_declarations: RuntimeGlobalDeclarationPlan,
    pub(crate) dynamic_function_sources: Vec<DynamicFunctionSource>,
    pub(crate) functions: Vec<FunctionIr>,
    pub(crate) body: BlockIr,
    pub(crate) owned_env_bindings: Vec<OwnedEnvBindingIr>,
    pub(crate) global_bindings: GlobalBindingPlan,
    pub(crate) host_builtins: Vec<HostBuiltinId>,
    pub(crate) builtin_ctor_calls: usize,
    pub(crate) builtin_static_calls: usize,
    pub(crate) error_builtin_calls: usize,
    pub(crate) aggregate_errors: usize,
    pub(crate) function_proto_calls: usize,
    pub(crate) function_proto_applies: usize,
    pub(crate) function_proto_binds: usize,
    pub(crate) function_proto_to_strings: usize,
    pub(crate) bound_functions: usize,
    pub(crate) bound_function_constructs: usize,
    pub(crate) boxed_builtin_calls: usize,
    pub(crate) boxed_builtin_constructs: usize,
    pub(crate) boxed_receiver_adaptations: usize,
    pub(crate) top_level_this_uses: usize,
    pub(crate) host_builtin_calls: usize,
    pub(crate) error_proto_to_strings: usize,
    pub(crate) diagnostics: Vec<IrDiagnostic>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ClassLoweringContext {
    pub(crate) private_name_ids: BTreeMap<String, PrivateNameId>,
    pub(crate) heritage_kind: ClassHeritageKind,
    pub(crate) super_base_shape: Option<Box<HeapShape>>,
    pub(crate) super_constructor_target: Option<FunctionId>,
    pub(crate) is_static: bool,
    pub(crate) is_derived_constructor: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct GeneratedFunctionOutput {
    pub(crate) return_info: ValueInfo,
    pub(crate) construct_this_info: Option<ValueInfo>,
}

#[derive(Debug, Clone)]
enum GeneratedClassElementFlow {
    Deferred,
    Executed(ExecutedClassElementPostState),
}

#[derive(Debug, Clone)]
struct ExecutedClassElementPostState {
    global_properties: GlobalProperties,
    well_known_symbol_prototype_properties: BTreeMap<(String, WellKnownSymbol), ValueInfo>,
    script_global_values: BTreeMap<String, ValueInfo>,
    captured_parent_values: Vec<(String, ValueInfo)>,
    invalidated_static_binding_names: BTreeSet<String>,
    boolean_alias_shapes_invalidated: bool,
    array_prototype_mutated: bool,
    number_prototype_to_string_state: PrototypeToStringState,
    boolean_prototype_to_string_state: PrototypeToStringState,
    unknown_user_code_effects_observed: bool,
    unknown_user_code_effects_introduced: bool,
    intervening_effect_epoch: u64,
    function_signature_shape_evidence: FunctionSignatureShapeEvidence,
    source_call_flow_effects: SourceCallFlowEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootThisBinding {
    GlobalObject,
    Undefined,
    EvalCaller,
}

impl RootThisBinding {
    const fn for_goal(goal: ParseGoal) -> Self {
        match goal {
            ParseGoal::Script => Self::GlobalObject,
            ParseGoal::Module => Self::Undefined,
        }
    }
}

enum OptionalPropertyReadAnalysis {
    ProvenEffectFree(ValueInfo),
    MayRunUserCode(ValueInfo),
}

enum PreArgumentHeapShapeSnapshots<'a> {
    NoHeapShapes,
    OneHeapShape(&'a mut Option<Box<HeapShape>>),
    TwoHeapShapes {
        first: &'a mut Option<Box<HeapShape>>,
        second: &'a mut Option<Box<HeapShape>>,
    },
}

impl PreArgumentHeapShapeSnapshots<'_> {
    fn invalidate(self) {
        match self {
            Self::NoHeapShapes => {}
            Self::OneHeapShape(heap_shape) => *heap_shape = None,
            Self::TwoHeapShapes { first, second } => {
                *first = None;
                *second = None;
            }
        }
    }
}

#[must_use = "lowered call arguments must account for values captured before their evaluation"]
struct LoweredCallArguments {
    arguments: Vec<TypedExpr>,
    invalidates_preceding_heap_shapes: bool,
}

impl LoweredCallArguments {
    fn into_arguments(self, preceding: PreArgumentHeapShapeSnapshots<'_>) -> Vec<TypedExpr> {
        if self.invalidates_preceding_heap_shapes {
            preceding.invalidate();
        }
        self.arguments
    }

    fn into_arguments_without_predecessor(self) -> Vec<TypedExpr> {
        self.into_arguments(PreArgumentHeapShapeSnapshots::NoHeapShapes)
    }

    fn into_arguments_after_expression(self, preceding: &mut TypedExpr) -> Vec<TypedExpr> {
        self.into_arguments(PreArgumentHeapShapeSnapshots::OneHeapShape(
            &mut preceding.heap_shape,
        ))
    }

    fn into_arguments_after_value(self, preceding: &mut ValueInfo) -> Vec<TypedExpr> {
        self.into_arguments(PreArgumentHeapShapeSnapshots::OneHeapShape(
            &mut preceding.heap_shape,
        ))
    }

    fn into_arguments_after_two_expressions(
        self,
        first: &mut TypedExpr,
        second: &mut TypedExpr,
    ) -> Vec<TypedExpr> {
        self.into_arguments(PreArgumentHeapShapeSnapshots::TwoHeapShapes {
            first: &mut first.heap_shape,
            second: &mut second.heap_shape,
        })
    }

    fn into_arguments_after_expression_and_value(
        self,
        expression: &mut TypedExpr,
        value: &mut ValueInfo,
    ) -> Vec<TypedExpr> {
        self.into_arguments(PreArgumentHeapShapeSnapshots::TwoHeapShapes {
            first: &mut expression.heap_shape,
            second: &mut value.heap_shape,
        })
    }
}

struct OptionalChainAnalysisState {
    current: ValueInfo,
    property_receiver: Option<ValueInfo>,
    short_circuit_reaches_result: bool,
    invocation_effects: AnalyzedInvocationEffects,
}

impl OptionalChainAnalysisState {
    fn from_target(target: &TypedExpr) -> Self {
        Self {
            current: target.value_info(),
            property_receiver: None,
            short_circuit_reaches_result: false,
            invocation_effects: AnalyzedInvocationEffects::already_applied(),
        }
    }
}

enum InvocationThisObservation<'a> {
    NotObserved,
    ConstructorCallee(&'a mut TypedExpr),
    Default,
    Current,
    ExplicitMethod {
        receiver: &'a mut TypedExpr,
        callee: &'a mut TypedExpr,
    },
    ExplicitValueMethod {
        receiver: &'a mut ValueInfo,
        callee: &'a mut TypedExpr,
    },
}

#[derive(Debug, Clone)]
enum CurrentThisBinding {
    Root(RootThisBinding),
    Activation(ValueInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FunctionSignature {
    pub(crate) id: FunctionId,
    pub(crate) to_string_representation: CallableToStringRepresentation,
    pub(crate) protocol: FunctionProtocolIr,
    pub(crate) callable: bool,
    pub(crate) class_heritage_kind: ClassHeritageKind,
    pub(crate) params: Vec<FunctionParamSignature>,
    pub(crate) return_kind: ValueKind,
    pub(crate) return_possible_kinds: KindSet,
    return_shape: FunctionReturnShape,
    pub(crate) return_targets: FunctionTargetKnowledge,
    pub(crate) constructor_instance: ValueInfo,
    pub(crate) this_info: ValueInfo,
    pub(crate) this_observed: bool,
    source_call_flow_effects: SourceCallFlowEffects,
}

impl FunctionSignature {
    fn has_live_shape_roots(&self) -> bool {
        matches!(self.return_shape, FunctionReturnShape::FlowSensitive(_))
            || self.constructor_instance.heap_shape.is_some()
            || self.this_info.heap_shape.is_some()
            || self.params.iter().any(|param| param.heap_shape.is_some())
    }

    fn return_info(&self) -> ValueInfo {
        ValueInfo {
            kind: self.return_kind,
            possible_kinds: self.return_possible_kinds,
            heap_shape: self.return_shape.cloned_shape(),
            function_targets: self.return_targets.clone(),
        }
    }

    fn merged_source_call_flow_effects(&self, observed: &Self) -> SourceCallFlowEffects {
        self.source_call_flow_effects
            .merge_observation(observed.source_call_flow_effects)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FunctionParamSignature {
    pub(crate) kind: ValueKind,
    pub(crate) possible_kinds: KindSet,
    pub(crate) heap_shape: Option<Box<HeapShape>>,
    pub(crate) function_targets: FunctionTargetKnowledge,
    pub(crate) observed: bool,
    pub(crate) has_default: bool,
    pub(crate) is_rest: bool,
}

/// Lowers a single source unit.
///
/// A module source is treated as a one-node graph, which is the only shape a
/// caller without a host module loader can produce. Use [`lower_module_graph`]
/// to lower a module together with its loaded dependency closure.
pub fn lower(source: &ParsedSource) -> ProgramIr {
    lower_with_host_surface_policy(source, HostSurfacePolicy::default())
}

/// Lowers one source unit under an explicit host-global authority.
pub fn lower_with_host_surface_policy(
    source: &ParsedSource,
    host_surface_policy: HostSurfacePolicy,
) -> ProgramIr {
    LoweringSession::default().lower_source(source, host_surface_policy)
}

impl LoweringSession {
    /// Lowers one retained source while reusing this compilation's prepared
    /// parse products and collecting their potential Realm-origin imports.
    pub fn lower_source(
        &self,
        source: &ParsedSource,
        host_surface_policy: HostSurfacePolicy,
    ) -> ProgramIr {
        match source {
            ParsedSource::Script(source) => {
                let mut allocations = AnalysisAllocationState::default();
                allocations.prepared_sources = self.prepared_sources.clone();
                allocations.script_import_origin =
                    Some(crate::direct_eval_context::ScriptImportOrigin::Discovery);
                lower_script_program_with_allocations(
                    source,
                    ParseGoal::Script,
                    source.source_text.len(),
                    vec![LoweringStage::ParsedSource],
                    None,
                    &modules::LinkedScriptDefinitions::default(),
                    host_surface_policy,
                    &mut allocations,
                    ScriptInstantiation::FreshEntry,
                )
            }
            ParsedSource::Module(source) => self.lower_loaded_graph(
                &ModuleGraphSources::single(source),
                host_surface_policy,
                None,
            ),
        }
    }
}

fn new_program(goal: ParseGoal, source_len: usize, stages: Vec<LoweringStage>) -> ProgramIr {
    ProgramIr {
        goal,
        stages,
        source_len,
        invariants: crate::LoweringInvariant::ALL.to_vec(),
        diagnostics: Vec::new(),
        script: None,
        modules: None,
    }
}

/// Complete binding/source/slot analysis before Script lowering consumes it.
/// Capacity admission stays at the caller, before any source owner is allocated.
fn prepare_script_analysis<'a>(
    script_source: &'a ParsedScript,
    script: &'a Script,
    interner: &'a Interner,
    definitions: &modules::LinkedScriptDefinitions,
    instantiation: &ScriptInstantiation,
    allocations: &mut AnalysisAllocationState,
) -> Analysis<'a> {
    let template_source = allocations.template_sources.for_parsed(script_source);
    let mut analysis =
        AnalysisBuilder::with_allocations(allocations, instantiation.clone(), template_source)
            .with_realm_reusable_module_environments(definitions.has_realm_reusable_modules())
            .finish(
                script,
                interner,
                CompilerSourceText::Utf16(script_source.compiler_source_utf16()),
            );
    analysis.function_global_hints = allocations
        .prepared_sources
        .function_global_hints(script_source);
    definitions.apply(script, &mut analysis, interner);
    analysis.prepare_runtime_script_slots(script, interner);
    analysis.source_function_global_hypotheses =
        finite_function_global_syntax::source_function_global_hypotheses(
            script, interner, &analysis,
        );
    allocations.commit_analysis(&analysis.allocations);
    analysis
}

fn lower_script_program_with_allocations(
    script_source: &ParsedScript,
    goal: ParseGoal,
    source_len: usize,
    stages: Vec<LoweringStage>,
    modules: Option<ModuleGraphIr>,
    definitions: &modules::LinkedScriptDefinitions,
    host_surface_policy: HostSurfacePolicy,
    allocations: &mut AnalysisAllocationState,
    instantiation: ScriptInstantiation,
) -> ProgramIr {
    let root_this_binding = if matches!(
        &instantiation,
        ScriptInstantiation::Prepared(PreparedScriptKind::DirectEval(_))
    ) {
        RootThisBinding::EvalCaller
    } else {
        RootThisBinding::for_goal(goal)
    };
    let mut program = new_program(goal, source_len, stages);
    program.modules = modules;

    script_source.with_compiler_session(|script, interner| {
        if let Err(message) = crate::runtime_direct_eval::private_capacity::require_capacity(
            allocations,
            script,
            &instantiation,
            source_len,
        ) {
            program.diagnostics.push(IrDiagnostic::unsupported(message));
            return;
        }
        let trace_phases = std::env::var_os("SCRIPTW_LOWER_TRACE").is_some();
        let t0 = std::time::Instant::now();
        let analysis = prepare_script_analysis(
            script_source,
            script,
            interner,
            definitions,
            &instantiation,
            allocations,
        );
        if trace_phases {
            eprintln!("scriptw lower trace: analysis: {:?}", t0.elapsed());
        }
        let t1 = std::time::Instant::now();
        let mut lowerer = ScriptLowerer::new(
            interner,
            &analysis,
            CompilerSourceText::Utf16(script_source.compiler_source_utf16()),
            root_this_binding,
            SCRIPT_OWNER_ID.to_string(),
            host_surface_policy,
        );
        lowerer.seed_returned_callable_parameter_candidates(script_source);
        let lowered = lowerer.lower(script);
        if trace_phases {
            eprintln!("scriptw lower trace: script-lower: {:?}", t1.elapsed());
        }
        program.script = Some(ScriptIr {
            foreign_modules: analysis.module_execution.foreign_modules.clone(),
            language_features: crate::LanguageFeaturePolicy::default(),
            template_source: analysis.template_source,
            eval_environment: analysis.owner_eval_environment(SCRIPT_OWNER_ID),
            module_prelude: None,
            script_entry_after_prelude: None,
            prepared_scripts: Vec::new(),
            runtime_declarations: lowered.runtime_declarations,
            prepared_dynamic_functions: Vec::new(),
            strict: script.strict(),
            functions: lowered.functions,
            body: lowered.body,
            owned_env_bindings: lowered.owned_env_bindings,
            global_bindings: lowered.global_bindings,
            host_builtins: lowered.host_builtins,
            builtin_ctor_calls: lowered.builtin_ctor_calls,
            builtin_static_calls: lowered.builtin_static_calls,
            error_builtin_calls: lowered.error_builtin_calls,
            aggregate_errors: lowered.aggregate_errors,
            function_proto_calls: lowered.function_proto_calls,
            function_proto_applies: lowered.function_proto_applies,
            function_proto_binds: lowered.function_proto_binds,
            function_proto_to_strings: lowered.function_proto_to_strings,
            bound_functions: lowered.bound_functions,
            bound_function_constructs: lowered.bound_function_constructs,
            boxed_builtin_calls: lowered.boxed_builtin_calls,
            boxed_builtin_constructs: lowered.boxed_builtin_constructs,
            boxed_receiver_adaptations: lowered.boxed_receiver_adaptations,
            top_level_this_uses: lowered.top_level_this_uses,
            host_builtin_calls: lowered.host_builtin_calls,
            error_proto_to_strings: lowered.error_proto_to_strings,
        });
        program.stages.push(LoweringStage::ScriptIrBuilt);
        if !lowered.diagnostics.is_empty() {
            program
                .stages
                .push(LoweringStage::UnsupportedFeaturesRecorded);
        } else {
            program.stages.push(LoweringStage::WasmReady);
        }
        program.diagnostics = lowered.diagnostics;
        compile_dynamic_function_sources(
            &mut program,
            lowered.dynamic_function_sources,
            &lowered.function_global_hints,
            host_surface_policy,
            allocations,
        );
        compile_dynamic_script_sources(
            &mut program,
            lowered.dynamic_script_sources,
            host_surface_policy,
            allocations,
        );
    });

    program
}

pub(crate) fn is_ecmascript_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

type ExactHelperContextId = String;
type ExactCallbackContextKey = (FunctionId, ExactHelperContextId);
type DirectCallContextKey = (FunctionId, ExactHelperContextId);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptGlobalCallObservationMode {
    DormantSummary,
    ExecutedFlow,
}

enum ImmutableBindingWriteOutcome {
    Ignored(IdentifierWriteReferenceIr),
    Abrupt(IdentifierWriteErrorIr),
}

struct PreparedIdentifierWrite {
    value: TypedExpr,
    ignored: Option<crate::reference::IgnoredIterationIdentifierWriteIr>,
}

impl PreparedIdentifierWrite {
    fn performed(value: TypedExpr) -> Self {
        Self {
            value,
            ignored: None,
        }
    }
}

pub(crate) struct ScriptLowerer<'a> {
    dynamic_script_sources: Vec<DynamicScriptSource>,
    dynamic_function_sources: Vec<DynamicFunctionSource>,
    function_source_binding_candidates: BTreeMap<FiniteSourceBindingKey, Vec<FiniteSourceValue>>,
    finite_source_capture_keys: BTreeMap<(usize, String), FiniteSourceBindingKey>,
    finite_source_body_scope: Option<usize>,
    function_source_parameter_candidates: BTreeMap<(FunctionId, usize), Vec<FiniteSourceValue>>,
    interner: &'a Interner,
    analysis: &'a Analysis<'a>,
    source_text: CompilerSourceText<'a>,
    current_owner_id: String,
    lexical_strictness: Option<Strictness>,
    host_surface_policy: HostSurfacePolicy,
    scopes: Vec<BTreeMap<String, BindingInfo>>,
    direct_lexical_scopes: Vec<bool>,
    var_bindings: BTreeMap<String, VarBindingInfo>,
    function_signatures: FunctionSignatures,
    visible_function_names: BTreeMap<String, FunctionId>,
    diagnostics: Vec<IrDiagnostic>,
    breakable_depth: usize,
    loop_depth: usize,
    ordinary_generator_region_depth: usize,
    ordinary_generator_switch_depth: usize,
    ordinary_generator_for_in_depth: usize,
    plain_async_with_depth: usize,
    plain_async_for_in_depth: usize,
    plain_async_for_of_depth: usize,
    plain_async_classic_depth: usize,
    plain_async_resource_depth: usize,
    mixed_async_generator_region_depth: usize,
    async_generator_source_domain: crate::async_generator_source::AsyncGeneratorSourceDomain,
    async_value_branch_context: AsyncValueBranchContext,
    labels: Vec<ActiveLabel>,
    is_function_body: bool,
    current_function_id: Option<FunctionId>,
    current_param_names: Vec<String>,
    current_return: Option<FunctionReturnObservation>,
    current_generator_resume_state: Option<u32>,
    current_async_resume_state: Option<u32>,
    current_resumable_plan: Option<ResumablePlanIr>,
    next_resumable_suspension_index: usize,
    async_expression_prefix: Option<Vec<StatementIr>>,
    /// Operands already evaluated into `async_expression_prefix`, keyed by the
    /// address of the AST node they came from. See `lower_expression`.
    pinned_async_operands: HashMap<usize, TypedExpr>,
    root_this_binding: RootThisBinding,
    current_this_binding: CurrentThisBinding,
    current_new_target_info: ValueInfo,
    current_construct_this_info: Option<ValueInfo>,
    global_properties: GlobalProperties,
    /// Globals observed written anywhere in the script: an append-only side
    /// channel fed by every real global-write recording, merged from nested
    /// bodies like the installed-getter sets, and carried from the prepass to
    /// the final pass. The final function pass runs before the final
    /// root-statement pass replays top-level writes, so a final body must not
    /// fold an intrinsic global this set names. It never touches flow state,
    /// so accumulating it cannot perturb any fixpoint.
    observed_script_global_writes: BTreeSet<String>,
    /// Per-intrinsic overrides of a well-known-symbol method on a builtin
    /// prototype, keyed by `(constructor name, symbol)`.
    ///
    /// The second component was a description `String` joined to its producer
    /// ~6,300 lines away by string equality alone. It is the closed domain, so
    /// it is the enum: a lookup naming a symbol no producer writes is now a
    /// compile error rather than a permanent miss.
    well_known_symbol_prototype_properties: BTreeMap<(String, WellKnownSymbol), ValueInfo>,
    /// Facts written by this lowerer (or one of its nested lowerers).
    nested_script_global_value_infos: BTreeMap<String, ValueInfo>,
    /// Script-global facts learned before this lowerer was entered.
    known_nested_script_global_value_infos: BTreeMap<String, ValueInfo>,
    /// Live script-global values observed when a source function is called.
    called_script_global_value_infos: BTreeMap<String, ValueInfo>,
    script_global_call_observation_mode: ScriptGlobalCallObservationMode,
    array_prototype_mutated: bool,
    number_prototype_to_string_state: PrototypeToStringState,
    boolean_prototype_to_string_state: PrototypeToStringState,
    dynamically_installed_getters: BTreeSet<FunctionId>,
    dynamically_installed_setters: BTreeSet<FunctionId>,
    unknown_user_code_effects_observed: bool,
    unknown_user_code_effects_introduced: bool,
    /// Capture plans cannot change while this lowerer borrows their Analysis.
    /// Keep physical and source-name membership once, separate from live facts.
    captured_binding_names: BTreeSet<String>,
    function_signature_shape_evidence: FunctionSignatureShapeEvidence,
    source_call_flow_effects: SourceCallFlowEffects,
    intervening_effect_epoch: u64,
    static_boolean_bindings: BTreeMap<String, bool>,
    static_string_bindings: StaticStringBindingFacts,
    static_to_string_regexp_object_bindings: BTreeSet<String>,
    sloppy_immutable_binding_storage_names: BTreeSet<String>,
    invalidated_static_binding_names: BTreeSet<String>,
    boolean_alias_shapes_invalidated: bool,
    top_level_this_uses: usize,
    used_host_builtins: BTreeSet<HostBuiltinId>,
    host_builtin_calls: usize,
    builtin_ctor_calls: usize,
    builtin_static_calls: usize,
    error_builtin_calls: usize,
    aggregate_errors: usize,
    function_proto_calls: usize,
    function_proto_applies: usize,
    function_proto_binds: usize,
    function_proto_to_strings: usize,
    bound_functions: usize,
    bound_function_constructs: usize,
    boxed_builtin_calls: usize,
    boxed_builtin_constructs: usize,
    boxed_receiver_adaptations: usize,
    generated_functions: Vec<FunctionIr>,
    generated_owned_env_bindings: Vec<OwnedEnvBindingIr>,
    /// Capture slots of SuperProperty destructuring targets lowered by the
    /// pattern in flight; its top-level entry declares them.
    pending_super_destructuring_slots: Vec<String>,
    next_generated_function_index: usize,
    next_temp_binding_index: usize,
    is_prepass: bool,
    active_direct_call_propagations: BTreeSet<FunctionId>,
    completed_direct_call_propagations: BTreeSet<DirectCallContextKey>,
    exact_context_callback_targets: BTreeMap<FunctionId, ExactHelperContextId>,
    exact_context_callback_observations: BTreeMap<ExactCallbackContextKey, FunctionSignature>,
    exact_context_callback_specializations: BTreeMap<ExactCallbackContextKey, FunctionId>,
    exact_context_function_observations: BTreeMap<ExactCallbackContextKey, FunctionSignature>,
    exact_context_function_specializations: BTreeMap<ExactCallbackContextKey, FunctionId>,
    class_context: Option<ClassLoweringContext>,
    private_environment_id: Option<PrivateEnvironmentId>,
    with_environment_chain: OrderedWithEnvironmentChain,
    captured_binding_positions: BTreeMap<String, CapturedBindingPosition>,
    error_proto_to_strings: usize,
}

impl<'a> ScriptLowerer<'a> {
    fn seed_definition_environment_positions(&mut self, function: &FunctionPlan<'a>) {
        let (binding_positions, with_positions) =
            captured_environment_positions(self.analysis, function);
        self.captured_binding_positions = binding_positions;
        let with_environments = with_positions
            .into_iter()
            .map(|(binding_name, position)| {
                function
                    .captures
                    .get(binding_name.as_str())
                    .expect("surrounding WithObject environment must be captured");
                PositionedWithEnvironment::captured(
                    ObjectEnvironmentBindingObject::materialized(
                        &binding_name,
                        // The hidden slot contains the once-boxed With entry
                        // value, independent of the original head's type.
                        Self::unknown_construct_result_info(),
                    ),
                    position,
                )
            })
            .collect();
        self.with_environment_chain.seed_captured(with_environments);
    }

    fn function_value_info(&self, function_id: &FunctionId) -> ValueInfo {
        self.function_value_info_with_intrinsic_prototype(function_id)
    }

    fn with_instance_prototype(
        mut info: ValueInfo,
        prototype: Option<Box<HeapShape>>,
    ) -> ValueInfo {
        if let Some(shape) = info.heap_shape.as_mut() {
            match shape.as_mut() {
                HeapShape::Object(object) => object.prototype = prototype,
                HeapShape::Array(array) => array.prototype = prototype,
            }
        }
        info
    }

    fn current_private_name_id(&self, name: PrivateName) -> Option<PrivateNameId> {
        let key = private_name_key(self.interner, name);
        self.analysis
            .resolve_private_name(self.private_environment_id, &key)
            .map(|(_, private_name_id)| private_name_id)
    }

    fn alloc_generated_function_id(&mut self, prefix: &str) -> FunctionId {
        let id = format!(
            "${prefix}.{}.{}",
            self.current_owner_id, self.next_generated_function_index
        );
        self.next_generated_function_index += 1;
        id
    }

    fn alloc_temp_binding_name(&mut self, hint: &str) -> String {
        let name = format!("${hint}{}", self.next_temp_binding_index);
        self.next_temp_binding_index += 1;
        name
    }

    fn alloc_suspension_owned_binding(&mut self, hint: &str, value_info: ValueInfo) -> String {
        let name = self.alloc_temp_binding_name(hint);
        self.add_suspension_owned_binding(name.clone(), BindingMode::Let);
        self.declare_binding(
            name.clone(),
            BindingInfo {
                mode: BindingMode::Let,
                storage_name: name.clone(),
                kind: value_info.kind,
                possible_kinds: value_info.possible_kinds,
                heap_shape: value_info.heap_shape,
                function_targets: value_info.function_targets,
                initialization: Initialization::Initialized,
            },
        );
        name
    }

    fn alloc_captured_property_receiver(&mut self) -> CapturedPropertyReceiverSlot {
        CapturedPropertyReceiverSlot::new(self.alloc_suspension_owned_binding(
            "async.logical.reference.receiver.",
            unknown_runtime_value_info(),
        ))
    }
    fn alloc_captured_property_target(&mut self) -> CapturedPropertyTargetSlot {
        CapturedPropertyTargetSlot::new(self.alloc_suspension_owned_binding(
            "async.logical.reference.target.",
            unknown_runtime_value_info(),
        ))
    }
    fn alloc_captured_property_key(&mut self) -> CapturedPropertyKeySlot {
        CapturedPropertyKeySlot::new(
            self.alloc_suspension_owned_binding(
                "async.logical.reference.key.",
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::from_kind(ValueKind::String)
                        .union(KindSet::from_kind(ValueKind::Symbol)),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                },
            ),
        )
    }

    /// Allocates the `[[Iterator]]` slot of an activation-backed Iterator
    /// Record.
    ///
    /// The three slot allocators exist so that a slot can only be obtained from
    /// the allocation that names it. Wrapping three interchangeable `String`s
    /// at the construction site would have left the original defect intact —
    /// `IteratorSlot::new(next_binding), NextMethodSlot::new(iterator_binding)`
    /// type-checks and miscompiles every `for await`. With these there is no
    /// `String` left in the expression to transpose, and swapping two of the
    /// three results is `E0308`.
    ///
    /// The hint strings are fixed here rather than passed in, so the emitted
    /// binding names cannot drift either.
    fn alloc_iterator_slot(&mut self) -> IteratorSlot {
        IteratorSlot::new(self.alloc_suspension_owned_binding(
            "async.forof.iterator.",
            ValueInfo::new(ValueKind::Object),
        ))
    }

    /// Allocates the `[[NextMethod]]` slot. See [`Self::alloc_iterator_slot`].
    fn alloc_next_method_slot(&mut self) -> NextMethodSlot {
        NextMethodSlot::new(self.alloc_suspension_owned_binding(
            "async.forof.next.",
            ValueInfo::new(ValueKind::Dynamic),
        ))
    }

    /// Allocates the `[[Done]]` slot. See [`Self::alloc_iterator_slot`].
    fn alloc_done_slot(&mut self) -> DoneSlot {
        DoneSlot::new(self.alloc_suspension_owned_binding(
            "async.forof.done.",
            ValueInfo::new(ValueKind::Boolean),
        ))
    }

    fn add_suspension_owned_binding(&mut self, name: String, mode: BindingMode) {
        if self
            .generated_owned_env_bindings
            .iter()
            .any(|binding| binding.name == name)
            || self
                .analysis
                .owner_plans
                .get(&self.current_owner_id)
                .is_some_and(|owner| owner.owned_env_slots.contains_key(&name))
        {
            return;
        }
        let slot = self
            .analysis
            .owner_plans
            .get(&self.current_owner_id)
            .map_or(0, |owner| owner.owned_env_slots.len() as u32)
            + self.generated_owned_env_bindings.len() as u32;
        self.generated_owned_env_bindings.push(OwnedEnvBindingIr {
            mutability: crate::EnvironmentBindingMutabilityIr::lexical(mode),
            name,
            slot,
        });
    }

    fn lexical_storage_name(&mut self, source_name: &str) -> String {
        let shadows_scope_binding = self.scopes.iter().rev().any(|scope| {
            scope.get(source_name).is_some_and(|binding| {
                // A *placeholder* entry (14.7.5.5 head binding, 10.2.11 step 21
                // parameter) backs no slot in its scope, so it is not a claim on
                // the name; an entry the sweep created (`Allocated`) and an
                // initialized entry both are. The match is exhaustive on
                // purpose: a third storage disposition must be decided here
                // rather than default to "does not shadow", which is the silent
                // slot-sharing bug this replaces.
                match binding.initialization {
                    Initialization::Uninitialized(UninitializedStorage::Placeholder) => false,
                    Initialization::Uninitialized(UninitializedStorage::Allocated)
                    | Initialization::Initialized => true,
                }
            })
        });
        let shadows_var_binding = self
            .var_bindings
            .get(source_name)
            .is_some_and(|binding| !binding.is_lexical_metadata);
        if shadows_scope_binding || shadows_var_binding {
            self.alloc_temp_binding_name("lex")
        } else {
            source_name.to_string()
        }
    }

    /// The storage name an InitializeBinding (9.1.1.1.4) must write.
    ///
    /// If BlockDeclarationInstantiation already created this binding in the
    /// current scope, that creation allocated the slot and this returns it.
    /// Recomputing instead is the M2b trap: the script top level and every
    /// function body are not direct-lexical scopes, so `lexical_storage_name`
    /// answers `$lexN` once the created entry is in scope, and the declarator
    /// would write a slot that nothing — neither an earlier read nor the
    /// capture analysis' `EnvironmentPlan` — resolves to. The token
    /// `PendingInitialization` carries the same name for the paths that take
    /// one; this covers the destructuring paths of ledger entry L2, which do
    /// not.
    fn direct_lexical_storage_name(&mut self, source_name: &str, span: boa_ast::Span) -> String {
        let created = self
            .scopes
            .last()
            .and_then(|scope| scope.get(source_name))
            .and_then(|binding| {
                // Exhaustive, not `== Uninitialized(Allocated)`: this is the
                // *second* consumer of `UninitializedStorage`, and an equality
                // test would let a third storage disposition compile here while
                // `lexical_storage_name` (above) reported `E0004` — silently
                // dropping the reuse rule and reopening M2b for the new
                // variant.
                match binding.initialization {
                    Initialization::Uninitialized(UninitializedStorage::Allocated) => {
                        Some(binding.storage_name.clone())
                    }
                    Initialization::Uninitialized(UninitializedStorage::Placeholder)
                    | Initialization::Initialized => None,
                }
            });
        if let Some(created) = created {
            return created;
        }
        if self.direct_lexical_scopes.last().copied().unwrap_or(false) {
            return scoped_lexical_binding_storage_name(source_name, span);
        }
        if self
            .analysis
            .owner_plans
            .get(&self.current_owner_id)
            .is_some_and(|owner| {
                let environment_id = owner
                    .body_environment_id
                    .unwrap_or(owner.activation_environment_id);
                self.analysis.environment_plans[&environment_id]
                    .owned_env_slots
                    .contains_key(source_name)
            })
        {
            // Captures and eval resolve this cell by the analysis-owned name;
            // outer binding metadata cannot authorize a different local slot.
            return source_name.to_string();
        }
        self.lexical_storage_name(source_name)
    }

    /// CreateMutableBinding (9.1.1.1.2) / CreateImmutableBinding (9.1.1.1.3) for
    /// one lexically scoped name, as `LexicalScopeInstantiation` performs them.
    ///
    /// `pub(crate)` only so the sweep — whose exhaustive `boa_ast::Declaration`
    /// match lives in `binding_lifecycle` — can reach it; it is not a general
    /// declaration entry point.
    pub(crate) fn create_lexical_binding(
        &mut self,
        source_name: &str,
        mode: BindingMode,
        span: boa_ast::Span,
    ) -> String {
        let storage_name = self.direct_lexical_storage_name(source_name, span);
        self.declare_binding(
            source_name.to_string(),
            BindingInfo {
                mode,
                storage_name: storage_name.clone(),
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
                initialization: Initialization::Uninitialized(UninitializedStorage::Allocated),
            },
        );
        storage_name
    }

    /// The declarative Environment Record push of 14.3.1.2 step 1, performed by
    /// `LexicalScopeInstantiation`'s block-shaped constructors.
    ///
    /// `pub(crate)` only so the sweep can own the frame it populates; the
    /// matching pop is [`Self::pop_instantiation_scope`], reachable only through
    /// `LexicalScopeInstantiation::finish`, which consumes the token.
    pub(crate) fn push_instantiation_scope(&mut self) {
        self.push_direct_lexical_scope();
    }

    /// The matching pop. See [`Self::push_instantiation_scope`].
    pub(crate) fn pop_instantiation_scope(&mut self) {
        self.pop_scope();
    }

    /// InitializeBinding (9.1.1.1.4)'s scope-record write, for
    /// `InitializedBinding::declare`. `pub(crate)` for the same reason as
    /// [`Self::create_lexical_binding`]: the typed half of the transition lives
    /// in `binding_lifecycle`, and its fields stay private there so the storage
    /// name the creation allocated cannot be swapped for a recomputed one.
    pub(crate) fn declare_initialized_binding(&mut self, source_name: String, info: BindingInfo) {
        self.declare_binding(source_name, info);
    }

    pub(crate) fn interner(&self) -> &'a Interner {
        self.interner
    }

    fn new(
        interner: &'a Interner,
        analysis: &'a Analysis<'a>,
        source_text: CompilerSourceText<'a>,
        root_this_binding: RootThisBinding,
        current_owner_id: String,
        host_surface_policy: HostSurfacePolicy,
    ) -> Self {
        let private_environment_id = analysis
            .owner_plans
            .get(&current_owner_id)
            .and_then(|owner| owner.private_environment_id);
        let captured_binding_names = analysis
            .function_plans
            .values()
            .flat_map(|plan| {
                plan.captures.iter().flat_map(|(storage_name, capture)| {
                    [storage_name.clone(), capture.source_name.clone()]
                })
            })
            .collect();
        let mut lowerer = Self {
            interner,
            analysis,
            captured_binding_names,
            source_text,
            current_owner_id,
            lexical_strictness: None,
            host_surface_policy,
            scopes: vec![BTreeMap::new()],
            direct_lexical_scopes: vec![false],
            var_bindings: BTreeMap::new(),
            function_signatures: FunctionSignatures::default(),
            visible_function_names: BTreeMap::new(),
            diagnostics: Vec::new(),
            breakable_depth: 0,
            loop_depth: 0,
            ordinary_generator_region_depth: 0,
            ordinary_generator_switch_depth: 0,
            ordinary_generator_for_in_depth: 0,
            plain_async_with_depth: 0,
            plain_async_for_in_depth: 0,
            plain_async_for_of_depth: 0,
            plain_async_classic_depth: 0,
            plain_async_resource_depth: 0,
            mixed_async_generator_region_depth: 0,
            async_generator_source_domain:
                crate::async_generator_source::AsyncGeneratorSourceDomain::FunctionBody,
            async_value_branch_context: AsyncValueBranchContext::Ordinary,
            labels: Vec::new(),
            is_function_body: false,
            current_function_id: None,
            current_param_names: Vec::new(),
            current_return: None,
            current_generator_resume_state: None,
            current_async_resume_state: None,
            current_resumable_plan: None,
            next_resumable_suspension_index: 0,
            async_expression_prefix: None,
            pinned_async_operands: HashMap::new(),
            root_this_binding,
            current_this_binding: CurrentThisBinding::Root(root_this_binding),
            current_new_target_info: if root_this_binding == RootThisBinding::EvalCaller {
                unknown_runtime_value_info()
            } else {
                ValueInfo::undefined()
            },
            current_construct_this_info: None,
            global_properties: builtin_catalog::global_properties(host_surface_policy),
            observed_script_global_writes: BTreeSet::new(),
            well_known_symbol_prototype_properties: BTreeMap::new(),
            nested_script_global_value_infos: BTreeMap::new(),
            known_nested_script_global_value_infos: BTreeMap::new(),
            called_script_global_value_infos: BTreeMap::new(),
            script_global_call_observation_mode: ScriptGlobalCallObservationMode::DormantSummary,
            // Array.prototype is mutable and can escape through dynamic code
            // such as Object.getPrototypeOf([]). Until specialization carries
            // a runtime/version guard, default to the generic observable path.
            array_prototype_mutated: true,
            number_prototype_to_string_state: PrototypeToStringState::Intrinsic,
            boolean_prototype_to_string_state: PrototypeToStringState::Intrinsic,
            dynamically_installed_getters: BTreeSet::new(),
            dynamically_installed_setters: BTreeSet::new(),
            unknown_user_code_effects_observed: false,
            unknown_user_code_effects_introduced: false,
            function_signature_shape_evidence: FunctionSignatureShapeEvidence::Available,
            source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            intervening_effect_epoch: 0,
            static_boolean_bindings: BTreeMap::new(),
            static_string_bindings: StaticStringBindingFacts::default(),
            static_to_string_regexp_object_bindings: BTreeSet::new(),
            sloppy_immutable_binding_storage_names: BTreeSet::new(),
            invalidated_static_binding_names: BTreeSet::new(),
            boolean_alias_shapes_invalidated: false,
            top_level_this_uses: 0,
            used_host_builtins: BTreeSet::new(),
            host_builtin_calls: 0,
            builtin_ctor_calls: 0,
            builtin_static_calls: 0,
            error_builtin_calls: 0,
            aggregate_errors: 0,
            function_proto_calls: 0,
            function_proto_applies: 0,
            function_proto_binds: 0,
            function_proto_to_strings: 0,
            bound_functions: 0,
            bound_function_constructs: 0,
            boxed_builtin_calls: 0,
            boxed_builtin_constructs: 0,
            boxed_receiver_adaptations: 0,
            generated_functions: Vec::new(),
            dynamic_function_sources: Vec::new(),
            function_source_binding_candidates: BTreeMap::new(),
            finite_source_capture_keys: BTreeMap::new(),
            finite_source_body_scope: None,
            function_source_parameter_candidates: BTreeMap::new(),
            dynamic_script_sources: Vec::new(),
            generated_owned_env_bindings: Vec::new(),
            pending_super_destructuring_slots: Vec::new(),
            next_generated_function_index: 0,
            next_temp_binding_index: 0,
            is_prepass: false,
            active_direct_call_propagations: BTreeSet::new(),
            completed_direct_call_propagations: BTreeSet::new(),
            exact_context_callback_targets: BTreeMap::new(),
            exact_context_callback_observations: BTreeMap::new(),
            exact_context_callback_specializations: BTreeMap::new(),
            exact_context_function_observations: BTreeMap::new(),
            exact_context_function_specializations: BTreeMap::new(),
            class_context: None,
            private_environment_id,
            with_environment_chain: OrderedWithEnvironmentChain::default(),
            captured_binding_positions: BTreeMap::new(),
            error_proto_to_strings: 0,
        };
        if matches!(
            analysis.script_instantiation,
            ScriptInstantiation::ModuleAfterGlobalScript
                | ScriptInstantiation::GlobalScriptAfterPrelude
        ) {
            // The earlier Script can replace globals and intrinsic properties.
            // Every analysis prepass must start from the same runtime boundary.
            lowerer.invalidate_unknown_user_code_effect_facts();
        }
        lowerer
    }

    fn lower(mut self, script: &Script) -> LoweredScript {
        self.function_signatures = builtin_catalog::function_signatures();
        for function_id in &self.analysis.function_order {
            let plan = self
                .analysis
                .function_plans
                .get(function_id)
                .expect("function plan must exist");
            self.function_signatures.insert(
                function_id.clone(),
                FunctionSignature {
                    id: function_id.clone(),
                    to_string_representation: plan.to_string_representation.clone(),
                    protocol: plan.protocol,
                    callable: true,
                    class_heritage_kind: ClassHeritageKind::None,
                    params: plan
                        .parameters
                        .as_ref()
                        .iter()
                        .map(|parameter| FunctionParamSignature {
                            kind: if parameter.is_rest_param() {
                                ValueKind::Array
                            } else {
                                ValueKind::Dynamic
                            },
                            possible_kinds: if parameter.is_rest_param() {
                                KindSet::from_kind(ValueKind::Array)
                            } else {
                                KindSet::all_runtime_tags()
                            },
                            heap_shape: None,
                            function_targets: if parameter.is_rest_param() {
                                FunctionTargetKnowledge::none()
                            } else {
                                FunctionTargetKnowledge::unknown()
                            },
                            observed: parameter.is_rest_param(),
                            has_default: parameter.init().is_some(),
                            is_rest: parameter.is_rest_param(),
                        })
                        .collect(),
                    return_kind: ValueKind::Dynamic,
                    return_possible_kinds: KindSet::all_runtime_tags(),
                    return_shape: FunctionReturnShape::Unobserved,
                    return_targets: FunctionTargetKnowledge::unknown(),
                    constructor_instance: if plan.protocol.is_constructable() {
                        Self::fresh_constructed_instance_info()
                    } else {
                        ValueInfo::undefined()
                    },
                    this_info: if plan.protocol.flavor() == FunctionFlavor::Arrow {
                        ValueInfo::undefined()
                    } else if plan.protocol.is_constructable() {
                        Self::function_construct_this_info(plan.id.clone())
                    } else {
                        self.global_this_info()
                    },
                    this_observed: false,
                    source_call_flow_effects: SourceCallFlowEffects::unobserved(),
                },
            );
        }
        let lower_trace = std::env::var_os("SCRIPTW_LOWER_TRACE").is_some();
        let tp = std::time::Instant::now();
        let mut prepass = ScriptLowerer::new(
            self.interner,
            self.analysis,
            self.source_text,
            self.root_this_binding,
            SCRIPT_OWNER_ID.to_string(),
            self.host_surface_policy,
        );
        prepass.function_signatures = self.function_signatures.clone();
        prepass.function_source_parameter_candidates =
            self.function_source_parameter_candidates.clone();
        prepass.is_prepass = true;
        prepass.script_global_call_observation_mode = ScriptGlobalCallObservationMode::ExecutedFlow;
        prepass.hoist_root_statement_items(script.statements().statements());
        // 16.1.7 step 17 on the global lexical Environment Record, which
        // `ScriptLowerer::new` already established — hence
        // `instantiate_in_current_scope`, not `instantiate`.
        let prepass_scope = LexicalScopeInstantiation::instantiate_in_current_scope(
            &mut prepass,
            script.statements().statements(),
        );
        match function_inference::InferenceStatementList::root(
            script.statements().statements(),
            &prepass,
        ) {
            Some(items) => {
                prepass.infer_root_statement_items(
                    items,
                    self.analysis.script_root_functions.as_slice(),
                    prepass_scope,
                );
            }
            None => {
                let _ = prepass.lower_root_statement_items(
                    script.statements().statements(),
                    self.analysis.script_root_functions.as_slice(),
                    prepass_scope,
                );
            }
        }
        prepass.script_global_call_observation_mode =
            ScriptGlobalCallObservationMode::DormantSummary;
        if let Some(root_scope) = prepass.scopes.first() {
            for (name, binding) in root_scope {
                let Some(metadata) = prepass
                    .var_bindings
                    .get_mut(name)
                    .filter(|metadata| metadata.is_lexical_metadata)
                else {
                    continue;
                };
                metadata.kind = binding.kind;
                metadata.possible_kinds = binding.possible_kinds;
                metadata.heap_shape = binding.heap_shape.clone();
                metadata.function_targets = binding.function_targets.clone();
            }
        }
        // Root effects have already invalidated every fact they can reach.
        // Replaying the sticky "an unknown call occurred" marker inside each
        // independently invoked function would instead invent an effect before
        // that function's first statement and erase exact call-site shapes.
        prepass.unknown_user_code_effects_observed = false;
        // Function bodies are skipped by the body prepass. Lower every planned
        // body once here so nested script-global writes are known before the
        // final function pass (whose order follows source order).
        for function_id in &self.analysis.function_order {
            let plan = self
                .analysis
                .function_plans
                .get(function_id)
                .expect("function plan must exist");
            prepass.infer_function(plan, None);
        }
        // Carry analysis summaries out of the prepass, but keep the root-entry
        // global facts. The final pass replays live effects in source order.
        let known_script_global_values = prepass.known_script_global_values();
        // The prepass accumulated every real global write (its own and every
        // nested body's) in a side channel that never touches flow state, so
        // carrying it cannot perturb any fixpoint.
        self.observed_script_global_writes = prepass.observed_script_global_writes;
        self.function_signatures = prepass.function_signatures;
        self.exact_context_function_observations = prepass.exact_context_function_observations;
        self.exact_context_callback_observations = prepass.exact_context_callback_observations;
        self.function_source_binding_candidates = prepass.function_source_binding_candidates;
        self.function_source_parameter_candidates = prepass.function_source_parameter_candidates;
        self.var_bindings = prepass.var_bindings;
        self.reset_script_global_var_flow_facts();
        self.known_nested_script_global_value_infos = known_script_global_values;
        self.dynamically_installed_getters = prepass.dynamically_installed_getters;
        self.dynamically_installed_setters = prepass.dynamically_installed_setters;
        self.prepare_root_function_bindings(self.analysis.script_root_functions.as_slice());
        self.hoist_root_statement_items(script.statements().statements());
        if lower_trace {
            eprintln!("scriptw lower trace: prepass: {:?}", tp.elapsed());
        }
        let tp = std::time::Instant::now();
        self.propagate_function_signatures();
        if lower_trace {
            eprintln!(
                "scriptw lower trace: propagate-signatures: {:?}",
                tp.elapsed()
            );
        }
        let tp = std::time::Instant::now();
        self.prepare_exact_context_specializations();
        if lower_trace {
            eprintln!("scriptw lower trace: exact-context: {:?}", tp.elapsed());
        }
        let tp = std::time::Instant::now();
        let mut functions = Vec::with_capacity(self.analysis.function_order.len());
        for function_id in &self.analysis.function_order {
            let plan = self
                .analysis
                .function_plans
                .get(function_id)
                .expect("function plan must exist");
            functions.push(self.lower_function(plan, None, None));
            let specializations = self
                .exact_context_callback_specializations
                .iter()
                .filter_map(|((callback_id, helper_context_id), synthetic_id)| {
                    (callback_id == function_id)
                        .then(|| (helper_context_id.clone(), synthetic_id.clone()))
                })
                .collect::<Vec<_>>();
            for (helper_context_id, synthetic_id) in specializations {
                functions.push(self.lower_function(
                    plan,
                    Some(synthetic_id),
                    Some((function_id.clone(), helper_context_id)),
                ));
            }
            let helper_specializations = self
                .exact_context_function_specializations
                .iter()
                .filter_map(|((helper_id, helper_context_id), synthetic_id)| {
                    (helper_id == function_id)
                        .then(|| (helper_context_id.clone(), synthetic_id.clone()))
                })
                .collect::<Vec<_>>();
            for (helper_context_id, synthetic_id) in helper_specializations {
                functions.push(self.lower_function(
                    plan,
                    Some(synthetic_id),
                    Some((function_id.clone(), helper_context_id)),
                ));
            }
        }
        if lower_trace {
            eprintln!("scriptw lower trace: final-functions: {:?}", tp.elapsed());
        }
        let tp = std::time::Instant::now();
        // 16.1.7 step 17: the script's `lexDeclarations` are created on the
        // global lexical Environment Record — the frame this lowerer was built
        // with — before the body evaluates.
        let scope = LexicalScopeInstantiation::instantiate_in_current_scope(
            &mut self,
            script.statements().statements(),
        );
        let body = self.lower_root_statement_items(
            script.statements().statements(),
            self.analysis.script_root_functions.as_slice(),
            scope,
        );
        if lower_trace {
            eprintln!("scriptw lower trace: final-body: {:?}", tp.elapsed());
        }
        functions.append(&mut self.generated_functions);
        let (global_bindings, restricted_global_functions) =
            self.script_global_bindings(self.analysis.script_root_functions.as_slice());
        let rejects_fresh_entry_declarations = matches!(
            self.analysis.script_instantiation,
            ScriptInstantiation::FreshEntry | ScriptInstantiation::ModuleAfterGlobalScript
        );
        for name in restricted_global_functions
            .into_iter()
            .filter(|_| rejects_fresh_entry_declarations)
        {
            self.unsupported_with_message(format!(
                "unsupported in scriptw wasm-aot first slice: global function declaration `{name}` is blocked by a non-configurable global property"
            ));
        }
        let mut owned_env_bindings = self
            .analysis
            .owner_plans
            .get(SCRIPT_OWNER_ID)
            .map(|owner| {
                owner
                    .owned_env_slots
                    .iter()
                    .map(|(name, slot)| {
                        self.analysis.owner_environment_binding(
                            SCRIPT_OWNER_ID,
                            name,
                            *slot,
                            self.interner,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Independently compiled Function bodies resolve these bindings through
        // the realm's Global Environment, even when the entry Script contains
        // no closure that captures them.
        for name in global_bindings.lexical_names() {
            if owned_env_bindings
                .iter()
                .any(|binding| &binding.name == name)
            {
                continue;
            }
            owned_env_bindings.push(OwnedEnvBindingIr {
                mutability: match global_bindings.lexical_bindings()[name] {
                    crate::GlobalLexicalBindingModeIr::Mutable => {
                        crate::EnvironmentBindingMutabilityIr::Mutable
                    }
                    crate::GlobalLexicalBindingModeIr::Immutable => {
                        crate::EnvironmentBindingMutabilityIr::Immutable { strict: true }
                    }
                },
                name: name.clone(),
                slot: owned_env_bindings.len() as u32,
            });
        }
        let runtime_declarations = self.runtime_global_declarations(script);
        LoweredScript {
            function_global_hints: self.collect_function_global_hints(),
            runtime_declarations,
            dynamic_script_sources: self.dynamic_script_sources,
            dynamic_function_sources: self.dynamic_function_sources,
            functions,
            body,
            owned_env_bindings,
            global_bindings,
            host_builtins: self.used_host_builtins.iter().copied().collect(),
            builtin_ctor_calls: self.builtin_ctor_calls,
            builtin_static_calls: self.builtin_static_calls,
            error_builtin_calls: self.error_builtin_calls,
            aggregate_errors: self.aggregate_errors,
            function_proto_calls: self.function_proto_calls,
            function_proto_applies: self.function_proto_applies,
            function_proto_binds: self.function_proto_binds,
            function_proto_to_strings: self.function_proto_to_strings,
            bound_functions: self.bound_functions,
            bound_function_constructs: self.bound_function_constructs,
            boxed_builtin_calls: self.boxed_builtin_calls,
            boxed_builtin_constructs: self.boxed_builtin_constructs,
            boxed_receiver_adaptations: self.boxed_receiver_adaptations,
            top_level_this_uses: self.top_level_this_uses,
            host_builtin_calls: self.host_builtin_calls,
            error_proto_to_strings: self.error_proto_to_strings,
            diagnostics: self.diagnostics,
        }
    }

    fn script_global_bindings(
        &self,
        root_functions: &[PendingFunction<'a>],
    ) -> (GlobalBindingPlan, Vec<String>) {
        let activation_id = self.analysis.owner_plans[SCRIPT_OWNER_ID].activation_environment_id;
        let binding_modes = &self.analysis.environment_plans[&activation_id].binding_modes;
        let lexical_bindings = self
            .var_bindings
            .iter()
            .filter(|(_, binding)| binding.is_lexical_metadata)
            .map(|(name, _)| {
                let mode = match binding_modes.get(name).unwrap_or_else(|| {
                    panic!("global lexical `{name}` must have an analyzed declaration mode")
                }) {
                    BindingMode::Let => GlobalLexicalBindingModeIr::Mutable,
                    BindingMode::Const => GlobalLexicalBindingModeIr::Immutable,
                    BindingMode::Var => panic!("global lexical `{name}` cannot be var"),
                };
                (name.clone(), mode)
            });
        let mut bindings = GlobalBindingPlan::from_parts(
            builtin_catalog::canonical_standard_global_bindings(),
            lexical_bindings,
        );
        let mut global_host_builtins = self.used_host_builtins.clone();
        global_host_builtins.extend(
            HostBuiltinId::every_realm_globals()
                .filter(|builtin| self.host_surface_policy.allows(*builtin)),
        );
        for builtin in global_host_builtins {
            if let Some(name) = builtin.global_name() {
                bindings.insert_initial(ScriptGlobalBindingIr {
                    name: name.to_string(),
                    initializer: GlobalPropertyInitializerIr::HostFunction(builtin),
                    declarations: GlobalDeclarationSetIr::None,
                });
            }
        }
        for (name, _) in self
            .var_bindings
            .iter()
            .filter(|(_, binding)| binding.is_script_global)
        {
            bindings.record_var(name.clone());
        }
        for name in self
            .analysis
            .annex_b_function_plans
            .values()
            .filter(|plan| plan.owner_id == SCRIPT_OWNER_ID && plan.copy_to_variable_environment)
            .map(|plan| plan.source_name.clone())
            .collect::<BTreeSet<_>>()
        {
            bindings.record_annex_b_var(name);
        }
        // 16.1.7 considers var-scoped function declarations in reverse list
        // order. Recording in source order makes replacement select exactly
        // the final declaration for a duplicated name.
        let mut restricted_functions = Vec::new();
        for function in root_functions {
            if self
                .analysis
                .module_execution
                .is_private_dispatcher_function(&function.id)
            {
                continue;
            }
            if bindings.record_function(function.name.clone(), function.id.clone())
                == GlobalFunctionDeclarationDispositionIr::RestrictedExistingProperty
            {
                restricted_functions.push(function.name.clone());
            }
        }
        (bindings, restricted_functions)
    }

    fn lower_root_statement_items(
        &mut self,
        items: &[StatementListItem],
        root_functions: &[PendingFunction<'a>],
        scope: LexicalScopeInstantiation,
    ) -> BlockIr {
        let root_function_bindings = root_functions
            .iter()
            .map(|function| (function.name.clone(), function.id.clone()))
            .collect::<Vec<_>>();
        self.lower_root_statement_items_with_function_bindings(
            items,
            &root_function_bindings,
            scope,
        )
    }

    /// The statement-list entry for the script top level (16.1.7
    /// GlobalDeclarationInstantiation step 17) **and every function body**
    /// (10.2.11 FunctionDeclarationInstantiation step 30).
    ///
    /// Both moments create the whole `lexDeclarations` list uninitialized before
    /// the body evaluates, and neither used to do so here — which is why
    /// `x; let x;` read slot `x` at the top level while the same program inside
    /// a block already threw. `scope` is the witness that the sweep ran.
    fn lower_root_statement_items_with_function_bindings(
        &mut self,
        items: &[StatementListItem],
        root_functions: &[(String, FunctionId)],
        scope: LexicalScopeInstantiation,
    ) -> BlockIr {
        let source_domain = self.async_generator_source_domain;
        let mut scope = scope;
        let mut lowered_items = Vec::new();

        self.prepare_annex_b_function_bindings();
        self.prepare_root_function_binding_ids(root_functions);
        lowered_items.extend(
            self.root_function_init_statements(root_functions)
                .into_iter()
                .map(|statement| {
                    LoweredStatementListItemIr::statement(statement, ValueKind::Undefined)
                }),
        );

        // A canonical module's instantiation boundary belongs to the module
        // lifecycle, before any evaluation resource lifetime begins.
        let evaluation_start = items
            .iter()
            .position(|item| {
                matches!(item, StatementListItem::Statement(statement)
                if self.analysis.module_execution.boundaries.contains(
                    &(std::ptr::from_ref(statement.as_ref()) as usize)
                ))
            })
            .map_or(0, |index| index + 1);
        let (instantiation, items) = items.split_at(evaluation_start);
        for item in instantiation {
            if let Some(item) = self.lower_statement_list_item_after_hoisting(
                item,
                &mut scope,
                StatementListPlacement::Root,
            ) {
                lowered_items.push(item);
            }
        }

        if let Some(source) = self.complete_resource_scope_source(items) {
            let mut prefix = lowered_items
                .into_iter()
                .map(|item| match item {
                    LoweredStatementListItemIr::Statement { statement, .. } => statement,
                    _ => unreachable!("function preambles contain no resource declarations"),
                })
                .collect::<Vec<_>>();
            let mut statements = if evaluation_start == 0 {
                Vec::new()
            } else {
                std::mem::take(&mut prefix)
            };
            let result = self.lower_async_generator_resource_items(
                source,
                &mut scope,
                prefix,
                StatementListPlacement::Root,
            );
            self.async_generator_source_domain = source_domain;
            scope.finish(self);
            return match result {
                Some((statement, result_kind)) => {
                    statements.push(statement);
                    BlockIr {
                        statements,
                        result_kind,
                        lexical_environment: None,
                    }
                }
                None => {
                    self.unsupported("resource scope differs from its checked registrations or continuation tape");
                    statements.push(StatementIr::Empty);
                    BlockIr {
                        statements,
                        result_kind: ValueKind::Undefined,
                        lexical_environment: None,
                    }
                }
            };
        }
        for item in items {
            if let Some(item) = self.lower_statement_list_item_after_hoisting(
                item,
                &mut scope,
                StatementListPlacement::Root,
            ) {
                lowered_items.push(item);
            }
        }

        // Ends the Environment Record the sweep created its bindings in. A root
        // statement list joins the frame its caller established, so this pops
        // nothing here; it is written because the token — not the caller — is
        // what decides that.
        self.async_generator_source_domain = source_domain;
        scope.finish(self);

        self.finish_disposable_scopes(lowered_items)
    }

    /// Lowers one statement list against the BlockDeclarationInstantiation that
    /// created its lexical bindings.
    ///
    /// `scope` is taken by value and its only constructors perform the sweep, so
    /// a statement-list entry cannot be written that forgets to predeclare.
    fn lower_statement_items(
        &mut self,
        items: &[StatementListItem],
        scope: LexicalScopeInstantiation,
    ) -> BlockIr {
        let source_domain = self.async_generator_source_domain;
        let mut scope = scope;
        let mut lowered_items = Vec::new();

        lowered_items.extend(
            self.lower_block_function_declarations(items)
                .into_iter()
                .map(|statement| {
                    LoweredStatementListItemIr::statement(statement, ValueKind::Undefined)
                }),
        );

        if let Some(source) = self.complete_resource_scope_source(items) {
            let prefix = lowered_items
                .into_iter()
                .map(|item| match item {
                    LoweredStatementListItemIr::Statement { statement, .. } => statement,
                    _ => unreachable!("function preambles contain no resource declarations"),
                })
                .collect();
            let result = self.lower_async_generator_resource_items(
                source,
                &mut scope,
                prefix,
                StatementListPlacement::Block,
            );
            self.async_generator_source_domain = source_domain;
            scope.finish(self);
            return match result {
                Some((statement, result_kind)) => BlockIr {
                    statements: vec![statement],
                    result_kind,
                    lexical_environment: None,
                },
                None => {
                    self.unsupported("resource scope differs from its checked registrations or continuation tape");
                    BlockIr {
                        statements: vec![StatementIr::Empty],
                        result_kind: ValueKind::Undefined,
                        lexical_environment: None,
                    }
                }
            };
        }
        for item in items {
            if let Some(item) = self.lower_statement_list_item_after_hoisting(
                item,
                &mut scope,
                StatementListPlacement::Block,
            ) {
                lowered_items.push(item);
            }
        }

        // 14.3.1.2's `env` ends with the statement list. The token pushed it and
        // the token pops it, so the sweep's scope and the lowering's scope are
        // the same frame by construction rather than by the caller remembering
        // to push before constructing.
        self.async_generator_source_domain = source_domain;
        scope.finish(self);

        self.finish_disposable_scopes(lowered_items)
    }

    fn complete_resource_scope_source<'source>(
        &self,
        items: &'source [StatementListItem],
    ) -> Option<crate::async_generator_source::AsyncGeneratorResourceScopeSource<'source>> {
        let execution = if self.async_generator_entry_state().is_some() {
            ResumableRegionProtocolIr::AsyncGenerator
        } else if self.plain_generator_entry_state().is_some() {
            ResumableRegionProtocolIr::Generator
        } else if self.plain_async_entry_state().is_some() {
            ResumableRegionProtocolIr::Async
        } else {
            return None;
        };
        crate::async_generator_source::AsyncGeneratorResourceScopeSource::for_protocol(
            items, execution,
        )
    }

    fn statement_list_ends_in_return(statements: &[StatementIr]) -> bool {
        match statements.last() {
            Some(StatementIr::Return(_)) => true,
            Some(StatementIr::AsyncGeneratorResourceScope(plan)) => {
                Self::statement_list_ends_in_return(&plan.body().block().statements)
            }
            Some(StatementIr::Block(body)) => Self::statement_list_ends_in_return(&body.statements),
            Some(StatementIr::SyncDisposableScope { body, .. }) => {
                Self::statement_list_ends_in_return(&body.statements)
            }
            Some(StatementIr::AsyncDisposableScope { body, .. }) => {
                Self::statement_list_ends_in_return(&body.statements)
            }
            _ => false,
        }
    }

    fn lower_block_function_declarations<'source>(
        &mut self,
        items: impl IntoIterator<Item = &'source StatementListItem>,
    ) -> Vec<StatementIr> {
        let mut last_function_by_name = BTreeMap::new();
        for item in items {
            if let Some(function) = BlockHoistableDeclaration::from_item(item) {
                last_function_by_name.insert(function.source_name(self.interner), function);
            }
        }
        last_function_by_name
            .into_values()
            .map(|function| match function {
                BlockHoistableDeclaration::Ordinary(function) => {
                    self.lower_function_declaration(function)
                }
                BlockHoistableDeclaration::Generator(function) => {
                    match linear_generator_plan_with_reason(function.body()) {
                        Ok(_) => self.lower_generator_declaration(function),
                        Err(reason) => {
                            self.unsupported(reason.message());
                            StatementIr::Empty
                        }
                    }
                }
                BlockHoistableDeclaration::Async(function) => {
                    self.lower_async_function_declaration(function)
                }
                BlockHoistableDeclaration::AsyncGenerator(function) => {
                    self.lower_async_generator_declaration(function)
                }
            })
            .collect()
    }

    /// A switch case body. It shares the CaseBlock's single instantiation with
    /// every other case (14.12.4), so the token map is borrowed rather than
    /// owned.
    fn lower_statement_items_without_function_initialization(
        &mut self,
        items: &[StatementListItem],
        scope: &mut LexicalScopeInstantiation,
    ) -> BlockIr {
        let mut statements = Vec::new();
        let mut result_kind = ValueKind::Undefined;
        for item in items {
            if let StatementListItem::Declaration(declaration) = item {
                if let Declaration::FunctionDeclaration(function) = declaration.as_ref() {
                    if let Some(copy) = self.lower_annex_b_function_copy(function) {
                        statements.push(copy);
                    }
                    continue;
                }
                if BlockHoistableDeclaration::from_item(item).is_some() {
                    continue;
                }
            }
            if let StatementListItem::Statement(statement) = item {
                if let Statement::Labelled(labelled) = statement.as_ref() {
                    if let Some(function) = labelled_function_declaration(labelled) {
                        if let Some(copy) = self.lower_annex_b_function_copy(function) {
                            statements.push(copy);
                        }
                        continue;
                    }
                }
            }
            let Some(item) = self
                .lower_statement_list_item(item, scope)
                .into_statement_list_entry()
            else {
                continue;
            };
            match item {
                LoweredStatementListItemIr::Statement {
                    statement,
                    result_kind: kind,
                } => {
                    statements.push(statement);
                    result_kind = kind;
                }
                LoweredStatementListItemIr::SyncDisposableScope { .. } => {
                    self.unsupported("using declaration in a switch CaseBlock");
                    statements.push(StatementIr::Empty);
                    result_kind = ValueKind::Undefined;
                }
                LoweredStatementListItemIr::AsyncDisposableScope(_) => {
                    self.unsupported("await using declaration in a switch CaseBlock");
                    statements.push(StatementIr::Empty);
                    result_kind = ValueKind::Undefined;
                }
            }
        }
        BlockIr {
            statements,
            result_kind,
            lexical_environment: None,
        }
    }

    fn prepare_annex_b_function_bindings(&mut self) {
        let names = self
            .analysis
            .annex_b_function_plans
            .values()
            .filter(|plan| {
                plan.owner_id == self.current_owner_id && plan.copy_to_variable_environment
            })
            .map(|plan| plan.source_name.clone())
            .collect::<BTreeSet<_>>();
        for name in names {
            if self.current_owner_id != SCRIPT_OWNER_ID
                && name == "arguments"
                && !self.analysis.owner_plans[&self.current_owner_id]
                    .root_bindings
                    .contains(&name)
            {
                continue;
            }
            self.hoist_var_name(name);
        }
    }

    fn prepare_root_function_bindings(&mut self, root_functions: &[PendingFunction<'a>]) {
        let root_function_bindings = root_functions
            .iter()
            .map(|function| (function.name.clone(), function.id.clone()))
            .collect::<Vec<_>>();
        self.prepare_root_function_binding_ids(&root_function_bindings);
    }

    fn prepare_root_function_binding_ids(&mut self, root_functions: &[(String, FunctionId)]) {
        self.visible_function_names.clear();
        for (name, function_id) in root_functions {
            let is_private_dispatcher = self
                .analysis
                .module_execution
                .is_private_dispatcher_function(function_id);
            let bare_function_info = self.function_value_info(function_id);
            let function_info = if self.current_owner_id == SCRIPT_OWNER_ID
                && self.script_variables_are_global()
                && !is_private_dispatcher
            {
                self.lookup_global_property(name)
                    .filter(|info| {
                        info.kind == ValueKind::Function
                            && info.function_targets.exact_single_target() == Some(function_id)
                    })
                    .unwrap_or(bare_function_info)
            } else {
                bare_function_info
            };
            self.visible_function_names
                .insert(name.clone(), function_id.clone());
            if self.current_owner_id == SCRIPT_OWNER_ID
                && self.script_variables_are_global()
                && !is_private_dispatcher
            {
                self.set_global_property_value_info_with_source(
                    name.clone(),
                    function_info.clone(),
                    GlobalPropertySource::GlobalWrite,
                );
            }
            if self.root_functions_need_body_initialization() || is_private_dispatcher {
                self.declare_binding(
                    name.clone(),
                    BindingInfo {
                        mode: BindingMode::Let,
                        storage_name: name.clone(),
                        kind: ValueKind::Function,
                        possible_kinds: KindSet::from_kind(ValueKind::Function),
                        heap_shape: function_info.heap_shape,
                        function_targets: FunctionTargetKnowledge::exact(function_id.clone()),
                        initialization: Initialization::Initialized,
                    },
                );
            }
        }
    }

    fn root_function_init_statements(
        &mut self,
        root_functions: &[(String, FunctionId)],
    ) -> Vec<StatementIr> {
        root_functions
            .iter()
            .filter(|(_, function_id)| {
                self.root_functions_need_body_initialization()
                    || self
                        .analysis
                        .module_execution
                        .is_private_dispatcher_function(function_id)
            })
            .map(|(name, function_id)| StatementIr::Lexical {
                mode: BindingMode::Let,
                name: name.clone(),
                init: TypedExpr::from_info(
                    self.function_value_info(function_id),
                    ExprIr::FunctionValue(function_id.clone()),
                ),
            })
            .collect()
    }

    fn is_registered_generator_declaration(&self, declaration: &Declaration) -> bool {
        let Declaration::GeneratorDeclaration(generator) = declaration else {
            return false;
        };
        self.analysis
            .function_declaration_ids
            .contains_key(&generator_declaration_key(generator))
    }

    fn is_registered_async_function_declaration(&self, declaration: &Declaration) -> bool {
        let Declaration::AsyncFunctionDeclaration(function) = declaration else {
            return false;
        };
        self.analysis
            .function_declaration_ids
            .contains_key(&async_function_declaration_key(function))
    }

    fn is_registered_async_generator_declaration(&self, declaration: &Declaration) -> bool {
        let Declaration::AsyncGeneratorDeclaration(function) = declaration else {
            return false;
        };
        self.analysis
            .function_declaration_ids
            .contains_key(&async_generator_declaration_key(function))
    }

    fn lower_statement_list_item_after_hoisting(
        &mut self,
        item: &StatementListItem,
        scope: &mut LexicalScopeInstantiation,
        placement: StatementListPlacement,
    ) -> Option<LoweredStatementListItemIr> {
        if let StatementListItem::Statement(statement) = item {
            if let Statement::Labelled(labelled) = statement.as_ref() {
                if let Some(function) = labelled_function_declaration(labelled) {
                    // A labelled function is hoisted like a direct one, and its
                    // Annex B copy runs where the declaration is evaluated.
                    return match placement {
                        StatementListPlacement::Block => {
                            self.lower_annex_b_function_copy(function).map(|statement| {
                                LoweredStatementListItemIr::statement(
                                    statement,
                                    ValueKind::Undefined,
                                )
                            })
                        }
                        StatementListPlacement::Root => None,
                    };
                }
            }
        }
        match placement {
            StatementListPlacement::Root => {
                if self.analysis.is_hoisted_default_export_initializer(item) {
                    return None;
                }
                if let StatementListItem::Declaration(declaration) = item {
                    if matches!(declaration.as_ref(), Declaration::FunctionDeclaration(_))
                        || self.is_registered_generator_declaration(declaration)
                        || self.is_registered_async_function_declaration(declaration)
                        || self.is_registered_async_generator_declaration(declaration)
                    {
                        return None;
                    }
                }
            }
            StatementListPlacement::Block => {
                if let StatementListItem::Declaration(declaration) = item {
                    if let Declaration::FunctionDeclaration(function) = declaration.as_ref() {
                        return self.lower_annex_b_function_copy(function).map(|statement| {
                            LoweredStatementListItemIr::statement(statement, ValueKind::Undefined)
                        });
                    }
                    if BlockHoistableDeclaration::from_item(item).is_some() {
                        return None;
                    }
                }
            }
        }
        self.lower_statement_list_item(item, scope)
            .into_statement_list_entry()
    }

    fn lower_statement_list_item(
        &mut self,
        item: &StatementListItem,
        scope: &mut LexicalScopeInstantiation,
    ) -> LoweredStatementListItemIr {
        let lowered = match item {
            StatementListItem::Statement(statement) => {
                let (statement, result_kind) = self.lower_statement(statement);
                LoweredStatementListItemIr::statement(statement, result_kind)
            }
            StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                Declaration::Lexical(LexicalDeclaration::Using(list)) => self
                    .lower_using_declaration(list.as_ref(), scope)
                    .map_or_else(
                        || {
                            LoweredStatementListItemIr::statement(
                                StatementIr::Empty,
                                ValueKind::Undefined,
                            )
                        },
                        |(execution, resources)| LoweredStatementListItemIr::SyncDisposableScope {
                            execution,
                            resources,
                        },
                    ),
                Declaration::Lexical(LexicalDeclaration::AwaitUsing(list)) => self
                    .lower_await_using_declaration(list.as_ref(), scope)
                    .map_or_else(
                        || {
                            LoweredStatementListItemIr::statement(
                                StatementIr::Empty,
                                ValueKind::Undefined,
                            )
                        },
                        LoweredStatementListItemIr::AsyncDisposableScope,
                    ),
                _ => {
                    let (mut statement, result_kind) = self.lower_declaration(declaration, scope);
                    if self.ordinary_generator_switch_depth > 0
                        || self.ordinary_generator_region_depth > 0
                        || self.ordinary_generator_for_in_depth > 0
                        || self.plain_async_for_in_depth > 0
                        || self.plain_async_for_of_depth > 0
                        || self.plain_async_classic_depth > 0
                        || self.plain_async_resource_depth > 0
                        || self.mixed_async_generator_region_depth > 0
                    {
                        let source =
                            CheckedEmptyStatementCompletionSource::from_declaration(declaration);
                        statement = StatementIr::EmptyStatementCompletion(Box::new(
                            EmptyStatementCompletionIr::new(source, statement),
                        ));
                    }
                    LoweredStatementListItemIr::statement(statement, result_kind)
                }
            },
        };
        if self.current_resumable_plan.is_some()
            && matches!(
                &lowered,
                LoweredStatementListItemIr::SyncDisposableScope { .. }
                    | LoweredStatementListItemIr::AsyncDisposableScope(_)
            )
        {
            self.async_generator_source_domain =
                crate::async_generator_source::AsyncGeneratorSourceDomain::ForeignIteratorBody;
        }
        lowered
    }

    fn lower_block(&mut self, block: &Block) -> BlockIr {
        // 14.3.1.2 BlockDeclarationInstantiation: the constructor pushes the
        // Block's declarative Environment Record and creates the whole
        // statement list's lexically scoped declarations in it, before any
        // statement of the block is evaluated. `lower_statement_items` ends the
        // frame by consuming the token, so every caller of `lower_block` is
        // relieved of the push/pop it used to perform — and can no longer
        // perform it against a different frame than the sweep populated.
        let scope =
            LexicalScopeInstantiation::instantiate(self, block.statement_list().statements());
        let mut lowered = self.lower_statement_items(block.statement_list().statements(), scope);
        lowered.lexical_environment = self.lower_materialized_lexical_environment(
            self.analysis
                .block_environment_ids
                .get(&(block as *const Block as usize))
                .copied(),
        );
        lowered
    }

    fn lower_materialized_lexical_environment(
        &self,
        environment_id: Option<EnvironmentId>,
    ) -> Option<LexicalEnvironmentIr> {
        environment_id
            .and_then(|environment_id| {
                self.analysis
                    .materialized_stage_a_environment(environment_id)
            })
            .filter(|environment| self.analysis.environment_has_runtime_storage(environment))
            .map(|environment| LexicalEnvironmentIr {
                initialization: crate::LexicalEnvironmentInitializationIr::Uninitialized,
                eval_environment: environment.eval_environment.clone(),
                bindings: environment
                    .owned_env_slots
                    .iter()
                    .map(|(name, slot)| {
                        self.analysis.owned_environment_binding(
                            environment.id,
                            name,
                            *slot,
                            self.interner,
                        )
                    })
                    .collect(),
            })
    }

    fn lower_runtime_lexical_environment(
        &self,
        environment_id: Option<EnvironmentId>,
    ) -> Option<LexicalEnvironmentIr> {
        environment_id
            .and_then(|environment_id| self.analysis.materialized_environment(environment_id))
            .filter(|environment| self.analysis.environment_has_runtime_storage(environment))
            .map(|environment| LexicalEnvironmentIr {
                initialization: crate::LexicalEnvironmentInitializationIr::Uninitialized,
                eval_environment: environment.eval_environment.clone(),
                bindings: environment
                    .owned_env_slots
                    .iter()
                    .map(|(name, slot)| {
                        self.analysis.owned_environment_binding(
                            environment.id,
                            name,
                            *slot,
                            self.interner,
                        )
                    })
                    .collect(),
            })
    }

    fn lower_for_in_of_environment(&self, loop_key: usize) -> Option<ForInOfEnvironmentIr> {
        let tdz_environment_id = self
            .analysis
            .for_in_of_tdz_environment_ids
            .get(&loop_key)
            .copied()?;
        let iteration_environment_id = self
            .analysis
            .for_in_of_iteration_environment_ids
            .get(&loop_key)
            .copied();
        let tdz_binding_names = self
            .analysis
            .environment_plans
            .get(&tdz_environment_id)
            .map(|environment| environment.binding_storage_names.iter().cloned().collect())
            .unwrap_or_default();
        Some(ForInOfEnvironmentIr {
            tdz_environment: self.lower_runtime_lexical_environment(Some(tdz_environment_id)),
            iteration_environment: self.lower_runtime_lexical_environment(iteration_environment_id),
            tdz_binding_names,
        })
    }

    fn lower_throw(&mut self, throw: &AstThrow) -> (StatementIr, ValueKind) {
        if self.async_generator_entry_state().is_some() {
            let Some((mut statements, value)) = self.lower_mixed_generator_value(throw.target())
            else {
                self.unsupported(
                    "async-generator Throw operand requires a complete value continuation",
                );
                return (StatementIr::Empty, ValueKind::Undefined);
            };
            statements.push(StatementIr::Throw(value));
            return (StatementIr::LexicalBlock(statements), ValueKind::Undefined);
        }
        if self.plain_generator_entry_state().is_some()
            && contains(throw.target(), ContainsSymbol::YieldExpression)
        {
            let Some((mut statements, value)) =
                self.lower_staged_generator_expression(throw.target())
            else {
                self.unsupported("generator Throw operand requires a complete value continuation");
                return (StatementIr::Empty, ValueKind::Undefined);
            };
            // Throw consumes GetValue only after the entire operand has
            // completed normally. Injected Return/Throw uses the original
            // suspension and pending-completion owners before this statement.
            statements.push(StatementIr::Throw(value));
            return (StatementIr::LexicalBlock(statements), ValueKind::Undefined);
        }
        if self.plain_async_entry_state().is_some()
            && contains(throw.target(), ContainsSymbol::AwaitExpression)
        {
            let Some((mut statements, value)) =
                self.lower_async_prefixed_expression(throw.target())
            else {
                self.unsupported("async Throw operand requires a complete value continuation");
                return (StatementIr::Empty, ValueKind::Undefined);
            };
            statements.push(StatementIr::Throw(value));
            return (StatementIr::LexicalBlock(statements), ValueKind::Undefined);
        }
        let value = self.lower_expression(throw.target());
        (StatementIr::Throw(value), ValueKind::Undefined)
    }

    fn lower_for_head_expression_with_tdz(
        &mut self,
        mode: BindingMode,
        name: &str,
        expression: &Expression,
    ) -> TypedExpr {
        if mode == BindingMode::Var {
            return self.lower_expression(expression);
        }

        self.push_scope();
        self.declare_binding(
            name.to_string(),
            BindingInfo::tdz_placeholder(mode, TdzPlaceholderName::for_source_name(name)),
        );
        let lowered = self.lower_expression(expression);
        self.pop_scope();
        lowered
    }

    // `tdz_binding_info` is now `BindingInfo::tdz_placeholder`, which takes a
    // `TdzPlaceholderName` so the reserved spelling and the `Placeholder` state
    // are minted together. `is_tdz_binding_storage_name` is gone: a storage-name
    // prefix is a name domain, not a lifecycle state, and no site decides
    // whether to throw by testing it any more. The name-domain question that
    // does survive is `TdzPlaceholderName::names_a_placeholder`.

    fn static_string_expression(&self, expr: &Expression) -> Option<String> {
        let Expression::Literal(literal) = Self::unwrap_parenthesized_expr(expr) else {
            return None;
        };
        let LiteralKind::String(sym) = literal.kind() else {
            return None;
        };
        Some(self.interned_runtime_string(*sym))
    }

    fn static_string_receiver_value(&self, receiver: &Expression) -> Option<String> {
        let receiver = Self::unwrap_parenthesized_expr(receiver);
        if let Some(value) = self.static_string_expression(receiver) {
            return Some(value);
        }
        let Expression::Identifier(identifier) = receiver else {
            return None;
        };
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        let binding = self.lookup_binding(&name)?;
        self.static_string_bindings.get(&binding).cloned()
    }

    fn static_to_string_returns_regexp_object_expr(&self, expr: &Expression) -> bool {
        match Self::unwrap_parenthesized_expr(expr) {
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                self.static_to_string_regexp_object_bindings.contains(&name)
            }
            Expression::ObjectLiteral(object) => {
                self.static_object_literal_to_string_returns_regexp(object)
            }
            _ => false,
        }
    }

    fn static_object_literal_to_string_returns_regexp(&self, object: &ObjectLiteral) -> bool {
        object.properties().iter().any(|property| {
            let PropertyDefinition::Property(name, value) = property else {
                return false;
            };
            self.property_name_to_static_key(name).as_deref() == Some("toString")
                && self.static_function_expression_returns_regexp(value)
        })
    }

    fn static_function_expression_returns_regexp(&self, expr: &Expression) -> bool {
        let Expression::FunctionExpression(function) = Self::unwrap_parenthesized_expr(expr) else {
            return false;
        };
        let mut statements = function.body().statements().iter();
        let Some(StatementListItem::Statement(statement)) = statements.next() else {
            return false;
        };
        if statements.next().is_some() {
            return false;
        }
        let Statement::Return(return_statement) = statement.as_ref() else {
            return false;
        };
        let Some(target) = return_statement.target() else {
            return false;
        };
        matches!(
            Self::unwrap_parenthesized_expr(target),
            Expression::RegExpLiteral(_)
        )
    }

    fn unwrap_parenthesized_expr(mut expr: &Expression) -> &Expression {
        while let Expression::Parenthesized(parenthesized) = expr {
            expr = parenthesized.expression();
        }
        expr
    }

    /// True when a declaration initializer containing `await` can be staged
    /// into the async statement prefix.
    ///
    /// This used to require the initializer to be a unary/arithmetic tree over
    /// single-level awaits, which refused every other shape — `const x =
    /// f(await p)`, `const x = obj[await k]`, `const x = \`${await p}\`` — even
    /// though the prefix handles them the same way. The only real requirement
    /// is that the initializer is evaluated once and every suspension is owned
    /// by its unconditional prefix or a plain async conditional branch.
    fn async_initializer_is_stageable(&self, expression: &Expression) -> bool {
        CheckedAsyncPrefixSource::new(expression, self).is_some()
    }

    /// True when an `await` inside `expression` is reached only on some paths,
    /// without a branch owner available in the current continuation context.
    ///
    /// The walk itself lives in `await_requires_branch_owner`, which
    /// recurses through every operand position instead of only the
    /// unary/binary spine this used to check: `f(cond && await p)` hides the
    /// short-circuit under a call argument, and treating that as unconditional
    /// hoisted the `await` out of the `&&` and ran it even when `cond` was
    /// falsy.
    ///
    /// A chain whose target is statically nullish keeps its own answer: the
    /// first `?.` link short-circuits every time, so the awaits behind it are
    /// uniformly skipped rather than path-dependent.
    fn has_branch_sensitive_await(&self, expression: &Expression) -> bool {
        if !contains(expression, ContainsSymbol::AwaitExpression) {
            return false;
        }
        if let Expression::Optional(optional) = Self::unwrap_parenthesized_expr(expression) {
            if !contains(optional.target(), ContainsSymbol::AwaitExpression)
                && optional
                    .chain()
                    .first()
                    .is_some_and(|operation| operation.shorted())
                && self.is_statically_nullish_optional_target(optional.target())
            {
                return false;
            }
        }
        let owner = if self.has_plain_async_value_branch_owner() {
            AwaitBranchOwner::PlainAsyncBranch
        } else {
            AwaitBranchOwner::UnconditionalPrefix
        };
        await_requires_branch_owner(expression, owner)
            || !async_expression_prefix::awaited_reference_operands_are_owned(expression, self)
    }

    /// Lower `expression` into a statement prefix plus the value left over
    /// once every `await` in it has been moved into its own statement.
    ///
    /// This is the general form of the async staging the lowerer already did
    /// for a handful of recognised shapes. Arming `async_expression_prefix`
    /// makes `lower_expression` rewrite each `await` it meets into a `let`
    /// binding plus an `AsyncAwait` statement appended to the prefix, leaving
    /// the temporary behind in the expression. Any statement that evaluates
    /// its head expression exactly once can therefore opt in, instead of each
    /// statement form hand-listing the expression shapes it knows how to take.
    ///
    /// Returns `None` when there is nothing to stage, when the suspension is a
    /// `yield` (the generator staging path owns those), or when an `await`
    /// sits behind a branch without an admitted continuation owner. Plain async
    /// conditional, logical and bounded optional Property/Call values retain their
    /// branch prefixes inside the existing If dispatcher. The checked while
    /// condition scope can restart it; other Reference, loop-head/body and
    /// generator branches retain their admission boundary.
    ///
    /// The previous prefix is saved and restored rather than cleared, so a
    /// nested statement that stages its own head cannot swallow the prefix an
    /// enclosing one is still collecting into.
    fn lower_async_prefixed_expression(
        &mut self,
        expression: &Expression,
    ) -> Option<(Vec<StatementIr>, TypedExpr)> {
        CheckedAsyncPrefixSource::new(expression, self).map(|source| source.lower(self))
    }

    /// True when `head` suspends, `rest` does not, and the suspension can be
    /// lifted into the statement prefix.
    ///
    /// `rest` is the part of the statement that is *not* the head — a loop
    /// body, a branch. It has to be `await`-free, because the prefix runs
    /// once, before the statement, so a suspension taken from a body would run
    /// too early and only once.
    fn head_await_is_stageable<'b, const N: usize>(
        &self,
        head: &Expression,
        rest: [Option<&'b Statement>; N],
    ) -> bool {
        CheckedAsyncPrefixSource::new(head, self).is_some()
            && rest
                .into_iter()
                .flatten()
                .all(|statement| !contains(statement, ContainsSymbol::AwaitExpression))
    }

    /// Run `lower` with the async statement prefix armed, and return whatever
    /// suspensions it staged alongside the lowered statement.
    ///
    /// Statement forms that evaluate a head expression once — a `switch`
    /// discriminant, a `for`/`for-of` iterable — build their whole statement
    /// in one call, so the prefix is armed around that call and the staged
    /// suspensions are spliced in ahead of it. Callers must first prove the
    /// rest of the statement is `await`-free, otherwise a suspension from a
    /// body would be hoisted out of the loop or branch that guards it.
    fn lower_with_async_head_prefix(
        &mut self,
        source: &Expression,
        lower: impl FnOnce(&mut Self) -> (StatementIr, ValueKind),
    ) -> (StatementIr, ValueKind) {
        let Some(source) = CheckedAsyncPrefixSource::new(source, self) else {
            self.unsupported("conditionally reached or mixed suspension in async statement head");
            return (StatementIr::Empty, ValueKind::Undefined);
        };
        source.lower_head(self, lower)
    }

    /// Materialize `value` into the async prefix when a later operand of the
    /// same expression suspends.
    ///
    /// Hoisting an `await` into the prefix moves it *ahead* of everything left
    /// inline in the residual expression, so an operand the spec evaluates
    /// first would otherwise run second. Binding it here restores the
    /// left-to-right order. Literals are left alone: they have no side effects
    /// and cannot observe one.
    fn pin_async_operand_before_suspension(
        &mut self,
        value: TypedExpr,
        rest_suspends: bool,
        hint: &str,
    ) -> TypedExpr {
        if !rest_suspends || self.async_expression_prefix.is_none() {
            return value;
        }
        if matches!(
            value.expr,
            ExprIr::Number(_)
                | ExprIr::String(_)
                | ExprIr::WellKnownSymbol(_)
                | ExprIr::Boolean(_)
                | ExprIr::BigInt(_)
                | ExprIr::Null
                | ExprIr::Undefined
        ) {
            return value;
        }
        let name = self.alloc_suspension_owned_binding(hint, value.value_info());
        self.async_expression_prefix
            .as_mut()
            .expect("async expression lowering must have a statement prefix")
            .push(StatementIr::Lexical {
                mode: BindingMode::Let,
                name: name.clone(),
                init: value,
            });
        self.lower_identifier_name(name, false)
    }

    fn is_statically_nullish_optional_target(&self, expression: &Expression) -> bool {
        match Self::unwrap_parenthesized_expr(expression) {
            Expression::Literal(literal) => {
                matches!(literal.kind(), LiteralKind::Null | LiteralKind::Undefined)
            }
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let nullish_kinds = KindSet::from_kind(ValueKind::Undefined)
                    .union(KindSet::from_kind(ValueKind::Null));
                self.lookup_binding(&name)
                    .map(|binding| binding.possible_kinds)
                    .or_else(|| {
                        self.lookup_global_property_info(&name)
                            .filter(|property| property.proven_present)
                            .map(|property| property.value_info.possible_kinds)
                    })
                    .is_some_and(|possible_kinds| possible_kinds.is_subset_of(nullish_kinds))
            }
            Expression::Binary(binary) if binary.op() == BinaryOp::Comma => {
                self.is_statically_nullish_optional_target(binary.rhs())
            }
            _ => false,
        }
    }

    fn is_number_prototype_property_expr(&self, expr: &Expression, property: &str) -> bool {
        self.is_constructor_prototype_property_expr(expr, NUMBER_NAME, property)
    }

    fn is_boolean_prototype_property_expr(&self, expr: &Expression, property: &str) -> bool {
        self.is_constructor_prototype_property_expr(expr, BOOLEAN_NAME, property)
    }

    fn is_object_prototype_property_expr(&self, expr: &Expression, property: &str) -> bool {
        self.is_constructor_prototype_property_expr(expr, OBJECT_NAME, property)
    }

    fn is_constructor_prototype_property_expr(
        &self,
        expr: &Expression,
        constructor_name: &str,
        property: &str,
    ) -> bool {
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) =
            Self::unwrap_parenthesized_expr(expr)
        else {
            return false;
        };
        let PropertyAccessField::Const(field) = access.field() else {
            return false;
        };
        if self.interner.resolve_expect(field.sym()).to_string() != property {
            return false;
        }
        let Expression::PropertyAccess(PropertyAccess::Simple(prototype_access)) =
            Self::unwrap_parenthesized_expr(access.target())
        else {
            return false;
        };
        let PropertyAccessField::Const(prototype_field) = prototype_access.field() else {
            return false;
        };
        if self
            .interner
            .resolve_expect(prototype_field.sym())
            .to_string()
            != "prototype"
        {
            return false;
        }
        let Expression::Identifier(identifier) =
            Self::unwrap_parenthesized_expr(prototype_access.target())
        else {
            return false;
        };
        self.interner.resolve_expect(identifier.sym()).to_string() == constructor_name
            && self.is_intrinsic_global_constructor(constructor_name)
    }

    fn is_constructor_prototype_expr(&self, expr: &Expression, constructor_name: &str) -> bool {
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) =
            Self::unwrap_parenthesized_expr(expr)
        else {
            return false;
        };
        let PropertyAccessField::Const(field) = access.field() else {
            return false;
        };
        if self.interner.resolve_expect(field.sym()).to_string() != "prototype" {
            return false;
        }
        let Expression::Identifier(identifier) = Self::unwrap_parenthesized_expr(access.target())
        else {
            return false;
        };
        self.interner.resolve_expect(identifier.sym()).to_string() == constructor_name
            && self.is_intrinsic_global_constructor(constructor_name)
    }

    fn is_intrinsic_global_constructor(&self, name: &str) -> bool {
        if self.lookup_binding(name).is_some() {
            return false;
        }
        if !self.identifier_resolves_to_intrinsic_global(name) {
            return false;
        }
        let Some(builtin) = StandardBuiltinId::all_globals()
            .iter()
            .copied()
            .find(|builtin| builtin.global_name() == Some(name))
        else {
            return false;
        };
        self.lookup_global_property_info(name)
            .is_some_and(|property| {
                property.proven_present
                    && property.value_info.function_targets.exact_single_target()
                        == Some(&builtin.function_id())
            })
    }

    fn static_bool_expr(expr: &TypedExpr) -> Option<bool> {
        match &expr.expr {
            ExprIr::StrictEquality { op, lhs, rhs } => {
                let lhs = Self::static_string_expr(lhs)?;
                let rhs = Self::static_string_expr(rhs)?;
                Some(match op {
                    EqualityBinaryOp::StrictEqual => lhs == rhs,
                    EqualityBinaryOp::StrictNotEqual => lhs != rhs,
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    fn static_string_expr(expr: &TypedExpr) -> Option<&str> {
        match &expr.expr {
            ExprIr::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// The resume state a plain `async function` body is currently at, or
    /// `None` for every other execution kind.
    ///
    /// Generators and async generators drive their loops off
    /// `current_generator_resume_state` (and, for async generators, a preplanned
    /// `ResumablePlanIr`); a plain async function has only the async state, so
    /// this is what distinguishes "needs the resumable loop lowering" from
    /// "already has it".
    fn plain_async_entry_state(&self) -> Option<u32> {
        if self.current_generator_resume_state.is_some() {
            return None;
        }
        self.current_async_resume_state
    }

    fn async_generator_entry_state(&self) -> Option<u32> {
        if self.current_resumable_plan.is_some()
            && self.async_generator_source_domain
                == crate::async_generator_source::AsyncGeneratorSourceDomain::FunctionBody
        {
            self.current_generator_resume_state
        } else {
            None
        }
    }

    fn split_resumable_loop_body(
        body: StatementIr,
        conditional_yield: bool,
    ) -> Option<(Vec<StatementIr>, StatementIr, Vec<StatementIr>, u32)> {
        let statements = match body {
            StatementIr::Block(block) if block.lexical_environment.is_none() => block.statements,
            StatementIr::LexicalBlock(statements) => statements,
            statement => vec![statement],
        };
        let statements = flatten_suspending_lexical_blocks(statements);
        let is_suspension = |statement: &StatementIr| {
            matches!(
                statement,
                StatementIr::GeneratorYield { .. }
                    | StatementIr::AsyncAwait { .. }
                    | StatementIr::ResumableClassDefinition(_)
            ) || conditional_yield
                && matches!(
                    statement,
                    StatementIr::GeneratorIf {
                        then_resume_state: Some(_),
                        else_resume_state: None,
                        ..
                    } | StatementIr::GeneratorIf {
                        then_resume_state: None,
                        else_resume_state: Some(_),
                        ..
                    }
                )
        };
        let suspension_index = statements.iter().position(is_suspension)?;
        let mut before_suspension = statements;
        let after_suspension = before_suspension.split_off(suspension_index + 1);
        let suspension_statement = before_suspension.pop()?;
        let last_resume_state = match &suspension_statement {
            StatementIr::ResumableClassDefinition(plan) => {
                if before_suspension
                    .iter()
                    .chain(&after_suspension)
                    .any(statement_contains_suspension)
                {
                    return None;
                }
                plan.exit_state()
            }
            StatementIr::AsyncAwait { suspend_state, .. } => {
                if before_suspension.iter().any(statement_contains_suspension) {
                    return None;
                }
                direct_await_sequence_resume_state(
                    &suspension_statement,
                    &after_suspension,
                    *suspend_state,
                )
                .ok()?
            }
            StatementIr::GeneratorYield { resume_state, .. }
            | StatementIr::GeneratorIf {
                then_resume_state: Some(resume_state),
                else_resume_state: None,
                ..
            }
            | StatementIr::GeneratorIf {
                then_resume_state: None,
                else_resume_state: Some(resume_state),
                ..
            } => {
                if after_suspension.iter().any(is_suspension) {
                    return None;
                }
                *resume_state
            }
            _ => unreachable!("the split selected a direct suspension"),
        };
        Some((
            before_suspension,
            suspension_statement,
            after_suspension,
            last_resume_state,
        ))
    }

    // `predeclare_block_lexical_bindings`, `predeclare_switch_lexical_bindings`
    // and `predeclare_direct_lexical_binding` lived here. They are now
    // `LexicalScopeInstantiation::instantiate` / `::instantiate_switch`
    // (`binding_lifecycle.rs`), which the three statement-list lowering entries
    // take **by value** — so a statement list can no longer be lowered without
    // BlockDeclarationInstantiation having run for it, and the
    // `boa_ast::Declaration` match that used to end in `_ => {}` is exhaustive.
    // The per-name creation half is `Self::create_lexical_binding`.

    fn lower_for_init(&mut self, init: &ForLoopInitializer) -> Option<ForInitIr> {
        match init {
            ForLoopInitializer::Expression(expr) => {
                Some(ForInitIr::Expression(self.lower_expression(expr)))
            }
            ForLoopInitializer::Var(var) => self.lower_var_init(var),
            ForLoopInitializer::Lexical(lexical) => {
                self.lower_for_lexical_init(lexical.declaration())
            }
        }
    }

    fn lower_for_lexical_init(&mut self, declaration: &LexicalDeclaration) -> Option<ForInitIr> {
        let (mode, list) = match declaration {
            LexicalDeclaration::Let(list) => (BindingMode::Let, list),
            LexicalDeclaration::Const(list) => (BindingMode::Const, list),
            LexicalDeclaration::Using(list) => {
                return self
                    .lower_for_sync_disposable_init(list.as_ref())
                    .map(ForInitIr::SyncDisposable);
            }
            LexicalDeclaration::AwaitUsing(_) => {
                self.unsupported("await using declaration");
                return None;
            }
        };

        if list
            .as_ref()
            .iter()
            .any(|variable| matches!(variable.binding(), Binding::Pattern(_)))
        {
            return self.lower_for_lexical_pattern_init(mode, list.as_ref());
        }

        let mut bindings = Vec::with_capacity(list.as_ref().len());
        for variable in list.as_ref() {
            bindings.push(self.lower_for_lexical_binding(mode, variable)?);
        }

        if bindings.len() == 1 {
            let binding = bindings.remove(0);
            return Some(ForInitIr::Lexical {
                mode: binding.mode,
                name: binding.name,
                init: binding.init,
            });
        }

        Some(ForInitIr::LexicalBlock(bindings))
    }

    /// Lowers a classic `for (using ...; ...; ...)` initializer into the
    /// non-empty resource capability that owns each binding's runtime
    /// InitializeBinding.
    fn lower_for_sync_disposable_init(
        &mut self,
        list: &[Variable],
    ) -> Option<SyncDisposableResourcesIr> {
        if list.is_empty() {
            self.unsupported("empty using declaration");
            return None;
        }
        if list
            .iter()
            .any(|variable| !matches!(variable.binding(), Binding::Identifier(_)))
        {
            self.unsupported("using declaration binding pattern");
            return None;
        }
        if list.iter().any(|variable| variable.init().is_none()) {
            self.unsupported("using declaration without initializer");
            return None;
        }

        let mut resources = Vec::with_capacity(list.len());
        for variable in list {
            let Binding::Identifier(identifier) = variable.binding() else {
                unreachable!("binding patterns were rejected before lowering")
            };
            let name = self.interner.resolve_expect(identifier.sym()).to_string();
            let initializer = variable
                .init()
                .expect("using initializers were validated before lowering");
            let init = self.lower_expression(initializer);
            self.static_to_string_regexp_object_bindings.remove(&name);
            let storage_name = scoped_lexical_binding_storage_name(&name, identifier.span());
            let initialized = InitializedBinding::without_creation(
                name.clone(),
                BindingMode::Const,
                storage_name,
                init,
            );
            let resource = initialized.into_sync_disposable_resource(self);
            let binding = self.lookup_binding(&name).unwrap_or_else(|| {
                panic!("for-loop using binding `{name}` must be declared before clearing facts")
            });
            self.static_string_bindings.remove(&binding);
            resources.push(resource);
        }

        let mut resources = resources.into_iter();
        let first = resources
            .next()
            .expect("a parsed using BindingList is non-empty");
        Some(SyncDisposableResourcesIr::new(first, resources.collect()))
    }

    /// `for (let [a, b] = x; …)` - a loop head that binds a pattern.
    ///
    /// `ForInitIr::Lexical`/`LexicalBlock` are name/initialiser pairs and
    /// cannot express a pattern, so the whole declaration is lowered to the
    /// statements the equivalent standalone `let [a, b] = x;` produces. The
    /// statements are handed over unwrapped rather than as one
    /// `StatementIr::LexicalBlock`: the block form opens its own emit scope,
    /// which would hide the bound names from the loop's test, update and body.
    fn lower_for_lexical_pattern_init(
        &mut self,
        mode: BindingMode,
        list: &[Variable],
    ) -> Option<ForInitIr> {
        let mut statements = Vec::with_capacity(list.len());
        for variable in list {
            match variable.binding() {
                Binding::Identifier(_) => {
                    let binding = self.lower_for_lexical_binding(mode, variable)?;
                    statements.push(StatementIr::Lexical {
                        mode: binding.mode,
                        name: binding.name,
                        init: binding.init,
                    });
                }
                Binding::Pattern(pattern) => {
                    let Some(init) = variable.init() else {
                        self.unsupported("destructuring binding without initializer");
                        return None;
                    };
                    let storage_names = supported_bound_names(self.interner, variable.binding())?
                        .into_iter()
                        .map(|bound| {
                            let storage =
                                scoped_lexical_binding_storage_name(&bound.source_name, bound.span);
                            (bound.source_name, storage)
                        })
                        .collect::<BTreeMap<_, _>>();
                    let (mut prefix, value) = self
                        .stage_await_destructuring_initializer(init)
                        .unwrap_or_else(|| (Vec::new(), self.lower_expression(init)));
                    statements.append(&mut prefix);
                    let mut bindings = self
                        .lower_pattern_lexical_binding_from_value_with_storage_names(
                            mode,
                            pattern,
                            value,
                            Some(&storage_names),
                        )?;
                    statements.append(&mut bindings);
                }
            }
        }
        Some(ForInitIr::Statements(statements))
    }

    fn lower_for_lexical_binding(
        &mut self,
        mode: BindingMode,
        variable: &Variable,
    ) -> Option<ForLexicalInitIr> {
        let Binding::Identifier(identifier) = variable.binding() else {
            self.unsupported("destructuring binding");
            return None;
        };

        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        if variable
            .init()
            .is_some_and(|expression| self.is_constructor_prototype_expr(expression, ARRAY_NAME))
        {
            self.array_prototype_mutated = true;
        }
        let static_to_string_regexp_object = variable
            .init()
            .is_some_and(|expression| self.static_to_string_returns_regexp_object_expr(expression));
        let static_string_value = variable
            .init()
            .and_then(|expression| self.static_string_expression(expression));
        let source_candidate = variable
            .init()
            .map(|expression| self.function_source_value_candidates(expression));
        let init = variable
            .init()
            .map(|expression| self.lower_expression(expression))
            .unwrap_or_else(TypedExpr::undefined);
        if static_to_string_regexp_object {
            self.static_to_string_regexp_object_bindings
                .insert(name.clone());
        } else {
            self.static_to_string_regexp_object_bindings.remove(&name);
        }

        let storage_name = scoped_lexical_binding_storage_name(&name, identifier.span());
        self.declare_binding(
            name.clone(),
            BindingInfo {
                mode,
                storage_name: storage_name.clone(),
                kind: init.kind,
                possible_kinds: init.possible_kinds,
                heap_shape: init.heap_shape.clone(),
                function_targets: init.function_targets.clone(),
                initialization: Initialization::Initialized,
            },
        );
        let binding = self.lookup_binding(&name).unwrap_or_else(|| {
            panic!("for-loop lexical binding `{name}` must be declared before installing facts")
        });
        if let Some(candidate) = source_candidate {
            self.set_finite_source_binding_candidates(&name, candidate);
        }
        if let Some(value) = static_string_value {
            self.static_string_bindings.insert(&binding, value);
        } else {
            self.static_string_bindings.remove(&binding);
        }
        Some(ForLexicalInitIr {
            mode,
            name: storage_name,
            init,
        })
    }

    fn lower_function_parameters<'b>(
        &mut self,
        parameters: &'b FormalParameterList,
        function_name: &str,
    ) -> Option<&'b FormalParameterList> {
        for parameter in parameters.as_ref() {
            let binding = parameter.variable().binding();
            if !is_supported_parameter_binding(binding) {
                self.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot first slice: unsupported parameter form in `{function_name}`"
                ));
                return None;
            }
        }
        Some(parameters)
    }

    fn lower_parameter_binding_pattern(
        &mut self,
        binding: &Binding,
        storage_name: &str,
    ) -> Vec<StatementIr> {
        let Binding::Pattern(pattern) = binding else {
            return Vec::new();
        };
        let storage_expr = TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::Identifier(storage_name.to_string()),
        );
        let destructuring_may_run_user_code = match pattern {
            Pattern::Array(_) => true,
            Pattern::Object(pattern) => !pattern.bindings().is_empty(),
        };
        if destructuring_may_run_user_code {
            self.invalidate_unknown_user_code_effects();
        }
        let Pattern::Object(pattern) = pattern else {
            let Pattern::Array(pattern) = pattern else {
                return Vec::new();
            };
            let Some(pattern) =
                self.lower_array_binding_pattern(BindingMode::Let, pattern.bindings(), None)
            else {
                return Vec::new();
            };
            return vec![StatementIr::DeclarationEvaluation(TypedExpr::from_info(
                ValueInfo::undefined(),
                ExprIr::ArrayDestructure {
                    value: Box::new(storage_expr),
                    pattern,
                    evaluation: ArrayDestructuringEvaluationIr::BindingInitialization,
                },
            ))];
        };
        let Some(pattern) =
            self.lower_object_binding_pattern(BindingMode::Let, pattern.bindings(), None)
        else {
            return Vec::new();
        };
        vec![StatementIr::DeclarationEvaluation(TypedExpr::from_info(
            ValueInfo::undefined(),
            ExprIr::ObjectDestructure {
                value: Box::new(storage_expr),
                pattern: Box::new(pattern),
            },
        ))]
    }

    fn lower_loop_body(&mut self, statement: &Statement) -> (StatementIr, ValueKind) {
        self.breakable_depth += 1;
        self.loop_depth += 1;
        let lowered = self.lower_statement(statement);
        self.loop_depth -= 1;
        self.breakable_depth -= 1;
        lowered
    }

    fn lower_return(&mut self, ret: &AstReturn) -> (StatementIr, ValueKind) {
        if !self.is_function_body {
            self.unsupported("top-level return");
            return (StatementIr::Empty, ValueKind::Undefined);
        }
        if self.current_resumable_plan.is_some() {
            let Some(target) = ret.target() else {
                let value = TypedExpr::undefined();
                self.record_return_expression(&value);
                return (StatementIr::Return(value), ValueKind::Undefined);
            };
            let Some((mut statements, value)) =
                self.lower_staged_async_generator_return_expression(target)
            else {
                self.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot first slice: async-generator return expression has a composite suspension boundary: {target:?}"
                ));
                return (StatementIr::Empty, ValueKind::Undefined);
            };
            self.record_return_info(
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                },
                FunctionReturnShape::Absent,
            );
            let (await_return, _) =
                self.lower_linear_async_await_value(value, AsyncResumeModeIr::Return);
            if statements.is_empty() {
                return (await_return, ValueKind::Dynamic);
            }
            statements.push(await_return);
            return (StatementIr::LexicalBlock(statements), ValueKind::Dynamic);
        }
        if let (Some(Expression::Yield(yield_expression)), Some(_)) =
            (ret.target(), self.current_generator_resume_state)
        {
            return self.lower_linear_generator_yield(
                yield_expression.target(),
                yield_expression.delegate(),
                GeneratorResumeModeIr::Return,
            );
        }
        if let Some(target) = ret.target().filter(|target| {
            self.current_generator_resume_state.is_some()
                && contains(*target, ContainsSymbol::YieldExpression)
        }) {
            let Some((mut statements, value)) = self.lower_staged_generator_expression(target)
            else {
                self.unsupported("generator return expression suspension");
                return (StatementIr::Empty, ValueKind::Undefined);
            };
            self.record_return_expression(&value);
            let kind = value.kind;
            statements.push(StatementIr::Return(value));
            return (StatementIr::LexicalBlock(statements), kind);
        }
        if let (Some(Expression::Await(await_expression)), Some(_)) =
            (ret.target(), self.current_async_resume_state)
        {
            self.record_return_info(
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                },
                FunctionReturnShape::Absent,
            );
            return self
                .lower_linear_async_await(await_expression.target(), AsyncResumeModeIr::Return);
        }
        // `return <expr with await>` in an async function: the operand is
        // evaluated once, so every suspension in it stages ahead of the
        // `return`. This used to accept only a top-level arithmetic binary,
        // which refused `return f(await p)`, `return [await p]`, and the rest.
        if let Some((mut statements, value)) = ret
            .target()
            .and_then(|target| self.lower_async_prefixed_expression(target))
        {
            self.record_return_expression(&value);
            statements.push(StatementIr::Return(value.clone()));
            return (
                StatementIr::Block(BlockIr {
                    statements,
                    result_kind: value.kind,
                    lexical_environment: None,
                }),
                value.kind,
            );
        }
        if let Some(Expression::ObjectLiteral(object)) =
            ret.target().map(Self::unwrap_parenthesized_expr)
        {
            if object
                .properties()
                .iter()
                .any(|property| matches!(property, PropertyDefinition::SpreadObject(_)))
            {
                let Some((mut statements, value)) = self.lower_returned_object_spread(object)
                else {
                    self.unsupported("returned object literal spread");
                    return (StatementIr::Empty, ValueKind::Undefined);
                };
                self.record_return_expression(&value);
                statements.push(StatementIr::Return(value));
                return (StatementIr::LexicalBlock(statements), ValueKind::Object);
            }
        }
        let value = ret
            .target()
            .map(|expression| self.lower_expression(expression))
            .unwrap_or_else(TypedExpr::undefined);
        self.record_return_expression(&value);
        (StatementIr::Return(value.clone()), value.kind)
    }

    fn take_resumable_suspension(
        &mut self,
        expected_kind: ResumableSuspensionKindIr,
    ) -> Option<ResumableSuspensionPointIr> {
        let index = self.next_resumable_suspension_index;
        let Some(suspension) = self
            .current_resumable_plan
            .as_ref()
            .and_then(|plan| plan.suspension_points.get(index))
            .cloned()
        else {
            self.unsupported_with_message(format!(
                "unsupported in scriptw wasm-aot first slice: async-generator resumable plan for `{}` has no {expected_kind:?} suspension at index {index}",
                self.current_function_id
                    .as_ref()
                    .map(String::as_str)
                    .unwrap_or("<unknown>")
            ));
            return None;
        };
        if suspension.kind != expected_kind {
            self.unsupported_with_message(format!(
                "unsupported in scriptw wasm-aot first slice: async-generator resumable plan for `{}` expected {expected_kind:?} at index {index}, found {:?}",
                self.current_function_id
                    .as_ref()
                    .map(String::as_str)
                    .unwrap_or("<unknown>"),
                suspension.kind
            ));
            return None;
        }

        let sequential = match expected_kind {
            ResumableSuspensionKindIr::Await | ResumableSuspensionKindIr::Yield => true,
            ResumableSuspensionKindIr::ForAwaitNext | ResumableSuspensionKindIr::ForAwaitClose => {
                false
            }
        };
        if sequential
            && self.async_generator_entry_state().is_some()
            && (self.current_generator_resume_state != Some(suspension.suspend_state)
                || self.current_async_resume_state != Some(suspension.suspend_state))
        {
            self.unsupported("async-generator suspension must consume its exact source state");
            return None;
        }

        self.next_resumable_suspension_index += 1;
        self.current_generator_resume_state = Some(suspension.resume_state);
        self.current_async_resume_state = Some(suspension.resume_state);
        Some(suspension)
    }

    fn lower_linear_generator_yield(
        &mut self,
        target: Option<&Expression>,
        delegate: bool,
        resume_mode: GeneratorResumeModeIr,
    ) -> (StatementIr, ValueKind) {
        let (mut statements, value) = match target {
            Some(target) if contains(target, ContainsSymbol::YieldExpression) => {
                if contains(target, ContainsSymbol::AwaitExpression) {
                    self.unsupported("mixed await/yield in a generator yield operand");
                    return (StatementIr::Empty, ValueKind::Undefined);
                }
                let Some(staged) = self.lower_staged_generator_expression(target) else {
                    self.unsupported("generator yield operand suspension");
                    return (StatementIr::Empty, ValueKind::Undefined);
                };
                staged
            }
            Some(target) => (Vec::new(), self.lower_expression(target)),
            None => (Vec::new(), TypedExpr::undefined()),
        };
        let (yield_statement, kind) =
            self.lower_linear_generator_yield_value(value, delegate, resume_mode);
        if statements.is_empty() {
            (yield_statement, kind)
        } else {
            statements.push(yield_statement);
            (StatementIr::LexicalBlock(statements), kind)
        }
    }

    fn lower_linear_generator_yield_value(
        &mut self,
        value: TypedExpr,
        delegate: bool,
        resume_mode: GeneratorResumeModeIr,
    ) -> (StatementIr, ValueKind) {
        // The caller can invoke escaped functions or mutate shared objects
        // before resuming, so pre-yield flow facts cannot describe the resume.
        self.invalidate_unknown_user_code_effects();
        let (suspend_state, resume_state) = if self.current_resumable_plan.is_some() {
            let (suspend_state, resume_state) = self
                .take_resumable_suspension(ResumableSuspensionKindIr::Yield)
                .map(|suspension| (suspension.suspend_state, suspension.resume_state))
                .unwrap_or_else(|| {
                    let suspend_state = self.current_generator_resume_state.unwrap_or_default();
                    (suspend_state, suspend_state + 1)
                });
            (suspend_state, resume_state)
        } else {
            let suspend_state = self
                .current_generator_resume_state
                .expect("linear generator lowering must have a resume state");
            let resume_state = suspend_state + 1;
            self.current_generator_resume_state = Some(resume_state);
            (suspend_state, resume_state)
        };
        let kind = value.kind;
        (
            StatementIr::GeneratorYield {
                value,
                form: if delegate {
                    YieldForm::Delegate(GeneratorDelegationProtocol::YIELD_STAR)
                } else {
                    YieldForm::Plain
                },
                suspend_state,
                resume_state,
                resume_mode,
            },
            kind,
        )
    }

    fn lower_staged_generator_expression(
        &mut self,
        expression: &Expression,
    ) -> Option<(Vec<StatementIr>, TypedExpr)> {
        if self.async_generator_entry_state().is_some() {
            self.lower_mixed_generator_value(expression)
        } else {
            self.lower_staged_generator_expression_legacy(expression)
        }
    }

    fn lower_staged_generator_expression_legacy(
        &mut self,
        expression: &Expression,
    ) -> Option<(Vec<StatementIr>, TypedExpr)> {
        if !contains(expression, ContainsSymbol::YieldExpression) {
            return Some((Vec::new(), self.lower_expression(expression)));
        }
        let admission = self.generator_value_branch_admission();
        GeneratorExpressionSourcePlan::new(expression, admission)?;
        if matches!(
            admission,
            GeneratorValueBranchAdmission::OrdinaryOutsideLoops
        ) {
            if let Expression::Optional(optional) = expression {
                if let Some(source) = GeneratorOptionalChainSource::new(optional) {
                    return self.lower_generator_optional_chain(source);
                }
            }
            if let Some(source) = GeneratorValueBranchSource::new(expression) {
                return self.lower_generator_value_branch(source);
            }
        }
        match expression {
            Expression::Parenthesized(parenthesized) => {
                self.lower_staged_generator_expression(parenthesized.expression())
            }
            Expression::Binary(_) | Expression::Unary(_) | Expression::TemplateLiteral(_) => {
                self.lower_staged_generator_eager_value(expression)
            }
            Expression::Yield(yield_expression) => {
                let (mut statements, value) = match yield_expression.target() {
                    Some(target) if contains(target, ContainsSymbol::YieldExpression) => {
                        self.lower_staged_generator_expression(target)?
                    }
                    Some(target) => (Vec::new(), self.lower_expression(target)),
                    None => (Vec::new(), TypedExpr::undefined()),
                };
                let received_name = self.alloc_suspension_owned_binding(
                    "generator.received.",
                    ValueInfo {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    },
                );
                statements.push(StatementIr::Lexical {
                    mode: BindingMode::Let,
                    name: received_name.clone(),
                    init: TypedExpr::undefined(),
                });
                let (yield_statement, _) = self.lower_linear_generator_yield_value(
                    value,
                    yield_expression.delegate(),
                    GeneratorResumeModeIr::AssignIdentifier(received_name.clone()),
                );
                statements.push(yield_statement);
                Some((statements, self.lower_identifier_name(received_name, false)))
            }
            Expression::Call(_) | Expression::New(_) | Expression::TaggedTemplate(_)
                if contains(expression, ContainsSymbol::YieldExpression) =>
            {
                self.lower_staged_generator_invocation(expression)
            }
            Expression::Assign(assignment)
                if matches!(assignment.lhs(), AssignTarget::WebCompatCall(_)) =>
            {
                let AssignTarget::WebCompatCall(call) = assignment.lhs() else {
                    unreachable!()
                };
                self.lower_resumable_web_compat_call_target(call)
            }
            Expression::Assign(assignment)
                if CheckedGeneratorCompoundAssignmentSource::new(assignment).is_some() =>
            {
                self.lower_generator_compound_assignment(
                    CheckedGeneratorCompoundAssignmentSource::new(assignment)?,
                )
            }
            Expression::Assign(assignment) if assignment.op() == AssignOp::Assign => {
                match assignment.lhs() {
                    AssignTarget::Identifier(identifier) => {
                        let name = self.interner.resolve_expect(identifier.sym()).to_string();
                        self.lower_staged_generator_identifier_assignment(name, assignment.rhs())
                    }
                    AssignTarget::Access(PropertyAccess::Simple(access)) => {
                        self.lower_staged_generator_property_assignment(access, assignment.rhs())
                    }
                    AssignTarget::Pattern(_) => self.lower_staged_generator_pattern_assignment(
                        GeneratorPatternAssignmentSource::new(
                            assignment,
                            self.generator_value_branch_admission(),
                        )?,
                    ),
                    AssignTarget::Access(PropertyAccess::Private(_) | PropertyAccess::Super(_)) => {
                        self.lower_resumable_property_assignment(assignment)
                    }
                    AssignTarget::WebCompatCall(_) => None,
                }
            }
            Expression::PropertyAccess(PropertyAccess::Simple(access))
                if contains(expression, ContainsSymbol::YieldExpression) =>
            {
                let (statements, _, value) = self.lower_staged_generator_property(access)?;
                Some((statements, value))
            }
            Expression::PropertyAccess(PropertyAccess::Private(access)) => {
                self.lower_resumable_private_read(access)
            }
            Expression::PropertyAccess(PropertyAccess::Super(_))
            | Expression::BinaryInPrivate(_) => {
                self.lower_staged_generator_special_read(expression)
            }
            Expression::Update(update) => self.lower_resumable_update(update),
            Expression::ImportCall(call) => self.lower_resumable_import_call(call),
            Expression::ArrayLiteral(array) => self.lower_staged_generator_array_literal(array),
            Expression::ObjectLiteral(object) => self.lower_staged_generator_object_literal(object),
            Expression::ClassExpression(class) => self.lower_staged_class_expression(class),
            expression if contains(expression, ContainsSymbol::YieldExpression) => None,
            _ => Some((Vec::new(), self.lower_expression(expression))),
        }
    }

    fn lower_returned_object_spread(
        &mut self,
        object: &ObjectLiteral,
    ) -> Option<(Vec<StatementIr>, TypedExpr)> {
        let object_info = ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape::default()))),
            function_targets: FunctionTargetKnowledge::none(),
        };
        let accumulator_name = self.alloc_temp_binding_name("object.spread.");
        self.declare_binding(
            accumulator_name.clone(),
            BindingInfo {
                mode: BindingMode::Let,
                storage_name: accumulator_name.clone(),
                kind: object_info.kind,
                possible_kinds: object_info.possible_kinds,
                heap_shape: object_info.heap_shape.clone(),
                function_targets: object_info.function_targets.clone(),
                initialization: Initialization::Initialized,
            },
        );
        let mut statements = vec![StatementIr::Lexical {
            mode: BindingMode::Let,
            name: accumulator_name.clone(),
            init: TypedExpr::from_info(object_info, ExprIr::ObjectLiteral(Vec::new())),
        }];
        for property in object.properties() {
            let PropertyDefinition::SpreadObject(source) = property else {
                return None;
            };
            let source = self.lower_expression(source);
            self.invalidate_unknown_user_code_effects();
            let accumulator = self.lower_identifier_name(accumulator_name.clone(), false);
            statements.push(StatementIr::Expression(
                TypedExpr::spec_copy_data_properties(accumulator, source),
            ));
        }
        Some((
            statements,
            self.lower_identifier_name(accumulator_name, false),
        ))
    }

    fn lower_discarded_generator_expression(
        &mut self,
        expression: &Expression,
    ) -> Option<Vec<StatementIr>> {
        let ungrouped = Self::unwrap_parenthesized_expr(expression);
        if contains(expression, ContainsSymbol::YieldExpression)
            && (matches!(
                ungrouped,
                Expression::Unary(_) | Expression::TemplateLiteral(_)
            ) || matches!(ungrouped,
                    Expression::Binary(binary) if !matches!(binary.op(), BinaryOp::Logical(_))))
        {
            let (mut prefix, value) = self.lower_staged_generator_expression(expression)?;
            prefix.push(StatementIr::Expression(value));
            return Some(prefix);
        }
        if matches!(
            self.generator_value_branch_admission(),
            GeneratorValueBranchAdmission::OrdinaryOutsideLoops
        ) && (GeneratorValueBranchSource::new(Self::unwrap_parenthesized_expr(expression))
            .is_some()
            || matches!(Self::unwrap_parenthesized_expr(expression), Expression::Optional(optional)
                if GeneratorOptionalChainSource::new(optional).is_some()))
        {
            let (mut statements, value) = self.lower_staged_generator_expression(expression)?;
            statements.push(StatementIr::Expression(value));
            return Some(statements);
        }
        match expression {
            Expression::Parenthesized(parenthesized) => {
                self.lower_discarded_generator_expression(parenthesized.expression())
            }
            Expression::Yield(yield_expression) => {
                let (statement, _) = self.lower_linear_generator_yield(
                    yield_expression.target(),
                    yield_expression.delegate(),
                    GeneratorResumeModeIr::Ignore,
                );
                Some(vec![statement])
            }
            Expression::ArrayLiteral(array) => {
                let mut statements = Vec::new();
                for element in array.as_ref().iter().flatten() {
                    if matches!(element, Expression::Spread(_)) {
                        return None;
                    }
                    statements.extend(self.lower_discarded_generator_expression(element)?);
                }
                Some(statements)
            }
            Expression::Binary(binary) if binary.op() == BinaryOp::Comma => {
                let mut statements = self.lower_discarded_generator_expression(binary.lhs())?;
                statements.extend(self.lower_discarded_generator_expression(binary.rhs())?);
                Some(statements)
            }
            Expression::Binary(binary)
                if binary.op() == BinaryOp::Arithmetic(ArithmeticOp::Add) =>
            {
                let (mut statements, lhs) = self.lower_staged_generator_expression(binary.lhs())?;
                let lhs_name =
                    self.alloc_suspension_owned_binding("generator.binary.lhs.", lhs.value_info());
                statements.push(StatementIr::Lexical {
                    mode: BindingMode::Let,
                    name: lhs_name.clone(),
                    init: lhs,
                });
                let (rhs_statements, rhs) = self.lower_staged_generator_expression(binary.rhs())?;
                statements.extend(rhs_statements);
                let possible_kinds = KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Number))
                    .union(KindSet::from_kind(ValueKind::BigInt));
                statements.push(StatementIr::Expression(TypedExpr::from_info(
                    ValueInfo {
                        kind: possible_kinds.as_value_kind(),
                        possible_kinds,
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::CoerciveAdd {
                        lhs: Box::new(self.lower_identifier_name(lhs_name, false)),
                        rhs: Box::new(rhs),
                    },
                )));
                Some(statements)
            }
            Expression::Conditional(conditional) => {
                let Expression::Yield(condition_yield) =
                    Self::unwrap_parenthesized_expr(conditional.condition())
                else {
                    return None;
                };
                if condition_yield
                    .target()
                    .is_some_and(|target| contains(target, ContainsSymbol::YieldExpression))
                {
                    return None;
                }
                let received_name = self.alloc_temp_binding_name("yield.condition.");
                self.declare_binding(
                    received_name.clone(),
                    BindingInfo {
                        mode: BindingMode::Let,
                        storage_name: received_name.clone(),
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                        initialization: Initialization::Initialized,
                    },
                );
                let (condition_yield_statement, _) = self.lower_linear_generator_yield(
                    condition_yield.target(),
                    condition_yield.delegate(),
                    GeneratorResumeModeIr::AssignIdentifier(received_name.clone()),
                );
                let entry_state = self.current_generator_resume_state?;

                let mut then_statements =
                    self.lower_discarded_generator_expression(conditional.if_true())?;
                let then_yield_statement = then_statements.pop()?;
                if !then_statements.is_empty()
                    || !matches!(&then_yield_statement, StatementIr::GeneratorYield { .. })
                {
                    return None;
                }
                let StatementIr::GeneratorYield {
                    resume_state: then_resume_state,
                    ..
                } = &then_yield_statement
                else {
                    unreachable!()
                };
                let then_resume_state = *then_resume_state;

                let mut else_statements =
                    self.lower_discarded_generator_expression(conditional.if_false())?;
                let else_yield_statement = else_statements.pop()?;
                if !else_statements.is_empty()
                    || !matches!(&else_yield_statement, StatementIr::GeneratorYield { .. })
                {
                    return None;
                }
                let StatementIr::GeneratorYield {
                    resume_state: else_resume_state,
                    ..
                } = &else_yield_statement
                else {
                    unreachable!()
                };
                let else_resume_state = *else_resume_state;
                let exit_state = self.current_generator_resume_state? + 1;
                self.current_generator_resume_state = Some(exit_state);
                let condition = self.lower_identifier_name(received_name.clone(), false);

                Some(vec![
                    StatementIr::Lexical {
                        mode: BindingMode::Let,
                        name: received_name,
                        init: TypedExpr::undefined(),
                    },
                    condition_yield_statement,
                    StatementIr::GeneratorIf {
                        condition,
                        then_before_yield: Vec::new(),
                        then_yield_statement: Some(Box::new(then_yield_statement)),
                        then_after_yield: Vec::new(),
                        else_before_yield: Vec::new(),
                        else_yield_statement: Some(Box::new(else_yield_statement)),
                        else_after_yield: Vec::new(),
                        entry_state,
                        then_resume_state: Some(then_resume_state),
                        else_resume_state: Some(else_resume_state),
                        exit_state,
                    },
                ])
            }
            Expression::Assign(assignment)
                if assignment.op() == AssignOp::Assign
                    && matches!(assignment.lhs(), AssignTarget::Identifier(_))
                    && contains(assignment.rhs(), ContainsSymbol::YieldExpression)
                    && matches!(
                        self.generator_value_branch_admission(),
                        GeneratorValueBranchAdmission::OrdinaryOutsideLoops
                    ) =>
            {
                let AssignTarget::Identifier(identifier) = assignment.lhs() else {
                    unreachable!("the staged assignment requires an identifier Reference")
                };
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let (mut statements, value) =
                    self.lower_staged_generator_identifier_assignment(name, assignment.rhs())?;
                statements.push(StatementIr::Expression(value));
                Some(statements)
            }
            Expression::Assign(assignment)
                if assignment.op() == AssignOp::Assign
                    && matches!(assignment.lhs(), AssignTarget::Identifier(_))
                    && contains(assignment.rhs(), ContainsSymbol::YieldExpression)
                    && matches!(
                        self.generator_value_branch_admission(),
                        GeneratorValueBranchAdmission::LinearOnly
                    )
                    && !self.uses_runtime_identifier_environment()
                    && self.with_environment_chain.is_empty() =>
            {
                let AssignTarget::Identifier(identifier) = assignment.lhs() else {
                    unreachable!("the retained linear assignment has an identifier target")
                };
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                let (mut statements, value) =
                    self.lower_staged_generator_expression(assignment.rhs())?;
                statements.push(StatementIr::Expression(
                    self.lower_identifier_assign_value(name, value),
                ));
                Some(statements)
            }
            Expression::Call(_)
            | Expression::New(_)
            | Expression::TaggedTemplate(_)
            | Expression::PropertyAccess(PropertyAccess::Simple(_))
            | Expression::ClassExpression(_)
            | Expression::ObjectLiteral(_)
            | Expression::Update(_)
            | Expression::Assign(_)
                if contains(expression, ContainsSymbol::YieldExpression) =>
            {
                let (mut statements, value) = self.lower_staged_generator_expression(expression)?;
                statements.push(StatementIr::Expression(value));
                Some(statements)
            }
            expression if contains(expression, ContainsSymbol::YieldExpression) => None,
            _ => Some(vec![StatementIr::Expression(
                self.lower_expression(expression),
            )]),
        }
    }

    fn lower_generator_template_assignment(
        &mut self,
        target_name: String,
        template: &TemplateLiteral,
    ) -> Option<Vec<StatementIr>> {
        let accumulator_name = self.alloc_suspension_owned_binding(
            "generator.template.",
            ValueInfo::new(ValueKind::String),
        );
        let mut statements = vec![StatementIr::Lexical {
            mode: BindingMode::Let,
            name: accumulator_name.clone(),
            init: TypedExpr::from_info(
                ValueInfo::new(ValueKind::String),
                ExprIr::String(String::new()),
            ),
        }];

        for element in template.elements() {
            let part = match element {
                TemplateElement::String(sym) => {
                    let value = self.interned_runtime_string(*sym);
                    TypedExpr::from_info(ValueInfo::new(ValueKind::String), ExprIr::String(value))
                }
                TemplateElement::Expr(expression)
                    if contains(expression, ContainsSymbol::YieldExpression) =>
                {
                    let Expression::Yield(yield_expression) =
                        Self::unwrap_parenthesized_expr(expression)
                    else {
                        return None;
                    };
                    if yield_expression
                        .target()
                        .is_some_and(|target| contains(target, ContainsSymbol::YieldExpression))
                    {
                        return None;
                    }
                    let received_name = self.alloc_temp_binding_name("yield.template.");
                    self.declare_binding(
                        received_name.clone(),
                        BindingInfo {
                            mode: BindingMode::Let,
                            storage_name: received_name.clone(),
                            kind: ValueKind::Dynamic,
                            possible_kinds: KindSet::all_runtime_tags(),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::unknown(),
                            initialization: Initialization::Initialized,
                        },
                    );
                    statements.push(StatementIr::Lexical {
                        mode: BindingMode::Let,
                        name: received_name.clone(),
                        init: TypedExpr::undefined(),
                    });
                    let (yield_statement, _) = self.lower_linear_generator_yield(
                        yield_expression.target(),
                        yield_expression.delegate(),
                        GeneratorResumeModeIr::AssignIdentifier(received_name.clone()),
                    );
                    statements.push(yield_statement);
                    // Substitutions use ToString (hint String), unlike `+`.
                    TypedExpr::spec_to_string(self.lower_identifier_name(received_name, false))
                }
                TemplateElement::Expr(expression) => {
                    TypedExpr::spec_to_string(self.lower_expression(expression))
                }
            };
            let accumulator = self.lower_identifier_name(accumulator_name.clone(), false);
            let concatenated = TypedExpr::from_info(
                ValueInfo::new(ValueKind::String),
                ExprIr::StringConcat {
                    lhs: Box::new(accumulator),
                    rhs: Box::new(part),
                },
            );
            statements.push(StatementIr::Expression(
                self.lower_identifier_assign_value(accumulator_name.clone(), concatenated),
            ));
        }

        let result = self.lower_identifier_name(accumulator_name, false);
        statements.push(StatementIr::Expression(
            self.lower_identifier_assign_value(target_name, result),
        ));
        Some(statements)
    }

    fn lower_linear_async_await(
        &mut self,
        target: &Expression,
        resume_mode: AsyncResumeModeIr,
    ) -> (StatementIr, ValueKind) {
        if self.async_expression_prefix.is_none()
            && contains(target, ContainsSymbol::AwaitExpression)
        {
            let Some((mut prefix, value)) = self.lower_async_prefixed_expression(target) else {
                self.unsupported("conditionally reached or mixed suspension in async await target");
                return (StatementIr::Empty, ValueKind::Undefined);
            };
            let (await_statement, kind) = self.lower_linear_async_await_value(value, resume_mode);
            prefix.push(await_statement);
            return (StatementIr::LexicalBlock(prefix), kind);
        }
        let value = self.lower_expression(target);
        self.lower_linear_async_await_value(value, resume_mode)
    }

    fn lower_linear_async_await_value(
        &mut self,
        value: TypedExpr,
        resume_mode: AsyncResumeModeIr,
    ) -> (StatementIr, ValueKind) {
        // PromiseResolve reads a thenable's `then` before suspension. That
        // getter can run user code even though the continuation runs later.
        self.invalidate_unknown_user_code_effects();
        let (suspend_state, resume_state) = if self.current_resumable_plan.is_some() {
            let (suspend_state, resume_state) = self
                .take_resumable_suspension(ResumableSuspensionKindIr::Await)
                .map(|suspension| (suspension.suspend_state, suspension.resume_state))
                .unwrap_or_else(|| {
                    let suspend_state = self.current_async_resume_state.unwrap_or_default();
                    (suspend_state, suspend_state + 1)
                });
            (suspend_state, resume_state)
        } else {
            let suspend_state = self
                .current_async_resume_state
                .expect("linear async lowering must have a resume state");
            let resume_state = suspend_state + 1;
            self.current_async_resume_state = Some(resume_state);
            (suspend_state, resume_state)
        };
        let kind = value.kind;
        (
            StatementIr::AsyncAwait {
                value,
                suspend_state,
                resume_state,
                resume_mode,
            },
            kind,
        )
    }

    fn lower_declaration(
        &mut self,
        declaration: &Declaration,
        scope: &mut LexicalScopeInstantiation,
    ) -> (StatementIr, ValueKind) {
        match declaration {
            Declaration::Lexical(lexical) => self.lower_lexical_declaration(lexical, scope),
            Declaration::FunctionDeclaration(function) => (
                self.lower_function_declaration(function),
                ValueKind::Undefined,
            ),
            // Source admission consumes the same checked classic-loop ranges
            // that lowering validates against its emitted regions. Refusals
            // retain their concrete source reason before body emission.
            Declaration::GeneratorDeclaration(function) => {
                match linear_generator_plan_with_reason(function.body()) {
                    Ok(_) => (
                        self.lower_generator_declaration(function),
                        ValueKind::Undefined,
                    ),
                    Err(reason) => {
                        self.unsupported(reason.message());
                        (StatementIr::Empty, ValueKind::Undefined)
                    }
                }
            }
            Declaration::AsyncFunctionDeclaration(function) => (
                self.lower_async_function_declaration(function),
                ValueKind::Undefined,
            ),
            Declaration::AsyncGeneratorDeclaration(function) => (
                self.lower_async_generator_declaration(function),
                ValueKind::Undefined,
            ),
            Declaration::ClassDeclaration(class) => self.lower_class_declaration(class, scope),
        }
    }

    /// Lowers `using a = expr, b = expr;` into one non-empty resource list.
    ///
    /// The returned entries, not generic lexical statements, own runtime
    /// InitializeBinding. This preserves 14.3.1.3's order: evaluate, validate
    /// and register through AddDisposableResource, then initialize the binding.
    fn lower_using_declaration(
        &mut self,
        list: &[Variable],
        scope: &mut LexicalScopeInstantiation,
    ) -> Option<(SyncDisposableScopeExecutionIr, SyncDisposableResourcesIr)> {
        let owner = self.admit_sync_disposable_scope_owner()?;
        if list.is_empty() {
            self.unsupported("empty using declaration");
            return None;
        }

        if list
            .iter()
            .any(|variable| !matches!(variable.binding(), Binding::Identifier(_)))
        {
            self.unsupported("using declaration binding pattern");
            return None;
        }
        if list.iter().any(|variable| variable.init().is_none()) {
            self.unsupported("using declaration without initializer");
            return None;
        }

        if owner == SyncDisposableScopeOwnerPlan::AsyncGenerator
            && list.iter().filter_map(Variable::init).any(|initializer| {
                contains(initializer, ContainsSymbol::AwaitExpression)
                    || contains(initializer, ContainsSymbol::YieldExpression)
            })
        {
            self.unsupported("suspension inside an async-generator using initializer");
            return None;
        }

        let execution = self.sync_disposable_scope_execution(owner);

        let mut resources = Vec::with_capacity(list.len());
        for variable in list {
            let Binding::Identifier(identifier) = variable.binding() else {
                unreachable!("binding patterns were rejected before lowering")
            };
            let name = self.interner.resolve_expect(identifier.sym()).to_string();
            let pending = scope.take(&name);
            let initializer = variable
                .init()
                .expect("using initializers were validated before lowering");
            let init = self.lower_expression(initializer);
            // AddDisposableResource performs the observable @@dispose lookup
            // before the binding becomes available to the following suffix.
            self.invalidate_unknown_user_code_effects();
            self.static_to_string_regexp_object_bindings.remove(&name);
            let init = LoweredInitializer::evaluated(init);
            let initialized = match pending {
                Some(pending) => pending.initialize(init),
                None => {
                    let storage_name = self.direct_lexical_storage_name(&name, identifier.span());
                    InitializedBinding::without_creation(
                        name.clone(),
                        BindingMode::Const,
                        storage_name,
                        init.into_expr(),
                    )
                }
            };
            let resource = initialized.into_sync_disposable_resource(self);
            let binding = self.lookup_binding(&name).unwrap_or_else(|| {
                panic!("using binding `{name}` must be declared before clearing facts")
            });
            self.static_string_bindings.remove(&binding);
            resources.push(resource);
        }

        let mut resources = resources.into_iter();
        let first = resources
            .next()
            .expect("a parsed using BindingList is non-empty");
        Some((
            execution,
            SyncDisposableResourcesIr::new(first, resources.collect()),
        ))
    }

    fn sync_disposable_scope_execution(
        &mut self,
        owner: SyncDisposableScopeOwnerPlan,
    ) -> SyncDisposableScopeExecutionIr {
        match owner {
            SyncDisposableScopeOwnerPlan::Immediate => SyncDisposableScopeExecutionIr::Immediate,
            SyncDisposableScopeOwnerPlan::PlainGenerator => {
                let binding_name = self.alloc_suspension_owned_binding(
                    "generator.dispose.capability.",
                    ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                );
                SyncDisposableScopeExecutionIr::PlainGenerator(
                    PlainGeneratorSyncDisposableCapabilityIr::new(binding_name),
                )
            }
            SyncDisposableScopeOwnerPlan::AsyncFunction => {
                let binding_name = self.alloc_suspension_owned_binding(
                    "async.dispose.capability.",
                    ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                );
                SyncDisposableScopeExecutionIr::AsyncFunction(
                    AsyncFunctionSyncDisposableCapabilityIr::new(binding_name),
                )
            }
            SyncDisposableScopeOwnerPlan::AsyncGenerator => {
                let binding_name = self.alloc_suspension_owned_binding(
                    "async.generator.dispose.capability.",
                    ValueInfo {
                        kind: ValueKind::Object,
                        possible_kinds: KindSet::from_kind(ValueKind::Object),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                );
                SyncDisposableScopeExecutionIr::AsyncGenerator(
                    AsyncGeneratorSyncDisposableCapabilityIr::new(binding_name),
                )
            }
        }
    }

    fn hoist_root_statement_items(&mut self, items: &[StatementListItem]) {
        for item in items {
            match item {
                StatementListItem::Statement(statement) => self.hoist_statement(statement),
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::Lexical(lexical) => {
                        self.hoist_lexical_declaration_metadata(lexical)
                    }
                    Declaration::ClassDeclaration(class) => {
                        self.hoist_class_declaration_metadata(class)
                    }
                    _ => {}
                },
            }
        }
    }

    fn hoist_nested_statement_items(&mut self, items: &[StatementListItem]) {
        for item in items {
            if let StatementListItem::Statement(statement) = item {
                self.hoist_statement(statement);
            }
        }
    }

    fn hoist_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Block(block) => {
                self.hoist_nested_statement_items(block.statement_list().statements())
            }
            Statement::If(if_statement) => {
                self.hoist_statement(if_statement.body());
                if let Some(else_node) = if_statement.else_node() {
                    self.hoist_statement(else_node);
                }
            }
            Statement::WhileLoop(while_loop) => self.hoist_statement(while_loop.body()),
            Statement::DoWhileLoop(do_while) => self.hoist_statement(do_while.body()),
            Statement::ForLoop(for_loop) => {
                if let Some(ForLoopInitializer::Var(var)) = for_loop.init() {
                    self.hoist_var_declaration(var);
                }
                self.hoist_statement(for_loop.body());
            }
            Statement::Switch(switch) => {
                for case in switch.cases() {
                    self.hoist_nested_statement_items(case.body().statements());
                }
            }
            Statement::Labelled(labelled) => {
                if let Some(statement) = labelled_base_statement(labelled) {
                    self.hoist_statement(statement);
                }
            }
            Statement::Try(try_statement) => {
                self.hoist_nested_statement_items(
                    try_statement.block().statement_list().statements(),
                );
                if let Some(catch) = try_statement.catch() {
                    self.hoist_nested_statement_items(catch.block().statement_list().statements());
                }
                if let Some(finally_block) = try_statement.finally() {
                    self.hoist_nested_statement_items(
                        finally_block.block().statement_list().statements(),
                    );
                }
            }
            Statement::ForInLoop(for_in) => {
                if let IterableLoopInitializer::Var(variable) = for_in.initializer() {
                    self.hoist_var_binding(variable);
                }
                self.hoist_statement(for_in.body());
            }
            Statement::ForOfLoop(for_of) => {
                if let IterableLoopInitializer::Var(variable) = for_of.initializer() {
                    self.hoist_var_binding(variable);
                }
                self.hoist_statement(for_of.body());
            }
            Statement::Var(var) => self.hoist_var_declaration(var),
            Statement::With(with) => self.hoist_statement(with.statement()),
            Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::Return(_)
            | Statement::Throw(_) => {}
        }
    }

    fn hoist_var_declaration(&mut self, declaration: &VarDeclaration) {
        for variable in declaration.0.as_ref() {
            self.hoist_var_binding(variable);
        }
    }

    fn hoist_var_binding(&mut self, variable: &Variable) {
        let mut names = Vec::new();
        collect_binding_names(self.interner, variable.binding(), &mut names);
        for name in names {
            self.hoist_var_name(name.clone());
            if variable.init().is_none() {
                self.register_finite_undefined_var_source_candidate(&name);
            }
        }
    }

    /// Records a script-level class name as global lexical metadata, like the
    /// `let`/`const` sweep beside it. A class declaration creates a mutable
    /// lexical binding (15.7.16), so without this entry the name is missing
    /// from `GlobalBindingPlan::lexical_bindings` and a later
    /// `GlobalDeclarationInstantiation` (notably `$262.evalScript`) cannot see
    /// the existing binding to collide with it.
    fn hoist_class_declaration_metadata(&mut self, class: &ClassDeclaration) {
        if self.current_owner_id != SCRIPT_OWNER_ID {
            return;
        }
        let name = self.interner.resolve_expect(class.name().sym()).to_string();
        let info = self.infer_class_declaration_binding_info(class);
        self.var_bindings
            .entry(name)
            .or_insert_with(|| VarBindingInfo {
                kind: info.kind,
                possible_kinds: info.possible_kinds,
                heap_shape: info.heap_shape.clone(),
                function_targets: info.function_targets.clone(),
                is_script_global: false,
                is_lexical_metadata: true,
            });
    }

    fn hoist_var_name(&mut self, name: String) {
        if self.current_owner_id != SCRIPT_OWNER_ID
            && self
                .var_bindings
                .get(&name)
                .is_some_and(|binding| binding.is_script_global)
        {
            self.var_bindings.insert(
                name,
                VarBindingInfo {
                    kind: ValueKind::Undefined,
                    possible_kinds: KindSet::from_kind(ValueKind::Undefined),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                    is_script_global: false,
                    is_lexical_metadata: false,
                },
            );
            return;
        }
        let is_script_global = self.current_owner_id == SCRIPT_OWNER_ID
            && self.script_variables_are_global()
            && !self
                .analysis
                .module_execution
                .is_private_dispatcher_name(&name);
        self.var_bindings.entry(name).or_insert(VarBindingInfo {
            kind: ValueKind::Undefined,
            possible_kinds: KindSet::from_kind(ValueKind::Undefined),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
            is_script_global,
            is_lexical_metadata: false,
        });
    }

    fn hoist_lexical_declaration_metadata(&mut self, declaration: &LexicalDeclaration) {
        if self.current_owner_id != SCRIPT_OWNER_ID {
            return;
        }
        let list = match declaration {
            LexicalDeclaration::Let(list) | LexicalDeclaration::Const(list) => list,
            LexicalDeclaration::Using(_) | LexicalDeclaration::AwaitUsing(_) => return,
        };
        for variable in list.as_ref() {
            let mut names = Vec::new();
            let info = match variable.binding() {
                Binding::Identifier(identifier) => {
                    names.push(self.interner.resolve_expect(identifier.sym()).to_string());
                    variable
                        .init()
                        .and_then(|expression| self.static_initializer_value_info(expression))
                        .unwrap_or_else(ValueInfo::undefined)
                }
                Binding::Pattern(pattern) => {
                    collect_binding_names(
                        self.interner,
                        &Binding::Pattern(pattern.clone()),
                        &mut names,
                    );
                    ValueInfo::undefined()
                }
            };
            for name in names {
                self.var_bindings
                    .entry(name)
                    .or_insert_with(|| VarBindingInfo {
                        kind: info.kind,
                        possible_kinds: info.possible_kinds,
                        heap_shape: info.heap_shape.clone(),
                        function_targets: info.function_targets.clone(),
                        is_script_global: false,
                        is_lexical_metadata: true,
                    });
            }
        }
    }

    fn global_this_info(&self) -> ValueInfo {
        let mut properties = BTreeMap::new();
        for (name, binding) in &self.var_bindings {
            if !binding.is_script_global {
                continue;
            }
            properties.insert(
                name.clone(),
                ObjectShapeProperty::Data(ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                }),
            );
        }
        for (name, info) in &self.global_properties {
            if !info.proven_present {
                continue;
            }
            properties.insert(
                name.clone(),
                ObjectShapeProperty::Data(info.value_info.clone()),
            );
        }
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: None,
                properties,
                private_brands: BTreeSet::new(),
                boxed_primitive: None,
            }))),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    fn root_this_info_for(&self, binding: RootThisBinding) -> ValueInfo {
        match binding {
            RootThisBinding::GlobalObject => self.global_this_info(),
            RootThisBinding::Undefined => ValueInfo::undefined(),
            RootThisBinding::EvalCaller => unknown_runtime_value_info(),
        }
    }

    fn root_this_info(&self) -> ValueInfo {
        self.root_this_info_for(self.root_this_binding)
    }

    fn current_this_info(&self) -> ValueInfo {
        match &self.current_this_binding {
            CurrentThisBinding::Root(binding) => self.root_this_info_for(*binding),
            CurrentThisBinding::Activation(info) => info.clone(),
        }
    }

    fn lower_current_this(&mut self) -> TypedExpr {
        match self.current_this_binding.clone() {
            CurrentThisBinding::Root(RootThisBinding::GlobalObject) => {
                self.top_level_this_uses += 1;
                TypedExpr::from_info(self.global_this_info(), ExprIr::This)
            }
            CurrentThisBinding::Root(RootThisBinding::Undefined) => TypedExpr::undefined(),
            CurrentThisBinding::Root(RootThisBinding::EvalCaller) => {
                TypedExpr::from_info(unknown_runtime_value_info(), ExprIr::This)
            }
            CurrentThisBinding::Activation(info) => TypedExpr::from_info(info, ExprIr::This),
        }
    }

    fn function_target_is_strict(&self, function_id: &FunctionId) -> bool {
        if StandardBuiltinId::from_function_id(function_id).is_some()
            || HostBuiltinId::from_function_id(function_id).is_some()
        {
            return true;
        }
        let original_id = self.original_exact_function_id(function_id);
        self.analysis
            .function_plans
            .get(&original_id)
            .is_some_and(|plan| plan.strict)
    }

    /// `[[Strict]]` for a Reference created by the code currently being
    /// lowered (13.1.3 ResolveBinding's `strict` argument; 13.3.2.1 / 13.3.3.1
    /// member-expression evaluation).
    ///
    /// This is the sole producer of [`Strictness`] in the lowerer, and every
    /// Reference-shaped IR node gets its field from here. The value is a
    /// property of the Reference at creation time, so it must be taken *when
    /// the Reference is built* and carried; recovering it later by asking what
    /// the currently-emitting code's mode is computes a different quantity
    /// that only coincides while creation and consumption sit in the same
    /// function body.
    ///
    /// An owner with no plan is `Strict`, never `Sloppy`. A spurious throw
    /// fails loudly in the first test that reaches it; a suppressed throw is
    /// invisible, which is how strict mode went unenforced for unresolvable
    /// references without any test noticing.
    fn reference_strictness(&self) -> Strictness {
        if let Some(strictness) = self.lexical_strictness {
            return strictness;
        }
        match self.analysis.owner_plans.get(&self.current_owner_id) {
            Some(owner) if owner.strict => Strictness::Strict,
            Some(_) => Strictness::Sloppy,
            None => Strictness::Strict,
        }
    }

    fn default_this_info_for_function_target(&self, function_id: &FunctionId) -> ValueInfo {
        if self.function_target_is_strict(function_id) {
            ValueInfo::undefined()
        } else {
            self.global_this_info()
        }
    }

    fn explicit_this_info_for_function_target(
        &self,
        function_id: &FunctionId,
        this_arg: &TypedExpr,
        fallback: ValueInfo,
    ) -> ValueInfo {
        if self.function_target_is_strict(function_id) {
            return this_arg.value_info();
        }
        if this_arg.possible_kinds.is_subset_of(
            KindSet::from_kind(ValueKind::Undefined).union(KindSet::from_kind(ValueKind::Null)),
        ) {
            self.global_this_info()
        } else {
            self.boxed_receiver_info_from_arg(this_arg)
                .unwrap_or(fallback)
        }
    }

    fn lookup_global_property(&self, name: &str) -> Option<ValueInfo> {
        self.global_properties
            .get(name)
            .filter(|info| info.proven_present)
            .map(|info| info.value_info.clone())
    }

    fn lookup_global_property_info(&self, name: &str) -> Option<&GlobalPropertyInfo> {
        self.global_properties.get(name)
    }

    fn global_property_is_proven_present(&self, name: &str) -> bool {
        self.global_properties
            .get(name)
            .is_some_and(|info| info.proven_present)
    }

    fn set_global_property_value_info(&mut self, name: String, info: ValueInfo) {
        self.set_global_property_value_info_with_source(
            name,
            info,
            GlobalPropertySource::GlobalWrite,
        );
    }

    fn set_global_property_value_info_with_source(
        &mut self,
        name: String,
        info: ValueInfo,
        source: GlobalPropertySource,
    ) {
        if self.global_properties.contains_key(&name) {
            if !Self::global_property_is_writable_name(&name) {
                return;
            }
            if let Some(existing) = self.global_properties.get_mut(&name) {
                existing.value_info = info.clone();
                existing.proven_present = true;
                existing.source = source;
            }
        } else {
            let configurable = !self.is_script_global_var_name(&name);
            self.global_properties.insert(
                name.clone(),
                GlobalPropertyInfo {
                    value_info: info.clone(),
                    proven_present: true,
                    configurable,
                    source,
                },
            );
        }
        let script_global_binding = self.script_global_var_binding_info(&name);
        self.record_binding_value_write(&name, script_global_binding.as_ref());
        self.set_script_global_var_value_info(&name, info.clone());
        self.record_nested_script_global_value_info(&name, info);
        // Every caller records a real write (all pass a write source), so a
        // final body may no longer fold this name. Past the writability
        // early-return above, non-writable bindings never reach this point.
        self.observed_script_global_writes.insert(name);
    }

    fn capture_pre_write_global_property_value(&self, name: &str) -> PreWriteGlobalPropertyValue {
        let value = Self::global_property_is_writable_name(name)
            .then(|| {
                self.global_properties
                    .get(name)
                    .map(|property| property.value_info.clone())
                    .or_else(|| self.script_global_var_value_info(name))
            })
            .flatten();
        PreWriteGlobalPropertyValue(value)
    }

    /// Merge a write which may be skipped with the global-property fact from
    /// before RHS lowering, without inventing presence for an absent property.
    fn merge_possible_global_property_value_info(
        &mut self,
        name: &str,
        before_write: PreWriteGlobalPropertyValue,
        info: ValueInfo,
    ) {
        if !Self::global_property_is_writable_name(name) {
            return;
        }
        let PreWriteGlobalPropertyValue(Some(before_write)) = before_write else {
            return;
        };
        let script_global_binding = self.script_global_var_binding_info(name);
        self.record_binding_value_write(name, script_global_binding.as_ref());
        let merged = self.merge_value_infos(before_write, info);
        if let Some(property) = self.global_properties.get_mut(name) {
            property.value_info = merged.clone();
            property.source = GlobalPropertySource::Merged;
        } else if self.is_script_global_var_name(name) {
            self.global_properties.insert(
                name.to_string(),
                GlobalPropertyInfo {
                    value_info: merged.clone(),
                    proven_present: true,
                    configurable: false,
                    source: GlobalPropertySource::Merged,
                },
            );
        }
        self.set_script_global_var_value_info(name, merged.clone());
        self.record_nested_script_global_value_info(name, merged);
    }

    fn invalidate_possible_global_property_value_info(&mut self, name: &str) {
        if !Self::global_property_is_writable_name(name) {
            return;
        }
        let is_script_global = self.is_script_global_var_name(name);
        let script_global_binding = self.script_global_var_binding_info(name);
        self.record_binding_value_write(name, script_global_binding.as_ref());
        let mut function_targets = FunctionTargetKnowledge::none();
        if let Some(property) = self.global_properties.get(name) {
            function_targets = function_targets.join(property.value_info.function_targets.clone());
        }
        if let Some(binding) = self.script_global_var_value_info(name) {
            function_targets = function_targets.join(binding.function_targets);
        }
        let mut info = unknown_runtime_value_info();
        info.function_targets = function_targets;
        info.widen_for_possible_replacement();
        if let Some(property) = self.global_properties.get_mut(name) {
            property.value_info = info.clone();
            property.source = GlobalPropertySource::Merged;
        } else {
            self.global_properties.insert(
                name.to_string(),
                GlobalPropertyInfo {
                    value_info: info.clone(),
                    proven_present: is_script_global,
                    configurable: !is_script_global,
                    source: GlobalPropertySource::Merged,
                },
            );
        }
        self.set_script_global_var_value_info(name, info.clone());
        self.record_nested_script_global_value_info(name, info);
    }

    fn invalidate_all_possible_global_property_value_infos(&mut self) {
        let mut names = self
            .global_properties
            .keys()
            .filter(|name| Self::global_property_is_writable_name(name))
            .cloned()
            .collect::<BTreeSet<_>>();
        names.extend(
            self.var_bindings
                .iter()
                .filter(|(_, binding)| binding.is_script_global)
                .map(|(name, _)| name.clone()),
        );
        for name in names {
            self.invalidate_possible_global_property_value_info(&name);
        }
    }

    /// Discard every flow fact which arbitrary source code can invalidate.
    ///
    /// Property-key coercion, accessors, and Proxy traps are calls even though
    /// they are not represented by an explicit call node in the source IR.
    /// Keeping this transaction in one place prevents those implicit calls
    /// from preserving a cache or prototype guard which an explicit call could
    /// have changed.
    fn visit_live_heap_shape_roots(
        &mut self,
        mut visit: impl FnMut(KindSet, &FunctionTargetKnowledge, &mut HeapShape) -> bool,
    ) {
        // This visitor can refine an existing shape or discard it. Its typed
        // input cannot manufacture a shape for an absent fact.
        fn visit_present(
            kinds: KindSet,
            targets: &FunctionTargetKnowledge,
            shape: &mut Option<Box<HeapShape>>,
            visit: &mut impl FnMut(KindSet, &FunctionTargetKnowledge, &mut HeapShape) -> bool,
        ) {
            if let Some(present) = shape.as_deref_mut() {
                if !visit(kinds, targets, present) {
                    *shape = None;
                }
            }
        }

        fn visit_signature(
            signature: &mut FunctionSignature,
            visit: &mut impl FnMut(KindSet, &FunctionTargetKnowledge, &mut HeapShape) -> bool,
        ) {
            signature.return_shape.visit_flow_sensitive(
                signature.return_possible_kinds,
                &signature.return_targets,
                visit,
            );
            visit_present(
                signature.constructor_instance.possible_kinds,
                &signature.constructor_instance.function_targets,
                &mut signature.constructor_instance.heap_shape,
                visit,
            );
            visit_present(
                signature.this_info.possible_kinds,
                &signature.this_info.function_targets,
                &mut signature.this_info.heap_shape,
                visit,
            );
            for param in &mut signature.params {
                visit_present(
                    param.possible_kinds,
                    &param.function_targets,
                    &mut param.heap_shape,
                    visit,
                );
            }
        }

        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                visit_present(
                    binding.possible_kinds,
                    &binding.function_targets,
                    &mut binding.heap_shape,
                    &mut visit,
                );
            }
        }
        for binding in self.var_bindings.values_mut() {
            visit_present(
                binding.possible_kinds,
                &binding.function_targets,
                &mut binding.heap_shape,
                &mut visit,
            );
        }
        self.global_properties.edit_matching(
            |_, property| property.value_info.heap_shape.is_some(),
            |_, property| {
                visit_present(
                    property.value_info.possible_kinds,
                    &property.value_info.function_targets,
                    &mut property.value_info.heap_shape,
                    &mut visit,
                )
            },
        );
        for info in self.well_known_symbol_prototype_properties.values_mut() {
            visit_present(
                info.possible_kinds,
                &info.function_targets,
                &mut info.heap_shape,
                &mut visit,
            );
        }
        for infos in [
            &mut self.nested_script_global_value_infos,
            &mut self.known_nested_script_global_value_infos,
        ] {
            for info in infos.values_mut() {
                visit_present(
                    info.possible_kinds,
                    &info.function_targets,
                    &mut info.heap_shape,
                    &mut visit,
                );
            }
        }
        if let CurrentThisBinding::Activation(info) = &mut self.current_this_binding {
            visit_present(
                info.possible_kinds,
                &info.function_targets,
                &mut info.heap_shape,
                &mut visit,
            );
        }
        visit_present(
            self.current_new_target_info.possible_kinds,
            &self.current_new_target_info.function_targets,
            &mut self.current_new_target_info.heap_shape,
            &mut visit,
        );
        if let Some(info) = &mut self.current_construct_this_info {
            visit_present(
                info.possible_kinds,
                &info.function_targets,
                &mut info.heap_shape,
                &mut visit,
            );
        }
        if let Some(returned) = &mut self.current_return {
            returned.shape.visit_flow_sensitive(
                returned.info.possible_kinds,
                &returned.info.function_targets,
                &mut visit,
            );
            returned.info.heap_shape = returned.shape.cloned_shape();
        }
        if let Some(context) = &mut self.class_context {
            visit_present(
                KindSet::from_kind(ValueKind::Object),
                &FunctionTargetKnowledge::none(),
                &mut context.super_base_shape,
                &mut visit,
            );
        }
        if !self.is_prepass {
            if self.is_function_body {
                self.function_signature_shape_evidence =
                    FunctionSignatureShapeEvidence::Invalidated;
            } else {
                self.function_signatures.edit_matching(
                    |_, signature| signature.has_live_shape_roots(),
                    |_, signature| visit_signature(signature, &mut visit),
                );
                for signature in self.exact_context_function_observations.values_mut() {
                    visit_signature(signature, &mut visit);
                }
                for signature in self.exact_context_callback_observations.values_mut() {
                    visit_signature(signature, &mut visit);
                }
            }
        }
        for operand in self.pinned_async_operands.values_mut() {
            visit_present(
                operand.possible_kinds,
                &operand.function_targets,
                &mut operand.heap_shape,
                &mut visit,
            );
        }
    }

    fn invalidate_unknown_user_code_effects(&mut self) {
        self.observe_live_script_global_values();
        self.invalidate_unknown_user_code_effect_facts();
    }

    fn invalidate_unknown_user_code_effect_facts(&mut self) {
        self.record_caller_flow_invalidation();
        self.unknown_user_code_effects_observed = true;
        self.unknown_user_code_effects_introduced = true;
        let unknown = unknown_runtime_value_info();
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                // Const protects the binding, not the object it denotes.
                if binding.mode != BindingMode::Const
                    && self.captured_binding_names.contains(&binding.storage_name)
                {
                    binding.kind = unknown.kind;
                    binding.possible_kinds = unknown.possible_kinds;
                    binding.function_targets.widen_for_possible_replacement();
                }
            }
        }
        for (name, binding) in &mut self.var_bindings {
            if self.captured_binding_names.contains(name) {
                binding.kind = unknown.kind;
                binding.possible_kinds = unknown.possible_kinds;
                binding.function_targets.widen_for_possible_replacement();
            }
        }
        self.visit_live_heap_shape_roots(|_, _, _| false);

        self.invalidate_all_possible_global_property_value_infos();
        self.global_properties.edit_matching(
            |_, property| property.configurable && property.proven_present,
            |_, property| property.proven_present = false,
        );
        for infos in [
            &mut self.nested_script_global_value_infos,
            &mut self.known_nested_script_global_value_infos,
        ] {
            for info in infos.values_mut() {
                info.widen_for_possible_replacement();
            }
        }

        self.well_known_symbol_prototype_properties.clear();
        self.array_prototype_mutated = true;
        self.number_prototype_to_string_state = PrototypeToStringState::Unknown;
        self.boolean_prototype_to_string_state = PrototypeToStringState::Unknown;

        self.static_boolean_bindings.clear();
        self.static_string_bindings.clear();
        self.static_to_string_regexp_object_bindings.clear();
    }

    fn clear_static_binding_facts(&mut self, name: &str, binding: Option<&BindingInfo>) {
        self.static_boolean_bindings.remove(name);
        if let Some(binding) = binding {
            self.static_string_bindings.remove(binding);
        }
        self.static_to_string_regexp_object_bindings.remove(name);
    }

    fn record_binding_value_write(&mut self, name: &str, binding: Option<&BindingInfo>) {
        self.record_caller_flow_invalidation();
        self.invalidated_static_binding_names
            .insert(name.to_string());
        self.clear_static_binding_facts(name, binding);
    }

    fn record_caller_flow_invalidation(&mut self) {
        self.intervening_effect_epoch = self.intervening_effect_epoch.saturating_add(1);
        self.source_call_flow_effects = SourceCallFlowEffects::may_invalidate_caller_flow();
    }

    fn script_global_var_value_info(&self, name: &str) -> Option<ValueInfo> {
        self.var_bindings
            .get(name)
            .filter(|binding| binding.is_script_global)
            .map(|binding| ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            })
    }

    fn set_script_global_var_value_info(&mut self, name: &str, info: ValueInfo) {
        let Some(binding) = self
            .var_bindings
            .get_mut(name)
            .filter(|binding| binding.is_script_global)
        else {
            return;
        };
        binding.kind = info.kind;
        binding.possible_kinds = info.possible_kinds;
        binding.heap_shape = info.heap_shape;
        binding.function_targets = info.function_targets;
    }

    fn record_nested_script_global_value_info(&mut self, name: &str, info: ValueInfo) {
        if !self.is_nested_script_global_var_name(name) {
            return;
        }
        let merged = match self.nested_script_global_value_infos.remove(name) {
            Some(existing) => self.merge_value_infos(existing, info),
            None => info,
        };
        self.nested_script_global_value_infos
            .insert(name.to_string(), merged);
    }

    fn observe_live_script_global_values(&mut self) {
        if !self.is_prepass
            || self.script_global_call_observation_mode
                != ScriptGlobalCallObservationMode::ExecutedFlow
        {
            return;
        }
        let observed_values = self
            .var_bindings
            .iter()
            .filter(|(_, binding)| binding.is_script_global)
            .map(|(name, binding)| {
                (
                    name.clone(),
                    ValueInfo {
                        kind: binding.kind,
                        possible_kinds: binding.possible_kinds,
                        heap_shape: binding.heap_shape.clone(),
                        function_targets: binding.function_targets.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();
        for (name, observed) in observed_values {
            let merged = match self.called_script_global_value_infos.remove(&name) {
                Some(existing) => self.merge_value_infos(existing, observed),
                None => observed,
            };
            self.called_script_global_value_infos.insert(name, merged);
        }
    }

    fn global_property_is_writable_name(name: &str) -> bool {
        !matches!(name, "Infinity" | "NaN" | "undefined")
    }

    fn record_global_property_delete(&mut self, name: &str) {
        let script_global_binding = self.script_global_var_binding_info(name);
        self.record_binding_value_write(name, script_global_binding.as_ref());
        let is_script_global_declaration = script_global_binding.is_some()
            || (self.script_variables_are_global()
                && !self
                    .analysis
                    .module_execution
                    .is_private_dispatcher_name(name)
                && self.analysis.owner_plans[SCRIPT_OWNER_ID]
                    .function_bindings
                    .contains_key(name));
        if let Some(info) = self.global_properties.get_mut(name) {
            if is_script_global_declaration {
                // Prepared declarations can reuse an existing descriptor or
                // create a configurable eval binding. Let runtime deletion
                // decide whether the property survives.
                info.proven_present = false;
                info.source = GlobalPropertySource::Merged;
                self.observed_script_global_writes.insert(name.to_string());
            } else if info.proven_present && info.configurable {
                info.proven_present = false;
                info.source = GlobalPropertySource::DefinitelyDeleted;
                self.observed_script_global_writes.insert(name.to_string());
            }
        }
    }

    fn mark_host_builtin_from_function_id(&mut self, function_id: &str) {
        if let Some(builtin) = HostBuiltinId::from_function_id(function_id) {
            self.used_host_builtins.insert(builtin);
        }
    }

    fn mark_host_builtins_from_info(&mut self, info: &ValueInfo) {
        for function_id in info.function_targets.known_targets() {
            self.mark_host_builtin_from_function_id(function_id);
        }
    }

    fn lower_bigint_literal<T: std::fmt::Display>(&mut self, value: &T, negate: bool) -> TypedExpr {
        let text = value.to_string();
        let Some(value) = Self::parse_bigint_literal(&text, negate) else {
            return self.unsupported_expr("invalid BigInt literal");
        };
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::BigInt),
            ExprIr::BigInt(BigIntLiteralIr::from_bigint(value)),
        )
    }

    fn parse_bigint_literal(text: &str, negate: bool) -> Option<BigInt> {
        if text.starts_with('-') {
            return None;
        }
        let unsigned = text.parse::<BigUint>().ok()?;
        let value = BigInt::from_biguint(Sign::Plus, unsigned);
        Some(if negate { -value } else { value })
    }

    fn lower_identifier_name(&mut self, name: String, allow_with: bool) -> TypedExpr {
        if allow_with
            && !self
                .analysis
                .module_execution
                .is_private_dispatcher_name(&name)
        {
            let fallback = self.locate_identifier_reference(&name);
            if let Some(objects) = self
                .with_environment_chain
                .select_preceding(fallback.declarative_position())
            {
                let plan = self.with_environment_reference_plan(name.clone(), objects);
                let fallback = self.lower_identifier_name(name, false);
                return plan.get_value(fallback);
            }
        }

        // GetBindingValue (9.1.1.1.6). The state test used to be a disjunction
        // of a storage-name prefix and a parallel string set; it is now the one
        // field on the resolved record, and the `Uninitialized` arm has to be
        // written. 13.5.3 step 3 exempts only an *unresolvable* Reference, so
        // this covers `typeof x` too.
        let binding = match self.resolve_binding_reference(&name) {
            BindingResolution::Uninitialized(violation) => return violation.into_throw(),
            BindingResolution::Initialized(binding) => Some(binding),
            BindingResolution::Unresolvable => None,
        };
        if let Some(binding) = binding.filter(|_| !self.is_unshadowed_script_global_binding(&name))
        {
            let mut info = ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            };
            if self.current_param_names.contains(&binding.storage_name)
                && info.possible_kinds == KindSet::all_runtime_tags()
            {
                info = ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                };
            }
            return TypedExpr::from_info(info, ExprIr::Identifier(binding.storage_name));
        }

        if name == GLOBAL_THIS_NAME {
            // A current root data-property origin can prove this identity.
            // A dormant function or an invalidated property must instead
            // resolve the public binding and observe an accessor's effects.
            if self.current_owner_id == SCRIPT_OWNER_ID
                && self.identifier_resolves_to_intrinsic_global(&name)
            {
                return TypedExpr::from_info(
                    self.global_this_info(),
                    ExprIr::ExecutionGlobalObject,
                );
            }
            let mut info = unknown_runtime_value_info();
            if let Some(property) = self.lookup_global_property_info(&name) {
                if property.source != GlobalPropertySource::DefinitelyDeleted {
                    info.function_targets = property.value_info.function_targets.clone();
                }
            }
            // Public globalThis can itself hold a callable. Retain possible
            // targets for finite-source admission without proving the Get's
            // result or restoring the initial global-object identity.
            info.function_targets.widen_for_possible_replacement();
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
            self.mark_host_builtins_from_info(&info);
            return TypedExpr::from_info(info, ExprIr::GlobalIdentifierRead { name });
        }

        if let Some(host) = self.host_surface_policy.resolve_global(&name) {
            self.used_host_builtins.insert(host);
        }

        if self.is_unshadowed_script_global_binding(&name) {
            // A dormant function can run after deletion exposes an inherited
            // replacement. Its declaration-time tag and shape are not proof.
            let info = self
                .lookup_global_property(&name)
                .filter(|_| self.current_owner_id == SCRIPT_OWNER_ID)
                .unwrap_or_else(|| {
                    let mut info = unknown_runtime_value_info();
                    if let Some(property) = self.lookup_global_property_info(&name) {
                        if property.source != GlobalPropertySource::DefinitelyDeleted {
                            info.function_targets = property.value_info.function_targets.clone();
                            info.function_targets.widen_for_possible_replacement();
                        }
                    }
                    info
                });
            self.mark_host_builtins_from_info(&info);
            return TypedExpr::from_info(info, ExprIr::GlobalIdentifierRead { name });
        }
        if let Some(info) = self.lookup_global_property(&name) {
            self.mark_host_builtins_from_info(&info);
            return TypedExpr::from_info(info, ExprIr::GlobalPropertyRead { name });
        }

        if self.root_functions_need_body_initialization() {
            if let Some(function_id) = self.visible_function_names.get(&name).cloned() {
                return TypedExpr::from_info(
                    self.function_value_info(&function_id),
                    ExprIr::FunctionValue(function_id),
                );
            }
        }

        if name == "arguments" && self.lookup_binding(LEXICAL_ARGUMENTS_NAME).is_some() {
            return TypedExpr::from_info(
                ValueInfo {
                    kind: ValueKind::Arguments,
                    possible_kinds: KindSet::from_kind(ValueKind::Arguments),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::none(),
                },
                ExprIr::Arguments,
            );
        }

        if name == "Infinity" {
            return TypedExpr::from_info(
                ValueInfo::new(ValueKind::Number),
                ExprIr::Number(f64::INFINITY.to_bits()),
            );
        }
        if name == "NaN" {
            return TypedExpr::from_info(
                ValueInfo::new(ValueKind::Number),
                ExprIr::Number(f64::NAN.to_bits()),
            );
        }
        if name == "undefined" {
            return TypedExpr::undefined();
        }

        // Losing proof that a configurable global still exists does not prove
        // its previous callable disappeared. A definite deletion does exclude
        // that previous value until an intervening effect widens the property.
        // Keep possible targets while runtime still performs ResolveBinding.
        let mut function_targets = self
            .lookup_global_property_info(&name)
            .filter(|property| property.source != GlobalPropertySource::DefinitelyDeleted)
            .map_or_else(FunctionTargetKnowledge::unknown, |property| {
                property.value_info.function_targets.clone()
            });
        function_targets.widen_for_possible_replacement();
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets,
            },
            ExprIr::GlobalIdentifierRead { name },
        )
    }

    fn with_environment_reference_plan(
        &mut self,
        name: String,
        objects: SelectedWithEnvironmentObjects,
    ) -> WithEnvironmentReferencePlan {
        let strictness = self.reference_strictness();
        objects.into_reference_plan(name, strictness)
    }

    fn script_global_var_binding_info(&self, name: &str) -> Option<BindingInfo> {
        self.var_bindings
            .get(name)
            .filter(|binding| binding.is_script_global)
            .map(|binding| binding.to_binding_info(name))
    }

    /// Lower `expression`, first evaluating any operand of it that the spec
    /// orders before a suspension deeper in the same node.
    ///
    /// Hoisting an `await` into the statement prefix moves it ahead of
    /// everything that stays inline in the residual expression, so
    /// `f(a(), await g())` would call `g` before `a`. The operands that
    /// precede the suspension are therefore lowered here, bound to prefix
    /// temporaries, and recorded in `pinned_async_operands`; when the
    /// form-specific lowering below reaches the same AST node it gets the
    /// temporary back instead of re-evaluating it.
    ///
    /// Invocations use a completed Reference/argument-list owner before this
    /// generic operand pinner. Saving only a member receiver or raw key would
    /// leave GetValue incorrectly on the far side of a hoisted await.
    fn lower_expression(&mut self, expression: &Expression) -> TypedExpr {
        if let Some(entry) = self
            .analysis
            .module_entry_evaluation
            .filter(|entry| entry.owns(expression))
        {
            let evaluation = self
                .lower_synchronous_module_expression(entry.operand())
                .expect("trusted entry has a canonical evaluation operation");
            return entry.lower(evaluation);
        }
        if let Some(module) = self.lower_synchronous_module_expression(expression) {
            return module;
        }
        if let Some(pinned) = self
            .pinned_async_operands
            .get(&(std::ptr::from_ref(expression) as usize))
        {
            return TypedExpr::clone(pinned);
        }
        if self.async_expression_prefix.is_none()
            || !contains(expression, ContainsSymbol::AwaitExpression)
        {
            return self.lower_expression_with_pinned_operands(expression);
        }
        if let Some(invocation) = self.lower_suspended_invocation(expression) {
            return invocation;
        }
        if let Some(value) = self.lower_async_reference_expression(expression) {
            return value;
        }
        let pins = self.pin_async_operands_before_suspension(expression);
        let value = self.lower_expression_with_pinned_operands(expression);
        for key in pins {
            self.pinned_async_operands.remove(&key);
        }
        value
    }

    /// Evaluate, in source order, every operand of `expression` that precedes
    /// the last one containing an `await`, and pin the results.
    ///
    /// Only the operands *before* the last suspension need pinning: anything
    /// after it already lowers into the residual expression on the far side of
    /// the hoisted `await`, which is where the spec evaluates it.
    ///
    /// Returns the keys added to `pinned_async_operands`, for the caller to
    /// drop once the expression has been lowered.
    fn pin_async_operands_before_suspension(&mut self, expression: &Expression) -> Vec<usize> {
        let operands = Self::ordered_operands_for_pinning(expression);
        let Some(last_suspending) = operands
            .iter()
            .rposition(|operand| contains(*operand, ContainsSymbol::AwaitExpression))
        else {
            return Vec::new();
        };
        let mut pins = Vec::new();
        for operand in &operands[..last_suspending] {
            let key = std::ptr::from_ref(*operand) as usize;
            if self.pinned_async_operands.contains_key(&key) {
                continue;
            }
            let value = self.lower_expression(operand);
            let value = self.pin_async_operand_before_suspension(value, true, "async.operand.");
            self.pinned_async_operands.insert(key, value);
            pins.push(key);
        }
        pins
    }

    /// The operands of `expression` in the order the spec evaluates them.
    ///
    /// A method call yields the *receiver* rather than the whole callee, so
    /// pinning it leaves the property lookup — and with it the `this` binding
    /// — attached to the pinned receiver instead of collapsing the call into a
    /// plain function value.
    ///
    /// Conditional, logical and bounded optional Property/Call values own their
    /// separate prefixes and result binding inside the existing
    /// plain async If dispatcher, so they do not use this eager operand list.
    /// Forms already handled where they are lowered — array literals,
    /// arithmetic, template literals — are absent too, and forms
    /// that are not listed simply get no pinning, which is what happened
    /// before this existed.
    fn ordered_operands_for_pinning(expression: &Expression) -> Vec<&Expression> {
        match expression {
            Expression::Assign(assignment) if assignment.op() == AssignOp::Assign => {
                let AssignTarget::Access(PropertyAccess::Simple(access)) = assignment.lhs() else {
                    return Vec::new();
                };
                // Retain GetValue of the base and raw key before a later
                // await. The ordinary Reference still owns ToPropertyKey and
                // PutValue after the RHS, including its resumed value.
                let mut operands = vec![access.target()];
                if let PropertyAccessField::Expr(key) = access.field() {
                    operands.push(key);
                }
                operands.push(assignment.rhs());
                operands
            }
            // Invocation owners complete the Reference and ArgumentListEvaluation
            // before this generic operand pinner is entered.
            Expression::Call(_) | Expression::New(_) | Expression::TaggedTemplate(_) => Vec::new(),
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => match access.field() {
                PropertyAccessField::Expr(key) => vec![access.target(), key],
                PropertyAccessField::Const(_) => Vec::new(),
            },
            Expression::ImportCall(call) => std::iter::once(call.argument())
                .chain(call.options())
                .collect(),
            Expression::ObjectLiteral(object) => object
                .properties()
                .iter()
                .filter_map(|property| match property {
                    PropertyDefinition::Property(_, value) => Some(value),
                    PropertyDefinition::SpreadObject(source) => Some(source),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn lower_expression_with_pinned_operands(&mut self, expression: &Expression) -> TypedExpr {
        match expression {
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if self.uses_runtime_identifier_environment_for(&name)
                    && !self
                        .analysis
                        .module_execution
                        .is_private_dispatcher_name(&name)
                {
                    self.environment_identifier(name, EnvironmentIdentifierOperationIr::Read)
                } else {
                    self.lower_identifier_name(name, true)
                }
            }
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::String(sym) => {
                    let value = self.interned_runtime_string(*sym);
                    TypedExpr::from_info(Self::string_value_info(&value), ExprIr::String(value))
                }
                LiteralKind::Num(value) => TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Number,
                        possible_kinds: KindSet::from_kind(ValueKind::Number),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::Number(value.to_bits()),
                ),
                LiteralKind::Int(value) => TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Number,
                        possible_kinds: KindSet::from_kind(ValueKind::Number),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::Number((*value as f64).to_bits()),
                ),
                LiteralKind::Bool(value) => TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Boolean,
                        possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::Boolean(*value),
                ),
                LiteralKind::Null => TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Null,
                        possible_kinds: KindSet::from_kind(ValueKind::Null),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::Null,
                ),
                LiteralKind::Undefined => TypedExpr::undefined(),
                LiteralKind::BigInt(value) => self.lower_bigint_literal(value.as_ref(), false),
            },
            Expression::Parenthesized(expression) => self.lower_expression(expression.expression()),
            Expression::ArrayLiteral(array) => self.lower_array_literal(array),
            Expression::ObjectLiteral(object) => self.lower_object_literal(object),
            Expression::Unary(unary) => self.lower_unary(unary.op(), unary.target()),
            Expression::Binary(binary) => {
                self.lower_binary(binary.op(), binary.lhs(), binary.rhs())
            }
            Expression::Assign(assign) => {
                self.lower_assign(assign.op(), assign.lhs(), assign.rhs())
            }
            Expression::Call(call) => self.lower_call(call.function(), call.args()),
            Expression::New(new_expr) => self.lower_new(new_expr),
            Expression::FunctionExpression(function) => self.lower_function_expression(function),
            Expression::GeneratorExpression(generator) => {
                self.lower_generator_expression(generator)
            }
            Expression::AsyncFunctionExpression(function) => {
                self.lower_async_function_expression(function)
            }
            Expression::AsyncGeneratorExpression(function) => {
                self.lower_async_generator_expression(function)
            }
            Expression::ArrowFunction(function) => self.lower_arrow_function(function),
            Expression::AsyncArrowFunction(function) => self.lower_async_arrow_function(function),
            Expression::ClassExpression(class) => self.lower_class_expression(class),
            Expression::PropertyAccess(access) => self.lower_property_access(access),
            Expression::Optional(optional) => self.lower_optional_property_chain(optional),
            Expression::SuperCall(call) => self.lower_super_call(call),
            Expression::This(_) => self.lower_current_this(),
            Expression::NewTarget(_) => {
                if !self.is_function_body
                    && !matches!(
                        self.direct_eval_invocation(),
                        Some(
                            scriptw_front::EvalInvocationContext::Function
                                | scriptw_front::EvalInvocationContext::Method
                                | scriptw_front::EvalInvocationContext::DerivedConstructor
                                | scriptw_front::EvalInvocationContext::ClassFieldInitializer
                        )
                    )
                {
                    return self.unsupported_expr("unsupported expression form: NewTarget");
                }
                TypedExpr::from_info(self.current_new_target_info.clone(), ExprIr::NewTarget)
            }
            Expression::Await(await_expression)
                if self.current_async_resume_state.is_some()
                    && self.async_expression_prefix.is_some() =>
            {
                let result_name = self.alloc_suspension_owned_binding(
                    "async.await.",
                    ValueInfo {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    },
                );
                self.async_expression_prefix
                    .as_mut()
                    .expect("async expression lowering must have a statement prefix")
                    .push(StatementIr::Lexical {
                        mode: BindingMode::Let,
                        name: result_name.clone(),
                        init: TypedExpr::undefined(),
                    });
                let (await_statement, _) = self.lower_linear_async_await(
                    await_expression.target(),
                    AsyncResumeModeIr::AssignIdentifier(result_name.clone()),
                );
                self.async_expression_prefix
                    .as_mut()
                    .expect("async expression lowering must have a statement prefix")
                    .push(await_statement);
                self.lower_identifier_name(result_name, false)
            }
            Expression::RegExpLiteral(regexp) => self.lower_regexp_literal(regexp),
            Expression::ImportCall(call) => {
                // 13.3.10.1 steps 3-6: both operands are evaluated before the
                // promise capability exists, so an abrupt completion here
                // throws normally. No `ToString`: steps 8-9 do that, after the
                // capability, where it rejects instead.
                let specifier = self.lower_expression(call.argument());
                let options = call.options().map(|options| self.lower_expression(options));
                let referrer = self.prepared_literal_import_referrer(call);
                match modules::lower_import_call(call, specifier, options, referrer) {
                    Ok(expression) => expression,
                    Err(message) => {
                        self.unsupported_with_message(message);
                        TypedExpr::undefined()
                    }
                }
            }
            Expression::Spread(_)
            | Expression::ImportMeta(_)
            | Expression::Await(_)
            | Expression::Yield(_)
            | Expression::FormalParameterList(_)
            | Expression::Debugger => {
                self.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot first slice: unsupported expression form: {expression:?}"
                ));
                TypedExpr::undefined()
            }
            Expression::TemplateLiteral(template) => self.lower_template_literal(template),
            Expression::TaggedTemplate(template) => self.lower_tagged_template(template),
            Expression::BinaryInPrivate(binary) => self.lower_private_in(binary),
            Expression::Conditional(conditional)
                if self.async_expression_prefix.is_some()
                    && self.has_plain_async_value_branch_owner()
                    && (contains(conditional.if_true(), ContainsSymbol::AwaitExpression)
                        || contains(conditional.if_false(), ContainsSymbol::AwaitExpression)) =>
            {
                self.lower_conditional_await_value(conditional)
            }
            Expression::Conditional(conditional) => {
                let condition = self.lower_expression(conditional.condition());
                let before_branch = self.capture_conditional_flow_facts();
                let then_expr = self.lower_expression(conditional.if_true());
                let then_facts = self.capture_conditional_flow_facts();
                self.install_conditional_flow_facts(before_branch);
                let else_expr = self.lower_expression(conditional.if_false());
                let else_facts = self.capture_conditional_flow_facts();
                self.merge_conditional_flow_facts(then_facts, else_facts);
                let info = self.merge_value_infos(then_expr.value_info(), else_expr.value_info());
                TypedExpr::from_info(
                    info,
                    ExprIr::Conditional {
                        condition: Box::new(condition),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    },
                )
            }
            Expression::Update(update) => self.lower_update(update.op(), update.target()),
        }
    }

    fn lower_function_expression(&mut self, function: &FunctionExpression) -> TypedExpr {
        let key = function_expression_key(function);
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            return self.unsupported_expr("function expression");
        };
        TypedExpr::from_info(
            self.function_value_info(&function_id),
            ExprIr::FunctionValue(function_id),
        )
    }

    fn lower_generator_expression(&mut self, generator: &GeneratorExpression) -> TypedExpr {
        if generator_function_is_aot_supported(generator.body(), generator.parameters()) {
            let key = generator_expression_key(generator);
            let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
                return self.unsupported_expr("generator expression");
            };
            return self.function_value_expr(function_id);
        }

        self.unsupported_expr("generator suspension")
    }

    fn lower_async_function_expression(&mut self, function: &AsyncFunctionExpression) -> TypedExpr {
        let key = async_function_expression_key(function);
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            return self.unsupported_expr("async function expression");
        };
        self.function_value_expr(function_id)
    }

    fn lower_async_generator_expression(
        &mut self,
        function: &AsyncGeneratorExpression,
    ) -> TypedExpr {
        let key = async_generator_expression_key(function);
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            return self.unsupported_expr("async generator expression");
        };
        self.function_value_expr(function_id)
    }

    fn lower_template_literal(&mut self, template: &TemplateLiteral) -> TypedExpr {
        let mut lowered: Option<TypedExpr> = None;
        let elements = template.elements();
        for (index, element) in elements.iter().enumerate() {
            // A later substitution that suspends would otherwise be awaited
            // before this part is even evaluated (13.2.8.6 evaluates the
            // substitutions left to right).
            let rest_suspends = elements[index + 1..].iter().any(|element| match element {
                TemplateElement::Expr(expression) => {
                    contains(expression, ContainsSymbol::AwaitExpression)
                }
                TemplateElement::String(_) => false,
            });
            let part = match element {
                TemplateElement::String(sym) => {
                    let value = self.interned_runtime_string(*sym);
                    TypedExpr::from_info(ValueInfo::new(ValueKind::String), ExprIr::String(value))
                }
                // 13.2.8.6: each substitution is ToString'd (hint String)
                // before the next one is evaluated; `+` would use hint Default.
                TemplateElement::Expr(expr) => {
                    TypedExpr::spec_to_string(self.lower_expression(expr))
                }
            };
            let part =
                self.pin_async_operand_before_suspension(part, rest_suspends, "async.template.");
            lowered = Some(if let Some(lhs) = lowered {
                TypedExpr::from_info(
                    ValueInfo::new(ValueKind::String),
                    ExprIr::StringConcat {
                        lhs: Box::new(lhs),
                        rhs: Box::new(part),
                    },
                )
            } else {
                part
            });
        }
        lowered.unwrap_or_else(|| {
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::String),
                ExprIr::String(String::new()),
            )
        })
    }

    fn lower_template_object(&mut self, template: &TaggedTemplate) -> TypedExpr {
        let raw = template
            .raws()
            .iter()
            .map(|value| self.interned_runtime_string(*value))
            .collect::<Vec<_>>();
        let cooked = template
            .cookeds()
            .iter()
            .map(|value| value.map(|value| self.interned_runtime_string(value)))
            .collect::<Vec<_>>();
        let template_info = ValueInfo {
            kind: ValueKind::Array,
            possible_kinds: KindSet::from_kind(ValueKind::Array),
            heap_shape: Some(Box::new(HeapShape::Array(ArrayShape::default()))),
            function_targets: FunctionTargetKnowledge::none(),
        };
        TypedExpr::from_info(
            template_info,
            ExprIr::TemplateObject(TemplateObjectIr {
                site_id: self
                    .analysis
                    .template_source
                    .expect("template lowering requires its actual parsed Script owner")
                    .site(template.identifier()),
                cooked,
                raw,
            }),
        )
    }

    fn lower_tagged_template(&mut self, template: &TaggedTemplate) -> TypedExpr {
        let mut args = Vec::with_capacity(template.exprs().len() + 1);
        args.push(self.lower_template_object(template));
        args.extend(
            template
                .exprs()
                .iter()
                .map(|expression| self.lower_expression(expression)),
        );

        let result_info = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::all_runtime_tags(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        let result = match template.tag() {
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                let receiver = self.lower_property_target(access.target());
                let key = match access.field() {
                    PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                        self.interner.resolve_expect(name.sym()).to_string(),
                    ),
                    PropertyAccessField::Expr(expression) => {
                        let Some(key) = self.lower_dynamic_object_property_key(expression) else {
                            return self.unsupported_expr("tagged template property key");
                        };
                        key
                    }
                };
                TypedExpr::from_info(
                    result_info,
                    ExprIr::CallMethod {
                        receiver: Box::new(receiver),
                        key,
                        args,
                    },
                )
            }
            tag => TypedExpr::from_info(
                result_info,
                ExprIr::CallIndirect {
                    direct_eval: None,
                    callee: Box::new(self.lower_expression(tag)),
                    this_arg: None,
                    args,
                    static_regexp_compilation: None,
                },
            ),
        };
        self.invalidate_unknown_user_code_effects();
        result
    }

    fn lower_arrow_function(&mut self, function: &ArrowFunction) -> TypedExpr {
        let key = arrow_function_key(function);
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            return self.unsupported_expr("arrow function");
        };
        if !self.is_function_body {
            let captures_lexical_arguments = self
                .analysis
                .function_plans
                .get(&function_id)
                .is_some_and(|plan| plan.captures.contains_key(LEXICAL_ARGUMENTS_NAME));
            if captures_lexical_arguments {
                return self.unsupported_expr("top-level `arguments`");
            }
        }
        TypedExpr::from_info(
            self.function_value_info(&function_id),
            ExprIr::FunctionValue(function_id),
        )
    }

    fn lower_async_arrow_function(&mut self, function: &AsyncArrowFunction) -> TypedExpr {
        let key = async_arrow_function_key(function);
        let Some(function_id) = self.analysis.function_expr_ids.get(&key).cloned() else {
            return self.unsupported_expr("async arrow function");
        };
        if !self.is_function_body {
            let captures_lexical_arguments = self
                .analysis
                .function_plans
                .get(&function_id)
                .is_some_and(|plan| plan.captures.contains_key(LEXICAL_ARGUMENTS_NAME));
            if captures_lexical_arguments {
                return self.unsupported_expr("top-level `arguments`");
            }
        }
        TypedExpr::from_info(
            self.function_value_info(&function_id),
            ExprIr::FunctionValue(function_id),
        )
    }

    fn lower_class_declaration(
        &mut self,
        class: &ClassDeclaration,
        scope: &mut LexicalScopeInstantiation,
    ) -> (StatementIr, ValueKind) {
        let name = self.interner.resolve_expect(class.name().sym()).to_string();
        let pending = scope.take(&name);
        // Allocated here, before ClassDefinitionEvaluation, exactly as before:
        // `direct_lexical_storage_name` can consume a `$lexN` temporary index,
        // and moving that consumption past `lower_class_common` would renumber
        // every generated name inside the class body. When `pending` is `Some`
        // the reuse rule returns the created name without allocating anything.
        let storage_name = self.direct_lexical_storage_name(&name, class.name().span());
        let constructor_execution_key = class
            .constructor()
            .map(class_constructor_key)
            .unwrap_or_else(|| class_default_constructor_key(class.linear_span()));
        // 15.7.16 step 1: ClassDefinitionEvaluation runs *before*
        // InitializeBoundName, so the class body evaluates while the class name
        // is still uninitialized in the enclosing scope. That is why the
        // `LoweredInitializer` is minted from the already-computed result rather
        // than the binding being initialized first.
        // Discover before evaluating heritage/element effects, but install only
        // after InitializeBoundName. The resolved outer cell retains the hint;
        // class-name and shadowing cells cannot borrow it by spelling.
        let source_candidates = self
            .finite_synthetic_class_constructor_candidates(class.super_ref(), class.constructor());
        let enclosing_prefix = self.async_expression_prefix.replace(Vec::new());
        let init = self.lower_class_common(
            SourceClassName::from_declaration(class, self.interner),
            self.analysis
                .module_execution
                .original_callable_source(class.linear_span())
                .map(str::to_owned)
                .unwrap_or_else(|| class_declaration_source_slice(class, self.source_text)),
            constructor_execution_key,
            class.super_ref(),
            class.constructor(),
            class.elements(),
            ClassNameInferenceIr::None,
        );
        let mut statements = std::mem::replace(&mut self.async_expression_prefix, enclosing_prefix)
            .expect("class declaration owns its evaluation prefix");
        // 15.7.16 step 2: InitializeBoundName(className, value, env). In the
        // `Some` arm the outer `storage_name` computed above is deliberately
        // *not* referenced: the token carries the name the creation allocated,
        // and there is no `String` here to substitute for it.
        let init = LoweredInitializer::evaluated(init);
        let initialized = match pending {
            Some(pending) => pending.initialize(init),
            None => InitializedBinding::without_creation(
                name.clone(),
                BindingMode::Let,
                storage_name,
                init.into_expr(),
            ),
        };
        let declaration = initialized.declare(self);
        if !source_candidates.is_empty() {
            self.set_finite_source_binding_candidates(&name, source_candidates);
        }
        if statements.is_empty() {
            (declaration, ValueKind::Undefined)
        } else {
            statements.push(declaration);
            (StatementIr::LexicalBlock(statements), ValueKind::Undefined)
        }
    }

    fn lower_class_expression(&mut self, class: &ClassExpression) -> TypedExpr {
        self.lower_class_expression_with_inferred_name(class, ClassNameInferenceIr::None)
    }

    fn lower_class_expression_with_inferred_name(
        &mut self,
        class: &ClassExpression,
        name_inference: ClassNameInferenceIr,
    ) -> TypedExpr {
        let constructor_execution_key = class
            .constructor()
            .map(class_constructor_key)
            .unwrap_or_else(|| class_default_constructor_key(class.linear_span()));
        self.lower_class_common(
            SourceClassName::from_expression(class, self.interner),
            self.analysis
                .module_execution
                .original_callable_source(class.linear_span())
                .map(str::to_owned)
                .unwrap_or_else(|| class_expression_source_slice(class, self.source_text)),
            constructor_execution_key,
            class.super_ref(),
            class.constructor(),
            class.elements(),
            name_inference,
        )
    }

    fn insert_class_method_shape(
        properties: &mut BTreeMap<String, ObjectShapeProperty>,
        key: String,
        function_id: FunctionId,
        kind: ClassMethodKindIr,
    ) {
        let previous = properties.remove(&key);
        let (getter, setter) = match previous {
            Some(ObjectShapeProperty::Accessor { getter, setter }) => (getter, setter),
            _ => (None, None),
        };
        let property = match kind {
            ClassMethodKindIr::Method => ObjectShapeProperty::Data(
                Self::function_value_info_with_constructable(function_id, false),
            ),
            ClassMethodKindIr::Getter => ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape { function_id }),
                setter,
            },
            ClassMethodKindIr::Setter => ObjectShapeProperty::Accessor {
                getter,
                setter: Some(ObjectAccessorShape { function_id }),
            },
        };
        properties.insert(key, property);
    }

    fn lower_class_common(
        &mut self,
        class_name: SourceClassName,
        class_source: String,
        constructor_execution_key: String,
        heritage: Option<&Expression>,
        constructor: Option<&FunctionExpression>,
        elements: &[ClassElement],
        name_inference: ClassNameInferenceIr,
    ) -> TypedExpr {
        let name_binding = class_name.binding_name().map(|_| {
            let environment_id = self
                .analysis
                .class_name_environment_ids
                .get(&constructor_execution_key)
                .copied()
                .unwrap_or_else(|| {
                    panic!(
                        "named class execution `{constructor_execution_key}` must have a lexical environment"
                    )
                });
            let environment = self
                .lower_runtime_lexical_environment(Some(environment_id))
                .expect("named class environment must have runtime storage");
            let storage_name = environment
                .bindings
                .first()
                .expect("named class environment must own its binding")
                .name
                .clone();
            ClassNameBindingIr {
                storage_name,
                environment,
            }
        });
        if let (Some(source_name), Some(binding)) = (class_name.binding_name(), &name_binding) {
            self.push_scope();
            self.declare_binding(
                source_name.to_owned(),
                BindingInfo {
                    mode: BindingMode::Const,
                    storage_name: binding.storage_name.clone(),
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                    initialization: Initialization::Initialized,
                },
            );
        }
        let has_name_binding = name_binding.is_some();
        // Class heritage and computed element names are strict code even when
        // their evaluation stays in the enclosing sloppy function activation.
        let enclosing_strictness = self.lexical_strictness.replace(Strictness::Strict);
        let lowered = self.lower_class_common_in_name_scope(
            class_name.into_label(),
            class_source,
            constructor_execution_key,
            heritage,
            constructor,
            elements,
            name_binding,
            name_inference,
        );
        self.lexical_strictness = enclosing_strictness;
        if has_name_binding {
            self.pop_scope();
        }
        lowered
    }

    fn capture_generated_class_element_flow(
        execution_kind: ClassElementExecutionKind,
        lowerer: &mut ScriptLowerer<'_>,
        captured_parent_values: Vec<(String, ValueInfo)>,
    ) -> GeneratedClassElementFlow {
        match execution_kind {
            ClassElementExecutionKind::None
            | ClassElementExecutionKind::InstanceFieldInitializer => {
                debug_assert!(captured_parent_values.is_empty());
                GeneratedClassElementFlow::Deferred
            }
            ClassElementExecutionKind::StaticFieldInitializer
            | ClassElementExecutionKind::StaticBlock => {
                let script_global_values = lowerer
                    .var_bindings
                    .iter()
                    .filter(|(_, binding)| binding.is_script_global)
                    .map(|(name, binding)| {
                        (
                            name.clone(),
                            ValueInfo {
                                kind: binding.kind,
                                possible_kinds: binding.possible_kinds,
                                heap_shape: binding.heap_shape.clone(),
                                function_targets: binding.function_targets.clone(),
                            },
                        )
                    })
                    .collect();
                GeneratedClassElementFlow::Executed(ExecutedClassElementPostState {
                    global_properties: std::mem::take(&mut lowerer.global_properties),
                    well_known_symbol_prototype_properties: std::mem::take(
                        &mut lowerer.well_known_symbol_prototype_properties,
                    ),
                    script_global_values,
                    captured_parent_values,
                    invalidated_static_binding_names: std::mem::take(
                        &mut lowerer.invalidated_static_binding_names,
                    ),
                    boolean_alias_shapes_invalidated: lowerer.boolean_alias_shapes_invalidated,
                    array_prototype_mutated: lowerer.array_prototype_mutated,
                    number_prototype_to_string_state: lowerer.number_prototype_to_string_state,
                    boolean_prototype_to_string_state: lowerer.boolean_prototype_to_string_state,
                    unknown_user_code_effects_observed: lowerer.unknown_user_code_effects_observed,
                    unknown_user_code_effects_introduced: lowerer
                        .unknown_user_code_effects_introduced,
                    intervening_effect_epoch: lowerer.intervening_effect_epoch,
                    function_signature_shape_evidence: lowerer.function_signature_shape_evidence,
                    source_call_flow_effects: lowerer.source_call_flow_effects,
                })
            }
        }
    }

    fn executed_class_receiver_post_state(
        &self,
        execution_kind: ClassElementExecutionKind,
        entry_info: &ValueInfo,
        this_post_state: Option<ValueInfo>,
        class_name_post_state: Option<ValueInfo>,
    ) -> Option<ValueInfo> {
        if !matches!(
            execution_kind,
            ClassElementExecutionKind::StaticFieldInitializer
                | ClassElementExecutionKind::StaticBlock
        ) {
            return this_post_state;
        }
        let this_post_state = this_post_state.unwrap_or_else(|| entry_info.clone());
        let class_name_post_state = class_name_post_state.unwrap_or_else(|| entry_info.clone());
        let this_changed = &this_post_state != entry_info;
        let class_name_changed = &class_name_post_state != entry_info;
        match (
            this_post_state.heap_shape.is_some(),
            class_name_post_state.heap_shape.is_some(),
        ) {
            (true, false) => return Some(this_post_state),
            (false, true) => return Some(class_name_post_state),
            (false, false) => {
                return Some(self.merge_value_infos(this_post_state, class_name_post_state));
            }
            (true, true) => {}
        }
        match (this_changed, class_name_changed) {
            (true, false) => Some(this_post_state),
            (false, true) => Some(class_name_post_state),
            (false, false) => Some(entry_info.clone()),
            (true, true) if this_post_state == class_name_post_state => Some(this_post_state),
            (true, true) => {
                let mut merged = self.merge_value_infos(this_post_state, class_name_post_state);
                merged.heap_shape = None;
                Some(merged)
            }
        }
    }

    fn install_generated_class_element_flow(&mut self, flow: GeneratedClassElementFlow) {
        let GeneratedClassElementFlow::Executed(post_state) = flow else {
            return;
        };
        if post_state.unknown_user_code_effects_introduced {
            self.invalidate_unknown_user_code_effect_facts();
        } else if post_state.boolean_alias_shapes_invalidated {
            self.invalidate_all_static_boolean_alias_shapes();
        }
        self.invalidated_static_binding_names
            .extend(post_state.invalidated_static_binding_names.iter().cloned());
        for name in &post_state.invalidated_static_binding_names {
            let binding = self
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(name).cloned())
                .or_else(|| {
                    self.var_bindings
                        .get(name)
                        .map(|binding| binding.to_binding_info(name))
                });
            self.clear_static_binding_facts(name, binding.as_ref());
        }
        for (name, info) in &post_state.script_global_values {
            self.set_script_global_var_value_info(name, info.clone());
        }
        self.global_properties = post_state.global_properties;
        self.well_known_symbol_prototype_properties =
            post_state.well_known_symbol_prototype_properties;
        self.array_prototype_mutated = post_state.array_prototype_mutated;
        self.number_prototype_to_string_state = post_state.number_prototype_to_string_state;
        self.boolean_prototype_to_string_state = post_state.boolean_prototype_to_string_state;
        for (source_name, captured_info) in post_state.captured_parent_values {
            self.install_binding_value_info(&source_name, captured_info.clone());
            if self.is_script_global_var_name(&source_name) {
                if let Some(property) = self.global_properties.get_mut(&source_name) {
                    property.value_info = captured_info;
                    property.proven_present = true;
                    property.source = GlobalPropertySource::GlobalWrite;
                }
                self.observed_script_global_writes
                    .insert(source_name.clone());
            }
        }
        self.unknown_user_code_effects_observed = post_state.unknown_user_code_effects_observed;
        self.function_signature_shape_evidence = self
            .function_signature_shape_evidence
            .join(post_state.function_signature_shape_evidence);
        self.source_call_flow_effects = self
            .source_call_flow_effects
            .combine_caller_flow(post_state.source_call_flow_effects);
        if !post_state.unknown_user_code_effects_introduced
            && post_state.intervening_effect_epoch > 0
        {
            self.intervening_effect_epoch = self.intervening_effect_epoch.saturating_add(1);
        }
    }

    fn mark_generated_function_as_generator(
        &mut self,
        function_id: &FunctionId,
        generator_plan: GeneratorPlanIr,
    ) {
        let function = self
            .generated_functions
            .iter_mut()
            .find(|function| &function.id == function_id)
            .unwrap_or_else(|| panic!("generated class method `{function_id}` must be lowered"));
        let generator_plan = if function.eval_environment.is_some() {
            crate::generator_loop_control::sequence_end(&function.body.statements, 0)
                .ok()
                .map(|exit| {
                    let mut points = Vec::new();
                    crate::generator_loop_control::collect_suspensions(
                        &function.body.statements,
                        &mut points,
                    );
                    finish_generator_plan(exit, points)
                        .expect("checked generator region exit fits its state domain")
                })
                .unwrap_or(generator_plan)
        } else {
            generator_plan
        };
        function.generator_plan = Some(generator_plan);
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_generated_ast_function(
        &mut self,
        function_id: FunctionId,
        function_name: String,
        to_string_representation: CallableToStringRepresentation,
        parameters: &FormalParameterList,
        body: &FunctionBody,
        protocol: FunctionProtocolIr,
        callable: bool,
        class_element_execution_kind: ClassElementExecutionKind,
        current_this_info: ValueInfo,
        current_construct_this_info: Option<ValueInfo>,
        self_binding_name: Option<String>,
        uses_super: bool,
        private_name_ids: BTreeMap<String, PrivateNameId>,
        class_context: ClassLoweringContext,
        prefix_statements: Vec<StatementIr>,
    ) -> GeneratedFunctionOutput {
        let execution_kind = protocol.execution_kind();
        let class_kind = protocol.class_kind();
        let constructable = protocol.is_constructable();
        let resumable_plan = if execution_kind == FunctionExecutionKind::AsyncGenerator {
            match async_generator_resumable_plan(body) {
                Ok(plan) => Some(plan),
                Err(error) => {
                    self.unsupported_with_message(format!(
                        "unsupported in scriptw wasm-aot: async-generator source graph: {error:?}"
                    ));
                    None
                }
            }
        } else {
            None
        };
        self.function_signatures.insert(
            function_id.clone(),
            FunctionSignature {
                id: function_id.clone(),
                to_string_representation: to_string_representation.clone(),
                protocol,
                callable,
                class_heritage_kind: class_context.heritage_kind,
                params: parameters
                    .as_ref()
                    .iter()
                    .map(|parameter| FunctionParamSignature {
                        kind: if parameter.is_rest_param() {
                            ValueKind::Array
                        } else {
                            ValueKind::Dynamic
                        },
                        possible_kinds: if parameter.is_rest_param() {
                            KindSet::from_kind(ValueKind::Array)
                        } else {
                            KindSet::all_runtime_tags()
                        },
                        heap_shape: None,
                        function_targets: if parameter.is_rest_param() {
                            FunctionTargetKnowledge::none()
                        } else {
                            FunctionTargetKnowledge::unknown()
                        },
                        observed: parameter.is_rest_param(),
                        has_default: parameter.init().is_some(),
                        is_rest: parameter.is_rest_param(),
                    })
                    .collect(),
                return_kind: ValueKind::Dynamic,
                return_possible_kinds: KindSet::all_runtime_tags(),
                return_shape: FunctionReturnShape::Unobserved,
                return_targets: FunctionTargetKnowledge::unknown(),
                constructor_instance: current_construct_this_info
                    .clone()
                    .unwrap_or_else(ValueInfo::undefined),
                this_info: current_this_info.clone(),
                this_observed: true,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );

        let mut current_new_target_info =
            Self::function_value_info_with_constructable(function_id.clone(), constructable);
        if class_kind == ClassFunctionKind::Constructor {
            let prototype_info = current_this_info
                .heap_shape
                .as_deref()
                .and_then(|shape| match shape {
                    HeapShape::Object(object) => object.prototype.clone(),
                    HeapShape::Array(array) => array.prototype.clone(),
                })
                .map(|shape| ValueInfo {
                    kind: ValueKind::Object,
                    possible_kinds: KindSet::from_kind(ValueKind::Object),
                    heap_shape: Some(shape),
                    function_targets: FunctionTargetKnowledge::unknown(),
                })
                .unwrap_or_else(Self::fresh_constructed_instance_info);
            if let Some(HeapShape::Object(shape)) =
                current_new_target_info.heap_shape.as_mut().map(Box::as_mut)
            {
                shape.properties.insert(
                    "prototype".to_string(),
                    ObjectShapeProperty::Data(prototype_info),
                );
            }
        } else {
            current_new_target_info =
                self.merge_value_infos(ValueInfo::undefined(), current_new_target_info);
        }

        let mut lowerer = ScriptLowerer::new(
            self.interner,
            self.analysis,
            self.source_text,
            self.root_this_binding,
            function_id.clone(),
            self.host_surface_policy,
        );
        lowerer.function_signatures = std::mem::take(&mut self.function_signatures);
        lowerer.visible_function_names = self.visible_function_names.clone();
        lowerer.global_properties = self.global_properties.clone();
        lowerer.observed_script_global_writes = self.observed_script_global_writes.clone();
        lowerer.well_known_symbol_prototype_properties =
            self.well_known_symbol_prototype_properties.clone();
        lowerer.array_prototype_mutated = self.array_prototype_mutated;
        lowerer.number_prototype_to_string_state = self.number_prototype_to_string_state;
        lowerer.boolean_prototype_to_string_state = self.boolean_prototype_to_string_state;
        lowerer.dynamically_installed_getters = self.dynamically_installed_getters.clone();
        lowerer.dynamically_installed_setters = self.dynamically_installed_setters.clone();
        lowerer.unknown_user_code_effects_observed = self.unknown_user_code_effects_observed;
        lowerer.function_signature_shape_evidence = self.function_signature_shape_evidence;
        lowerer.static_boolean_bindings = self.static_boolean_bindings.clone();
        lowerer.static_string_bindings = self.static_string_bindings.clone();
        lowerer.function_source_binding_candidates =
            self.function_source_binding_candidates.clone();
        lowerer.function_source_parameter_candidates =
            self.function_source_parameter_candidates.clone();
        lowerer.static_to_string_regexp_object_bindings =
            self.static_to_string_regexp_object_bindings.clone();
        lowerer.var_bindings = self.var_bindings.clone();
        lowerer.known_nested_script_global_value_infos =
            self.known_nested_script_global_value_infos.clone();
        match class_element_execution_kind {
            ClassElementExecutionKind::StaticFieldInitializer
            | ClassElementExecutionKind::StaticBlock => {
                lowerer.seed_live_script_global_var_properties();
                lowerer.script_global_call_observation_mode =
                    self.script_global_call_observation_mode;
            }
            ClassElementExecutionKind::None
            | ClassElementExecutionKind::InstanceFieldInitializer => {
                lowerer.seed_script_global_var_properties();
                lowerer.script_global_call_observation_mode =
                    ScriptGlobalCallObservationMode::DormantSummary;
            }
        }
        // Same observed-write degradation as `lower_function`, final phase only.
        if !self.is_prepass {
            self.degrade_observed_written_globals(&mut lowerer);
        }
        lowerer.used_host_builtins = self.used_host_builtins.clone();
        lowerer.host_builtin_calls = self.host_builtin_calls;
        lowerer.is_prepass = self.is_prepass;
        lowerer.is_function_body = true;
        lowerer.current_function_id = Some(function_id.clone());
        match execution_kind {
            FunctionExecutionKind::Generator
                if !generator_body_has_no_suspension(body)
                    && linear_generator_plan(body).is_some() =>
            {
                lowerer.current_generator_resume_state = Some(0);
            }
            FunctionExecutionKind::Async => {
                lowerer.current_async_resume_state = Some(0);
            }
            FunctionExecutionKind::AsyncGenerator => {
                if let Some(plan) = &resumable_plan {
                    lowerer.current_generator_resume_state = Some(plan.entry_state);
                    lowerer.current_async_resume_state = Some(plan.entry_state);
                    lowerer.current_resumable_plan = resumable_plan.clone();
                }
            }
            FunctionExecutionKind::Ordinary | FunctionExecutionKind::Generator => {}
        }
        lowerer.current_owner_id = function_id.clone();
        lowerer.current_this_binding = CurrentThisBinding::Activation(current_this_info.clone());
        lowerer.current_new_target_info = current_new_target_info;
        lowerer.current_construct_this_info = current_construct_this_info.clone();
        lowerer.class_context = Some(class_context.clone());
        lowerer.push_scope();

        let mut owned_env_bindings = self.generated_owned_env_bindings_for_owner(&function_id);
        let captured_bindings = self.generated_captured_bindings_for_owner(&function_id);
        for binding in &captured_bindings {
            let capture_environment_id = self
                .resolve_generated_capture_environment(&function_id, &binding.name)
                .unwrap_or_else(|| {
                    panic!(
                        "generated capture `{}` for `{function_id}` must resolve",
                        binding.name
                    )
                });
            let capture_owner_id =
                &self.analysis.environment_plans[&capture_environment_id].owner_id;
            let info = if generated_capture_can_use_definition_time_facts(
                binding.mode,
                class_element_execution_kind,
            ) {
                self.class_name_binding_value_info_by_storage_name(&binding.name)
                    .or_else(|| {
                        self.lookup_binding(&binding.source_name)
                            .map(|visible| ValueInfo {
                                kind: visible.kind,
                                possible_kinds: visible.possible_kinds,
                                heap_shape: visible.heap_shape,
                                function_targets: visible.function_targets,
                            })
                    })
                    .unwrap_or_else(|| {
                        lowerer.capture_value_info(capture_owner_id.as_str(), &binding.name)
                    })
            } else {
                unknown_runtime_value_info()
            };
            if binding.mode == BindingMode::Const
                && self
                    .analysis
                    .function_plans
                    .get(capture_owner_id)
                    .and_then(|owner| owner.self_binding_name.as_deref())
                    == Some(binding.source_name.as_str())
            {
                lowerer
                    .sloppy_immutable_binding_storage_names
                    .insert(binding.name.clone());
            }
            lowerer.declare_binding(
                binding.source_name.clone(),
                BindingInfo {
                    mode: binding.mode,
                    storage_name: binding.name.clone(),
                    kind: info.kind,
                    possible_kinds: info.possible_kinds,
                    heap_shape: info.heap_shape,
                    function_targets: info.function_targets,
                    initialization: Initialization::Initialized,
                },
            );
            lowerer.install_finite_source_capture_key(
                &binding.source_name,
                capture_environment_id,
                &binding.name,
            );
        }

        if let Some(self_binding_name) = self_binding_name.as_ref() {
            let function_info = lowerer.function_value_info(&function_id);
            lowerer
                .sloppy_immutable_binding_storage_names
                .insert(self_binding_name.clone());
            lowerer.declare_binding(
                self_binding_name.clone(),
                BindingInfo {
                    mode: BindingMode::Const,
                    storage_name: self_binding_name.clone(),
                    kind: function_info.kind,
                    possible_kinds: function_info.possible_kinds,
                    heap_shape: function_info.heap_shape,
                    function_targets: function_info.function_targets,
                    initialization: Initialization::Initialized,
                },
            );
        }

        lowerer.declare_binding(
            LEXICAL_ARGUMENTS_NAME.to_string(),
            BindingInfo {
                mode: BindingMode::Let,
                storage_name: LEXICAL_ARGUMENTS_NAME.to_string(),
                kind: ValueKind::Arguments,
                possible_kinds: KindSet::from_kind(ValueKind::Arguments),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
                initialization: Initialization::Initialized,
            },
        );

        let mut params = Vec::with_capacity(parameters.as_ref().len());
        let mut parameter_prefix_statements = Vec::new();
        let parameter_names = parameters
            .as_ref()
            .iter()
            .flat_map(|parameter| {
                let mut names = Vec::new();
                collect_binding_names(self.interner, parameter.variable().binding(), &mut names);
                names
            })
            .collect::<Vec<_>>();
        for name in &parameter_names {
            // 10.2.11 step 21: every BoundName of the formals is created
            // before any default initializer evaluates, so `function f(a = b, b)`
            // throws. Step 24/27 initializes them left to right, which is the
            // `declare_binding` further down that overwrites this entry with an
            // `Initialization::Initialized` one — the old `clear_tdz_binding`
            // loop was a second, separately ordered spelling of that overwrite
            // and is gone.
            lowerer.declare_binding(
                name.clone(),
                BindingInfo::tdz_placeholder(
                    BindingMode::Let,
                    TdzPlaceholderName::for_source_name(name),
                ),
            );
        }
        for (index, parameter) in parameters.as_ref().iter().enumerate() {
            let binding = parameter.variable().binding();
            if !is_supported_parameter_binding(binding) {
                lowerer.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot first slice: unsupported parameter form in `{function_name}`"
                ));
                continue;
            }
            let name = binding_parameter_storage_name(self.interner, binding, index);
            let default_init = parameter
                .init()
                .map(|expression| lowerer.lower_expression(expression));
            lowerer.declare_binding(
                name.clone(),
                BindingInfo {
                    mode: BindingMode::Let,
                    storage_name: name.clone(),
                    kind: if parameter.is_rest_param() {
                        ValueKind::Array
                    } else {
                        ValueKind::Dynamic
                    },
                    possible_kinds: if parameter.is_rest_param() {
                        KindSet::from_kind(ValueKind::Array)
                    } else {
                        KindSet::all_runtime_tags()
                    },
                    heap_shape: None,
                    function_targets: if parameter.is_rest_param() {
                        FunctionTargetKnowledge::none()
                    } else {
                        FunctionTargetKnowledge::unknown()
                    },
                    initialization: Initialization::Initialized,
                },
            );
            lowerer.current_param_names.push(name.clone());
            params.push(FunctionParamIr {
                name: name.clone(),
                kind: if parameter.is_rest_param() {
                    ValueKind::Array
                } else {
                    ValueKind::Dynamic
                },
                default_init,
                is_rest: parameter.is_rest_param(),
            });
            let binding_initializers = lowerer.lower_parameter_binding_pattern(binding, &name);
            if !binding_initializers.is_empty() {
                parameter_prefix_statements.push(StatementIr::ParameterInitialization {
                    parameter_index: index,
                    statements: binding_initializers,
                });
            }
        }

        let root_function_bindings = self
            .analysis
            .owner_plans
            .get(&function_id)
            .map(|owner| {
                owner
                    .function_bindings
                    .iter()
                    .map(|(name, id)| (name.clone(), id.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let body_environment = lowerer.begin_function_body_environment();
        lowerer.hoist_root_statement_items(body.statements());
        // 10.2.11 step 30, as above: `lexEnv` already exists.
        let body_scope = LexicalScopeInstantiation::instantiate_in_current_scope(
            &mut lowerer,
            body.statements(),
        );
        let lowered_body = lowerer.lower_root_statement_items_with_function_bindings(
            body.statements(),
            &root_function_bindings,
            body_scope,
        );
        let mut statements = prefix_statements;
        statements.extend(parameter_prefix_statements);
        let result_kind = lowered_body.result_kind;
        statements.extend(lowerer.finish_function_body_environment(lowered_body, body_environment));
        let body_ir = BlockIr {
            result_kind,
            statements,
            lexical_environment: None,
        };
        lowerer.source_call_flow_effects = lowerer.source_call_flow_effects.merge_observation(
            SourceCallFlowEffects::for_finalized_invocation(&params, &body_ir),
        );
        let derived_constructor_validation = if class_context.is_derived_constructor {
            Some(validate_derived_constructor_body(&body_ir))
        } else {
            None
        };
        let derived_constructor_summary = if class_context.is_derived_constructor {
            Some(summarize_block(&body_ir))
        } else {
            None
        };
        let captured_value_infos = match class_element_execution_kind {
            ClassElementExecutionKind::StaticBlock => captured_bindings
                .iter()
                .filter_map(|binding| {
                    lowerer.lookup_binding(&binding.source_name).map(|info| {
                        (
                            binding.source_name.clone(),
                            ValueInfo {
                                kind: info.kind,
                                possible_kinds: info.possible_kinds,
                                heap_shape: info.heap_shape,
                                function_targets: info.function_targets,
                            },
                        )
                    })
                })
                .collect::<Vec<_>>(),
            ClassElementExecutionKind::None
            | ClassElementExecutionKind::InstanceFieldInitializer
            | ClassElementExecutionKind::StaticFieldInitializer => Vec::new(),
        };
        owned_env_bindings.extend(lowerer.generated_owned_env_bindings.iter().cloned());
        lowerer.pop_scope();

        let mut return_info = lowerer
            .current_return
            .as_ref()
            .map(|returned| returned.info.clone())
            .unwrap_or_else(ValueInfo::undefined);
        if !Self::statement_list_ends_in_return(&body_ir.statements) {
            return_info = lowerer.merge_value_infos(return_info, ValueInfo::undefined());
        }
        if execution_kind == FunctionExecutionKind::Generator {
            return_info = ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::generator_instance_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            };
        } else if execution_kind == FunctionExecutionKind::AsyncGenerator {
            return_info = ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: Some(Self::async_generator_instance_shape()),
                function_targets: FunctionTargetKnowledge::none(),
            };
        } else if execution_kind == FunctionExecutionKind::Async {
            return_info = Self::value_info_from_shape(Some(Self::promise_instance_shape()));
        }
        if let Some(signature) = lowerer.function_signatures.get_mut(&function_id) {
            signature.return_kind = return_info.kind;
            signature.return_possible_kinds = return_info.possible_kinds;
            signature.return_shape = if execution_kind == FunctionExecutionKind::Ordinary {
                signature
                    .return_shape
                    .with_shape(return_info.heap_shape.clone())
            } else {
                FunctionReturnShape::flow_sensitive(return_info.heap_shape.clone())
            };
            signature.return_targets = return_info.function_targets.clone();
            signature.source_call_flow_effects = lowerer.source_call_flow_effects;
        }
        if let Some(expected) = lowerer
            .current_resumable_plan
            .as_ref()
            .map(|plan| plan.suspension_points.len())
        {
            if lowerer.next_resumable_suspension_index != expected {
                lowerer.unsupported("async-generator function has unconsumed source suspensions");
            }
        }
        if lowerer
            .current_resumable_plan
            .as_ref()
            .is_some_and(|plan| !plan.matches_resume_environment_plan())
        {
            lowerer.unsupported(
                "async-generator resume environments must retain their checked source owner",
            );
        }
        let resumable_plan = lowerer.current_resumable_plan.clone().or(resumable_plan);
        let class_name_post_state = captured_bindings
            .iter()
            .find(|binding| is_class_name_binding_storage_name(&binding.name))
            .and_then(|binding| {
                captured_value_infos
                    .iter()
                    .find(|(source_name, _)| source_name == &binding.source_name)
                    .map(|(_, info)| info.clone())
            });
        let construct_this_info = self.executed_class_receiver_post_state(
            class_element_execution_kind,
            &current_this_info,
            lowerer.current_construct_this_info.clone(),
            class_name_post_state,
        );
        let class_element_flow = Self::capture_generated_class_element_flow(
            class_element_execution_kind,
            &mut lowerer,
            captured_value_infos,
        );

        self.merge_nested_script_global_value_infos(&lowerer.nested_script_global_value_infos);
        self.merge_called_script_global_value_infos(&lowerer.called_script_global_value_infos);
        self.merge_child_compilation_records(&mut lowerer);
        self.function_signatures = lowerer.function_signatures;
        self.dynamically_installed_getters
            .extend(lowerer.dynamically_installed_getters);
        self.dynamically_installed_setters
            .extend(lowerer.dynamically_installed_setters);
        self.observed_script_global_writes
            .extend(std::mem::take(&mut lowerer.observed_script_global_writes));
        self.install_generated_class_element_flow(class_element_flow);
        self.used_host_builtins.extend(lowerer.used_host_builtins);
        self.host_builtin_calls = self.host_builtin_calls.max(lowerer.host_builtin_calls);

        let function_ir = FunctionIr {
            template_source: self.analysis.template_source,
            eval_environment: self.analysis.owner_eval_environment(&function_id),
            id: function_id.clone(),
            name: function_name,
            to_string_representation,
            protocol,
            generator_plan: None,
            resumable_plan,
            strict: class_kind != ClassFunctionKind::None
                || class_element_execution_kind != ClassElementExecutionKind::None,
            class_element_execution_kind,
            class_heritage_kind: class_context.heritage_kind,
            is_static_class_member: class_context.is_static,
            is_derived_constructor: class_context.is_derived_constructor,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            super_constructor_target: class_context.super_constructor_target.clone(),
            uses_super: uses_super
                || derived_constructor_validation
                    .as_ref()
                    .is_some_and(|validation| validation.super_calls > 0)
                || derived_constructor_summary
                    .as_ref()
                    .is_some_and(|summary| summary.super_uses > 0),
            this_before_super: derived_constructor_validation
                .as_ref()
                .is_some_and(|validation| validation.this_before_super)
                || derived_constructor_summary
                    .as_ref()
                    .is_some_and(|summary| summary.this_reads > 0 && summary.super_uses > 0),
            lexical_derived_activation: class_context.is_derived_constructor.then(|| {
                DerivedConstructorActivationIr {
                    owner_function_id: function_id.clone(),
                    this_binding: DERIVED_ACTIVATION_THIS_NAME.to_string(),
                    this_status_binding: DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                    new_target_binding: DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                    active_function_binding: DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
                }
            }),
            private_name_ids,
            captures_private_environment: false,
            is_nested: true,
            is_expression: true,
            is_named_expression: self_binding_name.is_some(),
            captures_lexical_this: false,
            captures_lexical_arguments: false,
            params,
            body: body_ir,
            return_kind: return_info.kind,
            return_shape: return_info.heap_shape.clone(),
            return_targets: return_info.function_targets.clone(),
            constructor_instance: lowerer
                .current_construct_this_info
                .clone()
                .unwrap_or_else(ValueInfo::undefined),
            owned_env_bindings,
            captured_bindings,
        };
        self.generated_functions.push(function_ir);

        GeneratedFunctionOutput {
            return_info,
            construct_this_info,
        }
    }

    fn lower_generated_expr_function(
        &mut self,
        function_id: FunctionId,
        function_name: String,
        to_string_representation: CallableToStringRepresentation,
        expression: &Expression,
        field_name: ClassFieldNameIr,
        current_this_info: ValueInfo,
        class_element_execution_kind: ClassElementExecutionKind,
        class_context: ClassLoweringContext,
    ) -> GeneratedFunctionOutput {
        self.function_signatures.insert(
            function_id.clone(),
            FunctionSignature {
                id: function_id.clone(),
                to_string_representation: to_string_representation.clone(),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: class_context.heritage_kind,
                params: Vec::new(),
                return_kind: ValueKind::Dynamic,
                return_possible_kinds: KindSet::all_runtime_tags(),
                return_shape: FunctionReturnShape::Unobserved,
                return_targets: FunctionTargetKnowledge::unknown(),
                constructor_instance: ValueInfo::undefined(),
                this_info: current_this_info.clone(),
                this_observed: true,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );

        let mut lowerer = ScriptLowerer::new(
            self.interner,
            self.analysis,
            self.source_text,
            self.root_this_binding,
            function_id.clone(),
            self.host_surface_policy,
        );
        lowerer.function_signatures = std::mem::take(&mut self.function_signatures);
        lowerer.visible_function_names = self.visible_function_names.clone();
        lowerer.global_properties = self.global_properties.clone();
        lowerer.observed_script_global_writes = self.observed_script_global_writes.clone();
        lowerer.well_known_symbol_prototype_properties =
            self.well_known_symbol_prototype_properties.clone();
        lowerer.array_prototype_mutated = self.array_prototype_mutated;
        lowerer.number_prototype_to_string_state = self.number_prototype_to_string_state;
        lowerer.boolean_prototype_to_string_state = self.boolean_prototype_to_string_state;
        lowerer.dynamically_installed_getters = self.dynamically_installed_getters.clone();
        lowerer.dynamically_installed_setters = self.dynamically_installed_setters.clone();
        lowerer.unknown_user_code_effects_observed = self.unknown_user_code_effects_observed;
        lowerer.function_signature_shape_evidence = self.function_signature_shape_evidence;
        lowerer.static_boolean_bindings = self.static_boolean_bindings.clone();
        lowerer.static_string_bindings = self.static_string_bindings.clone();
        lowerer.function_source_binding_candidates =
            self.function_source_binding_candidates.clone();
        lowerer.function_source_parameter_candidates =
            self.function_source_parameter_candidates.clone();
        lowerer.static_to_string_regexp_object_bindings =
            self.static_to_string_regexp_object_bindings.clone();
        lowerer.var_bindings = self.var_bindings.clone();
        lowerer.known_nested_script_global_value_infos =
            self.known_nested_script_global_value_infos.clone();
        match class_element_execution_kind {
            ClassElementExecutionKind::StaticFieldInitializer
            | ClassElementExecutionKind::StaticBlock => {
                lowerer.seed_live_script_global_var_properties();
                lowerer.script_global_call_observation_mode =
                    self.script_global_call_observation_mode;
            }
            ClassElementExecutionKind::None
            | ClassElementExecutionKind::InstanceFieldInitializer => {
                lowerer.seed_script_global_var_properties();
                lowerer.script_global_call_observation_mode =
                    ScriptGlobalCallObservationMode::DormantSummary;
            }
        }
        // Same observed-write degradation as `lower_function`, final phase only.
        if !self.is_prepass {
            self.degrade_observed_written_globals(&mut lowerer);
        }
        lowerer.used_host_builtins = self.used_host_builtins.clone();
        lowerer.host_builtin_calls = self.host_builtin_calls;
        lowerer.is_prepass = self.is_prepass;
        lowerer.is_function_body = true;
        lowerer.current_function_id = Some(function_id.clone());
        lowerer.current_owner_id = function_id.clone();
        lowerer.current_this_binding = CurrentThisBinding::Activation(current_this_info.clone());
        lowerer.current_new_target_info = ValueInfo::undefined();
        if class_element_execution_kind == ClassElementExecutionKind::StaticFieldInitializer {
            lowerer.current_construct_this_info = Some(current_this_info.clone());
        }
        lowerer.class_context = Some(class_context.clone());
        lowerer.push_scope();
        let owned_env_bindings = self.generated_owned_env_bindings_for_owner(&function_id);
        let captured_bindings = self.generated_captured_bindings_for_owner(&function_id);
        for binding in &captured_bindings {
            let capture_environment_id = self
                .resolve_generated_capture_environment(&function_id, &binding.name)
                .unwrap_or_else(|| {
                    panic!(
                        "generated capture `{}` for `{function_id}` must resolve",
                        binding.name
                    )
                });
            let capture_owner_id =
                &self.analysis.environment_plans[&capture_environment_id].owner_id;
            let info = if generated_capture_can_use_definition_time_facts(
                binding.mode,
                class_element_execution_kind,
            ) {
                self.class_name_binding_value_info_by_storage_name(&binding.name)
                    .or_else(|| {
                        self.lookup_binding(&binding.source_name)
                            .map(|visible| ValueInfo {
                                kind: visible.kind,
                                possible_kinds: visible.possible_kinds,
                                heap_shape: visible.heap_shape,
                                function_targets: visible.function_targets,
                            })
                    })
                    .unwrap_or_else(|| {
                        lowerer.capture_value_info(capture_owner_id.as_str(), &binding.name)
                    })
            } else {
                unknown_runtime_value_info()
            };
            lowerer.declare_binding(
                binding.source_name.clone(),
                BindingInfo {
                    mode: binding.mode,
                    storage_name: binding.name.clone(),
                    kind: info.kind,
                    possible_kinds: info.possible_kinds,
                    heap_shape: info.heap_shape,
                    function_targets: info.function_targets,
                    initialization: Initialization::Initialized,
                },
            );
            lowerer.install_finite_source_capture_key(
                &binding.source_name,
                capture_environment_id,
                &binding.name,
            );
        }
        lowerer.declare_binding(
            LEXICAL_ARGUMENTS_NAME.to_string(),
            BindingInfo {
                mode: BindingMode::Let,
                storage_name: LEXICAL_ARGUMENTS_NAME.to_string(),
                kind: ValueKind::Arguments,
                possible_kinds: KindSet::from_kind(ValueKind::Arguments),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
                initialization: Initialization::Initialized,
            },
        );
        let return_value = match Self::unwrap_parenthesized_expr(expression) {
            Expression::ClassExpression(class) if class.name_scope().is_none() => {
                // The parser can attach an inferred literal/private label to
                // ClassExpression.name(). Only name_scope records a source binding
                // identifier; an inferred label does not suppress NamedEvaluation.
                // NamedEvaluation supplies the original field key to class creation,
                // before the nested class's computed elements or static initializers.
                lowerer.lower_class_expression_with_inferred_name(
                    class,
                    ClassNameInferenceIr::FieldInitializer(field_name),
                )
            }
            _ if Self::is_anonymous_field_function_definition(expression) => {
                let mut lowered = lowerer.lower_expression(expression);
                if let ExprIr::FunctionValue(function_id) = &lowered.expr {
                    lowered.expr = ExprIr::ClassFieldNamedFunctionValue {
                        function_id: function_id.clone(),
                        name: field_name,
                    };
                }
                // An unsupported source function retains its existing diagnostic
                // and placeholder; no named callable is minted for that path.
                lowered
            }
            _ => lowerer.lower_expression(expression),
        };
        lowerer.record_return_expression(&return_value);
        let body = BlockIr {
            result_kind: return_value.kind,
            statements: vec![StatementIr::Return(return_value.clone())],
            lexical_environment: None,
        };
        lowerer.source_call_flow_effects = lowerer
            .source_call_flow_effects
            .merge_observation(SourceCallFlowEffects::for_finalized_invocation(&[], &body));
        let captured_value_infos = match class_element_execution_kind {
            ClassElementExecutionKind::StaticFieldInitializer => captured_bindings
                .iter()
                .filter_map(|binding| {
                    lowerer
                        .lookup_binding(&binding.source_name)
                        .map(|captured| {
                            (
                                binding.source_name.clone(),
                                ValueInfo {
                                    kind: captured.kind,
                                    possible_kinds: captured.possible_kinds,
                                    heap_shape: captured.heap_shape,
                                    function_targets: captured.function_targets,
                                },
                            )
                        })
                })
                .collect::<Vec<_>>(),
            ClassElementExecutionKind::None
            | ClassElementExecutionKind::InstanceFieldInitializer
            | ClassElementExecutionKind::StaticBlock => Vec::new(),
        };
        lowerer.pop_scope();
        let return_info = lowerer
            .current_return
            .as_ref()
            .map(|returned| returned.info.clone())
            .unwrap_or_else(ValueInfo::undefined);
        if let Some(signature) = lowerer.function_signatures.get_mut(&function_id) {
            signature.source_call_flow_effects = lowerer.source_call_flow_effects;
        }
        let class_name_post_state = captured_bindings
            .iter()
            .find(|binding| is_class_name_binding_storage_name(&binding.name))
            .and_then(|binding| {
                captured_value_infos
                    .iter()
                    .find(|(source_name, _)| source_name == &binding.source_name)
                    .map(|(_, info)| info.clone())
            });
        let construct_this_info = self.executed_class_receiver_post_state(
            class_element_execution_kind,
            &current_this_info,
            lowerer.current_construct_this_info.clone(),
            class_name_post_state,
        );
        let class_element_flow = Self::capture_generated_class_element_flow(
            class_element_execution_kind,
            &mut lowerer,
            captured_value_infos,
        );

        self.merge_nested_script_global_value_infos(&lowerer.nested_script_global_value_infos);
        self.merge_called_script_global_value_infos(&lowerer.called_script_global_value_infos);
        self.merge_child_compilation_records(&mut lowerer);
        self.function_signatures = lowerer.function_signatures;
        self.dynamically_installed_getters
            .extend(lowerer.dynamically_installed_getters);
        self.dynamically_installed_setters
            .extend(lowerer.dynamically_installed_setters);
        self.observed_script_global_writes
            .extend(std::mem::take(&mut lowerer.observed_script_global_writes));
        self.install_generated_class_element_flow(class_element_flow);
        self.used_host_builtins.extend(lowerer.used_host_builtins);
        self.host_builtin_calls = self.host_builtin_calls.max(lowerer.host_builtin_calls);
        self.generated_functions.push(FunctionIr {
            template_source: self.analysis.template_source,
            eval_environment: self.analysis.owner_eval_environment(&function_id),
            id: function_id.clone(),
            name: function_name,
            to_string_representation,
            protocol: FunctionProtocolIr::OrdinaryCallOnly,
            generator_plan: None,
            resumable_plan: None,
            strict: true,
            class_element_execution_kind,
            class_heritage_kind: class_context.heritage_kind,
            is_static_class_member: class_context.is_static,
            is_derived_constructor: class_context.is_derived_constructor,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            super_constructor_target: class_context.super_constructor_target.clone(),
            uses_super: false,
            this_before_super: false,
            lexical_derived_activation: None,
            private_name_ids: class_context.private_name_ids.clone(),
            captures_private_environment: false,
            is_nested: true,
            is_expression: true,
            is_named_expression: false,
            captures_lexical_this: false,
            captures_lexical_arguments: false,
            params: Vec::new(),
            body,
            return_kind: return_info.kind,
            return_shape: return_info.heap_shape.clone(),
            return_targets: return_info.function_targets.clone(),
            constructor_instance: ValueInfo::undefined(),
            owned_env_bindings,
            captured_bindings,
        });

        GeneratedFunctionOutput {
            return_info,
            construct_this_info,
        }
    }

    fn lower_generated_block_function(
        &mut self,
        function_id: FunctionId,
        function_name: String,
        to_string_representation: CallableToStringRepresentation,
        protocol: FunctionProtocolIr,
        callable: bool,
        class_element_execution_kind: ClassElementExecutionKind,
        current_this_info: ValueInfo,
        current_construct_this_info: Option<ValueInfo>,
        class_context: ClassLoweringContext,
        prefix_statements: Vec<StatementIr>,
        statements: Vec<StatementIr>,
    ) -> GeneratedFunctionOutput {
        let body = BlockIr {
            result_kind: ValueKind::Undefined,
            statements: prefix_statements.into_iter().chain(statements).collect(),
            lexical_environment: None,
        };
        self.function_signatures.insert(
            function_id.clone(),
            FunctionSignature {
                id: function_id.clone(),
                to_string_representation: to_string_representation.clone(),
                protocol,
                callable,
                class_heritage_kind: class_context.heritage_kind,
                params: Vec::new(),
                return_kind: ValueKind::Undefined,
                return_possible_kinds: KindSet::from_kind(ValueKind::Undefined),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: current_construct_this_info
                    .clone()
                    .unwrap_or_else(ValueInfo::undefined),
                this_info: current_this_info.clone(),
                this_observed: true,
                source_call_flow_effects: SourceCallFlowEffects::for_finalized_invocation(
                    &[],
                    &body,
                ),
            },
        );
        let lexical_derived_activation =
            class_context
                .is_derived_constructor
                .then(|| DerivedConstructorActivationIr {
                    owner_function_id: function_id.clone(),
                    this_binding: DERIVED_ACTIVATION_THIS_NAME.to_string(),
                    this_status_binding: DERIVED_ACTIVATION_THIS_STATUS_NAME.to_string(),
                    new_target_binding: DERIVED_ACTIVATION_NEW_TARGET_NAME.to_string(),
                    active_function_binding: DERIVED_ACTIVATION_FUNCTION_NAME.to_string(),
                });
        let owned_env_bindings = self.generated_owned_env_bindings_for_owner(&function_id);
        let captured_bindings = self.generated_captured_bindings_for_owner(&function_id);
        self.generated_functions.push(FunctionIr {
            template_source: self.analysis.template_source,
            eval_environment: self.analysis.owner_eval_environment(&function_id),
            id: function_id.clone(),
            name: function_name,
            to_string_representation,
            protocol,
            generator_plan: None,
            resumable_plan: None,
            strict: protocol.class_kind() != ClassFunctionKind::None,
            class_element_execution_kind,
            class_heritage_kind: class_context.heritage_kind,
            is_static_class_member: class_context.is_static,
            is_derived_constructor: class_context.is_derived_constructor,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            super_constructor_target: class_context.super_constructor_target.clone(),
            uses_super: false,
            this_before_super: false,
            lexical_derived_activation,
            private_name_ids: class_context.private_name_ids.clone(),
            captures_private_environment: false,
            is_nested: true,
            is_expression: true,
            is_named_expression: false,
            captures_lexical_this: false,
            captures_lexical_arguments: false,
            params: Vec::new(),
            body,
            return_kind: ValueKind::Undefined,
            return_shape: None,
            return_targets: FunctionTargetKnowledge::none(),
            constructor_instance: current_construct_this_info
                .clone()
                .unwrap_or_else(ValueInfo::undefined),
            owned_env_bindings,
            captured_bindings,
        });
        GeneratedFunctionOutput {
            return_info: ValueInfo::undefined(),
            construct_this_info: current_construct_this_info,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_generated_auto_accessor_function(
        &mut self,
        function_id: FunctionId,
        function_name: String,
        protocol: FunctionProtocolIr,
        backing_name: AutoAccessorBackingNameIr,
        current_this_info: ValueInfo,
        private_name_ids: BTreeMap<String, PrivateNameId>,
        heritage_kind: ClassHeritageKind,
        is_static: bool,
    ) {
        let value_info = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::all_runtime_tags(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        let backing_name_id = backing_name.private_name_id();
        let (params, body, return_info) = match protocol {
            FunctionProtocolIr::ClassGetter => {
                let read = TypedExpr::from_info(
                    value_info.clone(),
                    ExprIr::PrivateRead {
                        target: Box::new(TypedExpr::from_info(
                            current_this_info.clone(),
                            ExprIr::This,
                        )),
                        private_name_id: backing_name_id,
                    },
                );
                (
                    Vec::new(),
                    BlockIr {
                        result_kind: value_info.kind,
                        statements: vec![StatementIr::Return(read)],
                        lexical_environment: None,
                    },
                    value_info,
                )
            }
            FunctionProtocolIr::ClassSetter => {
                let parameter_name = "$auto_accessor_value".to_string();
                let value = TypedExpr::from_info(
                    value_info.clone(),
                    ExprIr::Identifier(parameter_name.clone()),
                );
                let write = TypedExpr::from_info(
                    value_info,
                    ExprIr::PrivateWrite {
                        target: Box::new(TypedExpr::from_info(
                            current_this_info.clone(),
                            ExprIr::This,
                        )),
                        private_name_id: backing_name_id,
                        value: Box::new(value),
                    },
                );
                (
                    vec![FunctionParamIr {
                        name: parameter_name,
                        kind: ValueKind::Dynamic,
                        default_init: None,
                        is_rest: false,
                    }],
                    BlockIr {
                        result_kind: ValueKind::Undefined,
                        statements: vec![StatementIr::Expression(write)],
                        lexical_environment: None,
                    },
                    ValueInfo::undefined(),
                )
            }
            _ => unreachable!("auto-accessor function must be a class getter or setter"),
        };
        self.function_signatures.insert(
            function_id.clone(),
            FunctionSignature {
                id: function_id.clone(),
                to_string_representation: CallableToStringRepresentation::NativeAnonymous,
                protocol,
                callable: false,
                class_heritage_kind: heritage_kind,
                params: params
                    .iter()
                    .map(|_| FunctionParamSignature {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                        observed: true,
                        has_default: false,
                        is_rest: false,
                    })
                    .collect(),
                return_kind: return_info.kind,
                return_possible_kinds: return_info.possible_kinds,
                return_shape: FunctionReturnShape::flow_sensitive(return_info.heap_shape.clone()),
                return_targets: return_info.function_targets.clone(),
                constructor_instance: ValueInfo::undefined(),
                this_info: current_this_info.clone(),
                this_observed: true,
                source_call_flow_effects: SourceCallFlowEffects::for_finalized_invocation(
                    &params, &body,
                ),
            },
        );
        self.generated_functions.push(FunctionIr {
            template_source: self.analysis.template_source,
            eval_environment: self.analysis.owner_eval_environment(&function_id),
            id: function_id,
            name: function_name,
            to_string_representation: CallableToStringRepresentation::NativeAnonymous,
            protocol,
            generator_plan: None,
            resumable_plan: None,
            strict: true,
            class_element_execution_kind: ClassElementExecutionKind::None,
            class_heritage_kind: heritage_kind,
            is_static_class_member: is_static,
            is_derived_constructor: false,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            super_constructor_target: None,
            uses_super: false,
            this_before_super: false,
            lexical_derived_activation: None,
            private_name_ids,
            captures_private_environment: true,
            is_nested: true,
            is_expression: true,
            is_named_expression: false,
            captures_lexical_this: false,
            captures_lexical_arguments: false,
            params,
            body,
            return_kind: return_info.kind,
            return_shape: return_info.heap_shape,
            return_targets: return_info.function_targets,
            constructor_instance: ValueInfo::undefined(),
            owned_env_bindings: Vec::new(),
            captured_bindings: Vec::new(),
        });
    }

    fn lower_indirect_method_call(
        &mut self,
        info: ValueInfo,
        mut callee: TypedExpr,
        receiver: TypedExpr,
        args: Vec<TypedExpr>,
        static_regexp_compilation: Option<StaticRegExpCompilation>,
        invocation_effects: AnalyzedInvocationEffects,
    ) -> TypedExpr {
        let receiver_is_callee_base = match &callee.expr {
            ExprIr::PropertyRead { target, .. } => target.expr == receiver.expr,
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => operands
                .first()
                .is_some_and(|target| target.expr == receiver.expr),
            _ => false,
        };
        if !receiver_is_callee_base {
            let call = TypedExpr::from_info(
                info,
                ExprIr::CallIndirect {
                    direct_eval: None,
                    callee: Box::new(callee),
                    this_arg: Some(Box::new(receiver)),
                    args,
                    static_regexp_compilation,
                },
            );
            return invocation_effects.attach_to_emitted_call(call);
        }

        let receiver_storage_name = self.alloc_temp_binding_name("call.receiver.");
        let materialized_receiver = TypedExpr::from_info(
            receiver.value_info(),
            ExprIr::Identifier(receiver_storage_name.clone()),
        );
        match &mut callee.expr {
            ExprIr::PropertyRead { target, .. } => {
                **target = TypedExpr::from_info(
                    target.value_info(),
                    ExprIr::Identifier(receiver_storage_name.clone()),
                );
            }
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => {
                let target = operands
                    .first_mut()
                    .expect("GetV property reference should have a target operand");
                *target = TypedExpr::from_info(
                    target.value_info(),
                    ExprIr::Identifier(receiver_storage_name.clone()),
                );
            }
            _ => unreachable!("property call receiver match should preserve reference IR"),
        }

        let call = invocation_effects.attach_to_emitted_call(TypedExpr::from_info(
            info.clone(),
            ExprIr::CallIndirect {
                direct_eval: None,
                callee: Box::new(callee),
                this_arg: Some(Box::new(materialized_receiver)),
                args,
                static_regexp_compilation,
            },
        ));
        TypedExpr::from_info(
            info,
            ExprIr::MaterializeBinding {
                name: receiver_storage_name,
                value: Box::new(receiver),
                body: Box::new(call),
            },
        )
    }

    fn known_json_parse_reviver_targets(&self, args: &[TypedExpr]) -> BTreeSet<FunctionId> {
        let mut reviver_targets = BTreeSet::new();
        let Some(reviver) = args.get(1) else {
            return reviver_targets;
        };
        if let Some(reviver_id) = self.resolve_single_function_target(reviver) {
            reviver_targets.insert(reviver_id);
        }
        reviver_targets.extend(reviver.function_targets.known_targets().iter().cloned());
        if let ExprIr::CallIndirect { callee, .. } = &reviver.expr {
            if let Some(factory_id) = self.resolve_single_function_target(callee) {
                if let Some(signature) = self.function_signatures.get(&factory_id) {
                    reviver_targets
                        .extend(signature.return_targets.known_targets().iter().cloned());
                }
            }
        }
        reviver_targets
    }

    fn observe_json_parse_reviver_targets(
        &mut self,
        reviver_targets: BTreeSet<FunctionId>,
        helper_context_id: &ExactHelperContextId,
    ) {
        let holder_info = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::HEAP_COERCIBLE_ONLY
                .union(KindSet::from_kind(ValueKind::Function)),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        let object_info = ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        };
        let value_info = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::all_runtime_tags(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        let reviver_param_infos = [ValueInfo::new(ValueKind::String), value_info, object_info];
        for reviver_id in reviver_targets {
            let can_specialize_reviver = self
                .analysis
                .function_plans
                .get(&reviver_id)
                .is_some_and(|plan| plan.captures.is_empty());
            if !can_specialize_reviver {
                self.merge_function_this_info(&reviver_id, holder_info.clone());
                self.merge_function_param_infos(&reviver_id, &reviver_param_infos);
                continue;
            }
            self.observe_exact_callback_this_info(
                &reviver_id,
                helper_context_id,
                holder_info.clone(),
            );
            self.observe_exact_callback_param_infos(
                &reviver_id,
                helper_context_id,
                &reviver_param_infos,
            );
        }
    }

    fn iterator_from_wrapper_value_info(&self, base: ValueInfo) -> ValueInfo {
        // GetIteratorFlattenable may call @@iterator and select an object other
        // than the input. OrdinaryHasInstance can then return that object,
        // including a callable or an Array, instead of allocating a wrapper.
        let mut function_targets = base.function_targets;
        function_targets.widen_for_possible_replacement();
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: Self::object_like_kind_set(),
            heap_shape: None,
            function_targets,
        }
    }

    fn static_string_typed_expr(value: String) -> TypedExpr {
        TypedExpr::from_info(Self::string_value_info(&value), ExprIr::String(value))
    }

    fn lower_call_args(
        &mut self,
        function_id: &FunctionId,
        args: &[Expression],
        this_observation: InvocationThisObservation<'_>,
    ) -> (Vec<TypedExpr>, ValueInfo, AnalyzedInvocationEffects) {
        let (_, args, info, invocation_effects) = self.lower_call_args_with_target(
            function_id,
            args,
            BuiltinCallContext::Call,
            this_observation,
        );
        (args, info, invocation_effects)
    }

    fn lower_call_args_expanding_spread(&mut self, args: &[Expression]) -> LoweredCallArguments {
        let mut lowered_args: Vec<TypedExpr> = Vec::with_capacity(args.len());
        let mut invalidates_preceding_heap_shapes = false;
        for arg in args {
            let before_argument_effect_epoch = self.intervening_effect_epoch;
            let (lowered_arg, expands_spread) = match arg {
                Expression::Spread(spread) => {
                    let spread_value = self.lower_expression(spread.target());
                    (
                        TypedExpr::from_info(
                            spread_value.value_info(),
                            ExprIr::SpreadArgument(SpreadArgumentIr {
                                value: Box::new(spread_value),
                                protocol: SpreadArgumentProtocol::ARGUMENT_LIST,
                            }),
                        ),
                        true,
                    )
                }
                _ => (self.lower_expression(arg), false),
            };
            if self.intervening_effect_epoch != before_argument_effect_epoch {
                invalidates_preceding_heap_shapes = true;
                for previous_arg in &mut lowered_args {
                    previous_arg.heap_shape = None;
                }
            }
            lowered_args.push(lowered_arg);
            if expands_spread {
                self.invalidate_unknown_user_code_effects();
                invalidates_preceding_heap_shapes = true;
                for observed_arg in &mut lowered_args {
                    observed_arg.heap_shape = None;
                }
            }
        }
        LoweredCallArguments {
            arguments: lowered_args,
            invalidates_preceding_heap_shapes,
        }
    }

    fn call_args_have_spread(args: &[TypedExpr]) -> bool {
        args.iter().any(|arg| {
            matches!(
                arg.expr,
                ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
            )
        })
    }

    fn finish_target_call_arguments(
        &mut self,
        function_id: &FunctionId,
        lowered: LoweredCallArguments,
        this_observation: InvocationThisObservation<'_>,
    ) -> Vec<TypedExpr> {
        let (arguments, this_info) = match this_observation {
            InvocationThisObservation::NotObserved => {
                (lowered.into_arguments_without_predecessor(), None)
            }
            InvocationThisObservation::ConstructorCallee(callee) => {
                (lowered.into_arguments_after_expression(callee), None)
            }
            InvocationThisObservation::Default => (
                lowered.into_arguments_without_predecessor(),
                Some(self.default_this_info_for_function_target(function_id)),
            ),
            InvocationThisObservation::Current => (
                lowered.into_arguments_without_predecessor(),
                Some(self.current_this_info()),
            ),
            InvocationThisObservation::ExplicitMethod { receiver, callee } => {
                let arguments = lowered.into_arguments_after_two_expressions(receiver, callee);
                (arguments, Some(receiver.value_info()))
            }
            InvocationThisObservation::ExplicitValueMethod { receiver, callee } => {
                let arguments = lowered.into_arguments_after_expression_and_value(callee, receiver);
                (arguments, Some(receiver.clone()))
            }
        };
        if let Some(this_info) = this_info {
            self.merge_function_this_info(function_id, this_info);
        }
        arguments
    }

    fn lower_call_args_with_target(
        &mut self,
        function_id: &FunctionId,
        args: &[Expression],
        context: BuiltinCallContext,
        this_observation: InvocationThisObservation<'_>,
    ) -> (
        FunctionId,
        Vec<TypedExpr>,
        ValueInfo,
        AnalyzedInvocationEffects,
    ) {
        if !self.function_signatures.contains_key(function_id) {
            self.unsupported_expr("indirect call");
            return (
                function_id.clone(),
                Vec::new(),
                ValueInfo::undefined(),
                AnalyzedInvocationEffects::already_applied(),
            );
        }
        if args.iter().any(|arg| matches!(arg, Expression::Spread(_))) {
            let lowered_args = self.lower_call_args_expanding_spread(args);
            let lowered_args =
                self.finish_target_call_arguments(function_id, lowered_args, this_observation);
            self.observe_live_script_global_values();
            let signature = self
                .function_signatures
                .get(function_id)
                .cloned()
                .expect("function signature must exist");
            match self.resolve_dynamic_source_call(function_id, Some(args), &lowered_args) {
                None => {}
                Some(ResolvedDynamicSourceCall::EvalPassThrough(_)) => {
                    unreachable!("spread arguments cannot prove eval's first value is non-String")
                }
                Some(ResolvedDynamicSourceCall::FunctionInvocation(_)) => {}
                Some(ResolvedDynamicSourceCall::RealmScriptInvocation(proof)) => {
                    self.mark_host_builtin_from_function_id(function_id);
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::RealmScriptConversionThrow(proof)) => {
                    self.mark_host_builtin_from_function_id(function_id);
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::IndirectEvalInvocation(proof)) => {
                    self.note_standard_builtin_call(StandardBuiltinId::EvalFunction);
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::ShadowRealmInvocation(proof)) => {
                    self.note_standard_builtin_call(
                        StandardBuiltinId::ShadowRealmPrototypeEvaluate,
                    );
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::CompiledScript(proof)) => {
                    if let Some(builtin) = StandardBuiltinId::from_function_id(function_id) {
                        self.note_standard_builtin_call(builtin);
                    } else {
                        self.mark_host_builtin_from_function_id(function_id);
                    }
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::Unsupported(unsupported)) => {
                    self.record_unsupported_dynamic_source(unsupported);
                    return (
                        function_id.clone(),
                        Vec::new(),
                        ValueInfo::undefined(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
            }
            if let Some(builtin) = Self::define_property_builtin(function_id) {
                let (result, effects) = self
                    .unknown_forwarded_define_property_call_analysis(builtin)
                    .into_parts();
                return (function_id.clone(), lowered_args, result, effects);
            }
            self.merge_unknown_spread_param_infos(function_id);
            let return_info = if StandardBuiltinId::from_function_id(function_id).is_some() {
                self.function_call_return_info(&signature)
            } else {
                ValueInfo::new(ValueKind::Dynamic)
            };
            let result = (
                function_id.clone(),
                lowered_args,
                return_info,
                AnalyzedInvocationEffects::already_applied(),
            );
            let may_invalidate_caller_flow =
                if StandardBuiltinId::from_function_id(function_id).is_some() {
                    self.function_may_run_user_code_synchronously(function_id)
                } else {
                    self.invocation_caller_flow_effects(function_id, &signature)
                        .may_invalidate_caller_flow()
                };
            if may_invalidate_caller_flow {
                self.invalidate_unknown_user_code_effects();
            }
            return result;
        }
        let lowered_args = self.lower_call_args_expanding_spread(args);
        let mut lowered_args =
            self.finish_target_call_arguments(function_id, lowered_args, this_observation);
        self.observe_live_script_global_values();
        let mut eval_pass_through =
            match self.resolve_dynamic_source_call(function_id, Some(args), &lowered_args) {
                None => None,
                Some(ResolvedDynamicSourceCall::EvalPassThrough(proof)) => Some(proof),
                Some(ResolvedDynamicSourceCall::RealmScriptInvocation(proof)) => {
                    self.mark_host_builtin_from_function_id(function_id);
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::RealmScriptConversionThrow(proof)) => {
                    self.mark_host_builtin_from_function_id(function_id);
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::IndirectEvalInvocation(proof)) => {
                    self.note_standard_builtin_call(StandardBuiltinId::EvalFunction);
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::ShadowRealmInvocation(proof)) => {
                    self.note_standard_builtin_call(
                        StandardBuiltinId::ShadowRealmPrototypeEvaluate,
                    );
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::FunctionInvocation(proof)) => {
                    if let Some(builtin) = StandardBuiltinId::from_function_id(function_id) {
                        self.note_standard_builtin_call(builtin);
                    } else {
                        self.mark_host_builtin_from_function_id(function_id);
                    }
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::CompiledScript(proof)) => {
                    if let Some(builtin) = StandardBuiltinId::from_function_id(function_id) {
                        self.note_standard_builtin_call(builtin);
                    } else {
                        self.mark_host_builtin_from_function_id(function_id);
                    }
                    return (
                        function_id.clone(),
                        lowered_args,
                        proof.into_result_info(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
                Some(ResolvedDynamicSourceCall::Unsupported(unsupported)) => {
                    self.record_unsupported_dynamic_source(unsupported);
                    return (
                        function_id.clone(),
                        Vec::new(),
                        ValueInfo::undefined(),
                        AnalyzedInvocationEffects::already_applied(),
                    );
                }
            };
        let arg_infos = lowered_args
            .iter()
            .map(TypedExpr::value_info)
            .collect::<Vec<_>>();
        let canonical_arg_infos = self.canonical_exact_context_arg_infos(&arg_infos);
        let helper_context_id = Self::exact_helper_context_id(function_id, &canonical_arg_infos);
        if StandardBuiltinId::from_function_id(function_id) == Some(StandardBuiltinId::JsonParse) {
            let reviver_targets = self.known_json_parse_reviver_targets(&lowered_args);
            self.observe_json_parse_reviver_targets(reviver_targets, &helper_context_id);
        }
        let effective_function_id = self
            .exact_context_function_specializations
            .get(&(function_id.clone(), helper_context_id.clone()))
            .cloned()
            .unwrap_or_else(|| function_id.clone());
        if !self.exact_context_callback_specializations.is_empty() {
            for arg in &mut lowered_args {
                let Some(callback_id) = self.resolve_single_function_target(arg) else {
                    continue;
                };
                let key = (
                    self.original_exact_function_id(&callback_id),
                    helper_context_id.clone(),
                );
                if let Some(synthetic_id) = self
                    .exact_context_callback_specializations
                    .get(&key)
                    .cloned()
                {
                    if matches!(&arg.expr, ExprIr::FunctionValue(_)) {
                        *arg = self.function_value_expr(synthetic_id);
                    }
                }
            }
        }

        let exact_prepass_call =
            self.is_prepass && self.analysis.function_plans.contains_key(function_id);
        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            for (param, arg) in signature.params.iter_mut().zip(lowered_args.iter()) {
                if param.is_rest {
                    break;
                }
                let arg_info = arg.value_info();
                Self::merge_signature_param_observation(param, &arg_info);
            }
            Self::merge_omitted_signature_params_as_undefined(signature, lowered_args.len());
        }
        if exact_prepass_call {
            if let Some(helper_context_id) = self
                .exact_context_callback_targets
                .get(function_id)
                .cloned()
            {
                let arg_infos = lowered_args
                    .iter()
                    .map(TypedExpr::value_info)
                    .collect::<Vec<_>>();
                let original_function_id = self.original_exact_function_id(function_id);
                let arg_infos = self.canonical_exact_context_arg_infos(&arg_infos);
                self.observe_exact_callback_param_infos(
                    &original_function_id,
                    &helper_context_id,
                    &arg_infos,
                );
            }
        }

        let callsite_return_info =
            if self.is_prepass || self.analysis.function_plans.contains_key(function_id) {
                self.propagate_direct_call_context(function_id, &canonical_arg_infos)
            } else {
                None
            };

        let exact_context_key = (function_id.clone(), helper_context_id.clone());
        let signature = self
            .exact_context_function_observations
            .get(&exact_context_key)
            .or_else(|| {
                self.exact_context_callback_observations
                    .get(&exact_context_key)
            })
            .filter(|_| !self.function_has_untracked_captures(function_id))
            .filter(|signature| {
                !signature.this_observed
                    || signature
                        .return_targets
                        .exact_targets()
                        .is_some_and(BTreeSet::is_empty)
            })
            .cloned()
            .or_else(|| {
                self.function_signatures
                    .get(&effective_function_id)
                    .cloned()
            })
            .expect("function signature must exist");

        for (_index, param) in signature.params.iter().enumerate().skip(lowered_args.len()) {
            if param.is_rest {
                break;
            }
            if param.kind == ValueKind::Number && !param.has_default {
                self.unsupported_with_message(format!(
                    "unsupported in scriptw wasm-aot first slice: missing numeric argument"
                ));
            }
        }

        for (index, param) in signature.params.iter().enumerate() {
            if param.is_rest || param.kind != ValueKind::Number {
                continue;
            }
            let Some(arg) = lowered_args.get(index).cloned() else {
                continue;
            };
            let arg_function_targets = arg.function_targets.clone();
            let Some(number_arg) = self.coerce_expr_to_number(arg) else {
                if let Some(signature) = self.function_signatures.get_mut(&effective_function_id) {
                    if let Some(param) = signature.params.get_mut(index) {
                        param.kind = ValueKind::Dynamic;
                        param.possible_kinds = KindSet::all_runtime_tags();
                        param.heap_shape = None;
                        param.function_targets =
                            param.function_targets.clone().join(arg_function_targets);
                        param.observed = true;
                    }
                }
                continue;
            };
            lowered_args[index] = number_arg;
        }

        if let Some(builtin) = StandardBuiltinId::from_function_id(&effective_function_id) {
            self.note_standard_builtin_call(builtin);
            if let Some(proof) = eval_pass_through.take() {
                debug_assert_eq!(builtin, StandardBuiltinId::EvalFunction);
                return (
                    effective_function_id,
                    lowered_args,
                    proof.into_result_info(),
                    AnalyzedInvocationEffects::already_applied(),
                );
            }
            let Some(analysis) = self.standard_builtin_call_info(builtin, &lowered_args, context)
            else {
                return (
                    effective_function_id,
                    Vec::new(),
                    ValueInfo::undefined(),
                    AnalyzedInvocationEffects::already_applied(),
                );
            };
            let (result, invocation_effects) = analysis.into_parts();
            return (
                effective_function_id,
                lowered_args,
                result,
                invocation_effects,
            );
        }

        debug_assert!(
            eval_pass_through.is_none(),
            "only the standard intrinsic eval identity can produce a pass-through proof"
        );

        let result = (
            effective_function_id,
            lowered_args,
            callsite_return_info.unwrap_or_else(|| self.function_call_return_info(&signature)),
            AnalyzedInvocationEffects::already_applied(),
        );
        if self
            .invocation_caller_flow_effects(function_id, &signature)
            .may_invalidate_caller_flow()
        {
            self.invalidate_unknown_user_code_effects();
        }
        result
    }

    fn resolve_single_function_target(&self, expr: &TypedExpr) -> Option<FunctionId> {
        if expr.kind != ValueKind::Function {
            return None;
        }
        expr.function_targets.exact_single_target().cloned()
    }

    fn original_exact_function_id(&self, function_id: &FunctionId) -> FunctionId {
        for ((original_id, _), synthetic_id) in self
            .exact_context_callback_specializations
            .iter()
            .chain(self.exact_context_function_specializations.iter())
        {
            if synthetic_id == function_id {
                return original_id.clone();
            }
        }
        function_id.clone()
    }

    fn function_may_run_user_code_synchronously(&self, function_id: &FunctionId) -> bool {
        let canonical_function_id = self
            .function_signatures
            .get(function_id)
            .map(|signature| &signature.id)
            .unwrap_or(function_id);
        if self
            .analysis
            .planned_source_function_ids
            .contains(canonical_function_id)
        {
            return true;
        }
        if StandardBuiltinId::from_function_id(canonical_function_id)
            .is_some_and(StandardBuiltinId::may_run_user_code_synchronously)
        {
            return true;
        }
        HostBuiltinId::from_function_id(canonical_function_id)
            .is_some_and(HostBuiltinId::may_run_user_code_synchronously)
    }

    fn invocation_caller_flow_effects(
        &self,
        _function_id: &FunctionId,
        signature: &FunctionSignature,
    ) -> InvocationCallerFlowEffects {
        if self
            .analysis
            .planned_source_function_ids
            .contains(&signature.id)
        {
            return InvocationCallerFlowEffects::from_source_call(
                signature.source_call_flow_effects,
            );
        }
        if let Some(builtin) = HostBuiltinId::from_function_id(&signature.id) {
            return InvocationCallerFlowEffects::from_host_builtin(builtin);
        }
        InvocationCallerFlowEffects::may_invalidate()
    }

    fn canonical_exact_context_arg_infos(&self, args: &[ValueInfo]) -> Vec<ValueInfo> {
        args.iter()
            .cloned()
            .map(|arg| self.canonical_exact_context_function_info(arg))
            .collect()
    }

    fn canonical_exact_context_function_info(&self, mut info: ValueInfo) -> ValueInfo {
        if info.kind != ValueKind::Function {
            return info;
        }
        info.function_targets = info
            .function_targets
            .map_known_targets(|function_id| self.original_exact_function_id(&function_id));
        info
    }

    fn specialize_exact_context_function_info(
        &self,
        mut info: ValueInfo,
        helper_context_id: Option<&ExactHelperContextId>,
    ) -> ValueInfo {
        let Some(helper_context_id) = helper_context_id else {
            return info;
        };
        if info.kind != ValueKind::Function {
            return info;
        }
        info.function_targets = info.function_targets.map_known_targets(|function_id| {
            let original_id = self.original_exact_function_id(&function_id);
            self.exact_context_callback_specializations
                .get(&(original_id.clone(), helper_context_id.clone()))
                .cloned()
                .unwrap_or(original_id)
        });
        info
    }

    fn note_standard_builtin_call(&mut self, builtin: StandardBuiltinId) {
        if builtin.constructable() {
            self.builtin_ctor_calls += 1;
        }
        if builtin.is_static_method() {
            self.builtin_static_calls += 1;
        }
        if builtin.is_error_constructor() {
            self.error_builtin_calls += 1;
        }
        if builtin == StandardBuiltinId::AggregateErrorConstructor {
            self.aggregate_errors += 1;
        }
        if builtin == StandardBuiltinId::FunctionPrototypeCall {
            self.function_proto_calls += 1;
        }
        if builtin == StandardBuiltinId::FunctionPrototypeApply {
            self.function_proto_applies += 1;
        }
        if builtin == StandardBuiltinId::FunctionPrototypeBind {
            self.function_proto_binds += 1;
        }
        if builtin == StandardBuiltinId::FunctionPrototypeToString {
            self.function_proto_to_strings += 1;
        }
        if builtin == StandardBuiltinId::ErrorPrototypeToString {
            self.error_proto_to_strings += 1;
        }
    }

    fn object_like_kind_set() -> KindSet {
        KindSet::from_kind(ValueKind::Object)
            .union(KindSet::from_kind(ValueKind::Array))
            .union(KindSet::from_kind(ValueKind::Function))
            .union(KindSet::from_kind(ValueKind::Arguments))
    }

    fn unknown_construct_result_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: Self::object_like_kind_set(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn construct_explicit_object_return_info(mut return_info: ValueInfo) -> Option<ValueInfo> {
        let possible_kinds = return_info
            .possible_kinds
            .intersection(Self::object_like_kind_set());
        if possible_kinds == KindSet::EMPTY {
            return None;
        }
        return_info.kind = possible_kinds.as_value_kind();
        return_info.possible_kinds = possible_kinds;
        Some(return_info)
    }

    /// `this` in an ordinary sloppy function is object-coerced before the body
    /// runs, but an unobserved call site provides no evidence that it is the
    /// global object. Keep the object-like domain and deliberately omit a
    /// shape so aliases of `this` cannot turn that fallback into a proof.
    fn unobserved_sloppy_this_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: Self::object_like_kind_set(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn value_info_from_shape(shape: Option<Box<HeapShape>>) -> ValueInfo {
        let kind = match shape.as_deref() {
            Some(HeapShape::Array(_)) => ValueKind::Array,
            _ => ValueKind::Object,
        };
        ValueInfo {
            kind,
            possible_kinds: KindSet::from_kind(kind),
            heap_shape: shape,
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn property_descriptor_shape(fields: Vec<(&'static str, ValueInfo)>) -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, info) in fields {
            properties.insert(name.to_string(), ObjectShapeProperty::Data(info));
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    fn generic_property_descriptor_shape() -> Box<HeapShape> {
        let dynamic = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::all_runtime_tags(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        Self::property_descriptor_shape(vec![
            ("value", dynamic.clone()),
            ("writable", dynamic.clone()),
            ("get", dynamic.clone()),
            ("set", dynamic.clone()),
            ("enumerable", Self::boolean_value_info()),
            ("configurable", Self::boolean_value_info()),
        ])
    }

    fn boolean_value_info() -> ValueInfo {
        ValueInfo::new(ValueKind::Boolean)
    }

    fn boxed_receiver_info_from_arg(&self, arg: &TypedExpr) -> Option<ValueInfo> {
        if arg
            .possible_kinds
            .is_subset_of(Self::object_like_kind_set())
        {
            return Some(arg.value_info());
        }
        if arg
            .possible_kinds
            .is_subset_of(Self::boxed_primitive_kind_set())
        {
            return Some(Self::boxed_primitive_instance_info(arg.value_info()));
        }
        None
    }

    fn merge_function_this_info(&mut self, function_id: &FunctionId, info: ValueInfo) {
        let Some(signature) = self.function_signatures.get(function_id).cloned() else {
            return;
        };
        if signature.protocol.flavor() == FunctionFlavor::Arrow {
            return;
        }
        let (next, observed) = if signature.this_observed {
            (self.merge_value_infos(signature.this_info, info), true)
        } else {
            (info, true)
        };
        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            signature.this_info = next;
            signature.this_observed = observed;
        }
    }

    fn merge_function_param_infos(&mut self, function_id: &FunctionId, args: &[ValueInfo]) {
        let Some(signature) = self.function_signatures.get_mut(function_id) else {
            return;
        };
        for (param, arg) in signature.params.iter_mut().zip(args.iter()) {
            if param.is_rest {
                break;
            }
            Self::merge_signature_param_observation(param, arg);
        }
    }

    fn merge_unknown_spread_param_infos(&mut self, function_id: &FunctionId) {
        let Some(signature) = self.function_signatures.get_mut(function_id) else {
            return;
        };
        let unknown_argument = ValueInfo::new(ValueKind::Dynamic);
        for param in &mut signature.params {
            if param.is_rest {
                break;
            }
            Self::merge_signature_param_observation(param, &unknown_argument);
        }
    }

    fn unknown_proxy_target_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: Self::object_like_kind_set(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn proxy_trap_argument_infos(trap: ProxyTrap, target: ValueInfo) -> Vec<ValueInfo> {
        let any = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::all_runtime_tags(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        // ToPropertyKey has exactly this two-kind codomain. In particular, a
        // Symbol must not be narrowed to String merely because most fixtures
        // use an identifier-like key.
        let property_key = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::from_kind(ValueKind::String)
                .union(KindSet::from_kind(ValueKind::Symbol)),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        };
        let descriptor = ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Self::generic_property_descriptor_shape()),
            function_targets: FunctionTargetKnowledge::none(),
        };
        let prototype = ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: Self::object_like_kind_set().union(KindSet::from_kind(ValueKind::Null)),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        };
        let arguments_list = ValueInfo::new(ValueKind::Array);
        let new_target = Self::unknown_proxy_target_info();

        match trap.signature() {
            ProxyTrapSignature::Target => vec![target],
            ProxyTrapSignature::TargetAndPropertyKey => vec![target, property_key],
            ProxyTrapSignature::TargetPropertyKeyReceiver => {
                vec![target, property_key, any]
            }
            ProxyTrapSignature::TargetPropertyKeyValueReceiver => {
                vec![target, property_key, any.clone(), any]
            }
            ProxyTrapSignature::TargetPropertyKeyDescriptor => {
                vec![target, property_key, descriptor]
            }
            ProxyTrapSignature::TargetAndPrototype => vec![target, prototype],
            ProxyTrapSignature::TargetThisArguments => vec![target, any, arguments_list],
            ProxyTrapSignature::TargetArgumentsNewTarget => {
                vec![target, arguments_list, new_target]
            }
        }
    }

    fn merge_proxy_trap_signature_hint(
        &mut self,
        trap: ProxyTrap,
        target: ValueInfo,
        function_id: &FunctionId,
    ) {
        let args = Self::proxy_trap_argument_infos(trap, target);
        self.merge_function_param_infos(function_id, &args);
        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            Self::merge_omitted_signature_params_as_undefined(signature, args.len());
        }
    }

    fn observe_proxy_handler_traps(&mut self, target: &TypedExpr, handler: &TypedExpr) {
        let Some(handler_shape) = handler.heap_shape.as_deref() else {
            return;
        };
        let trap_this = handler.value_info();
        for trap in ProxyTrap::ALL {
            let Some(ObjectShapeProperty::Data(handler_method)) =
                self.read_current_heap_shape_property(handler_shape, trap.property_name())
            else {
                continue;
            };
            if handler_method.kind != ValueKind::Function {
                continue;
            }
            for function_id in handler_method.function_targets.known_targets() {
                self.merge_function_this_info(function_id, trap_this.clone());
                self.merge_proxy_trap_signature_hint(trap, target.value_info(), function_id);
            }
        }
    }

    fn observe_exact_callback_this_info(
        &mut self,
        function_id: &FunctionId,
        helper_context_id: &ExactHelperContextId,
        info: ValueInfo,
    ) {
        let Some(signature) = self.function_signatures.get(function_id).cloned() else {
            return;
        };
        if signature.protocol.flavor() == FunctionFlavor::Arrow {
            return;
        }
        let key = (function_id.clone(), helper_context_id.clone());
        self.exact_context_callback_observations
            .entry(key.clone())
            .or_insert_with(|| {
                let mut signature = signature;
                signature.this_info = ValueInfo::undefined();
                signature.this_observed = false;
                for param in &mut signature.params {
                    if param.is_rest {
                        continue;
                    }
                    Self::clear_signature_param_observation(param);
                }
                signature
            });
        let Some(current) = self.exact_context_callback_observations.get(&key).cloned() else {
            return;
        };
        let next = if current.this_observed {
            self.merge_value_infos(current.this_info, info)
        } else {
            info
        };
        if let Some(observation) = self.exact_context_callback_observations.get_mut(&key) {
            observation.this_info = next;
            observation.this_observed = true;
        }
    }

    fn observe_exact_callback_param_infos(
        &mut self,
        function_id: &FunctionId,
        helper_context_id: &ExactHelperContextId,
        args: &[ValueInfo],
    ) {
        let Some(signature) = self.function_signatures.get(function_id).cloned() else {
            return;
        };
        let key = (function_id.clone(), helper_context_id.clone());
        let observation = self
            .exact_context_callback_observations
            .entry(key)
            .or_insert_with(|| {
                let mut signature = signature;
                signature.this_info = ValueInfo::undefined();
                signature.this_observed = false;
                for param in &mut signature.params {
                    if param.is_rest {
                        continue;
                    }
                    Self::clear_signature_param_observation(param);
                }
                signature
            });
        for (param, arg) in observation.params.iter_mut().zip(args.iter()) {
            if param.is_rest {
                break;
            }
            Self::merge_signature_param_observation(param, arg);
        }
        Self::merge_omitted_signature_params_as_undefined(observation, args.len());
    }

    fn forwarded_apply_args(&self, apply_arg: Option<&TypedExpr>) -> Option<Vec<TypedExpr>> {
        let Some(apply_arg) = apply_arg else {
            return Some(Vec::new());
        };
        if apply_arg.possible_kinds.is_subset_of(
            KindSet::from_kind(ValueKind::Undefined).union(KindSet::from_kind(ValueKind::Null)),
        ) {
            return Some(Vec::new());
        }
        let ExprIr::ArrayLiteral(elements) = &apply_arg.expr else {
            return None;
        };
        (!elements
            .iter()
            .any(|element| matches!(element.expr, ExprIr::ArrayHole)))
        .then(|| elements.clone())
    }

    fn merge_array_species_constructor_this_info(&mut self, receiver: &TypedExpr) {
        let Some(constructor_info) = self.read_object_shape(receiver, "constructor") else {
            return;
        };
        let Some(species_info) =
            self.read_well_known_symbol_shape(&constructor_info, WellKnownSymbol::Species)
        else {
            return;
        };
        let Some(ObjectShapeProperty::Data(prototype_info)) = self.read_object_shape_property(
            &TypedExpr::from_info(species_info.clone(), ExprIr::Undefined),
            "prototype",
        ) else {
            return;
        };
        if !matches!(
            prototype_info.kind,
            ValueKind::Object | ValueKind::Array | ValueKind::Function | ValueKind::Arguments
        ) {
            return;
        }
        let instance = Self::with_instance_prototype(
            Self::fresh_constructed_instance_info(),
            prototype_info.heap_shape,
        );
        for function_id in species_info.function_targets.known_targets() {
            self.merge_function_this_info(function_id, instance.clone());
        }
    }

    /// Result facts are also the admission boundary for this inferred family.
    /// Every admitted target has a result, and none selects a property name.
    fn inferred_indexed_collection_result_info(builtin: StandardBuiltinId) -> Option<ValueInfo> {
        match builtin {
            StandardBuiltinId::ArrayPrototypePush
            | StandardBuiltinId::ArrayPrototypeUnshift
            | StandardBuiltinId::ArrayPrototypeIndexOf
            | StandardBuiltinId::ArrayPrototypeLastIndexOf
            | StandardBuiltinId::ArrayPrototypeFindIndex
            | StandardBuiltinId::ArrayPrototypeFindLastIndex
            | StandardBuiltinId::TypedArrayPrototypeIndexOf
            | StandardBuiltinId::TypedArrayPrototypeLastIndexOf
            | StandardBuiltinId::TypedArrayPrototypeFindIndex
            | StandardBuiltinId::TypedArrayPrototypeFindLastIndex => {
                Some(ValueInfo::new(ValueKind::Number))
            }
            StandardBuiltinId::ArrayPrototypeIncludes
            | StandardBuiltinId::ArrayPrototypeEvery
            | StandardBuiltinId::ArrayPrototypeSome
            | StandardBuiltinId::TypedArrayPrototypeIncludes
            | StandardBuiltinId::TypedArrayPrototypeEvery
            | StandardBuiltinId::TypedArrayPrototypeSome => {
                Some(ValueInfo::new(ValueKind::Boolean))
            }
            StandardBuiltinId::ArrayPrototypeJoin
            | StandardBuiltinId::ArrayPrototypeToLocaleString => {
                Some(ValueInfo::new(ValueKind::String))
            }
            StandardBuiltinId::ArrayPrototypeForEach
            | StandardBuiltinId::TypedArrayPrototypeForEach => Some(ValueInfo::undefined()),
            StandardBuiltinId::ArrayPrototypeConcat
            | StandardBuiltinId::ArrayPrototypeSlice
            | StandardBuiltinId::ArrayPrototypeSplice
            | StandardBuiltinId::ArrayPrototypeFlat
            | StandardBuiltinId::ArrayPrototypeFlatMap
            | StandardBuiltinId::ArrayPrototypeMap
            | StandardBuiltinId::ArrayPrototypeFilter
            | StandardBuiltinId::ArrayPrototypeToReversed
            | StandardBuiltinId::ArrayPrototypeToSpliced
            | StandardBuiltinId::ArrayPrototypeToSorted
            | StandardBuiltinId::ArrayPrototypeWith => Some(Self::unshaped_array_result_info()),
            StandardBuiltinId::ArrayPrototypeFill
            | StandardBuiltinId::ArrayPrototypeSort
            | StandardBuiltinId::ArrayPrototypeReverse
            | StandardBuiltinId::ArrayPrototypeCopyWithin => {
                // These return ToObject(receiver), after mutations and hooks.
                Some(Self::unknown_construct_result_info())
            }
            StandardBuiltinId::ArrayPrototypePop
            | StandardBuiltinId::ArrayPrototypeShift
            | StandardBuiltinId::ArrayPrototypeAt
            | StandardBuiltinId::ArrayPrototypeFind
            | StandardBuiltinId::ArrayPrototypeFindLast
            | StandardBuiltinId::ArrayPrototypeReduce
            | StandardBuiltinId::ArrayPrototypeReduceRight
            | StandardBuiltinId::TypedArrayPrototypeFind
            | StandardBuiltinId::TypedArrayPrototypeFindLast
            | StandardBuiltinId::TypedArrayPrototypeReduce
            | StandardBuiltinId::TypedArrayPrototypeReduceRight
            | StandardBuiltinId::TypedArrayPrototypeToString => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            }),
            StandardBuiltinId::TypedArrayPrototypeMap
            | StandardBuiltinId::TypedArrayPrototypeFilter
            | StandardBuiltinId::ArrayPrototypeKeys
            | StandardBuiltinId::ArrayPrototypeEntries
            | StandardBuiltinId::ArrayPrototypeValues
            | StandardBuiltinId::TypedArrayPrototypeKeys
            | StandardBuiltinId::TypedArrayPrototypeEntries
            | StandardBuiltinId::TypedArrayPrototypeValues => Some(ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                // Iterator methods are mutable inherited properties, and a
                // species-created TypedArray has no validated static layout.
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }),
            _ => None,
        }
    }

    fn unshaped_array_result_info() -> ValueInfo {
        ValueInfo {
            // ArraySpeciesCreate may use a custom constructor, whose result
            // can be any ordinary object rather than an Array. Preserve the
            // runtime tag through bindings and calls instead of reconstructing
            // an Array tag from this unshaped result.
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::from_kind(ValueKind::Array)
                .union(KindSet::from_kind(ValueKind::Object))
                .union(KindSet::from_kind(ValueKind::Function))
                .union(KindSet::from_kind(ValueKind::Arguments)),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn array_literal_from_lowered(elements: Vec<TypedExpr>) -> TypedExpr {
        let mut shape = ArrayShape::default();
        shape.elements = elements.iter().map(TypedExpr::value_info).collect();
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Array,
                possible_kinds: KindSet::from_kind(ValueKind::Array),
                heap_shape: Some(Box::new(HeapShape::Array(shape))),
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::ArrayLiteral(elements),
        )
    }

    fn array_value_info_from_elements(elements: Vec<ValueInfo>) -> ValueInfo {
        let mut shape = ArrayShape::default();
        shape.elements = elements;
        ValueInfo {
            kind: ValueKind::Array,
            possible_kinds: KindSet::from_kind(ValueKind::Array),
            heap_shape: Some(Box::new(HeapShape::Array(shape))),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    fn function_value_expr(&self, function_id: FunctionId) -> TypedExpr {
        let info = self.function_value_info(&function_id);
        TypedExpr::from_info(info, ExprIr::FunctionValue(function_id))
    }

    fn accessor_return_info(&self, function_id: &FunctionId) -> ValueInfo {
        self.function_signatures
            .get(function_id)
            .map(|signature| self.function_call_return_info(signature))
            .unwrap_or_else(ValueInfo::undefined)
    }

    fn observe_proxy_handler_trap_expression_hints(&mut self, args: &[Expression]) {
        let Some(Expression::ObjectLiteral(handler)) = args.get(1) else {
            return;
        };
        for property in handler.properties() {
            match property {
                PropertyDefinition::Property(PropertyName::Literal(name), value) => {
                    let key = self.interner.resolve_expect(name.sym()).to_string();
                    if let Expression::FunctionExpression(function) = value {
                        let function_key = function_expression_key(function);
                        if let Some(function_id) =
                            self.analysis.function_expr_ids.get(&function_key).cloned()
                        {
                            self.merge_proven_proxy_trap_signature_hint(&key, &function_id);
                        }
                    }
                }
                PropertyDefinition::Property(PropertyName::Computed(expr), value) => {
                    let Some(key) = self
                        .try_static_string_key(expr)
                        .or_else(|| self.static_number_property_key(expr))
                    else {
                        continue;
                    };
                    if let Expression::FunctionExpression(function) = value {
                        let function_key = function_expression_key(function);
                        if let Some(function_id) =
                            self.analysis.function_expr_ids.get(&function_key).cloned()
                        {
                            self.merge_proven_proxy_trap_signature_hint(&key, &function_id);
                        }
                    }
                }
                PropertyDefinition::MethodDefinition(method) => {
                    if method.kind() != MethodDefinitionKind::Ordinary {
                        continue;
                    }
                    let Some(key) = (match method.name() {
                        PropertyName::Literal(name) => {
                            Some(self.interner.resolve_expect(name.sym()).to_string())
                        }
                        PropertyName::Computed(expr) => self
                            .try_static_string_key(expr)
                            .or_else(|| self.static_number_property_key(expr)),
                    }) else {
                        continue;
                    };
                    let function_key = object_method_key(method);
                    if let Some(function_id) =
                        self.analysis.function_expr_ids.get(&function_key).cloned()
                    {
                        self.merge_proven_proxy_trap_signature_hint(&key, &function_id);
                    }
                }
                _ => {}
            }
        }
    }

    /// A literal handler passed directly to `new Proxy` is a proven handler,
    /// so all thirteen trap signatures are safe to observe during the
    /// pre-lowering pass.
    fn merge_proven_proxy_trap_signature_hint(
        &mut self,
        property_name: &str,
        function_id: &FunctionId,
    ) {
        let Some(trap) = ProxyTrap::from_property_name(property_name) else {
            return;
        };
        self.merge_proxy_trap_signature_hint(trap, Self::unknown_proxy_target_info(), function_id);
    }

    /// Object literal lowering runs without proof that the object is a Proxy
    /// handler. Preserve its former five-name heuristic; in particular, do not
    /// narrow ordinary methods merely because they are named `apply`, `set`,
    /// or `construct`.
    fn merge_conservative_proxy_named_method_hint(
        &mut self,
        property_name: &str,
        function_id: &FunctionId,
    ) {
        let Some(trap) = ProxyTrap::from_property_name(property_name) else {
            return;
        };
        if !trap.has_conservative_object_literal_hint() {
            return;
        }
        self.merge_proxy_trap_signature_hint(trap, Self::unknown_proxy_target_info(), function_id);
    }

    fn observe_proxy_trap_value_hint(&mut self, trap_name: &str, value: &Expression) {
        let Expression::FunctionExpression(function) = value else {
            return;
        };
        let function_key = function_expression_key(function);
        if let Some(function_id) = self.analysis.function_expr_ids.get(&function_key).cloned() {
            self.merge_conservative_proxy_named_method_hint(trap_name, &function_id);
        }
    }

    fn observe_proxy_trap_method_hint(&mut self, trap_name: &str, method: &ObjectMethodDefinition) {
        if method.kind() != MethodDefinitionKind::Ordinary {
            return;
        }
        let function_key = object_method_key(method);
        if let Some(function_id) = self.analysis.function_expr_ids.get(&function_key).cloned() {
            self.merge_conservative_proxy_named_method_hint(trap_name, &function_id);
        }
    }

    fn lower_regexp_literal(&mut self, regexp: &RegExpLiteral) -> TypedExpr {
        let pattern = self.interned_runtime_string(regexp.pattern());
        let flags = self.interner.resolve_expect(regexp.flags()).to_string();
        let args = vec![
            TypedExpr::from_info(
                Self::string_value_info(&pattern),
                ExprIr::String(pattern.clone()),
            ),
            TypedExpr::from_info(
                Self::string_value_info(&flags),
                ExprIr::String(flags.clone()),
            ),
        ];
        let Some(result) = self.standard_builtin_call_info(
            StandardBuiltinId::RegExpConstructor,
            &args,
            BuiltinCallContext::RegExpLiteral,
        ) else {
            return self.unsupported_expr("regexp literal");
        };
        TypedExpr::from_info(
            result.into_non_call_result(),
            ExprIr::RegExpLiteral {
                static_compilation: Self::static_regexp_compilation_for_pattern(&pattern, &flags),
                source: pattern,
                flags,
            },
        )
    }

    // SuperCall's eager and suspended paths share lowering/super_construct.rs.
    fn lower_super_property_key(&mut self, field: &PropertyAccessField) -> Option<PropertyKeyIr> {
        Some(match field {
            PropertyAccessField::Const(name) => {
                PropertyKeyIr::StaticString(self.interner.resolve_expect(name.sym()).to_string())
            }
            PropertyAccessField::Expr(expr) => {
                let lowered = self.lower_expression(expr);
                match &lowered.expr {
                    ExprIr::String(key) => PropertyKeyIr::StaticString(key.clone()),
                    _ => PropertyKeyIr::StringExpr(Box::new(lowered)),
                }
            }
        })
    }

    fn lower_super_property_access(&mut self, access: &SuperPropertyAccess) -> TypedExpr {
        let Some((key, receiver, info)) = self.lower_super_property_reference_parts(access) else {
            return TypedExpr::undefined();
        };
        TypedExpr::from_info(info, ExprIr::SuperPropertyRead { key, receiver })
    }

    fn lower_private_in(&mut self, binary: &BinaryInPrivate) -> TypedExpr {
        let Some(private_name_id) = self.current_private_name_id(*binary.lhs()) else {
            return self.unsupported_expr("private class element");
        };
        let rhs = self.lower_expression(binary.rhs());
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::PrivateIn {
                private_name_id,
                rhs: Box::new(rhs),
            },
        )
    }

    fn lower_property_target(&mut self, target: &Expression) -> TypedExpr {
        self.lower_expression(target)
    }

    fn is_global_this_expr(&self, expr: &Expression) -> bool {
        matches!(
            Self::unwrap_parenthesized_expr(expr),
            Expression::Identifier(identifier)
                if self.interner.resolve_expect(identifier.sym()).to_string() == GLOBAL_THIS_NAME
                    && self.current_owner_id == SCRIPT_OWNER_ID
                    && self.identifier_resolves_to_intrinsic_global(GLOBAL_THIS_NAME)
        )
    }

    fn is_current_param_expr(&self, expr: &Expression) -> bool {
        let Expression::Identifier(identifier) = expr else {
            return false;
        };
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        self.current_param_names.iter().any(|param| param == &name)
    }

    fn is_builtin_reference_expr(&self, expr: &TypedExpr, name: &str) -> bool {
        (matches!(&expr.expr, ExprIr::Identifier(identifier) if identifier == name)
            || matches!(&expr.expr, ExprIr::GlobalPropertyRead { name: global_name } if global_name == name))
            && self.identifier_resolves_to_intrinsic_global(name)
    }

    fn is_builtin_property_expr(&self, expr: &TypedExpr, builtin: &str, property: &str) -> bool {
        matches!(
            &expr.expr,
            ExprIr::PropertyRead {
                target,
                key: PropertyKeyIr::StaticString(key),
            } if key == property && self.is_builtin_reference_expr(target, builtin)
        )
    }

    // `is_error_prototype_expr` and `is_error_constructor_expr` used to sit
    // here. The latter's only caller rejected `Error.stack` (and every native
    // error constructor's `.stack`) as an unsupported compiler slice. ECMA-262
    // defines no `stack` property on those constructors, so
    // the read is an ordinary [[Get]] that yields undefined unless the program
    // defined the property itself; it now lowers like any other property read.

    fn property_access_field_is_proven_numeric(&self, field: &PropertyAccessField) -> bool {
        let PropertyAccessField::Expr(expr) = field else {
            return false;
        };
        if self.try_constant_array_index_expr(expr).is_some() {
            return true;
        }
        let Expression::Identifier(identifier) = &**expr else {
            return false;
        };
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        self.lookup_binding(&name).is_some_and(|binding| {
            binding
                .possible_kinds
                .is_subset_of(KindSet::from_kind(ValueKind::Number))
        })
    }

    fn property_access_field_is_array_length(&self, field: &PropertyAccessField) -> bool {
        match field {
            PropertyAccessField::Const(name) => {
                self.interner.resolve_expect(name.sym()).to_string() == "length"
            }
            PropertyAccessField::Expr(expr) => {
                self.try_static_string_key(expr).as_deref() == Some("length")
            }
        }
    }

    fn lower_dynamic_object_property_key(&mut self, expr: &Expression) -> Option<PropertyKeyIr> {
        if let Some(key) = self.lower_static_property_key(expr) {
            return Some(key);
        }

        let lowered = self.lower_expression(expr);
        self.record_possible_to_primitive_effects(&lowered.value_info());
        if let ExprIr::String(key) = &lowered.expr {
            return Some(PropertyKeyIr::StaticString(key.clone()));
        }
        if lowered.kind == ValueKind::String {
            return Some(PropertyKeyIr::StringExpr(Box::new(lowered)));
        }
        if lowered.kind == ValueKind::Symbol {
            return Some(PropertyKeyIr::StringExpr(Box::new(lowered)));
        }
        if lowered.possible_kinds.contains(ValueKind::String) {
            return Some(PropertyKeyIr::StringExpr(Box::new(lowered)));
        }
        if lowered.possible_kinds.contains(ValueKind::Symbol) {
            return Some(PropertyKeyIr::StringExpr(Box::new(lowered)));
        }
        if lowered
            .possible_kinds
            .is_subset_of(KindSet::PROPERTY_KEY_COERCIBLE)
        {
            return Some(PropertyKeyIr::StringExpr(Box::new(lowered)));
        }

        None
    }

    fn lower_static_property_key(&mut self, expr: &Expression) -> Option<PropertyKeyIr> {
        let key = self.try_static_string_key(expr)?;
        if is_symbol_description(&key) {
            let lowered = self.lower_expression(expr);
            if lowered.kind == ValueKind::Symbol {
                return Some(PropertyKeyIr::StringExpr(Box::new(lowered)));
            }
        }
        Some(PropertyKeyIr::StaticString(key))
    }

    fn lower_well_known_symbol_property_key(
        &mut self,
        expr: &Expression,
    ) -> Option<(WellKnownSymbol, PropertyKeyIr)> {
        let symbol = self.try_well_known_symbol_key_name(expr)?;
        let lowered = self.lower_expression(expr);
        if lowered.kind != ValueKind::Symbol {
            return None;
        }
        Some((symbol, PropertyKeyIr::StringExpr(Box::new(lowered))))
    }

    fn spec_get_v_operand_from_property_key(key: &PropertyKeyIr) -> Option<TypedExpr> {
        match key {
            PropertyKeyIr::StaticString(name) => Some(TypedExpr::from_info(
                ValueInfo::new(ValueKind::String),
                ExprIr::String(name.clone()),
            )),
            PropertyKeyIr::StringExpr(expr) => Some((**expr).clone()),
            PropertyKeyIr::ArrayIndex(_) | PropertyKeyIr::ArrayLength => None,
        }
    }

    fn is_typed_array_value(&self, _target: &TypedExpr) -> bool {
        false
    }

    fn static_number_property_key(&self, expr: &Expression) -> Option<String> {
        let value = self.try_constant_array_index_expr(expr)?;
        Some(Self::static_number_property_key_from_value(value))
    }

    fn static_number_property_key_from_value(value: f64) -> String {
        Self::js_number_to_string(value)
    }

    fn static_number_index_expr(&self, value: f64) -> TypedExpr {
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Number,
                possible_kinds: KindSet::from_kind(ValueKind::Number),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::Number(value.to_bits()),
        )
    }

    fn static_array_numeric_property_key(&self, expr: &Expression) -> Option<PropertyKeyIr> {
        let value = Self::literal_number_value(Self::unwrap_parenthesized_expr(expr))?;
        if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= MAX_ARRAY_INDEX {
            Some(PropertyKeyIr::ArrayIndex(Box::new(
                self.static_number_index_expr(value),
            )))
        } else {
            Some(PropertyKeyIr::StaticString(Self::js_number_to_string(
                value,
            )))
        }
    }

    fn lower_array_property_key(&mut self, expr: &Expression) -> Option<PropertyKeyIr> {
        self.static_array_numeric_property_key(expr)
            .or_else(|| self.lower_dynamic_object_property_key(expr))
    }

    fn narrow_array_index_expr(&mut self, index: &mut TypedExpr) -> bool {
        if index.kind == ValueKind::Number {
            return true;
        }
        if index.possible_kinds == KindSet::all_runtime_tags() {
            if let ExprIr::Identifier(name) = &index.expr {
                self.set_binding_kind(name, ValueKind::Number);
                index.kind = ValueKind::Number;
                index.possible_kinds = KindSet::from_kind(ValueKind::Number);
                index.heap_shape = None;
                index.function_targets.replace_with_no_function();
                return true;
            }
        }
        if index.possible_kinds.contains(ValueKind::Number)
            && !index.possible_kinds.contains(ValueKind::String)
            && !index.possible_kinds.contains(ValueKind::Symbol)
        {
            index.kind = ValueKind::Number;
            index.possible_kinds = KindSet::from_kind(ValueKind::Number);
            index.heap_shape = None;
            index.function_targets.replace_with_no_function();
            return true;
        }
        false
    }

    fn lower_object_property_key(
        &mut self,
        target: TypedExpr,
        field: &PropertyAccessField,
    ) -> TypedExpr {
        let known_symbol = match field {
            PropertyAccessField::Expr(expression) => {
                self.try_well_known_symbol_key_name(expression)
            }
            PropertyAccessField::Const(_) => None,
        };
        if known_symbol.is_some() {
            let PropertyAccessField::Expr(expression) = field else {
                unreachable!("a known Symbol property has a computed key");
            };
            let (symbol, key) = self
                .lower_well_known_symbol_property_key(expression)
                .expect("the intrinsic Symbol key was proved before lowering it");
            return self.lower_object_well_known_symbol_property(target, symbol, key);
        }
        let known_property_name = match field {
            PropertyAccessField::Const(name) => {
                Some(self.interner.resolve_expect(name.sym()).to_string())
            }
            PropertyAccessField::Expr(expression) => {
                self.try_static_ordinary_property_key(expression)
            }
        };
        let known_property = known_property_name
            .as_deref()
            .and_then(|name| self.read_current_object_shape_property(&target, name));
        let unknown_getter_possible = known_property.is_none();
        let known_getter = match known_property {
            Some(ObjectShapeProperty::Accessor {
                getter: Some(getter),
                ..
            }) => Some(getter.function_id),
            Some(ObjectShapeProperty::Data(_))
            | Some(ObjectShapeProperty::Accessor { getter: None, .. })
            | None => None,
        };
        let known_builtin_getter = known_getter
            .as_ref()
            .and_then(|getter| StandardBuiltinId::from_function_id(getter));
        let receiver_provenance = if target.heap_shape.is_some() {
            BuiltinGetterReceiverProvenance::ProvenNonProxy
        } else {
            BuiltinGetterReceiverProvenance::MayBeProxy
        };
        let known_getter_may_call_user_code = match known_builtin_getter {
            Some(builtin) => {
                Self::standard_builtin_getter_may_call_user_code(builtin, &receiver_provenance)
            }
            None => known_getter.is_some(),
        };
        let builtin_getter_may_dispatch =
            known_builtin_getter.is_some() && known_getter_may_call_user_code;
        let property_get_may_call_user_code =
            unknown_getter_possible || known_getter_may_call_user_code;

        if unknown_getter_possible || builtin_getter_may_dispatch {
            self.observe_all_planned_source_as_unknown_property_hooks();
        }
        let result = self.lower_object_property_key_inner(target, field);
        if property_get_may_call_user_code {
            self.invalidate_unknown_user_code_effects();
        }
        result
    }

    fn lower_object_well_known_symbol_property(
        &mut self,
        target: TypedExpr,
        symbol: WellKnownSymbol,
        key: PropertyKeyIr,
    ) -> TypedExpr {
        if symbol == WellKnownSymbol::HasInstance
            && target.function_targets.exact_single_target()
                == Some(&StandardBuiltinId::FunctionPrototype.function_id())
        {
            // This intrinsic own property is non-writable and non-configurable.
            return TypedExpr::from_info(
                Self::standard_builtin_value_info(
                    StandardBuiltinId::FunctionPrototypeSymbolHasInstance,
                ),
                ExprIr::PropertyRead {
                    target: Box::new(target),
                    key,
                },
            );
        }
        let primitive_prototype = match target.kind {
            ValueKind::String => Some(IntrinsicPrototype::String),
            ValueKind::Symbol => Some(IntrinsicPrototype::Symbol),
            _ => None,
        };
        if let Some(prototype) = primitive_prototype {
            if let Some(read) =
                self.intrinsic_symbol_method_read(prototype, &target, symbol, key.clone())
            {
                return read;
            }
        }
        if let Some(read) = self.intrinsic_object_symbol_method_read(&target, symbol, key.clone()) {
            return read;
        }
        let property = self.read_current_object_symbol_shape_property(&target, symbol);
        let may_call_user_code = match &property {
            Some(ObjectShapeProperty::Data(_))
            | Some(ObjectShapeProperty::Accessor { getter: None, .. }) => false,
            Some(ObjectShapeProperty::Accessor {
                getter: Some(getter),
                ..
            }) => StandardBuiltinId::from_function_id(&getter.function_id).is_none_or(|builtin| {
                Self::standard_builtin_getter_may_call_user_code(
                    builtin,
                    &BuiltinGetterReceiverProvenance::ProvenNonProxy,
                )
            }),
            None => true,
        };
        if may_call_user_code {
            self.observe_all_planned_source_as_unknown_property_hooks();
        }
        let info = match property {
            Some(ObjectShapeProperty::Data(info)) => info,
            Some(ObjectShapeProperty::Accessor {
                getter: Some(getter),
                ..
            }) => {
                self.merge_function_this_info(&getter.function_id, target.value_info());
                if let Some(signature) = self.function_signatures.get_mut(&getter.function_id) {
                    Self::merge_omitted_signature_params_as_undefined(signature, 0);
                }
                self.accessor_return_info(&getter.function_id)
            }
            Some(ObjectShapeProperty::Accessor { getter: None, .. }) => ValueInfo::undefined(),
            None => self.unproven_object_property_info(&target, &shape_namespace_key(symbol)),
        };
        self.mark_host_builtins_from_info(&info);
        let key_operand = Self::spec_get_v_operand_from_property_key(&key)
            .expect("a retained well-known Symbol key is a GetV operand");
        let result = TypedExpr::spec_get_v_with_info(info, target, key_operand);
        if may_call_user_code {
            self.invalidate_unknown_user_code_effects();
        }
        result
    }

    fn lower_object_property_key_inner(
        &mut self,
        target: TypedExpr,
        field: &PropertyAccessField,
    ) -> TypedExpr {
        let key = match field {
            PropertyAccessField::Const(name) => {
                PropertyKeyIr::StaticString(self.interner.resolve_expect(name.sym()).to_string())
            }
            PropertyAccessField::Expr(expr) => {
                if let Some(key) = self.lower_static_property_key(expr) {
                    key
                } else if let Some(index) = self.try_constant_array_index_expr(expr) {
                    if self.is_typed_array_value(&target)
                        || target.possible_kinds.contains(ValueKind::Array)
                    {
                        PropertyKeyIr::ArrayIndex(Box::new(self.static_number_index_expr(index)))
                    } else if let Some(key) = self.static_number_property_key(expr) {
                        PropertyKeyIr::StaticString(key)
                    } else {
                        return self.unsupported_expr("object property key must be string");
                    }
                } else {
                    match self.lower_dynamic_object_property_key(expr) {
                        Some(key) => key,
                        None => {
                            let lowered = self.lower_expression(expr);
                            if lowered.kind == ValueKind::Number
                                && (self.is_typed_array_value(&target)
                                    || target.possible_kinds.contains(ValueKind::Array))
                            {
                                PropertyKeyIr::ArrayIndex(Box::new(lowered))
                            } else {
                                return self.unsupported_expr("object property key must be string");
                            }
                        }
                    }
                }
            }
        };
        let mutable_array_prototype_target = self.array_prototype_mutated
            && (Self::has_array_prototype_shape(&target)
                || self.is_builtin_property_expr(&target, ARRAY_NAME, "prototype"));
        if let PropertyKeyIr::StaticString(name) = &key {
            if target.possible_kinds.contains(ValueKind::Object) {
                if let Some(read) = self.intrinsic_object_method_read(
                    IntrinsicPrototype::RegExp,
                    &target,
                    name,
                    key.clone(),
                ) {
                    return read;
                }
            }
            let observable_array_prototype_lookup = self.array_prototype_mutated
                && (target.possible_kinds.contains(ValueKind::Array)
                    || mutable_array_prototype_target);
            let shape_property = if observable_array_prototype_lookup {
                // Array.prototype itself is mutable, so its intrinsic own
                // properties cannot provide a stable callee or value.
                if mutable_array_prototype_target {
                    None
                } else {
                    self.read_own_object_shape_property(&target, name)
                }
            } else {
                self.read_current_object_shape_property(&target, name)
            };
            let getter_function_id = match &shape_property {
                Some(ObjectShapeProperty::Accessor {
                    getter: Some(getter),
                    ..
                }) => Some(getter.function_id.clone()),
                Some(ObjectShapeProperty::Data(_))
                | Some(ObjectShapeProperty::Accessor { getter: None, .. })
                | None => None,
            };
            if let Some(getter_function_id) = &getter_function_id {
                self.merge_function_this_info(getter_function_id, target.value_info());
                if let Some(signature) = self.function_signatures.get_mut(getter_function_id) {
                    Self::merge_omitted_signature_params_as_undefined(signature, 0);
                }
            }
            if let Some(property) = shape_property {
                let info = match property {
                    ObjectShapeProperty::Data(info) => info,
                    ObjectShapeProperty::Accessor {
                        getter: Some(getter),
                        ..
                    } => self.accessor_return_info(&getter.function_id),
                    ObjectShapeProperty::Accessor { getter: None, .. } => ValueInfo::undefined(),
                };
                self.mark_host_builtins_from_info(&info);
                let result = TypedExpr::from_info(
                    info,
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key,
                    },
                );
                return result;
            }
            // 20.2.3: a function reaches `call`, `apply`, `bind` and
            // `toString` through `%Function.prototype%`. Its own properties are
            // known only through its shape (consulted above), and the
            // prototype's only through the recorded `Function.prototype` fact.
            let target_is_function_prototype = target.function_targets.exact_single_target()
                == Some(&StandardBuiltinId::FunctionPrototype.function_id());
            if target.kind == ValueKind::Function || target_is_function_prototype {
                // Current own/inherited descriptors were consumed above. A
                // remaining shape does not prove the Function prototype chain.
                let lookup = self
                    .intrinsic_method(IntrinsicPrototype::Function, name)
                    .unclaimed();
                if let Some(info) = lookup.callee_info() {
                    return TypedExpr::from_info(
                        info,
                        ExprIr::PropertyRead {
                            target: Box::new(target),
                            key: key.clone(),
                        },
                    );
                }
            }
            if name == "of" && self.is_builtin_reference_expr(&target, ARRAY_NAME) {
                return TypedExpr::from_info(
                    IntrinsicMethodLookup::Unproven(StandardBuiltinId::ArrayOf)
                        .callee_info()
                        .expect("an unproven catalogue entry retains its possible target"),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: key.clone(),
                    },
                );
            }
            if let Some(read) = self.intrinsic_object_method_read(
                IntrinsicPrototype::Object,
                &target,
                name,
                key.clone(),
            ) {
                return read;
            }
        }
        if let PropertyKeyIr::StringExpr(symbol_key) = &key {
            let symbol = match &symbol_key.expr {
                ExprIr::WellKnownSymbol(symbol) => Some(*symbol),
                _ => None,
            };
            if let Some(symbol) = symbol {
                return self.lower_object_well_known_symbol_property(target, symbol, key);
            }
        }
        let info = match &key {
            PropertyKeyIr::StaticString(key) => {
                if let Some(ObjectShapeProperty::Accessor {
                    getter: Some(getter),
                    ..
                }) = (!mutable_array_prototype_target)
                    .then(|| self.read_current_object_shape_property(&target, key))
                    .flatten()
                {
                    self.merge_function_this_info(&getter.function_id, target.value_info());
                }
                (!mutable_array_prototype_target)
                    .then(|| self.read_current_object_shape_property(&target, key))
                    .flatten()
                    .map(|property| match property {
                        ObjectShapeProperty::Data(info) => info,
                        ObjectShapeProperty::Accessor {
                            getter: Some(getter),
                            ..
                        } => self.accessor_return_info(&getter.function_id),
                        ObjectShapeProperty::Accessor { getter: None, .. } => {
                            ValueInfo::undefined()
                        }
                    })
                    .unwrap_or_else(|| self.unproven_object_property_info(&target, key))
            }
            PropertyKeyIr::StringExpr(_) => ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            PropertyKeyIr::ArrayIndex(_) if self.is_typed_array_value(&target) => {
                ValueInfo::new(ValueKind::Number)
            }
            PropertyKeyIr::ArrayIndex(_) | PropertyKeyIr::ArrayLength => ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
        };
        self.mark_host_builtins_from_info(&info);
        if let Some(key_operand) = Self::spec_get_v_operand_from_property_key(&key) {
            return TypedExpr::spec_get_v_with_info(info, target, key_operand);
        }
        TypedExpr::from_info(
            info,
            ExprIr::PropertyRead {
                target: Box::new(target),
                key,
            },
        )
    }

    fn lower_array_index_key(
        &mut self,
        target: TypedExpr,
        field: &PropertyAccessField,
    ) -> TypedExpr {
        if let PropertyAccessField::Const(name) = field {
            if self.interner.resolve_expect(name.sym()).to_string() == "length" {
                return TypedExpr::from_info(
                    ValueInfo::new(ValueKind::Number),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::ArrayLength,
                    },
                );
            }
        }
        if let PropertyAccessField::Expr(expression) = field {
            if let Some(PropertyKeyIr::ArrayIndex(index)) =
                self.static_array_numeric_property_key(expression)
            {
                if let Some(info) = self.read_array_shape(&target, &index) {
                    return TypedExpr::from_info(
                        info,
                        ExprIr::PropertyRead {
                            target: Box::new(target),
                            key: PropertyKeyIr::ArrayIndex(index),
                        },
                    );
                }
            }
        }
        // A named method, a hole, an absent element and an unbounded index may
        // all reach a mutable prototype or accessor. Use the same live Get
        // owner as every other object, including for Symbol.iterator.
        self.lower_object_property_key(target, field)
    }

    fn lower_string_index_key(
        &mut self,
        target: TypedExpr,
        field: &PropertyAccessField,
    ) -> TypedExpr {
        if let PropertyAccessField::Const(name) = field {
            let name = self.interner.resolve_expect(name.sym()).to_string();
            if name == "length" {
                return TypedExpr::from_info(
                    ValueInfo::new(ValueKind::Number),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::ArrayLength,
                    },
                );
            }
            return self.lower_primitive_property_key(IntrinsicPrototype::String, target, field);
        }
        let PropertyAccessField::Expr(expr) = field else {
            return self.unsupported_expr("unsupported string access");
        };
        if self.try_well_known_symbol_key_name(expr) == Some(WellKnownSymbol::Iterator) {
            if let Some((_, symbol_key)) = self.lower_well_known_symbol_property_key(expr) {
                if let Some(read) = self.intrinsic_symbol_method_read(
                    IntrinsicPrototype::String,
                    &target,
                    WellKnownSymbol::Iterator,
                    symbol_key,
                ) {
                    return read;
                }
            }
        }
        let key = self
            .classify_string_exotic_computed_key(expr)
            .into_property_key();
        // Out-of-range String indices and arbitrary computed names can reach
        // prototype accessors. Only a proved in-range own index excludes Get
        // hooks; literal indices here have no ToPropertyKey effects either.
        let own_index = match (&target.expr, &key) {
            (ExprIr::String(value), PropertyKeyIr::ArrayIndex(index)) => self
                .constant_array_index(index)
                .is_some_and(|index| index < value.encode_utf16().count()),
            _ => false,
        };
        if !own_index {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::PropertyRead {
                target: Box::new(target),
                key,
            },
        )
    }

    fn classify_string_exotic_computed_key(
        &mut self,
        expr: &Expression,
    ) -> StringExoticComputedKey {
        if let Some(key) = self.static_array_numeric_property_key(expr) {
            return match key {
                PropertyKeyIr::ArrayIndex(index) => StringExoticComputedKey::CanonicalIndex(index),
                key => StringExoticComputedKey::OrdinaryPropertyKey(key),
            };
        }

        if let Some(key) = self.lower_static_property_key(expr) {
            return match key {
                PropertyKeyIr::StaticString(name) => {
                    if let Some(index) = Self::static_string_exotic_index(&name) {
                        StringExoticComputedKey::CanonicalIndex(Box::new(
                            self.static_number_index_expr(index),
                        ))
                    } else {
                        StringExoticComputedKey::OrdinaryPropertyKey(PropertyKeyIr::StaticString(
                            name,
                        ))
                    }
                }
                key => StringExoticComputedKey::OrdinaryPropertyKey(key),
            };
        }

        let key = self.lower_expression(expr);
        StringExoticComputedKey::OrdinaryPropertyKey(PropertyKeyIr::StringExpr(Box::new(key)))
    }

    fn static_string_exotic_index(name: &str) -> Option<f64> {
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        if name.len() > 1 && name.starts_with('0') {
            return None;
        }
        let index = name.parse::<u64>().ok()?;
        ((index as f64) <= MAX_ARRAY_INDEX).then_some(index as f64)
    }

    fn lower_arguments_index_key(
        &mut self,
        target: TypedExpr,
        field: &PropertyAccessField,
    ) -> TypedExpr {
        if let PropertyAccessField::Const(name) = field {
            let name = self.interner.resolve_expect(name.sym()).to_string();
            if name == "length" {
                return TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Number,
                        possible_kinds: KindSet::from_kind(ValueKind::Number),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::ArrayLength,
                    },
                );
            }
            if name == "hasOwnProperty" {
                return TypedExpr::from_info(
                    Self::standard_builtin_value_info(
                        StandardBuiltinId::ObjectPrototypeHasOwnProperty,
                    ),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::StaticString(name),
                    },
                );
            }
            if name == "propertyIsEnumerable" {
                return TypedExpr::from_info(
                    Self::standard_builtin_value_info(
                        StandardBuiltinId::ObjectPrototypePropertyIsEnumerable,
                    ),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::StaticString(name),
                    },
                );
            }
            if name == "toString" {
                return TypedExpr::from_info(
                    Self::standard_builtin_value_info(StandardBuiltinId::ObjectPrototypeToString),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::StaticString(name),
                    },
                );
            }
            return TypedExpr::from_info(
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                },
                ExprIr::PropertyRead {
                    target: Box::new(target),
                    key: PropertyKeyIr::StaticString(name),
                },
            );
        }
        let PropertyAccessField::Expr(expr) = field else {
            return self.unsupported_expr("unsupported arguments access");
        };
        if self.try_well_known_symbol_key_name(expr) == Some(WellKnownSymbol::Iterator) {
            if let Some((_, symbol_key)) = self.lower_well_known_symbol_property_key(expr) {
                return TypedExpr::from_info(
                    Self::standard_builtin_value_info(StandardBuiltinId::ArrayPrototypeValues),
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: symbol_key,
                    },
                );
            }
        }
        let index = self.lower_expression(expr);
        if index.kind == ValueKind::String {
            if let ExprIr::String(key) = &index.expr {
                return TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    },
                    ExprIr::PropertyRead {
                        target: Box::new(target),
                        key: PropertyKeyIr::StaticString(key.clone()),
                    },
                );
            }
        }
        if index.kind != ValueKind::Number {
            return TypedExpr::from_info(
                ValueInfo {
                    kind: ValueKind::Dynamic,
                    possible_kinds: KindSet::all_runtime_tags(),
                    heap_shape: None,
                    function_targets: FunctionTargetKnowledge::unknown(),
                },
                ExprIr::PropertyRead {
                    target: Box::new(target),
                    key: PropertyKeyIr::StringExpr(Box::new(index)),
                },
            );
        }
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::PropertyRead {
                target: Box::new(target),
                key: PropertyKeyIr::ArrayIndex(Box::new(index)),
            },
        )
    }

    fn lower_web_compat_call_assignment_target(&mut self, call: &Call) -> TypedExpr {
        let evaluated_call = self.lower_call(call.function(), call.args());
        let throw = self.web_compat_call_assignment_reference_error();
        TypedExpr::from_info(
            throw.value_info(),
            ExprIr::Comma {
                lhs: Box::new(evaluated_call),
                rhs: Box::new(throw),
            },
        )
    }

    fn lower_web_compat_loop_assignment_target(
        &mut self,
        call: &Call,
        iterable: &Expression,
    ) -> TypedExpr {
        let evaluated_iterable = self.lower_expression(iterable);
        let call_throw = self.lower_web_compat_call_assignment_target(call);
        TypedExpr::from_info(
            call_throw.value_info(),
            ExprIr::Comma {
                lhs: Box::new(evaluated_iterable),
                rhs: Box::new(call_throw),
            },
        )
    }

    fn web_compat_call_assignment_reference_error(&self) -> TypedExpr {
        TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::RuntimeThrow {
                name: NativeErrorKind::ReferenceError,
                message: "function call assignment target",
            },
        )
    }

    /// PutValue (6.2.5.6) step 5.c into SetMutableBinding (9.1.1.1.5) step 6.b,
    /// for a binding whose `[[Mutable]]` is false, in the one place all of its
    /// callers share.
    ///
    /// Four call sites write through an identifier Reference — plain assignment
    /// (`x = v`), compound assignment (`x += v`), update (`x++`/`x--`) and
    /// destructuring assignment — and every one of them owes the same
    /// observable behaviour for a `const` target: run what the spec has already
    /// evaluated, then throw a `TypeError`. Three of the four used to
    /// `unsupported_expr` instead, which is a *compile-time* refusal of a
    /// program the spec says must run and then throw; the difference is
    /// visible to `assert.throws(TypeError, ...)`.
    ///
    /// `evaluated_operand` is whatever 13.15.2 / 13.4.4.1 has already evaluated
    /// by the time PutValue is reached: the RHS for `x = v`, the *result of the
    /// applied binary operator* for `x += v`, and `ToNumeric(oldValue)` for
    /// `x++`. It becomes the lhs of a `Comma`, which is what sequences its
    /// effects — and its own possible throws, which outrank this one — ahead of
    /// the immutability `TypeError`. A bare `RuntimeThrow` would drop them, and
    /// no test262 case in the three this unblocks would notice.
    /// The single spelling of "this write hits an immutable binding".
    ///
    /// Two things a reader needs, neither of which the compiler enforces yet:
    ///
    /// 1. **Destructuring now shares the same outcome vocabulary.** Its
    ///    [`IdentifierWriteReferenceIr`] distinguishes mutable, ignored
    ///    immutable and abrupt immutable writes, and
    ///    [`IdentifierWriteErrorIr`] owns these two messages. The former
    ///    `immutable: bool` plus backend-only message could drift without a
    ///    compile error.
    ///
    /// 2. **Reaching here is a claim about the source, and it is unmeasured for
    ///    five of the six callers.** Across all 174 snapshots under
    ///    `target/test262-scratch/`, `update of const binding` has hits and
    ///    `assignment to const binding` has zero, so only the update site has
    ///    test262 evidence behind its conversion from a refusal to this
    ///    spec-shaped TypeError. The premise the other five rest on is that
    ///    every `BindingMode::Const` reaching them is a *user* `const` /
    ///    class-name / function-self binding. That premise has been false here
    ///    before: `test262/snapshots/latest-5052292535410439978.json` attributes
    ///    90 `assignment to const binding` refusals to `built-ins/Array/fromAsync`,
    ///    whose 95 files contain no compound assignment at all — i.e. a
    ///    compiler-synthesized write to a synthesized const binding. Under a
    ///    refusal that was an honest `NotImplemented`; under this it is a silent
    ///    wrong answer.
    fn immutable_binding_write(
        &self,
        storage_name: &str,
        evaluated_operand: TypedExpr,
    ) -> TypedExpr {
        Self::render_immutable_binding_write(
            self.immutable_binding_write_outcome(storage_name),
            evaluated_operand,
        )
    }

    fn immutable_binding_write_outcome(&self, storage_name: &str) -> ImmutableBindingWriteOutcome {
        if is_class_name_binding_storage_name(storage_name) {
            ImmutableBindingWriteOutcome::Abrupt(IdentifierWriteErrorIr::ImmutableClassName)
        } else {
            // SetMutableBinding step 6.b: the write throws only when `S` is
            // true. A sloppy reference to a named function expression's own
            // binding is a silent no-op whose value is the operand — the live
            // case, and the reason this carve-out cannot be folded away.
            if self
                .sloppy_immutable_binding_storage_names
                .contains(storage_name)
                && !self.reference_strictness().throws_on_failed_set()
            {
                return ImmutableBindingWriteOutcome::Ignored(
                    IdentifierWriteReferenceIr::ignored_immutable_binding(storage_name.to_owned()),
                );
            }
            ImmutableBindingWriteOutcome::Abrupt(IdentifierWriteErrorIr::ImmutableBinding)
        }
    }

    fn render_immutable_binding_write(
        outcome: ImmutableBindingWriteOutcome,
        evaluated_operand: TypedExpr,
    ) -> TypedExpr {
        let error = match outcome {
            ImmutableBindingWriteOutcome::Ignored(_) => return evaluated_operand,
            ImmutableBindingWriteOutcome::Abrupt(error) => error,
        };
        let error_expr = TypedExpr::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::RuntimeThrow {
                name: error.kind(),
                message: error.message(),
            },
        );
        TypedExpr::from_info(
            error_expr.value_info(),
            ExprIr::Comma {
                lhs: Box::new(evaluated_operand),
                rhs: Box::new(error_expr),
            },
        )
    }

    fn lower_identifier_assign_value(&mut self, name: String, value: TypedExpr) -> TypedExpr {
        let reference = self.locate_identifier_reference(&name);
        self.lower_located_identifier_assign_value(name, value, reference)
    }

    fn lower_located_identifier_assign_value(
        &mut self,
        name: String,
        value: TypedExpr,
        reference: LocatedIdentifierReference,
    ) -> TypedExpr {
        self.lower_located_identifier_assign_value_with_evidence(name, value, reference)
            .value
    }

    fn lower_located_identifier_assign_value_with_evidence(
        &mut self,
        name: String,
        value: TypedExpr,
        reference: LocatedIdentifierReference,
    ) -> PreparedIdentifierWrite {
        // SetMutableBinding (9.1.1.1.5) step 3 runs *before* the immutability
        // test of step 6/7, and does not consult `S`: an assignment to an
        // uninitialized binding is a ReferenceError in sloppy mode too. The RHS
        // has already been evaluated (13.15.2 step 1.e), so its effects are kept
        // ahead of the throw.
        let binding = match reference {
            LocatedIdentifierReference::Declarative { resolution, .. } => match resolution {
                BindingResolution::Uninitialized(violation) => {
                    let error = violation.into_throw();
                    return PreparedIdentifierWrite::performed(TypedExpr::from_info(
                        error.value_info(),
                        ExprIr::Comma {
                            lhs: Box::new(value),
                            rhs: Box::new(error),
                        },
                    ));
                }
                BindingResolution::Initialized(binding) => Some(binding),
                BindingResolution::Unresolvable => {
                    unreachable!("a declarative location cannot be unresolvable")
                }
            },
            LocatedIdentifierReference::Unresolvable => None,
        };
        // Global var metadata owns declaration publication storage, not a
        // declarative source Reference. Its assignment must retain the actual
        // Global Environment even when the backend also has a write temporary.
        let binding = binding.filter(|_| !self.is_unshadowed_script_global_binding(&name));
        if let Some(binding) = binding {
            let storage_name = binding.storage_name.clone();
            if binding.mode == BindingMode::Const {
                // This is PutValue consumer 4.c, and it is decided here, at
                // lowering, which is why the identifier-write IR nodes carry no
                // `[[Strict]]` of their own. The body now lives in
                // `immutable_binding_write` so that the update and
                // compound-assignment consumers cannot drift from it.
                let outcome = self.immutable_binding_write_outcome(&storage_name);
                let ignored = match &outcome {
                    ImmutableBindingWriteOutcome::Ignored(reference) => Some(
                        crate::reference::IgnoredIterationIdentifierWriteIr::from_reference(
                            name,
                            reference.clone(),
                        )
                        .expect("the resolved immutable outcome is the ignored Reference"),
                    ),
                    ImmutableBindingWriteOutcome::Abrupt(_) => None,
                };
                return PreparedIdentifierWrite {
                    value: Self::render_immutable_binding_write(outcome, value),
                    ignored,
                };
            }

            let binding_info = if binding.mode != BindingMode::Var
                && binding.kind != ValueKind::Dynamic
                && binding.kind != ValueKind::Undefined
                && value.kind != binding.kind
            {
                self.merge_value_infos(
                    ValueInfo {
                        kind: binding.kind,
                        possible_kinds: binding.possible_kinds,
                        heap_shape: binding.heap_shape,
                        function_targets: binding.function_targets,
                    },
                    value.value_info(),
                )
            } else {
                value.value_info()
            };
            self.set_binding_value_info(&name, binding_info);

            PreparedIdentifierWrite::performed(TypedExpr::from_info(
                value.value_info(),
                ExprIr::AssignIdentifier {
                    name: storage_name,
                    value: Box::new(value),
                },
            ))
        } else {
            let implicit = !self.global_property_is_proven_present(&name);
            // PutValue step 2.a: assigning through an unresolvable Reference in
            // strict code is a ReferenceError. Whether the name resolves is only
            // decidable at run time (the global object may have grown the
            // property via `globalThis.x = ...`), so the strictness travels with
            // the node and the backend performs the presence check.
            let strictness = self.reference_strictness();
            self.set_global_property_value_info_with_source(
                name.clone(),
                value.value_info(),
                if implicit {
                    GlobalPropertySource::ImplicitGlobalWrite
                } else {
                    GlobalPropertySource::GlobalWrite
                },
            );
            PreparedIdentifierWrite::performed(TypedExpr::from_info(
                value.value_info(),
                ExprIr::GlobalPropertyWrite {
                    name,
                    value: Box::new(value),
                    implicit,
                    strictness,
                },
            ))
        }
    }

    fn lower_with_scoped_identifier_write(
        &mut self,
        name: String,
        value: TypedExpr,
        objects: SelectedWithEnvironmentObjects,
        fallback: LocatedIdentifierReference,
    ) -> TypedExpr {
        self.lower_with_scoped_identifier_write_with_evidence(name, value, objects, fallback)
            .value
    }

    /// The located fallback write is lowered against the plan's single bound
    /// RHS, so `value` is cloned into no branch.
    fn lower_with_scoped_identifier_write_with_evidence(
        &mut self,
        name: String,
        value: TypedExpr,
        objects: SelectedWithEnvironmentObjects,
        fallback: LocatedIdentifierReference,
    ) -> PreparedIdentifierWrite {
        // The located fallback may be skipped when an Object Environment
        // Record owns the name. Keep a mutable declarative binding's old
        // value in that normal-completion path rather than treating the
        // fallback's assignment as a definite write.
        let fallback_before = match &fallback {
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Initialized(binding),
                ..
            } => {
                if binding.mode == BindingMode::Const
                    || self.is_unshadowed_script_global_binding(&name)
                {
                    None
                } else {
                    Some(ValueInfo {
                        kind: binding.kind,
                        possible_kinds: binding.possible_kinds,
                        heap_shape: binding.heap_shape.clone(),
                        function_targets: binding.function_targets.clone(),
                    })
                }
            }
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Uninitialized(_) | BindingResolution::Unresolvable,
                ..
            }
            | LocatedIdentifierReference::Unresolvable => None,
        };
        let assigned_info = value.value_info();
        let plan = self.with_environment_reference_plan(name.clone(), objects);
        let bindings = PutValueBindings::allocate(|prefix| self.alloc_temp_binding_name(prefix));
        let mut ignored = None;
        let value = plan.put_value(bindings, value, |bound| {
            let write = self.lower_located_identifier_assign_value_with_evidence(
                name.clone(),
                bound,
                fallback,
            );
            ignored = write.ignored;
            write.value
        });
        if let Some(before) = fallback_before {
            let joined = self.merge_value_infos(before, assigned_info);
            self.set_binding_value_info(&name, joined);
        }
        // The selected object's PutValue can invoke a setter after the RHS.
        // Its effects also erase configurable-global presence manufactured
        // while constructing a fallback which may never execute. A later
        // source read must still perform ResolveBinding/GetBindingValue.
        self.observe_all_planned_source_as_unknown_property_hooks();
        self.invalidate_unknown_user_code_effects();
        PreparedIdentifierWrite { value, ignored }
    }

    fn lower_pattern_assign(&mut self, pattern: &Pattern, rhs: &Expression) -> TypedExpr {
        let value = self.lower_expression(rhs);
        self.lower_pattern_assign_value(pattern, value)
            .unwrap_or_else(TypedExpr::undefined)
    }

    /// Applies a *destructuring assignment* pattern (13.15.5) to an already lowered
    /// value. Both pattern shapes route through the shared assignment-pattern
    /// lowerings, so every leaf form they support - identifiers, property accesses,
    /// private names, defaults, rest and arbitrary nesting - is available anywhere a
    /// destructuring assignment can appear, including `for`/`for-in`/`for-of` heads.
    fn lower_pattern_assign_value(
        &mut self,
        pattern: &Pattern,
        value: TypedExpr,
    ) -> Option<TypedExpr> {
        let outer_slots = std::mem::take(&mut self.pending_super_destructuring_slots);
        let lowered = self.lower_pattern_assign_value_unscoped(pattern, value);
        let slots = std::mem::replace(&mut self.pending_super_destructuring_slots, outer_slots);
        let mut lowered = lowered?;
        for slot in slots.into_iter().rev() {
            lowered = TypedExpr::from_info(
                lowered.value_info(),
                ExprIr::MaterializeBinding {
                    name: slot,
                    value: Box::new(TypedExpr::undefined()),
                    body: Box::new(lowered),
                },
            );
        }
        Some(lowered)
    }

    fn lower_pattern_assign_value_unscoped(
        &mut self,
        pattern: &Pattern,
        value: TypedExpr,
    ) -> Option<TypedExpr> {
        let value_info = value.value_info();
        Some(match pattern {
            Pattern::Array(pattern) => {
                self.invalidate_unknown_user_code_effects();
                let pattern = self.lower_array_assignment_pattern(pattern.bindings())?;
                TypedExpr::from_info(
                    value_info,
                    ExprIr::ArrayDestructure {
                        value: Box::new(value),
                        pattern,
                        evaluation: ArrayDestructuringEvaluationIr::AssignmentEvaluation,
                    },
                )
            }
            Pattern::Object(pattern) => {
                if !pattern.bindings().is_empty() {
                    self.invalidate_unknown_user_code_effects();
                }
                let pattern = self.lower_object_assignment_pattern(pattern.bindings())?;
                TypedExpr::from_info(
                    value_info,
                    ExprIr::ObjectDestructure {
                        value: Box::new(value),
                        pattern: Box::new(pattern),
                    },
                )
            }
        })
    }

    /// Stages `await <expr>` used as a *destructuring* initializer into a
    /// suspension-owned temporary, returning the prologue statements plus an
    /// expression naming the received value.
    ///
    /// An identifier binding can hand its `await` straight to the linear async
    /// lowering, but a pattern cannot: the suspension has to stand as its own
    /// statement so the resumable driver can split the function there, while the
    /// pattern itself expands into many statements after the resume. Staging the
    /// awaited value first lets every pattern shape reuse the ordinary
    /// `*_from_value` lowerings, so destructuring composes with `await` the same
    /// way it composes with any other initializer.
    ///
    /// Returns `None` when there is nothing to stage - outside a resumable async
    /// body, or when the initializer is not an `await`.
    fn stage_await_destructuring_initializer(
        &mut self,
        init: &Expression,
    ) -> Option<(Vec<StatementIr>, TypedExpr)> {
        if self.current_async_resume_state.is_none() {
            return None;
        }
        let Expression::Await(await_expression) = Self::unwrap_parenthesized_expr(init) else {
            return None;
        };
        let received_name = self.alloc_suspension_owned_binding(
            "async.pattern.received.",
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
        );
        let mut statements = vec![StatementIr::Lexical {
            mode: BindingMode::Let,
            name: received_name.clone(),
            init: TypedExpr::undefined(),
        }];
        let (await_statement, _) = self.lower_linear_async_await(
            await_expression.target(),
            AsyncResumeModeIr::AssignIdentifier(received_name.clone()),
        );
        statements.push(await_statement);
        let value = self.lower_identifier_name(received_name, false);
        Some((statements, value))
    }

    fn lower_pattern_lexical_binding(
        &mut self,
        mode: BindingMode,
        pattern: &Pattern,
        init: &Expression,
    ) -> Option<Vec<StatementIr>> {
        // `const { a } = await p` - predeclare the bound names into TDZ (so the
        // awaited expression still observes them uninitialised), suspend, then run
        // the ordinary pattern lowering against the resumed value.
        if self.current_async_resume_state.is_some()
            && matches!(Self::unwrap_parenthesized_expr(init), Expression::Await(_))
        {
            let binding = Binding::Pattern(pattern.clone());
            let Some(bound_names) = supported_bound_names(self.interner, &binding) else {
                self.unsupported("destructuring binding");
                return None;
            };
            for bound in &bound_names {
                if !self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.contains_key(&bound.source_name))
                {
                    self.declare_binding(
                        bound.source_name.clone(),
                        BindingInfo::tdz_placeholder(
                            mode,
                            TdzPlaceholderName::for_source_name(&bound.source_name),
                        ),
                    );
                }
            }
            let (mut statements, value) = self.stage_await_destructuring_initializer(init)?;
            let mut bindings =
                self.lower_pattern_lexical_binding_from_value(mode, pattern, value)?;
            statements.append(&mut bindings);
            return Some(statements);
        }

        if let Pattern::Array(pattern) = pattern {
            let binding = Binding::Pattern(Pattern::Array(pattern.clone()));
            let Some(bound_names) = supported_bound_names(self.interner, &binding) else {
                self.unsupported("destructuring binding");
                return None;
            };
            for bound in &bound_names {
                if !self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.contains_key(&bound.source_name))
                {
                    self.declare_binding(
                        bound.source_name.clone(),
                        BindingInfo::tdz_placeholder(
                            mode,
                            TdzPlaceholderName::for_source_name(&bound.source_name),
                        ),
                    );
                }
            }

            let value = self.lower_expression(init);
            self.invalidate_unknown_user_code_effects();
            let pattern = self.lower_array_binding_pattern(mode, pattern.bindings(), None)?;
            return Some(vec![StatementIr::DeclarationEvaluation(
                TypedExpr::from_info(
                    ValueInfo::undefined(),
                    ExprIr::ArrayDestructure {
                        value: Box::new(value),
                        pattern,
                        evaluation: ArrayDestructuringEvaluationIr::BindingInitialization,
                    },
                ),
            )]);
        }

        let Pattern::Object(pattern) = pattern else {
            self.unsupported("destructuring binding");
            return None;
        };

        // Computed keys bind the same names as literal keys. The shared
        // ObjectDestructure owner evaluates each key and property value once.
        let binding = Binding::Pattern(Pattern::Object(pattern.clone()));
        let Some(bound_names) = supported_bound_names(self.interner, &binding) else {
            self.unsupported("destructuring binding");
            return None;
        };
        let bound_names: Vec<String> = bound_names
            .into_iter()
            .map(|bound| bound.source_name)
            .collect();

        for name in &bound_names {
            if !self
                .scopes
                .last()
                .is_some_and(|scope| scope.contains_key(name))
            {
                self.declare_binding(
                    name.clone(),
                    BindingInfo::tdz_placeholder(mode, TdzPlaceholderName::for_source_name(name)),
                );
            }
        }

        let init = self.lower_expression(init);
        let temp_name = self.alloc_temp_binding_name("destructure.internal.");
        let temp = TypedExpr::from_info(init.value_info(), ExprIr::Identifier(temp_name.clone()));
        let mut statements = vec![StatementIr::Lexical {
            mode: BindingMode::Let,
            name: temp_name,
            init,
        }];

        let mut bindings =
            self.lower_object_pattern_binding_from_value(mode, pattern.bindings(), temp, None)?;
        statements.append(&mut bindings);
        Some(statements)
    }

    fn lower_pattern_var_binding(
        &mut self,
        pattern: &Pattern,
        init: Option<&Expression>,
    ) -> Option<Vec<StatementIr>> {
        let Some(init) = init else {
            self.unsupported("destructuring binding without initializer");
            return None;
        };

        // `var { a } = await p` - suspend into a temporary first, then destructure
        // the resumed value. `var` needs no TDZ predeclaration.
        if let Some((mut statements, value)) = self.stage_await_destructuring_initializer(init) {
            statements.append(&mut self.lower_pattern_var_binding_from_value(pattern, value)?);
            return Some(statements);
        }

        let init = self.lower_expression(init);
        let temp_name = self.alloc_temp_binding_name("destructure.internal.");
        let temp = TypedExpr::from_info(init.value_info(), ExprIr::Identifier(temp_name.clone()));
        let mut statements = vec![StatementIr::Lexical {
            mode: BindingMode::Let,
            name: temp_name,
            init,
        }];
        statements.append(&mut self.lower_pattern_var_binding_from_value(pattern, temp)?);
        Some(statements)
    }

    fn lower_pattern_var_binding_from_value(
        &mut self,
        pattern: &Pattern,
        init: TypedExpr,
    ) -> Option<Vec<StatementIr>> {
        match pattern {
            Pattern::Array(pattern) => {
                self.invalidate_unknown_user_code_effects();
                let pattern =
                    self.lower_array_binding_pattern(BindingMode::Var, pattern.bindings(), None)?;
                Some(vec![StatementIr::DeclarationEvaluation(
                    TypedExpr::from_info(
                        ValueInfo::undefined(),
                        ExprIr::ArrayDestructure {
                            value: Box::new(init),
                            pattern,
                            evaluation: ArrayDestructuringEvaluationIr::BindingInitialization,
                        },
                    ),
                )])
            }
            Pattern::Object(pattern) => self.lower_object_pattern_binding_from_value(
                BindingMode::Var,
                pattern.bindings(),
                init,
                None,
            ),
        }
    }

    fn lower_pattern_lexical_binding_from_value(
        &mut self,
        mode: BindingMode,
        pattern: &Pattern,
        init: TypedExpr,
    ) -> Option<Vec<StatementIr>> {
        self.lower_pattern_lexical_binding_from_value_with_storage_names(mode, pattern, init, None)
    }

    fn lower_pattern_lexical_binding_from_value_with_storage_names(
        &mut self,
        mode: BindingMode,
        pattern: &Pattern,
        init: TypedExpr,
        storage_names: Option<&BTreeMap<String, String>>,
    ) -> Option<Vec<StatementIr>> {
        match pattern {
            Pattern::Object(pattern) => self.lower_object_pattern_binding_from_value(
                mode,
                pattern.bindings(),
                init,
                storage_names,
            ),
            Pattern::Array(pattern) => {
                self.invalidate_unknown_user_code_effects();
                let pattern =
                    self.lower_array_binding_pattern(mode, pattern.bindings(), storage_names)?;
                Some(vec![StatementIr::DeclarationEvaluation(
                    TypedExpr::from_info(
                        ValueInfo::undefined(),
                        ExprIr::ArrayDestructure {
                            value: Box::new(init),
                            pattern,
                            evaluation: ArrayDestructuringEvaluationIr::BindingInitialization,
                        },
                    ),
                )])
            }
        }
    }

    fn lower_object_pattern_binding_from_value(
        &mut self,
        mode: BindingMode,
        bindings: &[ObjectPatternElement],
        init: TypedExpr,
        storage_names: Option<&BTreeMap<String, String>>,
    ) -> Option<Vec<StatementIr>> {
        if !bindings.is_empty() {
            self.invalidate_unknown_user_code_effects();
        }
        // All object bindings consume one semantic pattern. The Wasm owner
        // retains each acquired property value across its default and target
        // initialization; lowering cannot clone an observable Get into a
        // conditional's condition and nondefault arm.
        let pattern = self.lower_object_binding_pattern(mode, bindings, storage_names)?;
        Some(vec![StatementIr::DeclarationEvaluation(
            TypedExpr::from_info(
                ValueInfo::undefined(),
                ExprIr::ObjectDestructure {
                    value: Box::new(init),
                    pattern: Box::new(pattern),
                },
            ),
        )])
    }

    fn lower_object_binding_pattern(
        &mut self,
        mode: BindingMode,
        bindings: &[ObjectPatternElement],
        storage_names: Option<&BTreeMap<String, String>>,
    ) -> Option<ObjectDestructuringPatternIr> {
        let mut properties = Vec::with_capacity(bindings.len());
        let mut rest = None;
        for binding in bindings {
            match binding {
                ObjectPatternElement::SingleName {
                    name,
                    ident,
                    default_init,
                } => {
                    let key = self.lower_object_destructuring_property_key(name);
                    let default = default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default));
                    let source_name = self.interner.resolve_expect(ident.sym()).to_string();
                    let storage_name = storage_names
                        .and_then(|storage_names| storage_names.get(&source_name))
                        .cloned()
                        .or_else(|| {
                            (mode == BindingMode::Var)
                                .then(|| self.lookup_binding(&source_name))
                                .flatten()
                                .map(|binding| binding.storage_name)
                        })
                        .unwrap_or_else(|| {
                            self.direct_lexical_storage_name(&source_name, ident.span())
                        });
                    // InitializeBinding (9.1.1.1.4) for one BoundName of a
                    // binding pattern (8.6.2 BindingInitialization). Ledger
                    // **L2**: no `PendingInitialization` is threaded here, so
                    // the ordering is correct by construction of the surrounding
                    // code rather than by type — the pattern's value and this
                    // element's default are both lowered above. The storage name
                    // is the one BlockDeclarationInstantiation allocated, via
                    // `direct_lexical_storage_name`'s reuse rule.
                    let target = self.destructuring_binding_target(
                        mode,
                        source_name.clone(),
                        storage_name.clone(),
                    );
                    self.record_destructuring_binding(
                        source_name.clone(),
                        BindingInfo {
                            mode,
                            storage_name: storage_name.clone(),
                            kind: ValueKind::Dynamic,
                            possible_kinds: KindSet::all_runtime_tags(),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::unknown(),
                            initialization: Initialization::Initialized,
                        },
                        &target,
                    );
                    properties.push(ObjectDestructuringPropertyIr {
                        key,
                        target,
                        default,
                    });
                }
                ObjectPatternElement::Pattern {
                    name,
                    pattern,
                    default_init,
                } => {
                    let key = self.lower_object_destructuring_property_key(name);
                    let default = default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default));
                    let target = match pattern {
                        Pattern::Array(pattern) => DestructuringTargetIr::NestedArray(Box::new(
                            self.lower_array_binding_pattern(
                                mode,
                                pattern.bindings(),
                                storage_names,
                            )?,
                        )),
                        Pattern::Object(pattern) => DestructuringTargetIr::NestedObject(Box::new(
                            self.lower_object_binding_pattern(
                                mode,
                                pattern.bindings(),
                                storage_names,
                            )?,
                        )),
                    };
                    properties.push(ObjectDestructuringPropertyIr {
                        key,
                        target,
                        default,
                    });
                }
                ObjectPatternElement::RestProperty { ident } => {
                    let source_name = self.interner.resolve_expect(ident.sym()).to_string();
                    let storage_name = storage_names
                        .and_then(|storage_names| storage_names.get(&source_name))
                        .cloned()
                        .or_else(|| {
                            (mode == BindingMode::Var)
                                .then(|| self.lookup_binding(&source_name))
                                .flatten()
                                .map(|binding| binding.storage_name)
                        })
                        .unwrap_or_else(|| {
                            self.direct_lexical_storage_name(&source_name, ident.span())
                        });
                    // InitializeBinding (9.1.1.1.4) for one BoundName of a
                    // binding pattern (8.6.2 BindingInitialization). Ledger
                    // **L2**: no `PendingInitialization` is threaded here, so
                    // the ordering is correct by construction of the surrounding
                    // code rather than by type — the pattern's value and this
                    // element's default are both lowered above. The storage name
                    // is the one BlockDeclarationInstantiation allocated, via
                    // `direct_lexical_storage_name`'s reuse rule.
                    let target = self.destructuring_binding_target(
                        mode,
                        source_name.clone(),
                        storage_name.clone(),
                    );
                    self.record_destructuring_binding(
                        source_name.clone(),
                        BindingInfo {
                            mode,
                            storage_name: storage_name.clone(),
                            kind: ValueKind::Object,
                            possible_kinds: KindSet::from_kind(ValueKind::Object),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                            initialization: Initialization::Initialized,
                        },
                        &target,
                    );
                    rest = Some(target);
                }
                ObjectPatternElement::AssignmentPropertyAccess { .. }
                | ObjectPatternElement::AssignmentRestPropertyAccess { .. } => {
                    self.unsupported("assignment target in binding pattern");
                    return None;
                }
            }
        }
        Some(ObjectDestructuringPatternIr { properties, rest })
    }

    fn lower_array_binding_pattern(
        &mut self,
        mode: BindingMode,
        bindings: &[ArrayPatternElement],
        storage_names: Option<&BTreeMap<String, String>>,
    ) -> Option<ArrayDestructuringPatternIr> {
        let mut elements = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let element = match binding {
                ArrayPatternElement::Elision => ArrayDestructuringElementIr::Elision,
                ArrayPatternElement::SingleName {
                    ident,
                    default_init,
                } => {
                    let default = default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default));
                    let source_name = self.interner.resolve_expect(ident.sym()).to_string();
                    let storage_name = storage_names
                        .and_then(|storage_names| storage_names.get(&source_name))
                        .cloned()
                        .or_else(|| {
                            (mode == BindingMode::Var)
                                .then(|| self.lookup_binding(&source_name))
                                .flatten()
                                .map(|binding| binding.storage_name)
                        })
                        .unwrap_or_else(|| {
                            self.direct_lexical_storage_name(&source_name, ident.span())
                        });
                    // InitializeBinding (9.1.1.1.4) for one BoundName of a
                    // binding pattern (8.6.2 BindingInitialization). Ledger
                    // **L2**: no `PendingInitialization` is threaded here, so
                    // the ordering is correct by construction of the surrounding
                    // code rather than by type — the pattern's value and this
                    // element's default are both lowered above. The storage name
                    // is the one BlockDeclarationInstantiation allocated, via
                    // `direct_lexical_storage_name`'s reuse rule.
                    let target = self.destructuring_binding_target(
                        mode,
                        source_name.clone(),
                        storage_name.clone(),
                    );
                    self.record_destructuring_binding(
                        source_name.clone(),
                        BindingInfo {
                            mode,
                            storage_name: storage_name.clone(),
                            kind: ValueKind::Dynamic,
                            possible_kinds: KindSet::all_runtime_tags(),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::unknown(),
                            initialization: Initialization::Initialized,
                        },
                        &target,
                    );
                    ArrayDestructuringElementIr::Target { target, default }
                }
                ArrayPatternElement::SingleNameRest { ident } => {
                    let source_name = self.interner.resolve_expect(ident.sym()).to_string();
                    let storage_name = storage_names
                        .and_then(|storage_names| storage_names.get(&source_name))
                        .cloned()
                        .or_else(|| {
                            (mode == BindingMode::Var)
                                .then(|| self.lookup_binding(&source_name))
                                .flatten()
                                .map(|binding| binding.storage_name)
                        })
                        .unwrap_or_else(|| {
                            self.direct_lexical_storage_name(&source_name, ident.span())
                        });
                    // InitializeBinding (9.1.1.1.4) for one BoundName of a
                    // binding pattern (8.6.2 BindingInitialization). Ledger
                    // **L2**: no `PendingInitialization` is threaded here, so
                    // the ordering is correct by construction of the surrounding
                    // code rather than by type — the pattern's value and this
                    // element's default are both lowered above. The storage name
                    // is the one BlockDeclarationInstantiation allocated, via
                    // `direct_lexical_storage_name`'s reuse rule.
                    let target = self.destructuring_binding_target(
                        mode,
                        source_name.clone(),
                        storage_name.clone(),
                    );
                    self.record_destructuring_binding(
                        source_name.clone(),
                        BindingInfo {
                            mode,
                            storage_name: storage_name.clone(),
                            kind: ValueKind::Array,
                            possible_kinds: KindSet::from_kind(ValueKind::Array),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                            initialization: Initialization::Initialized,
                        },
                        &target,
                    );
                    ArrayDestructuringElementIr::Rest { target }
                }
                ArrayPatternElement::Pattern {
                    pattern,
                    default_init,
                } => {
                    let default = default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default));
                    let target = match pattern {
                        Pattern::Array(pattern) => DestructuringTargetIr::NestedArray(Box::new(
                            self.lower_array_binding_pattern(
                                mode,
                                pattern.bindings(),
                                storage_names,
                            )?,
                        )),
                        Pattern::Object(pattern) => DestructuringTargetIr::NestedObject(Box::new(
                            self.lower_object_binding_pattern(
                                mode,
                                pattern.bindings(),
                                storage_names,
                            )?,
                        )),
                    };
                    ArrayDestructuringElementIr::Target { target, default }
                }
                ArrayPatternElement::PatternRest { pattern } => {
                    let target = match pattern {
                        Pattern::Array(pattern) => DestructuringTargetIr::NestedArray(Box::new(
                            self.lower_array_binding_pattern(
                                mode,
                                pattern.bindings(),
                                storage_names,
                            )?,
                        )),
                        Pattern::Object(pattern) => DestructuringTargetIr::NestedObject(Box::new(
                            self.lower_object_binding_pattern(
                                mode,
                                pattern.bindings(),
                                storage_names,
                            )?,
                        )),
                    };
                    ArrayDestructuringElementIr::Rest { target }
                }
                ArrayPatternElement::PropertyAccess { .. }
                | ArrayPatternElement::PropertyAccessRest { .. } => {
                    self.unsupported("assignment target in binding pattern");
                    return None;
                }
            };
            elements.push(element);
        }
        // 8.6.3 IteratorBindingInitialization: this pattern's own
        // `GetIterator`, and its 7.4.11 close under the `[[Done]]` guard, are
        // emitted by `compile_array_destructure_from_value_locals`. Stated here
        // because this is where the obligation is *incurred*.
        Some(ArrayDestructuringPatternIr {
            elements,
            protocol: ArrayPatternProtocol::ARRAY_DESTRUCTURING,
        })
    }

    fn lower_array_assignment_pattern(
        &mut self,
        bindings: &[ArrayPatternElement],
    ) -> Option<ArrayDestructuringPatternIr> {
        let mut elements = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let element = match binding {
                ArrayPatternElement::Elision => ArrayDestructuringElementIr::Elision,
                ArrayPatternElement::SingleName {
                    ident,
                    default_init,
                } => ArrayDestructuringElementIr::Target {
                    target: self.lower_array_assignment_identifier_target(*ident)?,
                    default: default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default)),
                },
                ArrayPatternElement::PropertyAccess {
                    access,
                    default_init,
                } => ArrayDestructuringElementIr::Target {
                    target: self.lower_array_assignment_property_target(access)?,
                    default: default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default)),
                },
                ArrayPatternElement::Pattern {
                    pattern,
                    default_init,
                } => ArrayDestructuringElementIr::Target {
                    target: self.lower_nested_assignment_pattern_target(pattern)?,
                    default: default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default)),
                },
                ArrayPatternElement::SingleNameRest { ident } => {
                    ArrayDestructuringElementIr::Rest {
                        target: self.lower_array_assignment_identifier_target(*ident)?,
                    }
                }
                ArrayPatternElement::PropertyAccessRest { access } => {
                    ArrayDestructuringElementIr::Rest {
                        target: self.lower_array_assignment_property_target(access)?,
                    }
                }
                ArrayPatternElement::PatternRest { pattern } => ArrayDestructuringElementIr::Rest {
                    target: self.lower_nested_assignment_pattern_target(pattern)?,
                },
            };
            elements.push(element);
        }
        // 13.15.5.5 IteratorDestructuringAssignmentEvaluation — a different
        // abstract operation with the same close discipline, running the same
        // emitter arm, which distinguishes the two by `ExprIr::ArrayDestructure`'s
        // closed evaluation operation rather than by protocol. Same witness.
        Some(ArrayDestructuringPatternIr {
            elements,
            protocol: ArrayPatternProtocol::ARRAY_DESTRUCTURING,
        })
    }

    /// Lowers a nested pattern that appears inside a *destructuring assignment*
    /// (13.15.5). Unlike binding patterns the leaves are assignment targets, so
    /// both nesting directions recurse through the assignment lowerings.
    fn lower_nested_assignment_pattern_target(
        &mut self,
        pattern: &Pattern,
    ) -> Option<DestructuringTargetIr> {
        Some(match pattern {
            Pattern::Array(pattern) => DestructuringTargetIr::NestedArray(Box::new(
                self.lower_array_assignment_pattern(pattern.bindings())?,
            )),
            Pattern::Object(pattern) => DestructuringTargetIr::NestedObject(Box::new(
                self.lower_object_assignment_pattern(pattern.bindings())?,
            )),
        })
    }

    fn lower_object_assignment_pattern(
        &mut self,
        bindings: &[ObjectPatternElement],
    ) -> Option<ObjectDestructuringPatternIr> {
        let mut properties = Vec::with_capacity(bindings.len());
        let mut rest = None;
        for binding in bindings {
            match binding {
                ObjectPatternElement::SingleName {
                    name,
                    ident,
                    default_init,
                } => properties.push(ObjectDestructuringPropertyIr {
                    key: self.lower_object_destructuring_property_key(name),
                    target: self.lower_array_assignment_identifier_target(*ident)?,
                    default: default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default)),
                }),
                ObjectPatternElement::AssignmentPropertyAccess {
                    name,
                    access,
                    default_init,
                } => properties.push(ObjectDestructuringPropertyIr {
                    key: self.lower_object_destructuring_property_key(name),
                    target: self.lower_array_assignment_property_target(access)?,
                    default: default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default)),
                }),
                ObjectPatternElement::RestProperty { ident } => {
                    rest = Some(self.lower_array_assignment_identifier_target(*ident)?);
                }
                ObjectPatternElement::AssignmentRestPropertyAccess { access } => {
                    rest = Some(self.lower_array_assignment_property_target(access)?);
                }
                ObjectPatternElement::Pattern {
                    name,
                    pattern,
                    default_init,
                } => {
                    let key = self.lower_object_destructuring_property_key(name);
                    let default = default_init
                        .as_ref()
                        .map(|default| self.lower_expression(default));
                    properties.push(ObjectDestructuringPropertyIr {
                        key,
                        target: self.lower_nested_assignment_pattern_target(pattern)?,
                        default,
                    });
                }
            }
        }
        Some(ObjectDestructuringPatternIr { properties, rest })
    }

    fn lower_object_destructuring_property_key(
        &mut self,
        name: &PropertyName,
    ) -> DestructuringPropertyKeyIr {
        match name {
            PropertyName::Literal(name) => {
                DestructuringPropertyKeyIr::Static(self.interned_runtime_string(name.sym()))
            }
            PropertyName::Computed(expression) => {
                DestructuringPropertyKeyIr::Computed(self.lower_expression(expression))
            }
        }
    }

    fn lower_array_assignment_identifier_target(
        &mut self,
        ident: boa_ast::expression::Identifier,
    ) -> Option<DestructuringTargetIr> {
        let source_name = self.interner.resolve_expect(ident.sym()).to_string();
        if self.uses_runtime_identifier_environment_for(&source_name) {
            return Some(DestructuringTargetIr::AssignmentIdentifier(
                IdentifierWriteReferenceIr::environment(source_name, self.reference_strictness()),
            ));
        }
        match self.resolve_binding_reference(&source_name) {
            // 13.15.5.3 resolves the target now but does not PutValue until the
            // iterator/property value and its default initializer have been
            // evaluated. Consuming the violation into a deferred Reference
            // preserves that order while making 9.1.1.1.5 step 3 executable
            // rather than a lowering refusal.
            BindingResolution::Uninitialized(violation) => {
                Some(DestructuringTargetIr::AssignmentIdentifier(
                    IdentifierWriteReferenceIr::uninitialized_binding(source_name, violation),
                ))
            }
            BindingResolution::Initialized(binding) => {
                let storage_name = binding.storage_name;
                let reference = match binding.mode {
                    BindingMode::Let | BindingMode::Var => {
                        self.set_binding_value_info(
                            &source_name,
                            ValueInfo {
                                kind: ValueKind::Dynamic,
                                possible_kinds: KindSet::all_runtime_tags(),
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::unknown(),
                            },
                        );
                        IdentifierWriteReferenceIr::mutable_binding(storage_name)
                    }
                    BindingMode::Const => {
                        let strictness = self.reference_strictness();
                        if self
                            .sloppy_immutable_binding_storage_names
                            .contains(&storage_name)
                            && !strictness.throws_on_failed_set()
                        {
                            IdentifierWriteReferenceIr::ignored_immutable_binding(storage_name)
                        } else if is_class_name_binding_storage_name(&storage_name) {
                            IdentifierWriteReferenceIr::immutable_class_name(storage_name)
                        } else {
                            IdentifierWriteReferenceIr::immutable_binding(storage_name)
                        }
                    }
                };
                Some(DestructuringTargetIr::AssignmentIdentifier(reference))
            }
            BindingResolution::Unresolvable => {
                let implicit = !self.global_property_is_proven_present(&source_name);
                self.set_global_property_value_info_with_source(
                    source_name.clone(),
                    ValueInfo {
                        kind: ValueKind::Dynamic,
                        possible_kinds: KindSet::all_runtime_tags(),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::unknown(),
                    },
                    if implicit {
                        GlobalPropertySource::ImplicitGlobalWrite
                    } else {
                        GlobalPropertySource::GlobalWrite
                    },
                );
                Some(DestructuringTargetIr::AssignmentIdentifier(
                    IdentifierWriteReferenceIr::global(source_name, self.reference_strictness()),
                ))
            }
        }
    }

    fn lower_array_assignment_property_target(
        &mut self,
        access: &PropertyAccess,
    ) -> Option<DestructuringTargetIr> {
        if let PropertyAccess::Private(access) = access {
            let Some(private_name_id) = self.current_private_name_id(access.field()) else {
                self.unsupported("private class element");
                return None;
            };
            return Some(DestructuringTargetIr::AssignmentPrivate {
                target: self.lower_property_target(access.target()),
                private_name_id,
            });
        }
        let access = match access {
            PropertyAccess::Simple(access) => access,
            PropertyAccess::Super(access) => return self.lower_super_destructuring_target(access),
            PropertyAccess::Private(_) => unreachable!("private access returned above"),
        };
        let target = self.lower_property_target(access.target());
        if !matches!(
            target.kind,
            ValueKind::Object
                | ValueKind::Array
                | ValueKind::Function
                | ValueKind::Arguments
                | ValueKind::Dynamic
                | ValueKind::Undefined
        ) {
            self.unsupported("property access on non-object target");
            return None;
        }
        let key = match access.field() {
            PropertyAccessField::Const(name) => DestructuringPropertyKeyIr::Static(
                self.interner.resolve_expect(name.sym()).to_string(),
            ),
            PropertyAccessField::Expr(expression) => {
                DestructuringPropertyKeyIr::Computed(self.lower_expression(expression))
            }
        };
        // Destructuring PutValue can dispatch a prototype setter, a Proxy
        // trap, or key-coercion source code. This target representation does
        // not yet carry closed accessor provenance, so discard all facts such
        // code could mutate before any later Reference trusts them.
        self.invalidate_unknown_user_code_effects();
        // 13.15.5.4 routes this element through PutValue on the Reference the
        // property access denotes, so its `[[Strict]]` comes from the code
        // that *created* the Reference — the same single producer every other
        // reference-shaped node uses — and not from whichever function the
        // backend later emits the pattern into.
        Some(DestructuringTargetIr::AssignmentProperty {
            target,
            key,
            strictness: self.reference_strictness(),
        })
    }

    /// InitializeBinding (9.1.1.1.4) for one `LexicalBinding`, and the sole
    /// place the identifier-declarator paths make the transition.
    ///
    /// `init` is a [`LoweredInitializer`] rather than a `TypedExpr` because
    /// 14.3.1.2 step 4 must precede step 5: there is no value of that type until
    /// the initializer has been lowered, so "clear the TDZ, then lower the
    /// initializer" — the ordering the deleted `clear_tdz_binding` call left to
    /// convention at ten sites — cannot be written. `let x = x;` therefore
    /// lowers its `x` read against the still-uninitialized binding.
    ///
    /// `pending` is the obligation BlockDeclarationInstantiation left. When it
    /// is present the storage name comes from the *creation* and cannot be
    /// recomputed here, and `initialize` consumes it, so 9.1.1.1.4 step 2's
    /// "must be uninitialized" assertion is `error[E0382]` rather than a review
    /// item.
    fn lower_lexical_binding_value(
        &mut self,
        mode: BindingMode,
        name: String,
        span: boa_ast::Span,
        init: LoweredInitializer,
        pending: Option<PendingInitialization>,
        storage_name: Option<String>,
    ) -> StatementIr {
        self.static_to_string_regexp_object_bindings.remove(&name);
        let initialized = match pending {
            Some(pending) => pending.initialize(init),
            None => {
                let storage_name =
                    storage_name.unwrap_or_else(|| self.direct_lexical_storage_name(&name, span));
                InitializedBinding::without_creation(
                    name.clone(),
                    mode,
                    storage_name,
                    init.into_expr(),
                )
            }
        };
        let statement = initialized.declare(self);
        let binding = self.lookup_binding(&name).unwrap_or_else(|| {
            panic!("lexical binding `{name}` must be declared before clearing static string facts")
        });
        self.static_string_bindings.remove(&binding);
        statement
    }

    fn property_name_to_static_key(&self, name: &PropertyName) -> Option<String> {
        match name {
            PropertyName::Literal(name) => Some(self.interned_runtime_string(name.sym())),
            PropertyName::Computed(expr) => self.try_static_ordinary_property_key(expr),
        }
    }

    /// Lowers `x op= rhs` for an identifier whose binding or right-hand side is
    /// not the plain number/string pair the specialised compound-assign nodes
    /// carry, by reading the binding and applying the ordinary binary operator.
    fn lower_identifier_arithmetic_general(
        &mut self,
        name: &str,
        storage_name: String,
        lhs_info: ValueInfo,
        arithmetic: ArithmeticOp,
        value: TypedExpr,
    ) -> TypedExpr {
        let lhs = TypedExpr::from_info(lhs_info, ExprIr::Identifier(storage_name.clone()));
        let result = self.combine_arithmetic(arithmetic, lhs, value);
        let info = result.value_info();
        self.set_binding_value_info(name, info.clone());
        TypedExpr::from_info(
            info,
            ExprIr::AssignIdentifier {
                name: storage_name,
                value: Box::new(result),
            },
        )
    }

    /// Lowers `ref op= rhs` where `ref` is a property Reference.
    ///
    /// Spec order (13.15.2 / 13.15.3) is: evaluate the LeftHandSideExpression
    /// **once**, GetValue it, evaluate `rhs`, then PutValue through *that same*
    /// Reference. Here the single evaluation is structural rather than
    /// conventional: the Reference is reified once as a [`ReferenceRecord`],
    /// `read` borrows it, `write` consumes it, and the temporaries its
    /// effectful operands were pinned into can only be discharged by spending
    /// the [`ReferencePins`] the same call produced.
    fn lower_property_reference_update(
        &mut self,
        access: &PropertyAccess,
        op: PropertyUpdateOp,
        rhs: &Expression,
    ) -> TypedExpr {
        let read = self.lower_expression(&Expression::PropertyAccess(access.clone()));
        let read_info = read.value_info();
        let base = match reference_base_of_lowered_read(read.expr) {
            Ok(base) => base,
            Err(unsupported) => return self.unsupported_expr(unsupported.feature()),
        };
        // 6.2.5: `[[Strict]]` is populated when the Reference is created, and
        // is carried from here to whichever PutValue consumes it.
        let mut record = ReferenceRecord::create(base, self.reference_strictness());
        let pins = self.pin_reference_operands(&mut record);

        let read = record.read(read_info.clone());
        let (value, shape_info, compose) = match op {
            PropertyUpdateOp::Logical(logical) => {
                let rhs = self.lower_conditionally_reached_expression(rhs);
                // The write only happens on the branch that evaluates `rhs`, so
                // the observable property type is the merge of both branches.
                let written_info = rhs.value_info();
                let merged = self.merge_value_infos(read_info, written_info);
                (
                    rhs,
                    merged.clone(),
                    Composition::ShortCircuit {
                        op: logical,
                        read,
                        merged,
                    },
                )
            }
            PropertyUpdateOp::Arithmetic(arithmetic) => {
                let rhs = self.lower_expression(rhs);
                let value = self.combine_arithmetic(arithmetic, read, rhs);
                let info = value.value_info();
                (value, info, Composition::Value)
            }
            PropertyUpdateOp::Bitwise(bitwise) => {
                let rhs = self.lower_expression(rhs);
                let op = match bitwise {
                    BitwiseOp::And => BitwiseBinaryOp::And,
                    BitwiseOp::Or => BitwiseBinaryOp::Or,
                    BitwiseOp::Xor => BitwiseBinaryOp::Xor,
                    BitwiseOp::Shl => BitwiseBinaryOp::Shl,
                    BitwiseOp::Shr => BitwiseBinaryOp::Shr,
                    BitwiseOp::UShr => BitwiseBinaryOp::UShr,
                };
                let value = self.combine_bitwise(op, read, rhs);
                let info = value.value_info();
                (value, info, Composition::Value)
            }
        };
        self.record_reference_write_shape(access, record.base(), shape_info);
        pins.materialize(record.write(value, compose))
    }

    /// Pins the operands of a Reference that PutValue must not re-evaluate.
    ///
    /// The pins are returned rather than wrapped here: 13.15.2's
    /// `MaterializeBinding` chain has to enclose the *whole* compound
    /// expression, which does not exist yet at this point. [`ReferencePins`]
    /// is the only value that can produce that chain, and it is `#[must_use]`
    /// and not `Clone`, so the wrap can be neither forgotten nor duplicated.
    ///
    /// The record decides *which* operands are pinnable (exhaustively, over
    /// [`ReferenceBase`]); this decides which of them actually need pinning and
    /// what the temporary is called. `ReferenceRecord::pin_operands` is the
    /// only producer of a [`ReferencePins`] in the workspace, so there is no
    /// way to reach `materialize` with a chain that belongs to no record.
    fn pin_reference_operands(&mut self, record: &mut ReferenceRecord) -> ReferencePins {
        record.pin_operands(|operand, expr| {
            if Self::is_repeatable_operand(&expr.expr) {
                return None;
            }
            let info = expr.value_info();
            let name = self.alloc_temp_binding_name(match operand {
                ReferenceOperand::Base => "compound.assign.target.",
                ReferenceOperand::ComputedKey => "compound.assign.key.",
            });
            let pinned = TypedExpr::from_info(info, ExprIr::Identifier(name.clone()));
            Some((name, std::mem::replace(expr, pinned)))
        })
    }

    /// The lowerer-side bookkeeping a PutValue implies: the recorded shape of
    /// whichever object the write lands on goes stale.
    ///
    /// Exhaustive over [`ReferenceBase`] with no catch-all, because "which
    /// object does this write land on" is exactly the question a fifth base
    /// shape would have to answer.
    fn record_reference_write_shape(
        &mut self,
        access: &PropertyAccess,
        base: &ReferenceBase,
        shape_info: ValueInfo,
    ) {
        match base {
            ReferenceBase::Property { key, .. } => {
                if let PropertyAccess::Simple(simple) = access {
                    self.update_written_shape(simple.target(), key, &shape_info);
                }
            }
            // PrivateSet writes a fixed slot of a known class shape; there is
            // no tracked property shape to invalidate.
            ReferenceBase::Private { .. } => {}
            ReferenceBase::Super { key, .. } => {
                // A super Reference reads through the home object's prototype
                // but writes with `this` as the Receiver (PutValue 3.c via
                // GetThisValue), so it is `this`'s recorded shape that goes
                // stale. This covers a constructor body; a method body has no
                // tracked `this` shape to update, the same gap the plain
                // `super.k = v` path has.
                self.update_binding_shape_path(
                    LEXICAL_THIS_NAME,
                    std::slice::from_ref(key),
                    shape_info,
                );
            }
            ReferenceBase::Global { name } => {
                self.set_global_property_value_info(name.clone(), shape_info);
            }
        }
    }

    /// Whether an operand can be duplicated into both the read and the
    /// write-back of a compound assignment without changing what is observed.
    fn is_repeatable_operand(expr: &ExprIr) -> bool {
        matches!(
            expr,
            ExprIr::Identifier(_)
                | ExprIr::This
                | ExprIr::ExecutionGlobalObject
                | ExprIr::Undefined
                | ExprIr::Null
                | ExprIr::Boolean(_)
                | ExprIr::Number(_)
                | ExprIr::String(_)
                | ExprIr::WellKnownSymbol(_)
        )
    }

    fn lower_property_assign_value(
        &mut self,
        access: &PropertyAccess,
        value: TypedExpr,
    ) -> TypedExpr {
        self.record_caller_flow_invalidation();
        if let PropertyAccess::Private(access) = access {
            let Some(private_name_id) = self.current_private_name_id(access.field()) else {
                return self.unsupported_expr("private class element");
            };
            return TypedExpr::from_info(
                value.value_info(),
                ExprIr::PrivateWrite {
                    target: Box::new(self.lower_property_target(access.target())),
                    private_name_id,
                    value: Box::new(value),
                },
            );
        }
        let access = match access {
            PropertyAccess::Simple(access) => access,
            PropertyAccess::Super(access) => {
                return self
                    .lower_super_property_assign_value(access, value)
                    .unwrap_or_else(TypedExpr::undefined);
            }
            PropertyAccess::Private(_) => unreachable!("private access returned above"),
        };
        let target = self.lower_property_target(access.target());
        let key = match target.kind {
            ValueKind::Object | ValueKind::Function => match access.field() {
                PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                    self.interner.resolve_expect(name.sym()).to_string(),
                ),
                PropertyAccessField::Expr(expr) => {
                    if let Some(key) = self.lower_static_property_key(expr) {
                        key
                    } else if let Some(index) = self.try_constant_array_index_expr(expr) {
                        if self.is_typed_array_value(&target) {
                            PropertyKeyIr::ArrayIndex(Box::new(
                                self.static_number_index_expr(index),
                            ))
                        } else if let Some(key) = self.static_number_property_key(expr) {
                            PropertyKeyIr::StaticString(key)
                        } else {
                            return self.unsupported_expr("object property key must be string");
                        }
                    } else if self.property_access_field_is_proven_numeric(access.field()) {
                        let mut lowered = self.lower_expression(expr);
                        if self.narrow_array_index_expr(&mut lowered) {
                            PropertyKeyIr::ArrayIndex(Box::new(lowered))
                        } else {
                            match self.lower_dynamic_object_property_key(expr) {
                                Some(key) => key,
                                None => {
                                    return self
                                        .unsupported_expr("object property key must be string");
                                }
                            }
                        }
                    } else {
                        match self.lower_dynamic_object_property_key(expr) {
                            Some(key) => key,
                            None => {
                                return self.unsupported_expr("object property key must be string");
                            }
                        }
                    }
                }
            },
            ValueKind::Array => match access.field() {
                PropertyAccessField::Const(name) => {
                    let name = self.interner.resolve_expect(name.sym()).to_string();
                    if name == "length" {
                        PropertyKeyIr::ArrayLength
                    } else {
                        PropertyKeyIr::StaticString(name)
                    }
                }
                PropertyAccessField::Expr(expr) => {
                    let Some(key) = self.lower_array_property_key(expr) else {
                        return self.unsupported_expr("array index must be number");
                    };
                    key
                }
            },
            ValueKind::Arguments => match access.field() {
                PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                    self.interner.resolve_expect(name.sym()).to_string(),
                ),
                PropertyAccessField::Expr(expr) => {
                    let Some(key) = self.lower_dynamic_object_property_key(expr) else {
                        return self.unsupported_expr("arguments property key");
                    };
                    key
                }
            },
            ValueKind::Dynamic | ValueKind::Undefined => match access.field() {
                PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                    self.interner.resolve_expect(name.sym()).to_string(),
                ),
                PropertyAccessField::Expr(expr) => {
                    if let Some(index) = self.try_constant_array_index_expr(expr) {
                        PropertyKeyIr::ArrayIndex(Box::new(self.static_number_index_expr(index)))
                    } else if self.property_access_field_is_proven_numeric(access.field()) {
                        let mut lowered = self.lower_expression(expr);
                        if self.narrow_array_index_expr(&mut lowered) {
                            PropertyKeyIr::ArrayIndex(Box::new(lowered))
                        } else {
                            match self.lower_dynamic_object_property_key(expr) {
                                Some(key) => key,
                                None => {
                                    return self
                                        .unsupported_expr("object property key must be string");
                                }
                            }
                        }
                    } else {
                        match self.lower_dynamic_object_property_key(expr) {
                            Some(key) => key,
                            None => {
                                return self.unsupported_expr("object property key must be string");
                            }
                        }
                    }
                }
            },
            _ => return self.unsupported_expr("property access on non-object target"),
        };
        self.update_written_shape(access.target(), &key, &value.value_info());
        let strictness = self.reference_strictness();
        TypedExpr::from_info(
            value.value_info(),
            ExprIr::PropertyWrite {
                target: Box::new(target),
                key,
                value: Box::new(value),
                strictness,
            },
        )
    }

    fn lower_property_assign(&mut self, access: &PropertyAccess, rhs: &Expression) -> TypedExpr {
        self.record_caller_flow_invalidation();
        match access {
            PropertyAccess::Simple(access) => {
                let target = self.lower_property_target(access.target());

                match target.kind {
                    ValueKind::Object | ValueKind::Function => {
                        let key = match access.field() {
                            PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                                self.interner.resolve_expect(name.sym()).to_string(),
                            ),
                            PropertyAccessField::Expr(expr) => {
                                if let Some(key) = self.lower_static_property_key(expr) {
                                    key
                                } else if let Some(index) = self.try_constant_array_index_expr(expr)
                                {
                                    if self.is_typed_array_value(&target) {
                                        PropertyKeyIr::ArrayIndex(Box::new(
                                            self.static_number_index_expr(index),
                                        ))
                                    } else if let Some(key) = self.static_number_property_key(expr)
                                    {
                                        PropertyKeyIr::StaticString(key)
                                    } else {
                                        return self.unsupported_expr(
                                            "object property key must be string",
                                        );
                                    }
                                } else if self
                                    .property_access_field_is_proven_numeric(access.field())
                                {
                                    let mut lowered = self.lower_expression(expr);
                                    if self.narrow_array_index_expr(&mut lowered) {
                                        PropertyKeyIr::ArrayIndex(Box::new(lowered))
                                    } else {
                                        match self.lower_dynamic_object_property_key(expr) {
                                            Some(key) => key,
                                            None => {
                                                return self.unsupported_expr(
                                                    "object property key must be string",
                                                );
                                            }
                                        }
                                    }
                                } else {
                                    match self.lower_dynamic_object_property_key(expr) {
                                        Some(key) => key,
                                        None => {
                                            return self.unsupported_expr(
                                                "object property key must be string",
                                            );
                                        }
                                    }
                                }
                            }
                        };
                        let value = self.lower_expression(rhs);
                        if self.is_builtin_property_expr(&target, ARRAY_NAME, "prototype")
                            || self.is_constructor_prototype_property_expr(
                                &Expression::PropertyAccess(PropertyAccess::Simple(access.clone())),
                                ARRAY_NAME,
                                "toString",
                            )
                        {
                            self.array_prototype_mutated = true;
                        }
                        if self.is_number_prototype_property_expr(
                            &Expression::PropertyAccess(PropertyAccess::Simple(access.clone())),
                            "toString",
                        ) {
                            self.number_prototype_to_string_state =
                                if self.is_object_prototype_property_expr(rhs, "toString") {
                                    PrototypeToStringState::ObjectPrototype
                                } else {
                                    PrototypeToStringState::Unknown
                                };
                        }
                        if self.is_boolean_prototype_property_expr(
                            &Expression::PropertyAccess(PropertyAccess::Simple(access.clone())),
                            "toString",
                        ) {
                            self.boolean_prototype_to_string_state =
                                if self.is_object_prototype_property_expr(rhs, "toString") {
                                    PrototypeToStringState::ObjectPrototype
                                } else {
                                    PrototypeToStringState::Unknown
                                };
                        }
                        if let PropertyKeyIr::StaticString(key_name) = &key {
                            if self.is_global_this_expr(access.target()) {
                                self.set_global_property_value_info_with_source(
                                    key_name.clone(),
                                    value.value_info(),
                                    GlobalPropertySource::GlobalWrite,
                                );
                            }
                            if let Some(ObjectShapeProperty::Accessor {
                                setter: Some(setter),
                                ..
                            }) = self.read_object_shape_property(&target, key_name)
                            {
                                self.merge_function_this_info(
                                    &setter.function_id,
                                    target.value_info(),
                                );
                            }
                        }
                        self.update_written_shape(access.target(), &key, &value.value_info());
                        let strictness = self.reference_strictness();
                        TypedExpr::from_info(
                            value.value_info(),
                            ExprIr::PropertyWrite {
                                target: Box::new(target),
                                key,
                                value: Box::new(value),
                                strictness,
                            },
                        )
                    }
                    ValueKind::Array => {
                        let key = match access.field() {
                            PropertyAccessField::Const(name) => {
                                let name = self.interner.resolve_expect(name.sym()).to_string();
                                if name == "length" {
                                    PropertyKeyIr::ArrayLength
                                } else {
                                    PropertyKeyIr::StaticString(name)
                                }
                            }
                            PropertyAccessField::Expr(expr) => {
                                let Some(key) = self.lower_array_property_key(expr) else {
                                    return self.unsupported_expr("array index must be number");
                                };
                                key
                            }
                        };
                        let value = self.lower_expression(rhs);
                        if Self::has_array_prototype_shape(&target) {
                            self.array_prototype_mutated = true;
                        }
                        self.update_written_shape(access.target(), &key, &value.value_info());
                        let strictness = self.reference_strictness();
                        TypedExpr::from_info(
                            value.value_info(),
                            ExprIr::PropertyWrite {
                                target: Box::new(target),
                                key,
                                value: Box::new(value),
                                strictness,
                            },
                        )
                    }
                    ValueKind::Arguments => {
                        let key = match access.field() {
                            PropertyAccessField::Const(name) => {
                                let name = self.interner.resolve_expect(name.sym()).to_string();
                                PropertyKeyIr::StaticString(name)
                            }
                            PropertyAccessField::Expr(expr) => {
                                let index = self.lower_expression(expr);
                                if index.kind == ValueKind::String {
                                    match &index.expr {
                                        ExprIr::String(key) => {
                                            PropertyKeyIr::StaticString(key.clone())
                                        }
                                        _ => PropertyKeyIr::StringExpr(Box::new(index)),
                                    }
                                } else if index.kind == ValueKind::Number {
                                    PropertyKeyIr::ArrayIndex(Box::new(index))
                                } else if index
                                    .possible_kinds
                                    .is_subset_of(KindSet::PROPERTY_KEY_COERCIBLE)
                                {
                                    PropertyKeyIr::StringExpr(Box::new(index))
                                } else {
                                    return self.unsupported_expr("arguments index must be number");
                                }
                            }
                        };
                        let value = self.lower_expression(rhs);
                        let strictness = self.reference_strictness();
                        TypedExpr::from_info(
                            value.value_info(),
                            ExprIr::PropertyWrite {
                                target: Box::new(target),
                                key,
                                value: Box::new(value),
                                strictness,
                            },
                        )
                    }
                    ValueKind::Dynamic | ValueKind::Undefined => {
                        let key = match access.field() {
                            PropertyAccessField::Const(name) => PropertyKeyIr::StaticString(
                                self.interner.resolve_expect(name.sym()).to_string(),
                            ),
                            PropertyAccessField::Expr(expr) => {
                                if let Some(index) = self.try_constant_array_index_expr(expr) {
                                    PropertyKeyIr::ArrayIndex(Box::new(
                                        self.static_number_index_expr(index),
                                    ))
                                } else if self
                                    .property_access_field_is_proven_numeric(access.field())
                                {
                                    let mut lowered = self.lower_expression(expr);
                                    if self.narrow_array_index_expr(&mut lowered) {
                                        PropertyKeyIr::ArrayIndex(Box::new(lowered))
                                    } else {
                                        match self.lower_dynamic_object_property_key(expr) {
                                            Some(key) => key,
                                            None => {
                                                return self.unsupported_expr(
                                                    "object property key must be string",
                                                );
                                            }
                                        }
                                    }
                                } else {
                                    match self.lower_dynamic_object_property_key(expr) {
                                        Some(key) => key,
                                        None => {
                                            return self.unsupported_expr(
                                                "object property key must be string",
                                            );
                                        }
                                    }
                                }
                            }
                        };
                        let value = self.lower_expression(rhs);
                        let strictness = self.reference_strictness();
                        TypedExpr::from_info(
                            value.value_info(),
                            ExprIr::PropertyWrite {
                                target: Box::new(target),
                                key,
                                value: Box::new(value),
                                strictness,
                            },
                        )
                    }
                    ValueKind::Symbol => {
                        // PutValue on a primitive base: OrdinarySet's final
                        // `Type(Receiver) is not Object` check makes [[Set]]
                        // fail without throwing; PutValue itself only
                        // throws when running in strict mode. Either way
                        // there is no real mutation: evaluate `rhs` (for its
                        // side effects/value) and drop the write.
                        let value = self.lower_expression(rhs);
                        // PutValue 3.d, folded at compile time: `[[Set]]` on a
                        // primitive Receiver always answers `false`, so the
                        // Reference's `[[Strict]]` alone decides the outcome.
                        if self.reference_strictness().throws_on_failed_set() {
                            TypedExpr::from_info(
                                value.value_info(),
                                ExprIr::RuntimeThrow {
                                    name: NativeErrorKind::TypeError,
                                    message: "Cannot create property on symbol",
                                },
                            )
                        } else {
                            value
                        }
                    }
                    _ => self.unsupported_expr("property access on non-object target"),
                }
            }
            PropertyAccess::Private(access) => {
                let Some(private_name_id) = self.current_private_name_id(access.field()) else {
                    return self.unsupported_expr("private class element");
                };
                let target = self.lower_property_target(access.target());
                let value = self.lower_expression(rhs);
                TypedExpr::from_info(
                    value.value_info(),
                    ExprIr::PrivateWrite {
                        target: Box::new(target),
                        private_name_id,
                        value: Box::new(value),
                    },
                )
            }
            PropertyAccess::Super(access) => {
                if self.class_context.is_none() {
                    return self.unsupported_expr("object literal method");
                }
                let Some(key) = self.lower_super_property_key(access.field()) else {
                    return TypedExpr::undefined();
                };
                let receiver = self.lower_current_this();
                let value = self.lower_expression(rhs);
                let strictness = self.reference_strictness();
                TypedExpr::from_info(
                    value.value_info(),
                    ExprIr::SuperPropertyWrite {
                        key,
                        receiver: Box::new(receiver),
                        value: Box::new(value),
                        strictness,
                    },
                )
            }
        }
    }

    /// 13.4 `++`/`--` on a property Reference.
    ///
    /// Extracted from `lower_update` so both the AST target and the ordinary,
    /// Super, and private Reference domains remain exhaustive.
    fn lower_property_access_update(&mut self, op: UpdateOp, access: &PropertyAccess) -> TypedExpr {
        match access {
            PropertyAccess::Simple(access) => {
                self.lower_ordinary_property_numeric_update(op, access)
            }
            PropertyAccess::Super(access) => self.lower_super_property_numeric_update(op, access),
            PropertyAccess::Private(access) => self.lower_private_numeric_update(op, access),
        }
    }

    fn lower_update(&mut self, op: UpdateOp, target: &UpdateTarget) -> TypedExpr {
        // `UpdateTarget` is a closed 3-variant boa enum
        // (`boa_ast-0.21.1/src/expression/operator/update/mod.rs:129`).
        // Matched exhaustively rather than as two `if let`s and a
        // `let ... else`, so a fourth production that yields a Reference is
        // `error[E0004]` here instead of a silent `unsupported_expr`. That is
        // invariant I7's AST half for update expressions.
        let identifier = match target {
            // Annex B `f()++` is a runtime ReferenceError, not a compiler gap.
            UpdateTarget::WebCompatCall(call) => {
                return self.lower_web_compat_call_assignment_target(call);
            }
            UpdateTarget::PropertyAccess(access) => {
                return self.lower_property_access_update(op, access);
            }
            UpdateTarget::Identifier(identifier) => identifier,
        };

        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        if self.uses_runtime_identifier_environment_for(&name) {
            let (operation, return_mode) = match op {
                UpdateOp::IncrementPost => (NumericUpdateOp::Increment, UpdateReturnMode::Postfix),
                UpdateOp::IncrementPre => (NumericUpdateOp::Increment, UpdateReturnMode::Prefix),
                UpdateOp::DecrementPost => (NumericUpdateOp::Decrement, UpdateReturnMode::Postfix),
                UpdateOp::DecrementPre => (NumericUpdateOp::Decrement, UpdateReturnMode::Prefix),
            };
            return self.environment_identifier(
                name,
                EnvironmentIdentifierOperationIr::Update {
                    operation,
                    return_mode,
                },
            );
        }
        let reference = self.locate_identifier_reference(&name);
        let selected = self
            .with_environment_chain
            .select_preceding(reference.declarative_position());
        let (op, return_mode) = match op {
            UpdateOp::IncrementPost => (NumericUpdateOp::Increment, UpdateReturnMode::Postfix),
            UpdateOp::IncrementPre => (NumericUpdateOp::Increment, UpdateReturnMode::Prefix),
            UpdateOp::DecrementPost => (NumericUpdateOp::Decrement, UpdateReturnMode::Postfix),
            UpdateOp::DecrementPre => (NumericUpdateOp::Decrement, UpdateReturnMode::Prefix),
        };
        if let Some(objects) = selected {
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
            let plan = self.with_environment_reference_plan(name.clone(), objects);
            let fallback = self.lower_located_identifier_numeric_update(
                name,
                op,
                return_mode,
                reference,
                IdentifierUpdateReachability::WithEnvironmentFallback,
            );
            let bindings =
                NumericUpdateBindings::allocate(|prefix| self.alloc_temp_binding_name(prefix));
            return plan.numeric_update(op, return_mode, bindings, fallback);
        }
        self.lower_located_identifier_numeric_update(
            name,
            op,
            return_mode,
            reference,
            IdentifierUpdateReachability::Definite,
        )
    }

    fn lower_global_identifier_numeric_update(
        &mut self,
        name: String,
        op: NumericUpdateOp,
        return_mode: UpdateReturnMode,
    ) -> TypedExpr {
        self.observe_all_planned_source_as_unknown_property_hooks();
        self.invalidate_unknown_user_code_effects();
        let possible_kinds =
            KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::BigInt));
        TypedExpr::from_info(
            ValueInfo {
                kind: possible_kinds.as_value_kind(),
                possible_kinds,
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::EnvironmentIdentifier(Box::new(EnvironmentIdentifierIr::global(
                name,
                self.reference_strictness(),
                EnvironmentIdentifierOperationIr::Update {
                    operation: op,
                    return_mode,
                },
            ))),
        )
    }

    fn lower_located_identifier_numeric_update(
        &mut self,
        name: String,
        op: NumericUpdateOp,
        return_mode: UpdateReturnMode,
        reference: LocatedIdentifierReference,
        reachability: IdentifierUpdateReachability,
    ) -> TypedExpr {
        if self.is_unshadowed_script_global_binding(&name)
            || matches!(&reference, LocatedIdentifierReference::Unresolvable)
        {
            return self.lower_global_identifier_numeric_update(name, op, return_mode);
        }
        // 13.4.4 / 13.4.5 UpdateExpression: GetValue then PutValue, so
        // 9.1.1.1.6 step 2 and 9.1.1.1.5 step 3 both apply. `x++` on an
        // uninitialized binding used to read the slot.
        let binding = match reference {
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Uninitialized(violation),
                ..
            } => return violation.into_throw(),
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Initialized(binding),
                ..
            } => binding,
            LocatedIdentifierReference::Unresolvable => {
                unreachable!("global updates use their retained Environment Reference")
            }
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Unresolvable,
                ..
            } => unreachable!("a declarative location cannot be unresolvable"),
        };
        if binding.mode == BindingMode::Const {
            // 13.4.4.1 (and 13.4.5.1 / the prefix forms) run
            //   1. expr = evaluate the UnaryExpression
            //   2. oldValue = ToNumeric(GetValue(expr))
            //   3. newValue = the numeric op on oldValue
            //   4. PutValue(expr, newValue)
            // — so the ToNumeric of step 2 happens *before* the PutValue of
            // step 4 fails, and it can throw first. `const s = Symbol();
            // s++` must report ToNumeric's TypeError, not the immutability
            // one, and `const o = { valueOf() { log(); return 1; } }; o++`
            // must call `valueOf`. That is why the operand handed to
            // `immutable_binding_write` is the coercion and not the bare
            // read. Step 3 is unobservable once step 4 always throws.
            let old_value = TypedExpr::spec_to_numeric(TypedExpr::from_info(
                ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                },
                ExprIr::Identifier(binding.storage_name.clone()),
            ));
            return self.immutable_binding_write(&binding.storage_name, old_value);
        }
        let update_kind = if binding
            .possible_kinds
            .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
        {
            NumericUpdateValueKind::BigInt
        } else if binding
            .possible_kinds
            .is_subset_of(KindSet::PRIMITIVE_ONLY.without(ValueKind::BigInt))
        {
            NumericUpdateValueKind::Number
        } else {
            NumericUpdateValueKind::Dynamic
        };
        let storage_name = binding.storage_name.clone();
        let emitted_update_kind = match reachability {
            IdentifierUpdateReachability::Definite => update_kind,
            IdentifierUpdateReachability::WithEnvironmentFallback => {
                NumericUpdateValueKind::Dynamic
            }
        };
        let updated_info = match reachability {
            IdentifierUpdateReachability::Definite => ValueInfo::new(update_kind.value_kind()),
            IdentifierUpdateReachability::WithEnvironmentFallback => {
                let mut value = ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape,
                    function_targets: binding.function_targets,
                };
                value.widen_for_possible_replacement();
                value
            }
        };
        self.set_binding_value_info(&name, updated_info);
        TypedExpr::from_info(
            ValueInfo::new(emitted_update_kind.value_kind()),
            ExprIr::UpdateIdentifier {
                name: storage_name,
                op,
                return_mode,
                value_kind: emitted_update_kind,
            },
        )
    }

    fn lower_global_identifier_typeof(&mut self, name: String) -> TypedExpr {
        if let Some(host) = self.host_surface_policy.resolve_global(&name) {
            self.used_host_builtins.insert(host);
        }
        // ResolveBinding can call an inherited Proxy HasProperty trap, and
        // GetBindingValue can invoke a getter. Neither operation may retain
        // source facts from before that user code; only absence skips Get.
        self.observe_all_planned_source_as_unknown_property_hooks();
        self.invalidate_unknown_user_code_effects();
        TypedExpr::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::TypeOfUnresolvedIdentifier { name },
        )
    }

    fn lower_unary(&mut self, op: UnaryOp, target: &Expression) -> TypedExpr {
        if matches!(op, UnaryOp::Delete) {
            return self.lower_delete(target);
        }
        if matches!(op, UnaryOp::TypeOf) {
            if let Expression::Identifier(identifier) = Self::unwrap_parenthesized_expr(target) {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if self.uses_runtime_identifier_environment_for(&name) {
                    return self
                        .environment_identifier(name, EnvironmentIdentifierOperationIr::Typeof);
                }
                let fallback = self.locate_identifier_reference(&name);
                let selected = self
                    .with_environment_chain
                    .select_preceding(fallback.declarative_position());
                if self.is_unshadowed_script_global_binding(&name)
                    || (name == GLOBAL_THIS_NAME
                        && self.lookup_binding(&name).is_none()
                        && !self.is_global_this_expr(target))
                {
                    let global_typeof = self.lower_global_identifier_typeof(name.clone());
                    return match selected {
                        Some(objects) => self
                            .with_environment_reference_plan(name, objects)
                            .typeof_value(global_typeof),
                        None => global_typeof,
                    };
                }
                let is_bound = (self.lookup_binding(&name).is_some()
                    && !self.is_unshadowed_script_global_binding(&name))
                    || self.global_property_is_proven_present(&name)
                    || (self.root_functions_need_body_initialization()
                        && self.visible_function_names.contains_key(&name))
                    || (name == "arguments"
                        && self.lookup_binding(LEXICAL_ARGUMENTS_NAME).is_some());
                if let Some(objects) = selected {
                    let plan = self.with_environment_reference_plan(name.clone(), objects);
                    if is_bound {
                        let fallback = self.lower_identifier_name(name, false);
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::String),
                            ExprIr::TypeOf {
                                expr: Box::new(plan.get_value(fallback)),
                            },
                        );
                    }
                    return plan.typeof_value(self.lower_global_identifier_typeof(name));
                }
                if !is_bound {
                    return self.lower_global_identifier_typeof(name);
                }
            }
        }
        let lowered_target = self.lower_expression(target);
        if op == UnaryOp::Plus {
            if let Some(value) = self.static_to_number_expr(target) {
                return TypedExpr::from_info(
                    ValueInfo::new(ValueKind::Number),
                    ExprIr::Number(value.to_bits()),
                );
            }
        }
        self.combine_unary_value(op, lowered_target)
    }

    fn lower_binary(&mut self, op: BinaryOp, lhs: &Expression, rhs: &Expression) -> TypedExpr {
        match op {
            BinaryOp::Arithmetic(arithmetic) => self.lower_arithmetic(arithmetic, lhs, rhs),
            BinaryOp::Relational(relational) => self.lower_relational(relational, lhs, rhs),
            BinaryOp::Logical(logical) => self.lower_logical(logical, lhs, rhs),
            BinaryOp::Bitwise(bitwise) => self.lower_bitwise(bitwise, lhs, rhs),
            BinaryOp::Comma => {
                let rhs_suspends = contains(rhs, ContainsSymbol::AwaitExpression);
                let lhs = self.lower_expression(lhs);
                let lhs =
                    self.pin_async_operand_before_suspension(lhs, rhs_suspends, "async.comma.lhs.");
                let rhs = self.lower_expression(rhs);
                TypedExpr::from_info(
                    rhs.value_info(),
                    ExprIr::Comma {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                )
            }
        }
    }

    fn lower_bitwise(
        &mut self,
        bitwise: BitwiseOp,
        lhs: &Expression,
        rhs: &Expression,
    ) -> TypedExpr {
        let rhs_suspends = contains(rhs, ContainsSymbol::AwaitExpression);
        let lhs = self.lower_expression(lhs);
        let lhs = self.pin_async_operand_before_suspension(lhs, rhs_suspends, "async.bitwise.lhs.");
        let rhs = self.lower_expression(rhs);
        let op = match bitwise {
            BitwiseOp::And => BitwiseBinaryOp::And,
            BitwiseOp::Or => BitwiseBinaryOp::Or,
            BitwiseOp::Xor => BitwiseBinaryOp::Xor,
            BitwiseOp::Shl => BitwiseBinaryOp::Shl,
            BitwiseOp::Shr => BitwiseBinaryOp::Shr,
            BitwiseOp::UShr => BitwiseBinaryOp::UShr,
        };
        self.combine_bitwise(op, lhs, rhs)
    }

    /// Applies a bitwise operator to two already-lowered operands.
    ///
    /// The IR retains both operands so the backend can evaluate both before
    /// starting the ordered ToNumeric conversions. The result domain records
    /// only normal completions: mixed Number/BigInt pairs and BigInt `>>>`
    /// throw instead of contributing a result kind.
    fn combine_bitwise(
        &mut self,
        op: BitwiseBinaryOp,
        lhs: TypedExpr,
        rhs: TypedExpr,
    ) -> TypedExpr {
        let lhs_primitive = self.to_primitive_info(&lhs, ToPrimitiveHint::Number);
        let rhs_primitive = self.to_primitive_info(&rhs, ToPrimitiveHint::Number);
        let (lhs_number, lhs_bigint) = numeric_domain(lhs_primitive.as_ref());
        let (rhs_number, rhs_bigint) = numeric_domain(rhs_primitive.as_ref());

        let mut result_kinds = KindSet::EMPTY;
        if lhs_number && rhs_number {
            result_kinds = result_kinds.union(KindSet::from_kind(ValueKind::Number));
        }
        if op.bigint_op().is_some() && lhs_bigint && rhs_bigint {
            result_kinds = result_kinds.union(KindSet::from_kind(ValueKind::BigInt));
        }
        if result_kinds == KindSet::EMPTY {
            // The expression has no normal completion, but every TypedExpr
            // still carries a representable result domain for unreachable
            // consumers. Number is also the only normal domain of `>>>`.
            result_kinds = KindSet::from_kind(ValueKind::Number);
        }

        TypedExpr::from_info(
            ValueInfo {
                kind: result_kinds.as_value_kind(),
                possible_kinds: result_kinds,
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::BitwiseNumeric {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        )
    }

    /// Applies a unary bitwise operator to an already-lowered operand.
    ///
    /// ToNumeric chooses the Number or BigInt operation after evaluating the
    /// operand once. Only those normal result kinds are recorded here; Symbol
    /// and other throwing coercions do not add a result domain.
    fn combine_unary_bitwise(&mut self, op: UnaryBitwiseOp, operand: TypedExpr) -> TypedExpr {
        let primitive = self.to_primitive_info(&operand, ToPrimitiveHint::Number);
        let (has_number, has_bigint) = numeric_domain(primitive.as_ref());
        let mut result_kinds = KindSet::EMPTY;
        if has_number {
            result_kinds = result_kinds.union(KindSet::from_kind(ValueKind::Number));
        }
        if has_bigint {
            result_kinds = result_kinds.union(KindSet::from_kind(ValueKind::BigInt));
        }
        if result_kinds == KindSet::EMPTY {
            result_kinds = KindSet::from_kind(ValueKind::Number);
        }

        TypedExpr::from_info(
            ValueInfo {
                kind: result_kinds.as_value_kind(),
                possible_kinds: result_kinds,
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::UnaryBitwiseNumeric {
                op,
                expr: Box::new(operand),
            },
        )
    }

    fn lower_arithmetic(
        &mut self,
        arithmetic: ArithmeticOp,
        lhs: &Expression,
        rhs: &Expression,
    ) -> TypedExpr {
        if matches!(arithmetic, ArithmeticOp::Exp) {
            if let (Some(lhs), Some(rhs)) =
                (self.static_number_expr(lhs), self.static_number_expr(rhs))
            {
                return TypedExpr::from_info(
                    ValueInfo::new(ValueKind::Number),
                    ExprIr::Number(Self::static_pow(lhs, rhs).to_bits()),
                );
            }
        }

        let lhs_is_direct_await =
            matches!(Self::unwrap_parenthesized_expr(lhs), Expression::Await(_));
        let mut lhs = self.lower_expression(lhs);
        if self.async_expression_prefix.is_some()
            && contains(rhs, ContainsSymbol::AwaitExpression)
            && !lhs_is_direct_await
        {
            let lhs_name =
                self.alloc_suspension_owned_binding("async.binary.lhs.", lhs.value_info());
            self.async_expression_prefix
                .as_mut()
                .expect("async expression lowering must have a statement prefix")
                .push(StatementIr::Lexical {
                    mode: BindingMode::Let,
                    name: lhs_name.clone(),
                    init: lhs,
                });
            lhs = self.lower_identifier_name(lhs_name, false);
        }
        let rhs = self.lower_expression(rhs);
        self.combine_arithmetic(arithmetic, lhs, rhs)
    }

    /// Applies `ApplyStringOrNumericBinaryOperator` to two already-lowered
    /// operands.
    ///
    /// Split out of `lower_arithmetic` so compound assignment (`x op= y`) can
    /// reuse the same coercion decisions: the operands of a compound assignment
    /// are a property *read* and the right-hand side, neither of which is an
    /// AST expression the operand-lowering prologue could accept.
    fn combine_arithmetic(
        &mut self,
        arithmetic: ArithmeticOp,
        lhs: TypedExpr,
        rhs: TypedExpr,
    ) -> TypedExpr {
        match arithmetic {
            ArithmeticOp::Add => {
                if let (ExprIr::String(left), ExprIr::String(right)) = (&lhs.expr, &rhs.expr) {
                    // Bound work and temporary storage per expression. Larger
                    // constants retain ordinary runtime concatenation rather
                    // than repeatedly copying an unbounded literal chain.
                    const MAX_FOLDED_STRING_TRANSPORT_BYTES: usize = 4096;
                    if left
                        .len()
                        .checked_add(right.len())
                        .is_some_and(|bytes| bytes <= MAX_FOLDED_STRING_TRANSPORT_BYTES)
                    {
                        // Transport strings encode isolated surrogates and a
                        // sentinel. Concatenate semantic UTF-16, then encode
                        // once so a pair formed at the boundary is canonical.
                        let mut units = decode_js_string_utf16(left);
                        units.extend(decode_js_string_utf16(right));
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::String),
                            ExprIr::String(encode_js_string_utf16(&units)),
                        );
                    }
                }
                if let (ExprIr::BigInt(lhs), ExprIr::BigInt(rhs)) = (&lhs.expr, &rhs.expr) {
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::BigInt),
                        ExprIr::BigInt(lhs.added(rhs)),
                    );
                }
                let lhs_primitive = self.to_primitive_info(&lhs, ToPrimitiveHint::Default);
                let rhs_primitive = self.to_primitive_info(&rhs, ToPrimitiveHint::Default);
                let lhs_proves_string = lhs
                    .possible_kinds
                    .is_subset_of(KindSet::from_kind(ValueKind::String));
                let rhs_proves_string = rhs
                    .possible_kinds
                    .is_subset_of(KindSet::from_kind(ValueKind::String));
                if lhs_proves_string || rhs_proves_string {
                    if lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                        && rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    {
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::String),
                            ExprIr::StringConcat {
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::String),
                        ExprIr::CoerciveAdd {
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                if let (Some(lhs_primitive), Some(rhs_primitive)) = (&lhs_primitive, &rhs_primitive)
                {
                    let lhs_primitive_proves_string = lhs_primitive
                        .possible_kinds
                        .is_subset_of(KindSet::from_kind(ValueKind::String));
                    let rhs_primitive_proves_string = rhs_primitive
                        .possible_kinds
                        .is_subset_of(KindSet::from_kind(ValueKind::String));
                    if lhs_primitive_proves_string || rhs_primitive_proves_string {
                        return TypedExpr::from_info(
                            self.merge_value_infos(lhs_primitive.clone(), rhs_primitive.clone()),
                            ExprIr::CoerciveAdd {
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                    if lhs_primitive.possible_kinds.contains(ValueKind::String)
                        || rhs_primitive.possible_kinds.contains(ValueKind::String)
                    {
                        let possible_kinds = KindSet::from_kind(ValueKind::String)
                            .union(KindSet::from_kind(ValueKind::Number))
                            .union(KindSet::from_kind(ValueKind::BigInt));
                        return TypedExpr::from_info(
                            ValueInfo {
                                kind: possible_kinds.as_value_kind(),
                                possible_kinds,
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::none(),
                            },
                            ExprIr::CoerciveAdd {
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                    if lhs_primitive
                        .possible_kinds
                        .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
                        && rhs_primitive
                            .possible_kinds
                            .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
                    {
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::BigInt),
                            ExprIr::CoerciveBinaryNumber {
                                op: ArithmeticBinaryOp::Add,
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                    if lhs_primitive
                        .possible_kinds
                        .is_subset_of(KindSet::PRIMITIVE_ONLY)
                        && rhs_primitive
                            .possible_kinds
                            .is_subset_of(KindSet::PRIMITIVE_ONLY)
                    {
                        let (lhs_number, lhs_bigint) = numeric_domain(Some(lhs_primitive));
                        let (rhs_number, rhs_bigint) = numeric_domain(Some(rhs_primitive));
                        let mut possible_kinds = KindSet::EMPTY;
                        if lhs_number && rhs_number {
                            possible_kinds =
                                possible_kinds.union(KindSet::from_kind(ValueKind::Number));
                        }
                        if lhs_bigint && rhs_bigint {
                            possible_kinds =
                                possible_kinds.union(KindSet::from_kind(ValueKind::BigInt));
                        }
                        if possible_kinds == KindSet::EMPTY {
                            possible_kinds = KindSet::from_kind(ValueKind::Number);
                        }
                        return TypedExpr::from_info(
                            ValueInfo {
                                kind: possible_kinds.as_value_kind(),
                                possible_kinds,
                                heap_shape: None,
                                function_targets: FunctionTargetKnowledge::none(),
                            },
                            ExprIr::CoerciveBinaryNumber {
                                op: ArithmeticBinaryOp::Add,
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                }
                if lhs.possible_kinds.contains(ValueKind::String)
                    || rhs.possible_kinds.contains(ValueKind::String)
                {
                    let possible_kinds = KindSet::from_kind(ValueKind::String)
                        .union(KindSet::from_kind(ValueKind::Number))
                        .union(KindSet::from_kind(ValueKind::BigInt));
                    return TypedExpr::from_info(
                        ValueInfo {
                            kind: possible_kinds.as_value_kind(),
                            possible_kinds,
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                        },
                        ExprIr::CoerciveAdd {
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                if lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    && rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                {
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::Number),
                        ExprIr::CoerciveBinaryNumber {
                            op: ArithmeticBinaryOp::Add,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                let lhs = match self.coerce_expr_to_number(lhs.clone()) {
                    Some(lhs) => lhs,
                    None => return self.unsupported_expr("string or coercive `+`"),
                };
                let rhs = match self.coerce_expr_to_number(rhs.clone()) {
                    Some(rhs) => rhs,
                    None => return self.unsupported_expr("string or coercive `+`"),
                };
                if lhs.kind != ValueKind::Number || rhs.kind != ValueKind::Number {
                    return self.unsupported_expr("string or coercive `+`");
                }
                TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Number,
                        possible_kinds: KindSet::from_kind(ValueKind::Number),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::BinaryNumber {
                        op: ArithmeticBinaryOp::Add,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                )
            }
            ArithmeticOp::Sub | ArithmeticOp::Mul | ArithmeticOp::Div | ArithmeticOp::Mod => {
                let lhs_primitive = self.to_primitive_info(&lhs, ToPrimitiveHint::Number);
                let rhs_primitive = self.to_primitive_info(&rhs, ToPrimitiveHint::Number);
                if let (Some(lhs_primitive), Some(rhs_primitive)) = (&lhs_primitive, &rhs_primitive)
                {
                    let op = match arithmetic {
                        ArithmeticOp::Sub => ArithmeticBinaryOp::Sub,
                        ArithmeticOp::Mul => ArithmeticBinaryOp::Mul,
                        ArithmeticOp::Div => ArithmeticBinaryOp::Div,
                        ArithmeticOp::Mod => ArithmeticBinaryOp::Mod,
                        ArithmeticOp::Add | ArithmeticOp::Exp => unreachable!(),
                    };
                    let can_convert_to_number = |info: &ValueInfo| {
                        [
                            ValueKind::Undefined,
                            ValueKind::Null,
                            ValueKind::Boolean,
                            ValueKind::Number,
                            ValueKind::String,
                        ]
                        .into_iter()
                        .any(|kind| info.possible_kinds.contains(kind))
                    };
                    let mut result_kinds = KindSet::EMPTY;
                    if lhs_primitive.possible_kinds.contains(ValueKind::BigInt)
                        && rhs_primitive.possible_kinds.contains(ValueKind::BigInt)
                    {
                        result_kinds = result_kinds.union(KindSet::from_kind(ValueKind::BigInt));
                    }
                    if can_convert_to_number(lhs_primitive) && can_convert_to_number(rhs_primitive)
                    {
                        result_kinds = result_kinds.union(KindSet::from_kind(ValueKind::Number));
                    }
                    if result_kinds == KindSet::EMPTY {
                        result_kinds = KindSet::from_kind(ValueKind::Number);
                    }
                    return TypedExpr::from_info(
                        ValueInfo {
                            kind: result_kinds.as_value_kind(),
                            possible_kinds: result_kinds,
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                        },
                        ExprIr::CoerciveBinaryNumber {
                            op,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                if lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    && rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                {
                    let op = match arithmetic {
                        ArithmeticOp::Sub => ArithmeticBinaryOp::Sub,
                        ArithmeticOp::Mul => ArithmeticBinaryOp::Mul,
                        ArithmeticOp::Div => ArithmeticBinaryOp::Div,
                        ArithmeticOp::Mod => ArithmeticBinaryOp::Mod,
                        ArithmeticOp::Add | ArithmeticOp::Exp => unreachable!(),
                    };
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::Number),
                        ExprIr::CoerciveBinaryNumber {
                            op,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                let lhs = match self.coerce_expr_to_number(lhs.clone()) {
                    Some(lhs) => lhs,
                    None => {
                        let op = match arithmetic {
                            ArithmeticOp::Sub => ArithmeticBinaryOp::Sub,
                            ArithmeticOp::Mul => ArithmeticBinaryOp::Mul,
                            ArithmeticOp::Div => ArithmeticBinaryOp::Div,
                            ArithmeticOp::Mod => ArithmeticBinaryOp::Mod,
                            ArithmeticOp::Add | ArithmeticOp::Exp => unreachable!(),
                        };
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::Number),
                            ExprIr::CoerciveBinaryNumber {
                                op,
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                };
                let rhs = match self.coerce_expr_to_number(rhs.clone()) {
                    Some(rhs) => rhs,
                    None => {
                        let op = match arithmetic {
                            ArithmeticOp::Sub => ArithmeticBinaryOp::Sub,
                            ArithmeticOp::Mul => ArithmeticBinaryOp::Mul,
                            ArithmeticOp::Div => ArithmeticBinaryOp::Div,
                            ArithmeticOp::Mod => ArithmeticBinaryOp::Mod,
                            ArithmeticOp::Add | ArithmeticOp::Exp => unreachable!(),
                        };
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::Number),
                            ExprIr::CoerciveBinaryNumber {
                                op,
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                };
                if lhs.kind != ValueKind::Number || rhs.kind != ValueKind::Number {
                    return self.unsupported_expr("coercive numeric operator");
                }
                let op = match arithmetic {
                    ArithmeticOp::Sub => ArithmeticBinaryOp::Sub,
                    ArithmeticOp::Mul => ArithmeticBinaryOp::Mul,
                    ArithmeticOp::Div => ArithmeticBinaryOp::Div,
                    ArithmeticOp::Mod => ArithmeticBinaryOp::Mod,
                    ArithmeticOp::Add | ArithmeticOp::Exp => unreachable!(),
                };
                TypedExpr::from_info(
                    ValueInfo {
                        kind: ValueKind::Number,
                        possible_kinds: KindSet::from_kind(ValueKind::Number),
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    },
                    ExprIr::BinaryNumber {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                )
            }
            ArithmeticOp::Exp => {
                if let (ExprIr::Number(lhs), ExprIr::Number(rhs)) = (&lhs.expr, &rhs.expr) {
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::Number),
                        ExprIr::Number(
                            Self::static_pow(f64::from_bits(*lhs), f64::from_bits(*rhs)).to_bits(),
                        ),
                    );
                }
                if let (ExprIr::BigInt(lhs), ExprIr::BigInt(rhs)) = (&lhs.expr, &rhs.expr) {
                    let rhs_value = rhs.to_bigint();
                    if rhs_value.sign() != Sign::Minus {
                        if let Some(exponent) = rhs_value.to_u32() {
                            return TypedExpr::from_info(
                                ValueInfo::new(ValueKind::BigInt),
                                ExprIr::BigInt(lhs.pow_u32(exponent)),
                            );
                        }
                    }
                }
                let lhs_primitive = self.to_primitive_info(&lhs, ToPrimitiveHint::Number);
                let rhs_primitive = self.to_primitive_info(&rhs, ToPrimitiveHint::Number);
                let bigint_or_number_info = || {
                    let possible_kinds = KindSet::from_kind(ValueKind::Number)
                        .union(KindSet::from_kind(ValueKind::BigInt));
                    ValueInfo {
                        kind: possible_kinds.as_value_kind(),
                        possible_kinds,
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    }
                };
                let bigint_exp_expr = |lhs: TypedExpr, rhs: TypedExpr, info: ValueInfo| {
                    TypedExpr::from_info(
                        info,
                        ExprIr::CoerciveBinaryNumber {
                            op: ArithmeticBinaryOp::Exp,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    )
                };
                if let (Some(lhs_primitive), Some(rhs_primitive)) = (&lhs_primitive, &rhs_primitive)
                {
                    let lhs_has_bigint = lhs_primitive.possible_kinds.contains(ValueKind::BigInt);
                    let rhs_has_bigint = rhs_primitive.possible_kinds.contains(ValueKind::BigInt);
                    if lhs_has_bigint || rhs_has_bigint {
                        let result_info = if lhs_primitive
                            .possible_kinds
                            .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
                            && rhs_primitive
                                .possible_kinds
                                .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
                        {
                            ValueInfo::new(ValueKind::BigInt)
                        } else {
                            bigint_or_number_info()
                        };
                        return bigint_exp_expr(lhs, rhs, result_info);
                    } else {
                        return TypedExpr::from_info(
                            ValueInfo::new(ValueKind::Number),
                            ExprIr::CoerciveBinaryNumber {
                                op: ArithmeticBinaryOp::Exp,
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        );
                    }
                }
                if lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    && rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    && (lhs.possible_kinds.contains(ValueKind::BigInt)
                        || rhs.possible_kinds.contains(ValueKind::BigInt))
                {
                    let result_info = if lhs
                        .possible_kinds
                        .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
                        && rhs
                            .possible_kinds
                            .is_subset_of(KindSet::from_kind(ValueKind::BigInt))
                    {
                        ValueInfo::new(ValueKind::BigInt)
                    } else {
                        bigint_or_number_info()
                    };
                    return bigint_exp_expr(lhs, rhs, result_info);
                }
                if lhs
                    .possible_kinds
                    .is_subset_of(KindSet::PRIMITIVE_ONLY.without(ValueKind::BigInt))
                    && rhs
                        .possible_kinds
                        .is_subset_of(KindSet::PRIMITIVE_ONLY.without(ValueKind::BigInt))
                {
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::Number),
                        ExprIr::CoerciveBinaryNumber {
                            op: ArithmeticBinaryOp::Exp,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                let lhs = match self.coerce_expr_to_number(lhs.clone()) {
                    Some(lhs) => lhs,
                    None => return self.unsupported_expr("exponentiation operator"),
                };
                let rhs = match self.coerce_expr_to_number(rhs.clone()) {
                    Some(rhs) => rhs,
                    None => return self.unsupported_expr("exponentiation operator"),
                };
                if lhs.kind == ValueKind::Number && rhs.kind == ValueKind::Number {
                    return TypedExpr::from_info(
                        ValueInfo::new(ValueKind::Number),
                        ExprIr::BinaryNumber {
                            op: ArithmeticBinaryOp::Exp,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        },
                    );
                }
                self.unsupported_expr("exponentiation operator")
            }
        }
    }

    fn lower_relational(
        &mut self,
        relational: RelationalOp,
        lhs: &Expression,
        rhs: &Expression,
    ) -> TypedExpr {
        let rhs_suspends = contains(rhs, ContainsSymbol::AwaitExpression);
        let lhs = self.lower_expression(lhs);
        let lhs =
            self.pin_async_operand_before_suspension(lhs, rhs_suspends, "async.relational.lhs.");
        let rhs = self.lower_expression(rhs);
        self.combine_relational(relational, lhs, rhs)
    }

    fn heap_shape_has_prototype(shape: &HeapShape, target: &HeapShape) -> bool {
        let mut current = match shape {
            HeapShape::Object(object) => object.prototype.as_deref(),
            HeapShape::Array(array) => array.prototype.as_deref(),
        };
        while let Some(shape) = current {
            if shape == target {
                return true;
            }
            current = match shape {
                HeapShape::Object(object) => object.prototype.as_deref(),
                HeapShape::Array(array) => array.prototype.as_deref(),
            };
        }
        false
    }

    fn lower_logical(
        &mut self,
        logical: LogicalOp,
        lhs: &Expression,
        rhs: &Expression,
    ) -> TypedExpr {
        if self.async_expression_prefix.is_some()
            && self.has_plain_async_value_branch_owner()
            && contains(rhs, ContainsSymbol::AwaitExpression)
        {
            return self.lower_logical_await_value(logical, lhs, rhs);
        }
        let lhs = self.lower_expression(lhs);
        if let Some(lhs_bool) = Self::static_bool_expr(&lhs) {
            match (logical, lhs_bool) {
                (LogicalOp::And, false) | (LogicalOp::Or, true) => return lhs,
                _ => {}
            }
        }
        if logical == LogicalOp::Coalesce
            && !lhs.possible_kinds.contains(ValueKind::Undefined)
            && !lhs.possible_kinds.contains(ValueKind::Null)
        {
            return lhs;
        }
        let definitely_truthy = lhs.possible_kinds.is_subset_of(
            KindSet::from_kind(ValueKind::Object)
                .union(KindSet::from_kind(ValueKind::Array))
                .union(KindSet::from_kind(ValueKind::Arguments))
                .union(KindSet::from_kind(ValueKind::Symbol)),
        );
        if logical == LogicalOp::Or && definitely_truthy {
            return lhs;
        }
        let rhs = self.lower_conditionally_reached_expression(rhs);

        let op = match logical {
            LogicalOp::And => LogicalBinaryOp::And,
            LogicalOp::Or => LogicalBinaryOp::Or,
            LogicalOp::Coalesce => LogicalBinaryOp::Coalesce,
        };
        TypedExpr::from_info(
            self.merge_value_infos(lhs.value_info(), rhs.value_info()),
            ExprIr::LogicalShortCircuit {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        )
    }

    fn to_primitive_info(&mut self, expr: &TypedExpr, hint: ToPrimitiveHint) -> Option<ValueInfo> {
        let info = expr.value_info();
        let primitive = self.value_info_to_primitive(&info, hint);
        self.record_possible_to_primitive_effects(&info);
        primitive
    }

    fn record_possible_to_primitive_effects(&mut self, info: &ValueInfo) {
        if [
            ValueKind::Object,
            ValueKind::Array,
            ValueKind::Arguments,
            ValueKind::Function,
        ]
        .into_iter()
        .any(|kind| info.possible_kinds.contains(kind))
        {
            self.invalidate_unknown_user_code_effects();
        }
    }

    fn value_info_to_primitive(
        &mut self,
        info: &ValueInfo,
        hint: ToPrimitiveHint,
    ) -> Option<ValueInfo> {
        // Function objects are ordinary objects for ToPrimitive purposes (they
        // just also happen to be callable): absent an own/inherited
        // `@@toPrimitive`/`valueOf` returning a primitive, OrdinaryToPrimitive
        // falls through to `toString`, which for a function is inherited from
        // `Function.prototype.toString` (source text) rather than
        // `Object.prototype.toString`. `object_to_primitive_kinds` already
        // walks the tracked shape's own properties for overrides and falls
        // back to `String` when none are found, which matches that default.
        if !info.possible_kinds.is_subset_of(
            KindSet::PRIMITIVE_OR_HEAP_COERCIBLE.union(KindSet::from_kind(ValueKind::Function)),
        ) {
            return None;
        }

        let mut possible_kinds = KindSet::EMPTY;
        for kind in [
            ValueKind::Undefined,
            ValueKind::Null,
            ValueKind::Boolean,
            ValueKind::Number,
            ValueKind::String,
            ValueKind::Symbol,
            ValueKind::BigInt,
            ValueKind::Object,
            ValueKind::Array,
            ValueKind::Arguments,
            ValueKind::Function,
        ] {
            if !info.possible_kinds.contains(kind) {
                continue;
            }
            let next = match kind {
                ValueKind::Undefined
                | ValueKind::Null
                | ValueKind::Boolean
                | ValueKind::Number
                | ValueKind::String
                | ValueKind::Symbol
                | ValueKind::BigInt => KindSet::from_kind(kind),
                ValueKind::Object | ValueKind::Function => {
                    self.object_to_primitive_kinds(info.heap_shape.as_deref(), hint)?
                }
                // Indexed contents do not prove a conversion result: own or
                // inherited hooks, including Array.prototype.toString's live
                // join lookup, can return any primitive kind.
                ValueKind::Array | ValueKind::Arguments => KindSet::PRIMITIVE_ONLY,
                ValueKind::Dynamic => return None,
            };
            possible_kinds = possible_kinds.union(next);
        }

        if possible_kinds == KindSet::EMPTY {
            return None;
        }

        Some(ValueInfo {
            kind: possible_kinds.as_value_kind(),
            possible_kinds,
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        })
    }

    fn object_to_primitive_kinds(
        &mut self,
        shape: Option<&HeapShape>,
        hint: ToPrimitiveHint,
    ) -> Option<KindSet> {
        let Some(HeapShape::Object(shape)) = shape else {
            return Some(KindSet::from_kind(ValueKind::String));
        };

        if !matches!(hint, ToPrimitiveHint::String) {
            if let Some(primitive) = &shape.boxed_primitive {
                return Some(primitive.possible_kinds);
            }
        }

        // 7.1.1 ToPrimitive step 2 looks up `@@toPrimitive` first, then
        // OrdinaryToPrimitive tries the two ordinary method names in a
        // hint-dependent order. The *order* is the spec obligation and the
        // *kinds* differ, so the list is typed rather than three bare strings.
        let order: &[ToPrimitiveLookupKey; 3] = match hint {
            ToPrimitiveHint::String => &[
                ToPrimitiveLookupKey::Symbol(WellKnownSymbol::ToPrimitive),
                ToPrimitiveLookupKey::Method("toString"),
                ToPrimitiveLookupKey::Method("valueOf"),
            ],
            ToPrimitiveHint::Default | ToPrimitiveHint::Number => &[
                ToPrimitiveLookupKey::Symbol(WellKnownSymbol::ToPrimitive),
                ToPrimitiveLookupKey::Method("valueOf"),
                ToPrimitiveLookupKey::Method("toString"),
            ],
        };
        for key in order {
            let entry = match key {
                ToPrimitiveLookupKey::Symbol(symbol) => {
                    shape.properties.get(&shape_namespace_key(*symbol))
                }
                ToPrimitiveLookupKey::Method(name) => shape.properties.get(*name),
            };
            let Some(ObjectShapeProperty::Data(info)) = entry else {
                continue;
            };
            if !info.possible_kinds.contains(ValueKind::Function) {
                continue;
            }
            let function_targets = info.function_targets.exact_targets()?;
            if function_targets.is_empty() {
                continue;
            }
            let mut return_kinds = KindSet::EMPTY;
            for function_id in function_targets {
                let Some(signature) = self.function_signatures.get(function_id) else {
                    return None;
                };
                for kind in [
                    ValueKind::Undefined,
                    ValueKind::Null,
                    ValueKind::Boolean,
                    ValueKind::Number,
                    ValueKind::String,
                    ValueKind::Symbol,
                    ValueKind::BigInt,
                ] {
                    if signature.return_possible_kinds.contains(kind) {
                        return_kinds = return_kinds.union(KindSet::from_kind(kind));
                    }
                }
            }
            if return_kinds != KindSet::EMPTY {
                return Some(return_kinds);
            }
        }

        Some(KindSet::from_kind(ValueKind::String))
    }

    fn try_static_string_key(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Literal(literal) => {
                let LiteralKind::String(sym) = literal.kind() else {
                    return None;
                };
                Some(self.interned_runtime_string(*sym))
            }
            // A well-known symbol is not a string key, but this compiler encodes
            // its value as its [[Description]]; `lower_static_property_key` is
            // what re-separates the two, via `is_symbol_description`.
            Expression::PropertyAccess(PropertyAccess::Simple(_)) => self
                .try_well_known_symbol_key_name(expr)
                .map(|symbol| symbol.description().to_string()),
            _ => None,
        }
    }

    fn try_static_ordinary_property_key(&self, expr: &Expression) -> Option<String> {
        match expr {
            Expression::Literal(literal) => {
                let LiteralKind::String(sym) = literal.kind() else {
                    return self.static_number_property_key(expr);
                };
                Some(self.interned_runtime_string(*sym))
            }
            _ => self.static_number_property_key(expr),
        }
    }

    /// Whether `target_name` names the real builtin `Symbol` intrinsic here.
    ///
    /// An active `with` scope, a shadowing lexical binding, or a global that is
    /// not provably the builtin all make `Symbol.iterator` mean something this
    /// compiler must not resolve statically. Both well-known-symbol paths ask
    /// exactly this question; before this helper existed they asked it with two
    /// byte-identical inline copies of the four clauses, ~5,700 lines apart.
    fn expression_is_builtin_symbol_intrinsic(&self, target_name: &str) -> bool {
        target_name == SYMBOL_NAME && self.identifier_resolves_to_intrinsic_global(target_name)
    }

    /// ResolveBinding (9.1.1) for a name a constant fold wants to treat as its
    /// canonical intrinsic global: true only when nothing shadows or replaced it.
    ///
    /// The four clauses are the ones the two intrinsic guards below and above
    /// already spelled inline, lifted so there is one of them:
    ///
    /// 1. no active `with` object, whose binding object could supply the name
    ///    at run time (14.11.2);
    /// 2. no Environment Record in the chain binds it — 9.1.1 resolves against
    ///    the environment *before* the global object, so a parameter, `let`,
    ///    `const`, `var` or catch parameter named `Infinity` wins;
    /// 3. the global property is proven present, and
    /// 4. it is still the standard or host intrinsic, not something a
    ///    `GlobalWrite` replaced.
    ///
    /// Testing a fold's precondition by **spelling alone** has already shipped
    /// wrong answers here three times: the two recorded on
    /// `identifier_is_builtin_native_error`, and a third at the
    /// `Number.prototype.toPrecision` fold, where the new 21.1.3.5 step 5
    /// RangeError arm turned `function f(){ let Infinity = 5; return
    /// (1.5).toPrecision(Infinity); }` from a correct runtime answer into an
    /// emitted `RangeError` throw. Any new resolver that maps an identifier
    /// spelling to a value must call this.
    fn identifier_resolves_to_intrinsic_global(&self, name: &str) -> bool {
        self.with_environment_chain.is_empty()
            && self.lookup_binding(name).is_none()
            && self
                .lookup_global_property_info(name)
                .is_some_and(|property| {
                    property.proven_present
                        && matches!(
                            property.source,
                            GlobalPropertySource::Builtin | GlobalPropertySource::HostBuiltin
                        )
                })
    }

    /// The value of `Infinity`, `NaN` or `undefined` **when the identifier
    /// really is the builtin global** (ECMA-262 19.1.1, 19.1.2, 19.1.3), and
    /// `None` for every other spelling and for a shadowed one.
    ///
    /// `undefined` maps to `NaN` because every caller is inside a `ToNumber`
    /// (7.1.4 table row `Undefined` → `NaN`). This numeric projection does not
    /// distinguish `undefined` from an actual NaN value.
    fn static_global_number_identifier(&self, name: &str) -> Option<f64> {
        let value = match name {
            "Infinity" => f64::INFINITY,
            "NaN" | "undefined" => f64::NAN,
            _ => return None,
        };
        self.identifier_resolves_to_intrinsic_global(name)
            .then_some(value)
    }

    /// The native-error intrinsic `name` denotes, **if** the identifier really
    /// resolves to the builtin.
    ///
    /// `NativeErrorKind::from_str(name).is_some()` tests the *spelling* of an
    /// identifier and nothing else, but 9.1.1 resolves a name against the
    /// environment before the global object, so a parameter named `TypeError`
    /// shadows it. Two constant-folds keyed off the bare spelling and gave
    /// silent wrong answers:
    ///
    /// - `function f(TypeError) { for (const k in TypeError) return k; } f({a:1})`
    ///   elided the loop and answered `undefined` instead of `"a"`;
    /// - `function g(RangeError) { return RangeError.propertyIsEnumerable("prototype"); }`
    ///   folded to `false` whatever the argument was.
    ///
    /// Neither was introduced by the `NativeErrorKind` retrofit — the nine-arm
    /// `matches!` it replaced was equally unguarded — but the retrofit is where
    /// the asymmetry with `expression_is_builtin_symbol_intrinsic` above became
    /// visible, and the guard is the same four clauses.
    fn identifier_is_builtin_native_error(&self, name: &str) -> Option<NativeErrorKind> {
        let kind = NativeErrorKind::from_str(name)?;
        self.identifier_resolves_to_intrinsic_global(name)
            .then_some(kind)
    }

    fn has_regexp_prototype_shape(target: &TypedExpr) -> bool {
        target.heap_shape.as_deref() == Some(Self::regexp_prototype_shape().as_ref())
    }

    /// The well-known symbol `expr` denotes, if it is a static `Symbol.x` read
    /// of the real intrinsic.
    ///
    /// This is the only parse of a well-known symbol out of source text.
    /// `WellKnownSymbol::from_member_name` replaced the fifteen-element
    /// `matches!` whitelist that used to live here, and the identical one that
    /// used to live in `lower_property_access`; returning the enum rather than
    /// an `Option<String>` is what makes a consumer that compares against a
    /// misspelling `error[E0599]` instead of a silently dead fast path.
    fn try_well_known_symbol_key_name(&self, expr: &Expression) -> Option<WellKnownSymbol> {
        let Expression::PropertyAccess(PropertyAccess::Simple(access)) = expr else {
            return None;
        };
        let Expression::Identifier(identifier) = access.target() else {
            return None;
        };
        let target_name = self.interner.resolve_expect(identifier.sym()).to_string();
        if !self.expression_is_builtin_symbol_intrinsic(&target_name) {
            return None;
        }
        let PropertyAccessField::Const(name) = access.field() else {
            return None;
        };
        let member_name = self.interner.resolve_expect(name.sym()).to_string();
        WellKnownSymbol::from_member_name(SymbolMemberName::new(&member_name))
    }

    fn static_to_boolean_arg(&self, arg: Option<&Expression>) -> Option<bool> {
        match arg {
            None => Some(false),
            Some(arg) => self.static_to_boolean_expr(arg),
        }
    }

    fn expression_cannot_be_symbol(&self, expr: &Expression) -> bool {
        self.expression_cannot_be_kind(expr, ValueKind::Symbol)
    }

    fn expression_cannot_be_kind(&self, expr: &Expression, excluded: ValueKind) -> bool {
        let expr = Self::unwrap_parenthesized_expr(expr);
        match expr {
            Expression::Literal(literal) => {
                let kind = match literal.kind() {
                    LiteralKind::Bool(_) => ValueKind::Boolean,
                    LiteralKind::String(_) => ValueKind::String,
                    LiteralKind::Num(_) | LiteralKind::Int(_) => ValueKind::Number,
                    LiteralKind::Null => ValueKind::Null,
                    LiteralKind::Undefined => ValueKind::Undefined,
                    LiteralKind::BigInt(_) => ValueKind::BigInt,
                };
                kind != excluded
            }
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if matches!(name.as_str(), "undefined" | "NaN" | "Infinity")
                    && self.identifier_resolves_to_intrinsic_global(&name)
                {
                    let kind = if name == "undefined" {
                        ValueKind::Undefined
                    } else {
                        ValueKind::Number
                    };
                    return kind != excluded;
                }
                let possible_kinds = self
                    .lookup_binding(&name)
                    .map(|binding| binding.possible_kinds)
                    .unwrap_or_else(|| {
                        self.capture_value_info(&self.current_owner_id, &name)
                            .possible_kinds
                    });
                !possible_kinds.contains(excluded)
            }
            _ => false,
        }
    }

    fn static_to_boolean_expr(&self, expr: &Expression) -> Option<bool> {
        let expr = Self::unwrap_parenthesized_expr(expr);
        match expr {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::Bool(value) => Some(*value),
                LiteralKind::String(sym) => {
                    Some(!self.interner.resolve_expect(*sym).to_string().is_empty())
                }
                LiteralKind::Num(value) => Some(*value != 0.0 && !value.is_nan()),
                LiteralKind::Int(value) => Some(*value != 0),
                LiteralKind::Null | LiteralKind::Undefined => Some(false),
                LiteralKind::BigInt(value) => {
                    let text = value.as_ref().to_string();
                    Some(text.trim_end_matches('n') != "0")
                }
            },
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if let Some(value) = self.static_global_number_identifier(&name) {
                    return Some(value != 0.0 && !value.is_nan());
                }
                let (kind, possible_kinds) = self
                    .lookup_binding(&name)
                    .map(|binding| (binding.kind, binding.possible_kinds))
                    .unwrap_or_else(|| {
                        let info = self.capture_value_info(&self.current_owner_id, &name);
                        (info.kind, info.possible_kinds)
                    });
                if possible_kinds.is_singleton() {
                    match kind {
                        ValueKind::Undefined | ValueKind::Null => Some(false),
                        ValueKind::Object
                        | ValueKind::Array
                        | ValueKind::Arguments
                        | ValueKind::Function
                        | ValueKind::Symbol => Some(true),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Expression::Unary(unary) => match unary.op() {
                UnaryOp::Plus | UnaryOp::Minus => self
                    .static_to_number_expr(unary.target())
                    .map(|value| value != 0.0 && !value.is_nan()),
                _ => None,
            },
            Expression::ArrayLiteral(_)
            | Expression::ObjectLiteral(_)
            | Expression::FunctionExpression(_) => Some(true),
            Expression::New(_) => Some(true),
            Expression::Call(call) => {
                let callee = Self::unwrap_parenthesized_expr(call.function());
                let Expression::Identifier(identifier) = callee else {
                    return None;
                };
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if !self.identifier_resolves_to_intrinsic_global(&name)
                    || call
                        .args()
                        .iter()
                        .any(|argument| matches!(argument, Expression::Spread(_)))
                {
                    return None;
                }
                match name.as_str() {
                    BOOLEAN_NAME => self.static_to_boolean_arg(call.args().first()),
                    "Symbol" => Some(true),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn static_boolean_receiver_value(&self, receiver: &Expression) -> Option<bool> {
        match Self::unwrap_parenthesized_expr(receiver) {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::Bool(value) => Some(*value),
                _ => None,
            },
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if let Some(value) = self.static_boolean_bindings.get(&name) {
                    return Some(*value);
                }
                None
            }
            Expression::New(new_expr) => {
                let Expression::Identifier(constructor) =
                    Self::unwrap_parenthesized_expr(new_expr.constructor())
                else {
                    return None;
                };
                if self.interner.resolve_expect(constructor.sym()).to_string() != BOOLEAN_NAME
                    || !self.identifier_resolves_to_intrinsic_global(BOOLEAN_NAME)
                    || new_expr
                        .arguments()
                        .iter()
                        .any(|argument| matches!(argument, Expression::Spread(_)))
                {
                    return None;
                }
                return self.static_to_boolean_arg(new_expr.arguments().first());
            }
            Expression::Call(call) => {
                let Expression::Identifier(callee) =
                    Self::unwrap_parenthesized_expr(call.function())
                else {
                    return None;
                };
                if self.interner.resolve_expect(callee.sym()).to_string() != BOOLEAN_NAME
                    || !self.identifier_resolves_to_intrinsic_global(BOOLEAN_NAME)
                    || call
                        .args()
                        .iter()
                        .any(|argument| matches!(argument, Expression::Spread(_)))
                {
                    return None;
                }
                self.static_to_boolean_arg(call.args().first())
            }
            Expression::Unary(unary) if unary.op() == UnaryOp::Not => self
                .static_to_boolean_expr(unary.target())
                .map(|value| !value),
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                let Expression::Identifier(target) =
                    Self::unwrap_parenthesized_expr(access.target())
                else {
                    return None;
                };
                if self.interner.resolve_expect(target.sym()).to_string() != BOOLEAN_NAME
                    || !self.identifier_resolves_to_intrinsic_global(BOOLEAN_NAME)
                {
                    return None;
                }
                let PropertyAccessField::Const(field) = access.field() else {
                    return None;
                };
                if self.interner.resolve_expect(field.sym()).to_string() == "prototype" {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn static_to_number_expr(&self, expr: &Expression) -> Option<f64> {
        let expr = Self::unwrap_parenthesized_expr(expr);
        if let Some(value) = Self::literal_number_value(expr) {
            return Some(value);
        }
        match expr {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::String(sym) => {
                    Self::parse_number_string(&self.interner.resolve_expect(*sym).to_string())
                }
                LiteralKind::Bool(true) => Some(1.0),
                LiteralKind::Bool(false) | LiteralKind::Null => Some(0.0),
                LiteralKind::Undefined => Some(f64::NAN),
                _ => None,
            },
            Expression::Identifier(identifier) => {
                // 9.1.1 resolves the name against the environment first, so the
                // spelling alone is not enough — see
                // `static_global_number_identifier`.
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                self.static_global_number_identifier(&name)
            }
            Expression::Unary(unary) => match unary.op() {
                UnaryOp::Plus => self.static_to_number_expr(unary.target()),
                UnaryOp::Minus => self
                    .static_to_number_expr(unary.target())
                    .map(|value| -value),
                _ => None,
            },
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                let Expression::Identifier(target) =
                    Self::unwrap_parenthesized_expr(access.target())
                else {
                    return None;
                };
                let target_name = self.interner.resolve_expect(target.sym()).to_string();
                if target_name != NUMBER_NAME
                    || !self.identifier_resolves_to_intrinsic_global(NUMBER_NAME)
                {
                    return None;
                }
                let PropertyAccessField::Const(field) = access.field() else {
                    return None;
                };
                match self
                    .interner
                    .resolve_expect(field.sym())
                    .to_string()
                    .as_str()
                {
                    "NaN" => Some(f64::NAN),
                    "POSITIVE_INFINITY" => Some(f64::INFINITY),
                    "NEGATIVE_INFINITY" => Some(f64::NEG_INFINITY),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn parse_number_string(input: &str) -> Option<f64> {
        let trimmed = input
            .trim_start_matches(is_ecmascript_whitespace)
            .trim_end_matches(is_ecmascript_whitespace);
        if trimmed.is_empty() {
            return Some(0.0);
        }
        match trimmed {
            "Infinity" | "+Infinity" => return Some(f64::INFINITY),
            "-Infinity" => return Some(f64::NEG_INFINITY),
            _ => {}
        }
        if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            if hex.is_empty() || !hex.bytes().all(|digit| digit.is_ascii_hexdigit()) {
                return Some(f64::NAN);
            }
            return u64::from_str_radix(hex, 16).ok().map(|value| value as f64);
        }
        if let Some(binary) = trimmed
            .strip_prefix("0b")
            .or_else(|| trimmed.strip_prefix("0B"))
        {
            if binary.is_empty() || !binary.bytes().all(|digit| matches!(digit, b'0' | b'1')) {
                return Some(f64::NAN);
            }
            return u64::from_str_radix(binary, 2)
                .ok()
                .map(|value| value as f64);
        }
        if let Some(octal) = trimmed
            .strip_prefix("0o")
            .or_else(|| trimmed.strip_prefix("0O"))
        {
            if octal.is_empty() || !octal.bytes().all(|digit| matches!(digit, b'0'..=b'7')) {
                return Some(f64::NAN);
            }
            return u64::from_str_radix(octal, 8).ok().map(|value| value as f64);
        }
        if trimmed
            .bytes()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'))
        {
            return Some(f64::NAN);
        }
        Some(trimmed.parse::<f64>().unwrap_or(f64::NAN))
    }

    fn static_parse_float_input(&self, expr: &Expression) -> Option<String> {
        let expr = Self::unwrap_parenthesized_expr(expr);
        if let Some(value) = Self::literal_number_value(expr) {
            return Some(Self::js_number_to_string(value));
        }
        match expr {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::String(sym) => Some(self.interner.resolve_expect(*sym).to_string()),
                LiteralKind::Bool(true) => Some("true".to_string()),
                LiteralKind::Bool(false) => Some("false".to_string()),
                LiteralKind::Null => Some("null".to_string()),
                LiteralKind::Undefined => Some("undefined".to_string()),
                _ => None,
            },
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                if !self.identifier_resolves_to_intrinsic_global(&name) {
                    return None;
                }
                match name.as_str() {
                    "Infinity" => Some("Infinity".to_string()),
                    "NaN" => Some("NaN".to_string()),
                    "undefined" => Some("undefined".to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn js_number_to_string(value: f64) -> String {
        ryu_js::Buffer::new().format(value).to_string()
    }

    fn read_object_shape(&self, target: &TypedExpr, key: &str) -> Option<ValueInfo> {
        let property = self.read_object_shape_property(target, key)?;
        Some(match property {
            ObjectShapeProperty::Data(info) => info,
            ObjectShapeProperty::Accessor {
                getter: Some(getter),
                ..
            } => self.accessor_return_info(&getter.function_id),
            ObjectShapeProperty::Accessor { getter: None, .. } => ValueInfo::undefined(),
        })
    }

    fn read_object_shape_property(
        &self,
        target: &TypedExpr,
        key: &str,
    ) -> Option<ObjectShapeProperty> {
        self.read_current_object_shape_property(target, key)
    }

    fn read_well_known_symbol_shape_property(
        shape: Option<&HeapShape>,
        symbol: WellKnownSymbol,
    ) -> Option<ObjectShapeProperty> {
        let shape = shape?;
        let (properties, prototype) = match shape {
            HeapShape::Object(shape) => (&shape.properties, shape.prototype.as_deref()),
            HeapShape::Array(shape) => (&shape.properties, shape.prototype.as_deref()),
        };
        properties
            .get(&shape_namespace_key(symbol))
            .cloned()
            .or_else(|| Self::read_well_known_symbol_shape_property(prototype, symbol))
    }

    fn read_well_known_symbol_shape(
        &self,
        target: &ValueInfo,
        symbol: WellKnownSymbol,
    ) -> Option<ValueInfo> {
        Some(
            match self.read_current_heap_shape_property(
                target.heap_shape.as_deref()?,
                &shape_namespace_key(symbol),
            )? {
                ObjectShapeProperty::Data(info) => info,
                ObjectShapeProperty::Accessor {
                    getter: Some(getter),
                    ..
                } => self.accessor_return_info(&getter.function_id),
                ObjectShapeProperty::Accessor { getter: None, .. } => ValueInfo::undefined(),
            },
        )
    }

    fn read_own_object_shape_property(
        &self,
        target: &TypedExpr,
        key: &str,
    ) -> Option<ObjectShapeProperty> {
        if shape_property_name_is_symbol_keyed(key) {
            return None;
        }
        match target.heap_shape.as_deref()? {
            HeapShape::Object(object) => object.properties.get(key).cloned(),
            HeapShape::Array(array) => array.properties.get(key).cloned(),
        }
    }

    fn read_array_shape(&self, target: &TypedExpr, index: &TypedExpr) -> Option<ValueInfo> {
        let HeapShape::Array(shape) = target.heap_shape.as_deref()? else {
            return None;
        };
        if shape.provenance != HeapShapeProvenance::Program {
            return None;
        }
        let index = self.constant_array_index(index)?;
        let element = shape.elements.get(index)?;
        // The current element representation also uses Undefined for a hole.
        // Such a slot cannot prove an own data property. A dynamic or absent
        // index likewise may reach an inherited getter.
        (!element.possible_kinds.contains(ValueKind::Undefined)).then(|| element.clone())
    }

    fn constant_array_index(&self, index: &TypedExpr) -> Option<usize> {
        let ExprIr::Number(bits) = &index.expr else {
            return None;
        };
        let value = f64::from_bits(*bits);
        if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
            return None;
        }
        let index = value as usize;
        (index <= MAX_STATIC_ARRAY_SHAPE_INDEX).then_some(index)
    }

    fn update_written_shape(
        &mut self,
        target: &Expression,
        key: &PropertyKeyIr,
        value: &ValueInfo,
    ) {
        if self.update_well_known_symbol_prototype_property(target, key, Some(value)) {
            return;
        }
        let Some((root, mut path)) = self.binding_shape_path(target) else {
            return;
        };
        path.push(key.clone());
        self.update_binding_shape_path(&root, &path, value.clone());
    }

    fn update_well_known_symbol_prototype_property(
        &mut self,
        target: &Expression,
        key: &PropertyKeyIr,
        value: Option<&ValueInfo>,
    ) -> bool {
        let PropertyKeyIr::StringExpr(key) = key else {
            return false;
        };
        let key_is_symbol = key.kind == ValueKind::Symbol;
        if !key.possible_kinds.contains(ValueKind::Symbol) {
            return false;
        }
        let Some((root, path)) = self.binding_shape_path(target) else {
            return key_is_symbol;
        };
        let targets_builtin_prototype = path.as_slice()
            == [PropertyKeyIr::StaticString("prototype".to_string())]
            && self.lookup_binding(&root).is_none()
            && self
                .lookup_global_property_info(&root)
                .is_some_and(|property| {
                    property.proven_present && property.source == GlobalPropertySource::Builtin
                });
        if !targets_builtin_prototype {
            return key_is_symbol;
        }
        if key_is_symbol {
            if let ExprIr::WellKnownSymbol(symbol) = &key.expr {
                match value {
                    Some(value) => {
                        self.well_known_symbol_prototype_properties
                            .insert((root, *symbol), value.clone());
                    }
                    None => {
                        self.well_known_symbol_prototype_properties
                            .remove(&(root, *symbol));
                    }
                }
                return true;
            }
        }
        self.well_known_symbol_prototype_properties
            .retain(|(constructor_name, _), _| constructor_name != &root);
        key_is_symbol
    }

    fn clear_binding_shape(&mut self, target: &Expression) {
        let Some((root, path)) = self.binding_shape_path(target) else {
            return;
        };
        self.update_binding_shape_path(
            &root,
            &path,
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
        );
    }

    fn static_array_shape_len(target: &TypedExpr) -> Option<usize> {
        let HeapShape::Array(shape) = target.heap_shape.as_deref()? else {
            return None;
        };
        Some(shape.elements.len())
    }

    fn array_shape_has_custom_prototype(target: &TypedExpr) -> bool {
        matches!(
            target.heap_shape.as_deref(),
            Some(HeapShape::Array(shape)) if shape.prototype.is_some()
        )
    }

    fn has_array_prototype_shape(target: &TypedExpr) -> bool {
        let expected = Self::array_prototype_shape();
        target.heap_shape.as_deref() == Some(expected.as_ref())
    }

    fn binding_shape_path(&mut self, expr: &Expression) -> Option<(String, Vec<PropertyKeyIr>)> {
        match expr {
            Expression::This(_) => Some((LEXICAL_THIS_NAME.to_string(), Vec::new())),
            Expression::Identifier(identifier) => Some((
                self.interner.resolve_expect(identifier.sym()).to_string(),
                Vec::new(),
            )),
            Expression::PropertyAccess(PropertyAccess::Simple(access)) => {
                let (root, mut path) = self.binding_shape_path(access.target())?;
                let key = match access.field() {
                    PropertyAccessField::Const(name) => {
                        let name = self.interner.resolve_expect(name.sym()).to_string();
                        if name == "length" {
                            PropertyKeyIr::ArrayLength
                        } else {
                            PropertyKeyIr::StaticString(name.to_string())
                        }
                    }
                    PropertyAccessField::Expr(expr) => {
                        if let Some(key) = self.lower_static_property_key(expr) {
                            key
                        } else if let Some(index) = self.try_constant_array_index_expr(expr) {
                            PropertyKeyIr::ArrayIndex(Box::new(TypedExpr::from_info(
                                ValueInfo {
                                    kind: ValueKind::Number,
                                    possible_kinds: KindSet::from_kind(ValueKind::Number),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::none(),
                                },
                                ExprIr::Number(index.to_bits()),
                            )))
                        } else {
                            return Some((root, path));
                        }
                    }
                };
                path.push(key);
                Some((root, path))
            }
            _ => None,
        }
    }

    /// Invalidates the two facts a property write can make stale through an
    /// untracked object alias: a remembered Boolean value and the copied heap
    /// shape that still names its old prototype method.
    ///
    /// Binding shapes are copied by value; there is no object-identity carrier
    /// joining `let alias = value` back to `value`. The pre-write Boolean fold
    /// domain is therefore the smallest sound alias domain available here.
    /// Preserve only the precisely resolved write target so the caller can
    /// apply its exact shape update, and erase every other candidate's shape.
    fn invalidate_static_boolean_alias_shapes(&mut self, target_name: &str) {
        self.invalidate_static_boolean_alias_shapes_except(Some(target_name));
    }

    fn invalidate_all_static_boolean_alias_shapes(&mut self) {
        self.invalidate_static_boolean_alias_shapes_except(None);
    }

    fn invalidate_static_boolean_alias_shapes_except(&mut self, target_name: Option<&str>) {
        let target_location = target_name
            .filter(|name| *name != LEXICAL_THIS_NAME)
            .and_then(|name| self.lookup_binding_with_location(name))
            .map(|(_, location)| location);
        let candidates = std::mem::take(&mut self.static_boolean_bindings);
        if candidates.is_empty() {
            return;
        }
        self.boolean_alias_shapes_invalidated = true;

        for (scope_index, scope) in self.scopes.iter_mut().enumerate() {
            for (name, binding) in scope {
                let is_target = target_name == Some(name.as_str())
                    && target_location == Some(BindingLookupLocation::Scope(scope_index));
                if candidates.contains_key(name) && !is_target {
                    binding.heap_shape = None;
                }
            }
        }
        for (name, binding) in &mut self.var_bindings {
            let is_target = target_name == Some(name.as_str())
                && (target_location == Some(BindingLookupLocation::VariableEnvironment)
                    || (target_location.is_none() && binding.is_script_global));
            if candidates.contains_key(name) && !is_target {
                binding.heap_shape = None;
            }
        }
        self.global_properties.edit_matching(
            |name, property| {
                let is_target = target_name == Some(name.as_str())
                    && !matches!(target_location, Some(BindingLookupLocation::Scope(_)));
                candidates.contains_key(name)
                    && !is_target
                    && property.value_info.heap_shape.is_some()
            },
            |_, property| property.value_info.heap_shape = None,
        );
        for infos in [
            &mut self.nested_script_global_value_infos,
            &mut self.known_nested_script_global_value_infos,
        ] {
            for (name, info) in infos {
                let is_target = target_name == Some(name.as_str())
                    && !matches!(target_location, Some(BindingLookupLocation::Scope(_)));
                if candidates.contains_key(name) && !is_target {
                    info.heap_shape = None;
                }
            }
        }
        if target_name != Some(LEXICAL_THIS_NAME) && candidates.contains_key(LEXICAL_THIS_NAME) {
            if let Some(info) = &mut self.current_construct_this_info {
                info.heap_shape = None;
            }
        }
    }

    fn update_binding_shape_path(&mut self, name: &str, path: &[PropertyKeyIr], value: ValueInfo) {
        if !path.is_empty() {
            self.invalidate_static_boolean_alias_shapes(name);
        }
        if name == LEXICAL_THIS_NAME {
            if let Some(current) = self.current_construct_this_info.clone() {
                self.current_construct_this_info =
                    Some(Self::apply_shape_write(current, path, value));
            }
            return;
        }
        let is_nested_global = self.is_nested_script_global_var_name(name);
        let is_visible_script_global =
            self.visible_function_names.contains_key(name) && self.is_script_global_var_name(name);
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                let binding_value = ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                };
                let next = Self::apply_shape_write(binding_value, path, value);
                let global_next = next.clone();
                binding.kind = next.kind;
                binding.possible_kinds = next.possible_kinds;
                binding.heap_shape = next.heap_shape;
                binding.function_targets = next.function_targets;
                if is_visible_script_global {
                    self.set_global_property_value_info_with_source(
                        name.to_string(),
                        global_next,
                        GlobalPropertySource::GlobalWrite,
                    );
                }
                return;
            }
        }
        let mut written_info = None;
        if let Some(binding) = self.var_bindings.get_mut(name) {
            let binding_value = ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            };
            let next = Self::apply_shape_write(binding_value, path, value.clone());
            let written = next.clone();
            binding.kind = next.kind;
            binding.possible_kinds = next.possible_kinds;
            binding.heap_shape = next.heap_shape;
            binding.function_targets = next.function_targets;
            written_info = Some(written);
        }
        if let Some(global) = self.global_properties.get_mut(name) {
            global.value_info = Self::apply_shape_write(global.value_info.clone(), path, value);
            // A property store through a non-writable binding (`undefined.x
            // = ...`) can neither replace the binding nor mutate the
            // primitive it denotes; only a writable name may lose its
            // intrinsic source here.
            if Self::global_property_is_writable_name(name) {
                global.proven_present = true;
                global.source = GlobalPropertySource::GlobalWrite;
                self.observed_script_global_writes.insert(name.to_string());
            }
            written_info = Some(global.value_info.clone());
        }
        if is_nested_global {
            if let Some(info) = written_info {
                self.record_nested_script_global_value_info(name, info);
            }
        }
    }

    fn apply_shape_write(
        mut target: ValueInfo,
        path: &[PropertyKeyIr],
        value: ValueInfo,
    ) -> ValueInfo {
        if path.is_empty() {
            return value;
        }
        let Some(shape) = target.heap_shape.as_mut() else {
            return target;
        };
        // A string-keyed write must never land on (or shadow) a symbol-keyed
        // shape entry. Leaving the property untracked is safe: string-keyed
        // reads of such a name never consult the shape either.
        if let PropertyKeyIr::StaticString(key) = &path[0] {
            if shape_property_name_is_symbol_keyed(key) {
                return target;
            }
        }
        match (shape.as_mut(), &path[0]) {
            (HeapShape::Object(object), PropertyKeyIr::StaticString(key)) => {
                if path.len() == 1 {
                    match object.properties.get(key).cloned() {
                        Some(ObjectShapeProperty::Accessor { getter, setter }) => {
                            object.properties.insert(
                                key.clone(),
                                ObjectShapeProperty::Accessor { getter, setter },
                            );
                        }
                        _ => {
                            object
                                .properties
                                .insert(key.clone(), ObjectShapeProperty::Data(value));
                        }
                    }
                } else if let Some(ObjectShapeProperty::Data(existing)) =
                    object.properties.get(key).cloned()
                {
                    object.properties.insert(
                        key.clone(),
                        ObjectShapeProperty::Data(Self::apply_shape_write(
                            existing,
                            &path[1..],
                            value,
                        )),
                    );
                }
            }
            (HeapShape::Object(_), PropertyKeyIr::StringExpr(_)) => {
                target.heap_shape = None;
            }
            (HeapShape::Array(array), PropertyKeyIr::ArrayIndex(index)) => {
                let Some(index) = Self::constant_array_index_static(index) else {
                    target.heap_shape = None;
                    return target;
                };
                if array.elements.len() <= index {
                    array.elements.resize(index + 1, ValueInfo::undefined());
                }
                if path.len() == 1 {
                    array.elements[index] = value;
                } else {
                    let existing = array.elements[index].clone();
                    array.elements[index] = Self::apply_shape_write(existing, &path[1..], value);
                }
            }
            (HeapShape::Array(_), PropertyKeyIr::ArrayLength) => {
                // ArrayShape does not track the assigned length or whether it
                // truncates/creates holes. Keeping the old dense element vector
                // would make later concat/flat/index inference overwrite tags.
                target.heap_shape = None;
            }
            (HeapShape::Array(array), PropertyKeyIr::StaticString(key)) => {
                if path.len() == 1 {
                    match array.properties.get(key).cloned() {
                        Some(ObjectShapeProperty::Accessor { getter, setter }) => {
                            array.properties.insert(
                                key.clone(),
                                ObjectShapeProperty::Accessor { getter, setter },
                            );
                        }
                        _ => {
                            array
                                .properties
                                .insert(key.clone(), ObjectShapeProperty::Data(value));
                        }
                    }
                } else if let Some(ObjectShapeProperty::Data(existing)) =
                    array.properties.get(key).cloned()
                {
                    array.properties.insert(
                        key.clone(),
                        ObjectShapeProperty::Data(Self::apply_shape_write(
                            existing,
                            &path[1..],
                            value,
                        )),
                    );
                } else {
                    target.heap_shape = None;
                }
            }
            (HeapShape::Array(_), PropertyKeyIr::StringExpr(_)) => {
                target.heap_shape = None;
            }
            (HeapShape::Object(_), PropertyKeyIr::ArrayIndex(_) | PropertyKeyIr::ArrayLength) => {
                target.heap_shape = None;
            }
        }
        target
    }

    fn constant_array_index_static(index: &TypedExpr) -> Option<usize> {
        let ExprIr::Number(bits) = &index.expr else {
            return None;
        };
        let value = f64::from_bits(*bits);
        if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
            return None;
        }
        let index = value as usize;
        (index <= MAX_STATIC_ARRAY_SHAPE_INDEX).then_some(index)
    }

    fn try_constant_array_index_expr(&self, expr: &Expression) -> Option<f64> {
        let Expression::Literal(literal) = expr else {
            return None;
        };
        match literal.kind() {
            LiteralKind::Num(value) => {
                if value.is_finite() && *value >= 0.0 && value.fract() == 0.0 {
                    Some(*value)
                } else {
                    None
                }
            }
            LiteralKind::Int(value) if *value >= 0 => Some(*value as f64),
            _ => None,
        }
    }

    fn literal_number_value(expr: &Expression) -> Option<f64> {
        match expr {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::Num(value) => Some(*value),
                LiteralKind::Int(value) => Some(*value as f64),
                _ => None,
            },
            Expression::Unary(unary) => {
                let value = Self::literal_number_value(unary.target())?;
                match unary.op() {
                    UnaryOp::Plus => Some(value),
                    UnaryOp::Minus => Some(-value),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn static_is_integral_number(value: f64) -> bool {
        value.is_finite() && value.trunc() == value
    }

    fn static_is_odd_integral_number(value: f64) -> bool {
        Self::static_is_integral_number(value)
            && value.abs() <= 9_007_199_254_740_991.0
            && (value.abs() as u64) % 2 == 1
    }

    fn static_is_negative_zero(value: f64) -> bool {
        value.to_bits() == (-0.0f64).to_bits()
    }

    fn static_pow(base: f64, exponent: f64) -> f64 {
        if exponent == 0.0 {
            return 1.0;
        }
        if exponent.is_nan() || base.is_nan() {
            return f64::NAN;
        }
        if exponent.is_infinite() {
            let abs_base = base.abs();
            if abs_base > 1.0 {
                return if exponent.is_sign_positive() {
                    f64::INFINITY
                } else {
                    0.0
                };
            }
            if abs_base == 1.0 {
                return f64::NAN;
            }
            return if exponent.is_sign_positive() {
                0.0
            } else {
                f64::INFINITY
            };
        }
        if base == f64::INFINITY {
            return if exponent > 0.0 { f64::INFINITY } else { 0.0 };
        }
        if base == f64::NEG_INFINITY {
            if exponent > 0.0 {
                return if Self::static_is_odd_integral_number(exponent) {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
            }
            return if Self::static_is_odd_integral_number(exponent) {
                -0.0
            } else {
                0.0
            };
        }
        if Self::static_is_negative_zero(base) {
            if exponent > 0.0 {
                return if Self::static_is_odd_integral_number(exponent) {
                    -0.0
                } else {
                    0.0
                };
            }
            return if Self::static_is_odd_integral_number(exponent) {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        if base == 0.0 {
            return if exponent > 0.0 { 0.0 } else { f64::INFINITY };
        }
        if base < 0.0 && !Self::static_is_integral_number(exponent) {
            return f64::NAN;
        }
        base.powf(exponent)
    }

    /// Recognize number expressions whose evaluation has no observable effects.
    /// Property accesses always retain runtime Get: a receiver's source name
    /// cannot prove its identity, property value, or absence of a getter.
    fn static_number_expr(&self, expr: &Expression) -> Option<f64> {
        match Self::unwrap_parenthesized_expr(expr) {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::Num(value) => Some(*value),
                LiteralKind::Int(value) => Some(*value as f64),
                _ => None,
            },
            Expression::Identifier(identifier) => {
                // Ledger row **LN5**'s original site. `let Infinity = 5;
                // Math.clz32(Infinity)` folded against the builtin's value here
                // because the test was on the spelling alone.
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                match name.as_str() {
                    "NaN" | "Infinity" => self.static_global_number_identifier(&name),
                    _ => None,
                }
            }
            Expression::Unary(unary) => {
                let value = self.static_number_expr(unary.target())?;
                match unary.op() {
                    UnaryOp::Plus => Some(value),
                    UnaryOp::Minus => Some(-value),
                    _ => None,
                }
            }
            Expression::Binary(binary) => {
                let lhs = self.static_number_expr(binary.lhs())?;
                let rhs = self.static_number_expr(binary.rhs())?;
                match binary.op() {
                    BinaryOp::Arithmetic(ArithmeticOp::Add) => Some(lhs + rhs),
                    BinaryOp::Arithmetic(ArithmeticOp::Sub) => Some(lhs - rhs),
                    BinaryOp::Arithmetic(ArithmeticOp::Mul) => Some(lhs * rhs),
                    BinaryOp::Arithmetic(ArithmeticOp::Div) => Some(lhs / rhs),
                    BinaryOp::Arithmetic(ArithmeticOp::Mod) => Some(lhs % rhs),
                    BinaryOp::Arithmetic(ArithmeticOp::Exp) => Some(Self::static_pow(lhs, rhs)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn declare_binding(&mut self, name: String, info: BindingInfo) {
        self.finite_source_capture_keys
            .remove(&(self.scopes.len() - 1, name.clone()));
        self.scopes
            .last_mut()
            .expect("scope stack must exist")
            .insert(name, info);
    }

    fn set_binding_value_info(&mut self, name: &str, info: ValueInfo) -> Option<()> {
        if !self.scopes.iter().any(|scope| scope.contains_key(name))
            && !self.var_bindings.contains_key(name)
        {
            return None;
        }
        let binding = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
            .or_else(|| {
                self.var_bindings
                    .get(name)
                    .map(|binding| binding.to_binding_info(name))
            })
            .unwrap_or_else(|| panic!("binding `{name}` must exist before recording its write"));
        self.record_binding_value_write(name, Some(&binding));
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.kind = info.kind;
                binding.possible_kinds = info.possible_kinds;
                binding.heap_shape = info.heap_shape.clone();
                binding.function_targets = info.function_targets.clone();
                return Some(());
            }
        }
        if let Some(binding) = self.var_bindings.get_mut(name) {
            let is_script_global = binding.is_script_global;
            binding.kind = info.kind;
            binding.possible_kinds = info.possible_kinds;
            binding.heap_shape = info.heap_shape.clone();
            binding.function_targets = info.function_targets.clone();
            if is_script_global {
                self.set_global_property_value_info(name.to_string(), info);
            }
            return Some(());
        }
        None
    }

    fn widen_binding_for_possible_replacement(&mut self, name: &str) -> Option<()> {
        let binding = self.lookup_binding(name)?;
        let mut value = ValueInfo {
            kind: binding.kind,
            possible_kinds: binding.possible_kinds,
            heap_shape: binding.heap_shape,
            function_targets: binding.function_targets,
        };
        value.widen_for_possible_replacement();
        self.set_binding_value_info(name, value)
    }

    /// Replaces the facts of a binding this lowerer declares. A name it does
    /// not declare is left alone: an executed static element of a class inside
    /// a closure can capture a variable from beyond that closure, which the
    /// closure never names itself, so the facts live in an outer lowerer.
    fn install_binding_value_info(&mut self, name: &str, info: ValueInfo) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.kind = info.kind;
                binding.possible_kinds = info.possible_kinds;
                binding.heap_shape = info.heap_shape;
                binding.function_targets = info.function_targets;
                return;
            }
        }
        let Some(binding) = self.var_bindings.get_mut(name) else {
            return;
        };
        binding.kind = info.kind;
        binding.possible_kinds = info.possible_kinds;
        binding.heap_shape = info.heap_shape;
        binding.function_targets = info.function_targets;
    }

    fn set_owner_binding_value_info(&mut self, name: &str, info: ValueInfo) {
        let owner_binding = self
            .scopes
            .first()
            .and_then(|scope| scope.get(name).cloned())
            .or_else(|| {
                self.var_bindings
                    .get(name)
                    .map(|binding| binding.to_binding_info(name))
            });
        if let Some(owner_binding) = owner_binding.as_ref() {
            self.record_binding_value_write(name, Some(owner_binding));
        }
        if let Some(binding) = self
            .scopes
            .first_mut()
            .and_then(|scope| scope.get_mut(name))
        {
            binding.kind = info.kind;
            binding.possible_kinds = info.possible_kinds;
            binding.heap_shape = info.heap_shape;
            binding.function_targets = info.function_targets;
            return;
        }
        if let Some(binding) = self.var_bindings.get_mut(name) {
            binding.kind = info.kind;
            binding.possible_kinds = info.possible_kinds;
            binding.heap_shape = info.heap_shape;
            binding.function_targets = info.function_targets;
            return;
        }
        self.unsupported_with_message(format!(
            "unsupported in scriptw wasm-aot first slice: Annex B declaration `{name}` has no owner variable binding"
        ));
    }

    fn coerce_expr_to_number(&mut self, mut expr: TypedExpr) -> Option<TypedExpr> {
        if expr.possible_kinds == KindSet::from_kind(ValueKind::Number) {
            expr.kind = ValueKind::Number;
            return Some(expr);
        }
        if expr.possible_kinds.contains(ValueKind::BigInt)
            || expr.possible_kinds.contains(ValueKind::Symbol)
        {
            return None;
        }
        if expr.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY) {
            expr.kind = ValueKind::Number;
            expr.possible_kinds = KindSet::from_kind(ValueKind::Number);
            expr.heap_shape = None;
            expr.function_targets.replace_with_no_function();
            return Some(expr);
        }
        if expr.possible_kinds != KindSet::all_runtime_tags() {
            return None;
        }

        match &expr.expr {
            ExprIr::Identifier(name) => {
                self.set_binding_kind(name, ValueKind::Number)?;
                expr.kind = ValueKind::Number;
                expr.possible_kinds = KindSet::from_kind(ValueKind::Number);
                expr.heap_shape = None;
                expr.function_targets.replace_with_no_function();
                Some(expr)
            }
            ExprIr::CallNamed { name, .. } => {
                let (next_info, next_shape) = {
                    let function_id = self.visible_function_names.get(name)?;
                    let signature = self.function_signatures.get(function_id)?;
                    self.merge_signature_return_value_observation(
                        signature,
                        ValueInfo {
                            kind: ValueKind::Number,
                            possible_kinds: KindSet::from_kind(ValueKind::Number),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                        },
                        &FunctionReturnShape::Absent,
                    )
                };
                if let Some(function_id) = self.visible_function_names.get(name).cloned() {
                    if let Some(signature) = self.function_signatures.get_mut(&function_id) {
                        signature.return_kind = next_info.kind;
                        signature.return_possible_kinds = next_info.possible_kinds;
                        signature.return_shape = next_shape;
                        signature.return_targets = next_info.function_targets.clone();
                    }
                }
                expr.kind = ValueKind::Number;
                expr.possible_kinds = KindSet::from_kind(ValueKind::Number);
                expr.heap_shape = None;
                expr.function_targets.replace_with_no_function();
                Some(expr)
            }
            ExprIr::CallIndirect { callee, .. } => {
                let function_id = self.resolve_single_function_target(callee)?;
                let (next_info, next_shape) = {
                    let signature = self.function_signatures.get(&function_id)?;
                    self.merge_signature_return_value_observation(
                        signature,
                        ValueInfo {
                            kind: ValueKind::Number,
                            possible_kinds: KindSet::from_kind(ValueKind::Number),
                            heap_shape: None,
                            function_targets: FunctionTargetKnowledge::none(),
                        },
                        &FunctionReturnShape::Absent,
                    )
                };
                if let Some(signature) = self.function_signatures.get_mut(&function_id) {
                    signature.return_kind = next_info.kind;
                    signature.return_possible_kinds = next_info.possible_kinds;
                    signature.return_shape = next_shape;
                    signature.return_targets = next_info.function_targets.clone();
                }
                expr.kind = ValueKind::Number;
                expr.possible_kinds = KindSet::from_kind(ValueKind::Number);
                expr.heap_shape = None;
                expr.function_targets.replace_with_no_function();
                Some(expr)
            }
            _ => None,
        }
    }

    fn capture_value_info(&self, owner_id: &str, name: &str) -> ValueInfo {
        if name == LEXICAL_THIS_NAME {
            if owner_id == SCRIPT_OWNER_ID {
                return self.root_this_info();
            }
            return self
                .function_signature_for_current_flow(owner_id)
                .map(|signature| signature.this_info.clone())
                .unwrap_or_else(ValueInfo::undefined);
        }
        if name == LEXICAL_ARGUMENTS_NAME {
            return ValueInfo {
                kind: ValueKind::Arguments,
                possible_kinds: KindSet::from_kind(ValueKind::Arguments),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            };
        }
        if name == LEXICAL_NEW_TARGET_NAME {
            if owner_id == SCRIPT_OWNER_ID {
                return ValueInfo::undefined();
            }
            let Some(signature) = self.function_signatures.get(owner_id) else {
                return ValueInfo::undefined();
            };
            if signature.protocol.flavor() == FunctionFlavor::Arrow {
                return ValueInfo::undefined();
            }
            if signature.protocol.is_constructable()
                || signature.protocol.class_kind() == ClassFunctionKind::Constructor
            {
                return self.merge_value_infos(
                    ValueInfo::undefined(),
                    self.function_value_info(&signature.id),
                );
            }
            return ValueInfo::undefined();
        }
        if let Some(info) = self.class_name_binding_value_info_by_storage_name(name) {
            return info;
        }
        let Some(owner) = self.analysis.owner_plans.get(owner_id) else {
            return ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            };
        };
        if owner_id == SCRIPT_OWNER_ID {
            if let Some(info) = self.lookup_global_property(name) {
                return info;
            }
        }
        if let Some(function_id) = owner.function_bindings.get(name) {
            return self.function_value_info(function_id);
        }
        if let Some(info) = self.capture_parameter_value_info(owner_id, name) {
            return info;
        }
        if let Some(info) = self.infer_owner_var_binding_info(owner_id, name) {
            return info;
        }
        if owner_id == SCRIPT_OWNER_ID {
            if let Some(binding) = self.var_bindings.get(name) {
                return ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                };
            }
        }
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::all_runtime_tags(),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn class_name_binding_value_info_by_storage_name(
        &self,
        storage_name: &str,
    ) -> Option<ValueInfo> {
        if !is_class_name_binding_storage_name(storage_name) {
            return None;
        }
        self.scopes
            .iter()
            .rev()
            .flat_map(BTreeMap::values)
            .find(|binding| binding.storage_name == storage_name)
            .map(|binding| ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            })
    }

    fn capture_parameter_value_info(&self, owner_id: &str, name: &str) -> Option<ValueInfo> {
        let plan = self.analysis.function_plans.get(owner_id)?;
        let signature = self.function_signature_for_current_flow(owner_id)?;
        let mut positional_index = 0usize;
        for parameter in plan.parameters.as_ref() {
            if let Binding::Identifier(identifier) = parameter.variable().binding() {
                let parameter_name = self.interner.resolve_expect(identifier.sym()).to_string();
                if parameter_name == name {
                    let signature_param = signature.params.get(positional_index)?;
                    return Some(ValueInfo {
                        kind: signature_param.possible_kinds.as_value_kind(),
                        possible_kinds: signature_param.possible_kinds,
                        heap_shape: signature_param.heap_shape.clone(),
                        function_targets: signature_param.function_targets.clone(),
                    });
                }
            }
            positional_index += 1;
        }
        None
    }

    fn infer_owner_var_binding_info(&self, owner_id: &str, name: &str) -> Option<ValueInfo> {
        let result = self
            .analysis
            .binding_inference
            .recipes(owner_id, name)
            .find_map(|recipe| match recipe {
                crate::analysis::BindingInferenceRecipe::Initializer(initializer) => initializer
                    .map(|expression| self.static_initializer_value_info(expression))
                    .unwrap_or_else(|| Some(ValueInfo::undefined())),
                crate::analysis::BindingInferenceRecipe::Class(class) => {
                    Some(self.infer_class_declaration_binding_info(class))
                }
                crate::analysis::BindingInferenceRecipe::ForInKey => {
                    let kinds = KindSet::from_kind(ValueKind::String)
                        .union(KindSet::from_kind(ValueKind::Undefined));
                    Some(ValueInfo {
                        kind: kinds.as_value_kind(),
                        possible_kinds: kinds,
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    })
                }
            });
        // Native IR controls also evaluate the previous independent whole-AST
        // lookup in this exact current context. No result/fixpoint is cached.
        #[cfg(test)]
        {
            let expected = if owner_id == SCRIPT_OWNER_ID {
                self.infer_var_binding_info_from_items(self.analysis.script_items, name)
            } else {
                self.analysis.function_plans.get(owner_id).and_then(|plan| {
                    self.infer_var_binding_info_from_items(plan.body.statements(), name)
                })
            };
            assert_eq!(
                result, expected,
                "indexed declaration lookup for {owner_id}/{name}"
            );
        }
        result
    }

    #[cfg(test)]
    fn infer_var_binding_info_from_items(
        &self,
        items: &[StatementListItem],
        name: &str,
    ) -> Option<ValueInfo> {
        for item in items {
            let info = match item {
                StatementListItem::Statement(statement) => {
                    self.infer_var_binding_info_from_statement(statement, name)
                }
                StatementListItem::Declaration(declaration) => match declaration.as_ref() {
                    Declaration::Lexical(lexical) => {
                        self.infer_lexical_binding_info_from_declaration(lexical, name)
                    }
                    Declaration::ClassDeclaration(class) => {
                        if self.interner.resolve_expect(class.name().sym()).to_string() == name {
                            Some(self.infer_class_declaration_binding_info(class))
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
            };
            if info.is_some() {
                return info;
            }
        }
        None
    }

    #[cfg(test)]
    fn infer_var_binding_info_from_statement(
        &self,
        statement: &Statement,
        name: &str,
    ) -> Option<ValueInfo> {
        match statement {
            Statement::Var(var) => self.infer_var_binding_info_from_declaration(var, name),
            Statement::Block(block) => {
                self.infer_var_binding_info_from_items(block.statement_list().statements(), name)
            }
            Statement::If(if_statement) => self
                .infer_var_binding_info_from_statement(if_statement.body(), name)
                .or_else(|| {
                    if_statement.else_node().and_then(|else_node| {
                        self.infer_var_binding_info_from_statement(else_node, name)
                    })
                }),
            Statement::WhileLoop(while_loop) => {
                self.infer_var_binding_info_from_statement(while_loop.body(), name)
            }
            Statement::DoWhileLoop(do_while) => {
                self.infer_var_binding_info_from_statement(do_while.body(), name)
            }
            Statement::ForLoop(for_loop) => {
                if let Some(ForLoopInitializer::Var(var)) = for_loop.init() {
                    if let Some(info) = self.infer_var_binding_info_from_declaration(var, name) {
                        return Some(info);
                    }
                }
                self.infer_var_binding_info_from_statement(for_loop.body(), name)
            }
            Statement::ForInLoop(for_in) => {
                let loop_key_name = match for_in.initializer() {
                    IterableLoopInitializer::Identifier(identifier) => {
                        Some(self.interner.resolve_expect(identifier.sym()).to_string())
                    }
                    IterableLoopInitializer::Var(variable) => {
                        let Binding::Identifier(identifier) = variable.binding() else {
                            return self.infer_var_binding_info_from_statement(for_in.body(), name);
                        };
                        Some(self.interner.resolve_expect(identifier.sym()).to_string())
                    }
                    _ => None,
                };
                if loop_key_name.as_deref() == Some(name) {
                    // A `for-in` loop variable is a String only if the loop body
                    // runs at least once, and nothing here can prove that: the
                    // head is an arbitrary expression this pre-pass never
                    // evaluates, `{}` has no enumerable keys, and a statically
                    // nullish head takes 14.7.5.6 step 3.a's break completion and
                    // assigns nothing at all. A hoisted `var` is `undefined` in
                    // every one of those cases, so the honest static type is
                    // `String | Undefined`.
                    //
                    // This also matches what the lowering pass itself concludes:
                    // `lower_for_in_loop` merges the pre-loop bindings with the
                    // post-body ones (`merge_var_bindings` / `merge_global_properties`),
                    // which unions `Undefined` with `String`. Publishing a proven
                    // `String` here made the seeded *global* disagree with the
                    // merged *local* for the same source program, and the seed
                    // won, because it is already in `before_globals` when the
                    // merge runs. `for (var k in null) {} k + 1` then lowered to
                    // a `StringConcat` and produced `"undefined1"` where the spec
                    // requires `NaN`.
                    let key_kinds = KindSet::from_kind(ValueKind::String)
                        .union(KindSet::from_kind(ValueKind::Undefined));
                    return Some(ValueInfo {
                        kind: key_kinds.as_value_kind(),
                        possible_kinds: key_kinds,
                        heap_shape: None,
                        function_targets: FunctionTargetKnowledge::none(),
                    });
                }
                self.infer_var_binding_info_from_statement(for_in.body(), name)
            }
            Statement::Switch(switch) => {
                for case in switch.cases() {
                    if let Some(info) =
                        self.infer_var_binding_info_from_items(case.body().statements(), name)
                    {
                        return Some(info);
                    }
                }
                None
            }
            Statement::Labelled(labelled) => labelled_base_statement(labelled)
                .and_then(|statement| self.infer_var_binding_info_from_statement(statement, name)),
            Statement::Expression(_)
            | Statement::Empty
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Debugger
            | Statement::ForOfLoop(_)
            | Statement::Return(_)
            | Statement::Throw(_)
            | Statement::Try(_)
            | Statement::With(_) => None,
        }
    }

    #[cfg(test)]
    fn infer_var_binding_info_from_declaration(
        &self,
        declaration: &VarDeclaration,
        name: &str,
    ) -> Option<ValueInfo> {
        for variable in declaration.0.as_ref() {
            let Binding::Identifier(identifier) = variable.binding() else {
                continue;
            };
            if self.interner.resolve_expect(identifier.sym()).to_string() != name {
                continue;
            }
            return variable
                .init()
                .map(|expression| self.static_initializer_value_info(expression))
                .unwrap_or_else(|| Some(ValueInfo::undefined()));
        }
        None
    }

    #[cfg(test)]
    fn infer_lexical_binding_info_from_declaration(
        &self,
        declaration: &LexicalDeclaration,
        name: &str,
    ) -> Option<ValueInfo> {
        let list = match declaration {
            LexicalDeclaration::Let(list) | LexicalDeclaration::Const(list) => list,
            LexicalDeclaration::Using(_) | LexicalDeclaration::AwaitUsing(_) => return None,
        };
        for variable in list.as_ref() {
            let Binding::Identifier(identifier) = variable.binding() else {
                continue;
            };
            if self.interner.resolve_expect(identifier.sym()).to_string() != name {
                continue;
            }
            return variable
                .init()
                .map(|expression| self.static_initializer_value_info(expression))
                .unwrap_or_else(|| Some(ValueInfo::undefined()));
        }
        None
    }

    fn infer_class_declaration_binding_info(&self, class: &ClassDeclaration) -> ValueInfo {
        let heritage_info = class
            .super_ref()
            .and_then(|heritage| self.static_class_heritage_value_info(heritage));
        let heritage_prototype = heritage_info
            .as_ref()
            .and_then(|info| info.heap_shape.as_deref())
            .and_then(|shape| self.read_current_heap_shape_property(shape, "prototype"))
            .and_then(|property| match property {
                ObjectShapeProperty::Data(info) => info.heap_shape,
                ObjectShapeProperty::Accessor { .. } => None,
            });
        let prototype_info = ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: heritage_prototype,
                properties: BTreeMap::new(),
                private_brands: BTreeSet::new(),
                boxed_primitive: None,
            }))),
            function_targets: FunctionTargetKnowledge::none(),
        };
        let mut properties = BTreeMap::new();
        properties.insert(
            "prototype".to_string(),
            ObjectShapeProperty::Data(prototype_info),
        );
        ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: heritage_info.and_then(|info| info.heap_shape),
                properties,
                private_brands: BTreeSet::new(),
                boxed_primitive: None,
            }))),
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    fn static_class_heritage_value_info(&self, expression: &Expression) -> Option<ValueInfo> {
        match Self::unwrap_parenthesized_expr(expression) {
            Expression::Identifier(identifier) => {
                let name = self.interner.resolve_expect(identifier.sym()).to_string();
                StandardBuiltinId::all_globals()
                    .iter()
                    .copied()
                    .find(|builtin| builtin.global_name() == Some(name.as_str()))
                    .map(Self::standard_builtin_value_info)
            }
            _ => None,
        }
    }

    fn static_initializer_value_info(&self, expression: &Expression) -> Option<ValueInfo> {
        let expression = Self::unwrap_parenthesized_expr(expression);
        if self
            .analysis
            .module_execution
            .imports
            .contains_key(&(std::ptr::from_ref(expression) as usize))
        {
            // The linked source placeholder denotes an indirect live binding;
            // its literal spelling is not a value that a closure can capture.
            return Some(unknown_runtime_value_info());
        }
        match expression {
            Expression::Literal(literal) => match literal.kind() {
                LiteralKind::Num(_) | LiteralKind::Int(_) => {
                    Some(ValueInfo::new(ValueKind::Number))
                }
                LiteralKind::Bool(_) => Some(ValueInfo::new(ValueKind::Boolean)),
                LiteralKind::String(_) => Some(ValueInfo::new(ValueKind::String)),
                LiteralKind::Null => Some(ValueInfo::new(ValueKind::Null)),
                LiteralKind::Undefined => Some(ValueInfo::undefined()),
                LiteralKind::BigInt(_) => Some(ValueInfo::new(ValueKind::BigInt)),
            },
            Expression::FunctionExpression(function) => {
                let key = function_expression_key(function);
                self.analysis
                    .function_expr_ids
                    .get(&key)
                    .map(|function_id| self.function_value_info(function_id))
            }
            Expression::ArrowFunction(function) => {
                let key = arrow_function_key(function);
                self.analysis
                    .function_expr_ids
                    .get(&key)
                    .map(|function_id| self.function_value_info(function_id))
            }
            Expression::ArrayLiteral(array) => {
                if array
                    .as_ref()
                    .iter()
                    .any(|element| matches!(element, Some(Expression::Spread(_))))
                {
                    return None;
                }
                let mut shape = ArrayShape::default();
                for element in array.as_ref() {
                    let info = element
                        .as_ref()
                        .and_then(|element| self.static_initializer_value_info(element))
                        .unwrap_or_else(ValueInfo::undefined);
                    shape.elements.push(info);
                }
                Some(ValueInfo {
                    kind: ValueKind::Array,
                    possible_kinds: KindSet::from_kind(ValueKind::Array),
                    heap_shape: Some(Box::new(HeapShape::Array(shape))),
                    function_targets: FunctionTargetKnowledge::none(),
                })
            }
            Expression::RegExpLiteral(_) => Some(Self::value_info_from_shape(Some(
                Self::regexp_instance_shape(),
            ))),
            Expression::New(new_expr) => {
                match Self::unwrap_parenthesized_expr(new_expr.constructor()) {
                    Expression::Identifier(identifier) => {
                        let constructor_name =
                            self.interner.resolve_expect(identifier.sym()).to_string();
                        match constructor_name.as_str() {
                            ARRAY_BUFFER_NAME => Some(Self::value_info_from_shape(Some(
                                Self::array_buffer_instance_shape(),
                            ))),
                            DATA_VIEW_NAME => Some(Self::value_info_from_shape(Some(
                                Self::data_view_instance_shape(),
                            ))),
                            SHARED_ARRAY_BUFFER_NAME => Some(Self::value_info_from_shape(Some(
                                Self::shared_array_buffer_instance_shape(),
                            ))),
                            _ => StandardBuiltinId::all_globals()
                                .iter()
                                .copied()
                                .find(|builtin| {
                                    builtin.global_name() == Some(constructor_name.as_str())
                                        && Self::is_typed_array_constructor(*builtin)
                                })
                                .map(|builtin| {
                                    Self::value_info_from_shape(Some(
                                        Self::typed_array_instance_shape_for_constructor(builtin),
                                    ))
                                }),
                        }
                    }
                    _ => None,
                }
            }
            Expression::PropertyAccess(_) => {
                if self.is_constructor_prototype_property_expr(expression, BIGINT_NAME, "toString")
                {
                    return Some(Self::standard_builtin_value_info(
                        StandardBuiltinId::BigIntPrototypeToString,
                    ));
                }
                if self.is_constructor_prototype_property_expr(expression, BIGINT_NAME, "valueOf") {
                    return Some(Self::standard_builtin_value_info(
                        StandardBuiltinId::BigIntPrototypeValueOf,
                    ));
                }
                if self.is_constructor_prototype_property_expr(
                    expression,
                    BIGINT_NAME,
                    "toLocaleString",
                ) {
                    return Some(Self::standard_builtin_value_info(
                        StandardBuiltinId::BigIntPrototypeToLocaleString,
                    ));
                }
                None
            }
            _ => None,
        }
    }

    fn is_script_global_object_capture(
        &self,
        name: &str,
        source_name: &str,
        owner_id: &str,
    ) -> bool {
        if TdzPlaceholderName::names_a_placeholder(name) {
            return false;
        }
        name == source_name
            && owner_id == SCRIPT_OWNER_ID
            && (self
                .var_bindings
                .get(source_name)
                .is_some_and(|binding| binding.is_script_global)
                || (self.script_variables_are_global()
                    && !self
                        .analysis
                        .module_execution
                        .is_private_dispatcher_name(source_name)
                    && self.analysis.owner_plans[SCRIPT_OWNER_ID]
                        .function_bindings
                        .contains_key(source_name)))
    }

    fn generated_owned_env_bindings_for_owner(&self, owner_id: &str) -> Vec<OwnedEnvBindingIr> {
        let owner = self
            .analysis
            .owner_plans
            .get(owner_id)
            .unwrap_or_else(|| panic!("generated function owner `{owner_id}` must be planned"));
        owner
            .owned_env_slots
            .iter()
            .map(|(name, slot)| {
                self.analysis
                    .owner_environment_binding(owner_id, name, *slot, self.interner)
            })
            .collect()
    }

    fn generated_captured_bindings_for_owner(&self, owner_id: &str) -> Vec<CapturedBindingIr> {
        let local_bindings = self
            .analysis
            .owner_plans
            .get(owner_id)
            .unwrap_or_else(|| panic!("generated function owner `{owner_id}` must be planned"))
            .root_bindings
            .clone();
        self.analysis
            .owner_free_refs
            .get(owner_id)
            .unwrap_or_else(|| {
                panic!("generated function owner `{owner_id}` must have finalized references")
            })
            .iter()
            .filter(|(name, _)| {
                !local_bindings.contains(*name)
                    || self.analysis.owner_plans[owner_id]
                        .parameter_external_refs
                        .contains(*name)
            })
            .filter_map(|(name, source_name)| {
                let environment_id = self.resolve_generated_capture_environment(owner_id, name)?;
                let environment = self.analysis.environment_plans.get(&environment_id)?;
                let capture_owner_id = environment.owner_id.clone();
                let slot = *environment.owned_env_slots.get(name)?;
                if self.is_script_global_object_capture(name, source_name, &capture_owner_id) {
                    return None;
                }
                Some(CapturedBindingIr {
                    name: name.clone(),
                    source_name: source_name.clone(),
                    mode: *environment.binding_modes.get(name).expect(
                        "generated capture mode must be planned on its physical environment",
                    ),
                    slot,
                    hops: self.generated_capture_hops(owner_id, environment_id),
                })
            })
            .collect()
    }

    fn resolve_generated_capture_environment(
        &self,
        owner_id: &str,
        name: &str,
    ) -> Option<EnvironmentId> {
        let mut cursor = Some(
            self.analysis
                .owner_plans
                .get(owner_id)?
                .definition_environment_cursor
                .clone(),
        );
        while let Some(current) = cursor {
            let environment = self
                .analysis
                .environment_plans
                .get(&current.environment_id)?;
            if self
                .analysis
                .physical_binding_environments
                .get(name)
                .is_some_and(|environments| environments.contains(&environment.id))
            {
                return Some(
                    if self
                        .analysis
                        .materialized_environment(environment.id)
                        .is_some()
                    {
                        environment.id
                    } else {
                        self.analysis.owner_plans[&environment.owner_id].activation_environment_id
                    },
                );
            }
            cursor = environment.parent_cursor.clone();
        }
        None
    }

    fn generated_capture_hops(
        &self,
        current_owner_id: &str,
        target_environment_id: EnvironmentId,
    ) -> u32 {
        let owner = self
            .analysis
            .owner_plans
            .get(current_owner_id)
            .expect("generated function owner must be planned");
        let activation = &self.analysis.environment_plans[&owner.activation_environment_id];
        let mut cursor = if !self.analysis.environment_has_runtime_storage(activation) {
            owner
                .parent_owner_id
                .as_ref()
                .map(|_| owner.definition_environment_cursor.clone())
        } else {
            Some(EnvironmentCursor {
                owner_id: current_owner_id.to_string(),
                environment_id: owner.activation_environment_id,
            })
        };
        let mut hops = 0;
        while let Some(current) = cursor {
            let environment = &self.analysis.environment_plans[&current.environment_id];
            if self.analysis.environment_has_runtime_storage(environment) {
                if environment.id == target_environment_id {
                    return hops;
                }
                hops += 1;
            }
            cursor = environment.parent_cursor.clone();
        }
        panic!("generated capture environment must be reachable from its definition")
    }

    fn is_nested_script_global_var_name(&self, name: &str) -> bool {
        self.current_owner_id != SCRIPT_OWNER_ID
            && self
                .var_bindings
                .get(name)
                .is_some_and(|binding| binding.is_script_global)
    }

    fn is_script_global_var_name(&self, name: &str) -> bool {
        !self
            .analysis
            .module_execution
            .is_private_dispatcher_name(name)
            && self
                .var_bindings
                .get(name)
                .is_some_and(|binding| binding.is_script_global)
    }

    fn is_unshadowed_script_global_binding(&self, name: &str) -> bool {
        !self.has_scope_binding(name)
            && (self.is_script_global_var_name(name)
                || (self.lookup_binding(name).is_none()
                    && self.script_variables_are_global()
                    && !self
                        .analysis
                        .module_execution
                        .is_private_dispatcher_name(name)
                    && self.analysis.owner_plans[SCRIPT_OWNER_ID]
                        .function_bindings
                        .contains_key(name)))
    }

    fn has_scope_binding(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
    }

    fn seed_script_global_var_properties(&mut self) {
        let globals = self
            .var_bindings
            .iter()
            .filter(|(_, binding)| binding.is_script_global)
            .map(|(name, binding)| {
                let binding_info = ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                };
                let inferred_info = self.infer_owner_var_binding_info(SCRIPT_OWNER_ID, name);
                let info = match inferred_info {
                    Some(inferred)
                        if binding_info.kind == ValueKind::Undefined
                            || binding_info.kind == ValueKind::Dynamic =>
                    {
                        inferred
                    }
                    _ => binding_info,
                };
                let info = match self.known_nested_script_global_value_infos.get(name) {
                    Some(nested) => self.merge_value_infos(info, nested.clone()),
                    None => info,
                };
                (name.clone(), info)
            })
            .collect::<Vec<_>>();
        for (name, info) in globals {
            if let Some(existing_value) = self
                .global_properties
                .get(&name)
                .map(|existing| existing.value_info.clone())
            {
                let merged = self.merge_value_infos(existing_value, info);
                let existing = self
                    .global_properties
                    .get_mut(&name)
                    .expect("global property must still exist");
                existing.value_info = merged;
                existing.proven_present = true;
                continue;
            }
            self.global_properties.insert(
                name,
                GlobalPropertyInfo {
                    value_info: info,
                    proven_present: true,
                    configurable: false,
                    source: GlobalPropertySource::GlobalWrite,
                },
            );
        }
    }

    fn seed_live_script_global_var_properties(&mut self) {
        let missing_globals = self
            .var_bindings
            .iter()
            .filter(|(name, binding)| {
                binding.is_script_global && !self.global_properties.contains_key(*name)
            })
            .map(|(name, binding)| {
                (
                    name.clone(),
                    ValueInfo {
                        kind: binding.kind,
                        possible_kinds: binding.possible_kinds,
                        heap_shape: binding.heap_shape.clone(),
                        function_targets: binding.function_targets.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();
        for (name, value_info) in missing_globals {
            self.global_properties.insert(
                name,
                GlobalPropertyInfo {
                    value_info,
                    proven_present: true,
                    configurable: false,
                    source: GlobalPropertySource::GlobalWrite,
                },
            );
        }
    }

    fn known_script_global_values(&self) -> BTreeMap<String, ValueInfo> {
        self.var_bindings
            .iter()
            .filter(|(_, binding)| binding.is_script_global)
            .map(|(name, binding)| {
                let root_value = ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                };
                let value = match self.nested_script_global_value_infos.get(name) {
                    Some(nested_value) => self.merge_value_infos(root_value, nested_value.clone()),
                    None => root_value,
                };
                let value = match self.called_script_global_value_infos.get(name) {
                    Some(called_value) => self.merge_value_infos(value, called_value.clone()),
                    None => value,
                };
                (name.clone(), value)
            })
            .collect()
    }

    fn reset_script_global_var_flow_facts(&mut self) {
        for binding in self
            .var_bindings
            .values_mut()
            .filter(|binding| binding.is_script_global)
        {
            binding.kind = ValueKind::Undefined;
            binding.possible_kinds = KindSet::from_kind(ValueKind::Undefined);
            binding.heap_shape = None;
            binding.function_targets.replace_with_no_function();
        }
    }

    /// Degrade every observed-written intrinsic in a nested body lowerer to
    /// an unknown dynamic value, so shape reads, static calls and intrinsic
    /// folds in the emitted body observe the possible replacement. Only
    /// builtin-sourced entries degrade: declared script vars keep their
    /// seeded values, which the nested-global flow machinery (not this set)
    /// already keeps sound across bodies. Callers gate on the final phase;
    /// prepass and propagation bodies keep baseline facts. The degradation
    /// itself never feeds the write set back: these names are already members
    /// by construction.
    fn degrade_observed_written_globals(&self, lowerer: &mut ScriptLowerer<'a>) {
        for name in &self.observed_script_global_writes {
            let Some(property) = lowerer.global_properties.get_mut(name) else {
                continue;
            };
            if !matches!(
                property.source,
                GlobalPropertySource::Builtin | GlobalPropertySource::HostBuiltin
            ) {
                continue;
            }
            property.value_info = ValueInfo::new(ValueKind::Dynamic);
            property.source = GlobalPropertySource::GlobalWrite;
        }
    }

    fn merge_nested_script_global_value_infos(
        &mut self,
        nested_globals: &BTreeMap<String, ValueInfo>,
    ) {
        for (name, nested_info) in nested_globals {
            if !self
                .var_bindings
                .get(name)
                .is_some_and(|binding| binding.is_script_global)
            {
                continue;
            }
            let next = match self.nested_script_global_value_infos.remove(name) {
                Some(existing) => self.merge_value_infos(existing, nested_info.clone()),
                None => nested_info.clone(),
            };
            self.nested_script_global_value_infos
                .insert(name.clone(), next);
        }
    }

    fn merge_called_script_global_value_infos(
        &mut self,
        called_globals: &BTreeMap<String, ValueInfo>,
    ) {
        for (name, called_info) in called_globals {
            let next = match self.called_script_global_value_infos.remove(name) {
                Some(existing) => self.merge_value_infos(existing, called_info.clone()),
                None => called_info.clone(),
            };
            self.called_script_global_value_infos
                .insert(name.clone(), next);
        }
    }

    fn lookup_binding(&self, name: &str) -> Option<BindingInfo> {
        self.lookup_binding_with_location(name)
            .map(|(binding, _)| binding)
    }

    fn lookup_binding_with_location(
        &self,
        name: &str,
    ) -> Option<(BindingInfo, BindingLookupLocation)> {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find_map(|(scope_index, scope)| {
                scope
                    .get(name)
                    .cloned()
                    .map(|binding| (binding, BindingLookupLocation::Scope(scope_index)))
            })
            .or_else(|| {
                self.var_bindings.get(name).and_then(|binding| {
                    if binding.is_script_global && self.current_owner_id != SCRIPT_OWNER_ID {
                        return None;
                    }
                    Some((
                        binding.to_binding_info(name),
                        BindingLookupLocation::VariableEnvironment,
                    ))
                })
            })
    }

    fn declarative_binding_position(
        &self,
        binding: &BindingInfo,
        location: BindingLookupLocation,
    ) -> DeclarativeEnvironmentPosition {
        self.captured_binding_positions
            .get(&binding.storage_name)
            .copied()
            .map(DeclarativeEnvironmentPosition::captured)
            .unwrap_or_else(|| {
                let depth = match location {
                    BindingLookupLocation::Scope(scope_index) => {
                        CurrentScopeDepth::of_binding_scope(scope_index)
                    }
                    BindingLookupLocation::VariableEnvironment => CurrentScopeDepth::activation(),
                };
                DeclarativeEnvironmentPosition::current(depth)
            })
    }

    fn locate_identifier_reference(&self, name: &str) -> LocatedIdentifierReference {
        let Some((binding, location)) = self.lookup_binding_with_location(name) else {
            return LocatedIdentifierReference::Unresolvable;
        };
        let position = self.declarative_binding_position(&binding, location);
        LocatedIdentifierReference::Declarative {
            resolution: BindingResolution::of(Some(binding)),
            position,
        }
    }

    /// ResolveBinding (9.1.2.1) followed by the 9.1.1.1.6 step 2 / 9.1.1.1.5
    /// step 3 state test, for the seven sites that perform GetValue or PutValue
    /// on an Environment Record Reference.
    ///
    /// `lookup_binding` stays as it is for the other call sites, which ask
    /// metadata questions (`is_some`, `possible_kinds`, `storage_name` as a map
    /// key) and must not be forced to decide what TDZ means for them. This is
    /// the only accessor that answers the lifecycle question, and the only way
    /// to obtain a [`TdzViolation`].
    fn resolve_binding_reference(&self, name: &str) -> BindingResolution {
        BindingResolution::of(self.lookup_binding(name))
    }

    fn set_binding_kind(&mut self, name: &str, kind: ValueKind) -> Option<()> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.kind = kind;
                binding.possible_kinds = KindSet::from_kind(kind);
                binding.heap_shape = None;
                binding.function_targets = ValueInfo::new(kind).function_targets;
                self.static_to_string_regexp_object_bindings.remove(name);
                self.record_param_kind(name, kind);
                return Some(());
            }
        }
        if let Some(binding) = self.var_bindings.get_mut(name) {
            binding.kind = kind;
            binding.possible_kinds = KindSet::from_kind(kind);
            binding.heap_shape = None;
            binding.function_targets = ValueInfo::new(kind).function_targets;
            self.static_to_string_regexp_object_bindings.remove(name);
            return Some(());
        }
        None
    }

    fn record_param_kind(&mut self, name: &str, kind: ValueKind) {
        let Some(function_id) = &self.current_function_id else {
            return;
        };
        let Some(index) = self
            .current_param_names
            .iter()
            .position(|param| param == name)
        else {
            return;
        };
        if let Some(signature) = self.function_signatures.get_mut(function_id) {
            if let Some(param) = signature.params.get_mut(index) {
                if param.is_rest {
                    return;
                }
                let info = ValueInfo::new(kind);
                Self::merge_signature_param_observation(param, &info);
            }
        }
    }

    fn record_return_expression(&mut self, value: &TypedExpr) {
        let return_shape = if value.heap_shape.is_none() {
            FunctionReturnShape::Absent
        } else {
            let recreated_dependencies = match &value.expr {
                ExprIr::CallIndirect { callee, .. } => self
                    .resolve_single_function_target(callee)
                    .and_then(|function_id| {
                        let dependencies = self
                            .function_signatures
                            .get(&function_id)?
                            .return_shape
                            .recreated_dependencies()?
                            .clone();
                        match &callee.expr {
                            ExprIr::FunctionValue(emitted_function_id) => (self
                                .canonical_function_target(emitted_function_id)
                                == self.canonical_function_target(&function_id))
                            .then_some(dependencies),
                            ExprIr::GlobalPropertyRead { name }
                            | ExprIr::GlobalIdentifierRead { name } => {
                                let expected = self.canonical_function_target(&function_id);
                                self.global_function_target_matches(name, &expected)
                                    .then(|| {
                                        dependencies
                                            .with_global_function_target(name.clone(), expected)
                                    })
                                    .flatten()
                            }
                            _ => None,
                        }
                    }),
                _ => None,
            };
            if let Some(dependencies) = recreated_dependencies {
                FunctionReturnShape::recreated_per_call_with_dependencies(
                    value
                        .heap_shape
                        .clone()
                        .expect("per-call recreated return must have a heap shape"),
                    dependencies,
                )
            } else {
                FunctionReturnShape::flow_sensitive(value.heap_shape.clone())
            }
        };
        self.record_return_info(value.value_info(), return_shape);
    }

    fn record_return_info(&mut self, info: ValueInfo, return_shape: FunctionReturnShape) {
        self.current_return = Some(match self.current_return.take() {
            Some(existing) => {
                let merged_info = self.merge_value_infos(existing.info, info.clone());
                let merged_shape = FunctionReturnShape::merged(
                    &existing.shape,
                    &return_shape,
                    merged_info.heap_shape.clone(),
                );
                FunctionReturnObservation {
                    info: merged_info,
                    shape: merged_shape,
                }
            }
            None => FunctionReturnObservation {
                info: info.clone(),
                shape: return_shape.clone(),
            },
        });
        if let Some(function_id) = &self.current_function_id {
            let next_return = self.function_signatures.get(function_id).map(|signature| {
                self.merge_signature_return_value_observation(
                    signature,
                    info.clone(),
                    &return_shape,
                )
            });
            if let Some((next_info, next_shape)) = next_return {
                if let Some(signature) = self.function_signatures.get_mut(function_id) {
                    signature.return_kind = next_info.kind;
                    signature.return_possible_kinds = next_info.possible_kinds;
                    signature.return_shape = next_shape;
                    signature.return_targets = next_info.function_targets;
                }
            }
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
        self.direct_lexical_scopes.push(false);
    }

    fn push_direct_lexical_scope(&mut self) {
        self.push_scope();
        *self
            .direct_lexical_scopes
            .last_mut()
            .expect("scope markers must match scope stack") = true;
    }

    fn pop_scope(&mut self) {
        let leaving = self.scopes.len() - 1;
        self.finite_source_capture_keys
            .retain(|(index, _), _| *index != leaving);
        if self.finite_source_body_scope == Some(leaving) {
            self.finite_source_body_scope = None;
        }
        let scope = self
            .scopes
            .pop()
            .expect("scope stack must contain the scope being left");
        self.direct_lexical_scopes
            .pop()
            .expect("scope markers must match the scope stack");
        for (name, binding) in &scope {
            self.clear_static_binding_facts(name, Some(binding));
        }
    }

    // `mark_tdz_binding`, `clear_tdz_binding` and `is_tdz_binding` lived here,
    // over a `tdz_scopes: Vec<BTreeSet<String>>` that was zipped positionally
    // against `scopes`. The state is now a field of the binding record itself
    // (`BindingInfo::initialization`), so the two stacks cannot fall out of step
    // — there is one stack, and a re-introduction of the second is
    // `error[E0609] no field 'tdz_scopes' on type 'ScriptLowerer'`.

    fn unsupported_expr(&mut self, feature: &str) -> TypedExpr {
        self.unsupported_with_message(format!(
            "unsupported in scriptw wasm-aot first slice: {feature}"
        ));
        TypedExpr::undefined()
    }

    fn unsupported(&mut self, feature: &str) {
        self.unsupported_with_message(format!(
            "unsupported in scriptw wasm-aot first slice: {feature}"
        ));
    }

    fn unsupported_with_message(&mut self, message: String) {
        self.diagnostics.push(IrDiagnostic::unsupported(message));
    }
}
