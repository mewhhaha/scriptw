//! Source-independent builtin recipes with isolated per-compilation fact overlays.
//!
//! The caches contain compiler descriptions only: no parsed source, AST interner,
//! caller context, runtime environment, Realm identity or flow observations.

use super::*;
use std::sync::OnceLock;

static PRODUCT_GLOBALS: OnceLock<BTreeMap<String, GlobalPropertyInfo>> = OnceLock::new();
static TEST262_GLOBALS: OnceLock<BTreeMap<String, GlobalPropertyInfo>> = OnceLock::new();
static FUNCTION_SIGNATURES: OnceLock<BTreeMap<FunctionId, FunctionSignature>> = OnceLock::new();

fn global_property_baseline(
    policy: HostSurfacePolicy,
) -> &'static BTreeMap<String, GlobalPropertyInfo> {
    let cache = match policy {
        HostSurfacePolicy::Product => &PRODUCT_GLOBALS,
        HostSurfacePolicy::Test262 => &TEST262_GLOBALS,
    };
    cache.get_or_init(|| ScriptLowerer::build_builtin_global_properties(policy))
}

/// Whether the fresh-realm catalog seeds a fact for the global `name`.
pub(super) fn is_builtin_global(policy: HostSurfacePolicy, name: &str) -> bool {
    global_property_baseline(policy).contains_key(name)
}

/// The fresh-realm fact of the builtin global `name`.
pub(super) fn builtin_global(
    policy: HostSurfacePolicy,
    name: &str,
) -> Option<&'static GlobalPropertyInfo> {
    global_property_baseline(policy).get(name)
}

pub(super) fn global_properties(policy: HostSurfacePolicy) -> GlobalProperties {
    GlobalProperties::new(global_property_baseline(policy))
}

fn function_signature_baseline() -> &'static BTreeMap<FunctionId, FunctionSignature> {
    FUNCTION_SIGNATURES.get_or_init(ScriptLowerer::build_builtin_function_signatures)
}

pub(super) fn function_signatures() -> FunctionSignatures {
    FunctionSignatures::new(function_signature_baseline())
}

impl ScriptLowerer<'_> {
    fn build_builtin_global_properties(
        host_surface_policy: HostSurfacePolicy,
    ) -> BTreeMap<String, GlobalPropertyInfo> {
        let mut properties = BTreeMap::new();
        properties.insert(
            GLOBAL_THIS_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: ValueInfo::new(ValueKind::Object),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        for builtin in host_surface_policy.global_builtins() {
            properties.insert(
                builtin
                    .global_name()
                    .expect("global host catalog row must have a name")
                    .to_string(),
                GlobalPropertyInfo {
                    value_info: Self::host_function_value_info(builtin),
                    proven_present: true,
                    configurable: true,
                    source: GlobalPropertySource::HostBuiltin,
                },
            );
        }
        properties.insert(
            REFLECT_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: Self::reflect_object_value_info(),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            MATH_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: Self::math_object_value_info(),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            JSON_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: Self::json_object_value_info(),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            ATOMICS_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: Self::atomics_object_value_info(),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            TEMPORAL_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: Self::temporal_object_value_info(),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            INTL_NAME.to_string(),
            GlobalPropertyInfo {
                value_info: Self::intl_object_value_info(),
                proven_present: true,
                configurable: true,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            "Infinity".to_string(),
            GlobalPropertyInfo {
                value_info: ValueInfo::new(ValueKind::Number),
                proven_present: true,
                configurable: false,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            "NaN".to_string(),
            GlobalPropertyInfo {
                value_info: ValueInfo::new(ValueKind::Number),
                proven_present: true,
                configurable: false,
                source: GlobalPropertySource::Builtin,
            },
        );
        properties.insert(
            "undefined".to_string(),
            GlobalPropertyInfo {
                value_info: ValueInfo::undefined(),
                proven_present: true,
                configurable: false,
                source: GlobalPropertySource::Builtin,
            },
        );
        for builtin in StandardBuiltinId::all_globals() {
            if let Some(name) = builtin.global_name() {
                properties.insert(
                    name.to_string(),
                    GlobalPropertyInfo {
                        value_info: Self::standard_builtin_value_info(*builtin),
                        proven_present: true,
                        configurable: true,
                        source: GlobalPropertySource::Builtin,
                    },
                );
            }
        }
        properties
    }

    fn build_builtin_function_signatures() -> BTreeMap<FunctionId, FunctionSignature> {
        let mut signatures = BTreeMap::new();
        signatures.insert(
            HostBuiltinId::Print.function_id(),
            FunctionSignature {
                id: HostBuiltinId::Print.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::Print.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Undefined,
                return_possible_kinds: KindSet::from_kind(ValueKind::Undefined),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::Gc.function_id(),
            FunctionSignature {
                id: HostBuiltinId::Gc.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::Gc.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Undefined,
                return_possible_kinds: KindSet::from_kind(ValueKind::Undefined),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::AssertThrows.function_id(),
            FunctionSignature {
                id: HostBuiltinId::AssertThrows.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::AssertThrows.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Undefined,
                return_possible_kinds: KindSet::from_kind(ValueKind::Undefined),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::IsConstructor.function_id(),
            FunctionSignature {
                id: HostBuiltinId::IsConstructor.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::IsConstructor.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Boolean,
                return_possible_kinds: KindSet::from_kind(ValueKind::Boolean),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::DetachArrayBuffer.function_id(),
            FunctionSignature {
                id: HostBuiltinId::DetachArrayBuffer.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::DetachArrayBuffer.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Undefined,
                return_possible_kinds: KindSet::from_kind(ValueKind::Undefined),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        for (builtin, return_kind, return_possible_kinds) in [
            (
                HostBuiltinId::AgentStart,
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
            ),
            (
                HostBuiltinId::AgentBroadcast,
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
            ),
            (
                HostBuiltinId::AgentReceiveBroadcast,
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
            ),
            (
                HostBuiltinId::AgentReport,
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
            ),
            (
                HostBuiltinId::AgentGetReport,
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::String).union(KindSet::from_kind(ValueKind::Null)),
            ),
            (
                HostBuiltinId::AgentSleep,
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
            ),
            (
                HostBuiltinId::AgentMonotonicNow,
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
            ),
            (
                HostBuiltinId::AgentLeaving,
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
            ),
        ] {
            signatures.insert(
                builtin.function_id(),
                FunctionSignature {
                    id: builtin.function_id(),
                    to_string_representation: CallableToStringRepresentation::NativeNamed(
                        builtin.as_str().to_string(),
                    ),
                    protocol: FunctionProtocolIr::OrdinaryCallOnly,
                    callable: true,
                    class_heritage_kind: ClassHeritageKind::None,
                    params: Vec::new(),
                    return_kind,
                    return_possible_kinds,
                    return_shape: FunctionReturnShape::Absent,
                    return_targets: FunctionTargetKnowledge::none(),
                    constructor_instance: ValueInfo::undefined(),
                    this_info: ValueInfo::undefined(),
                    this_observed: false,
                    source_call_flow_effects: SourceCallFlowEffects::unobserved(),
                },
            );
        }
        signatures.insert(
            HostBuiltinId::ParseInt.function_id(),
            FunctionSignature {
                id: HostBuiltinId::ParseInt.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::ParseInt.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Number,
                return_possible_kinds: KindSet::from_kind(ValueKind::Number),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::ParseFloat.function_id(),
            FunctionSignature {
                id: HostBuiltinId::ParseFloat.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::ParseFloat.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Number,
                return_possible_kinds: KindSet::from_kind(ValueKind::Number),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::CreateRealm.function_id(),
            FunctionSignature {
                id: HostBuiltinId::CreateRealm.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::CreateRealm.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Object,
                return_possible_kinds: KindSet::from_kind(ValueKind::Object),
                return_shape: FunctionReturnShape::recreated_per_call(
                    Self::synthetic_realm_record_shape(),
                ),
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        for intrinsic in DynamicSourceIntrinsic::ALL
            .iter()
            .copied()
            .filter(|intrinsic| {
                !matches!(
                    intrinsic,
                    DynamicSourceIntrinsic::Function(DynamicFunctionKind::Ordinary)
                )
            })
        {
            signatures.insert(
                intrinsic.function_id().to_string(),
                Self::dynamic_source_intrinsic_signature(intrinsic),
            );
        }
        signatures.insert(
            HostBuiltinId::GetAbstractModuleSource.function_id(),
            FunctionSignature {
                id: HostBuiltinId::GetAbstractModuleSource.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::GetAbstractModuleSource.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Function,
                return_possible_kinds: KindSet::from_kind(ValueKind::Function),
                // The defining Realm owns one mutable intrinsic identity, not
                // a fresh constructor shape recreated by each host call.
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::exact(
                    StandardBuiltinId::AbstractModuleSourceConstructor.function_id(),
                ),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::CreateHTMLDDA.function_id(),
            FunctionSignature {
                id: HostBuiltinId::CreateHTMLDDA.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::CreateHTMLDDA.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Function,
                return_possible_kinds: KindSet::from_kind(ValueKind::Function),
                return_shape: FunctionReturnShape::flow_sensitive(Some(Self::function_heap_shape(
                    false,
                ))),
                return_targets: FunctionTargetKnowledge::unknown(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        signatures.insert(
            HostBuiltinId::HTMLDDA.function_id(),
            FunctionSignature {
                id: HostBuiltinId::HTMLDDA.function_id(),
                to_string_representation: CallableToStringRepresentation::NativeNamed(
                    HostBuiltinId::HTMLDDA.as_str().to_string(),
                ),
                protocol: FunctionProtocolIr::OrdinaryCallOnly,
                callable: true,
                class_heritage_kind: ClassHeritageKind::None,
                params: Vec::new(),
                return_kind: ValueKind::Null,
                return_possible_kinds: KindSet::from_kind(ValueKind::Null),
                return_shape: FunctionReturnShape::Absent,
                return_targets: FunctionTargetKnowledge::none(),
                constructor_instance: ValueInfo::undefined(),
                this_info: ValueInfo::undefined(),
                this_observed: false,
                source_call_flow_effects: SourceCallFlowEffects::unobserved(),
            },
        );
        for builtin in StandardBuiltinId::all_functions() {
            signatures.insert(
                builtin.function_id(),
                Self::standard_builtin_signature(*builtin),
            );
        }
        signatures
    }
}

/// The same canonical initializers serve ordinary Script binding plans and the
/// declaration-free bootstrap surrounding independently instantiated Scripts.
pub(super) fn canonical_standard_global_bindings() -> impl Iterator<Item = ScriptGlobalBindingIr> {
    [
        (GLOBAL_THIS_NAME, GlobalPropertyInitializerIr::Intrinsic),
        ("Infinity", GlobalPropertyInitializerIr::Infinity),
        ("NaN", GlobalPropertyInitializerIr::NaN),
        ("undefined", GlobalPropertyInitializerIr::Undefined),
        (REFLECT_NAME, GlobalPropertyInitializerIr::ReflectObject),
        (MATH_NAME, GlobalPropertyInitializerIr::MathObject),
        (JSON_NAME, GlobalPropertyInitializerIr::JsonObject),
        (ATOMICS_NAME, GlobalPropertyInitializerIr::AtomicsObject),
        (TEMPORAL_NAME, GlobalPropertyInitializerIr::TemporalObject),
        (INTL_NAME, GlobalPropertyInitializerIr::IntlObject),
    ]
    .into_iter()
    .chain(
        StandardBuiltinId::all_globals()
            .into_iter()
            .filter_map(|builtin| {
                builtin
                    .global_name()
                    .map(|name| (name, GlobalPropertyInitializerIr::BuiltinFunction(*builtin)))
            }),
    )
    .map(|(name, initializer)| ScriptGlobalBindingIr {
        name: name.to_owned(),
        initializer,
        declarations: GlobalDeclarationSetIr::None,
    })
}

/// Fill only absent canonical names. Source-function replacements and var
/// declaration metadata in an ordinary Script retain their exact ownership.
pub(super) fn retain_canonical_standard_globals(plan: &mut GlobalBindingPlan) {
    for binding in canonical_standard_global_bindings() {
        if plan.get(&binding.name).is_none() {
            plan.insert_initial(binding);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod flow_tests;
