mod async_for_of_iterator;
pub use async_for_of_iterator::{
    AsyncFunctionForAwaitOfIteratorIr, AsyncFunctionForOfIteratorExecutionIr,
    AsyncFunctionForOfIteratorPlanIr, AsyncFunctionForOfSynchronousIteratorIr,
};

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, ToPrimitive, Zero};
use scriptw_front::ParseGoal;

pub use crate::generator_for_of_iterator::GeneratorForOfIteratorValueStorageIr;
use crate::{
    ArithmeticBinaryOp, ArrayPatternProtocol, ArraySpreadProtocol, AsyncFunctionForOfBodyError,
    AsyncFunctionForOfBodyIr, AsyncFunctionIfPlanIr, AsyncFunctionLabelledPlanIr,
    AsyncFunctionSwitchIr, AsyncFunctionWhileConditionIr, BindingMode, BitwiseBinaryOp,
    CallableToStringRepresentation, CompletionRecordIr, EcmaLanguageType, EqualityBinaryOp,
    FunctionProtocolIr, GeneratorDelegationProtocol, GeneratorForOfIteratorPlanIr, HostBuiltinId,
    IrDiagnostic, IrDiagnosticKind, IteratorProtocolWitness, IteratorRecordIr, LogicalBinaryOp,
    LoweringStage, NativeErrorKind, NumericUpdateOp, NumericUpdateValueKind,
    PreparedDynamicFunction, RegExpProgram, RelationalBinaryOp, SpecOperationIr,
    SpreadArgumentProtocol, StandardBuiltinId, ToPrimitiveHint, UnaryBitwiseOp, UpdateReturnMode,
};
use crate::{
    AsyncGeneratorIfIr, AsyncGeneratorLoopIr, EmptyStatementCompletionIr, GeneratorLoopKindIr,
    OrdinaryGeneratorArrayDestructuringIr, OrdinaryGeneratorIfIr, OrdinaryGeneratorLoopIr,
    OrdinaryGeneratorSwitchIr,
};

use crate::{
    ImportPhaseIr, ModuleEntryEvaluationIr, ModuleGraphIr, ModuleUnitId, PreparedScript,
    PreparedScriptOutcome, PreparedScriptUnit, RuntimeGlobalDeclarationPlan,
};

/// Reference Records (6.2.5) and their `[[Strict]]`. See
/// `docs/rust-rewrite/contracts/reference-records.md`.
///
/// Declared here rather than in `lib.rs` because `ExprIr`'s reference-write
/// variants carry `Strictness` in their fields, so the type has to be in scope
/// in this file; the `#[path]` keeps the module a sibling file on disk.
#[path = "reference.rs"]
pub mod reference;

pub use reference::{
    carried_put_value_failure, CapturedOrdinaryPropertyWriteIr, IdentifierWriteDisposition,
    IdentifierWriteErrorIr, IdentifierWriteReferenceIr, OrdinaryPropertyAssignmentIr,
    OrdinaryPropertyEagerCompoundAssignmentIr, OrdinaryPropertyGetCaptureIr,
    OrdinaryPropertyLogicalAssignmentIr, OrdinaryPropertyNumericUpdateIr, PropertyHookTargets,
    PutValueFailure, Strictness, SuperPropertyCaptureMode, SuperPropertyMutationIr,
    SuperPropertyMutationOperationIr, SuperPropertyReferenceCaptureIr,
    SuspendedPropertyReferenceIr, SuspendedPropertyReferenceUse,
};

/// Numeric conversion codomains (7.1.5, 7.1.6, 7.1.7, 7.1.9, 7.1.20, 7.1.22).
/// See `docs/rust-rewrite/contracts/numeric-conversion-codomains.md`.
///
/// Declared here rather than in `lib.rs` for the same reason as `reference`:
/// `lib.rs` is a single-lane hub owned by another area this round, and the
/// `#[path]` keeps the module a sibling file on disk.
#[path = "numeric_conversions.rs"]
pub mod numeric_conversions;

/// Numeric conversion and residue reference APIs shared with the backend.
pub use numeric_conversions::{
    reference_to_index, reference_to_int32, reference_to_length, reference_to_uint16,
    reference_to_uint32, residue_pow2_i64, ExtendedInteger, FiniteInteger, IntegerOrInfinity,
    ResidueCarrier, ResidueWidth, ToIndexOutcome, Uint16, Uint32, MAX_SAFE_INTEGER_U64,
};

pub type FunctionId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticRegExpCompilation {
    Program(RegExpProgram),
    InvalidSyntax { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrivateNameId {
    class_scope: u32,
    name_ordinal: u32,
}

impl PrivateNameId {
    pub(crate) const fn new(class_scope: u32, name_ordinal: u32) -> Self {
        Self {
            class_scope,
            name_ordinal,
        }
    }

    pub const fn name_ordinal(self) -> u32 {
        self.name_ordinal
    }

    pub const fn class_scope(self) -> u32 {
        self.class_scope
    }
}

impl std::fmt::Display for PrivateNameId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.class_scope, self.name_ordinal)
    }
}

pub fn private_data_key(private_name_id: PrivateNameId) -> String {
    format!("$class.private.data.{private_name_id}")
}

pub fn private_brand_key(private_name_id: PrivateNameId) -> String {
    format!("$class.private.brand.{private_name_id}")
}

pub fn private_getter_key(private_name_id: PrivateNameId) -> String {
    format!("$class.private.getter.{private_name_id}")
}

pub fn private_setter_key(private_name_id: PrivateNameId) -> String {
    format!("$class.private.setter.{private_name_id}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    Symbol,
    Object,
    Array,
    Function,
    Arguments,
    BigInt,
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigIntLiteralIr {
    pub decimal: String,
    pub low_bits: u64,
    pub requires_arbitrary_precision_storage: bool,
}

impl BigIntLiteralIr {
    pub fn from_bigint(value: BigInt) -> Self {
        let decimal = value.to_string();
        let low_bits = Self::low_bits(&value);
        let requires_arbitrary_precision_storage = value.to_i64().is_none();
        Self {
            decimal,
            low_bits,
            requires_arbitrary_precision_storage,
        }
    }

    pub fn from_i64(value: i64) -> Self {
        Self::from_bigint(BigInt::from(value))
    }

    pub fn from_u64_payload(bits: u64) -> Self {
        Self {
            decimal: bits.to_string(),
            low_bits: bits,
            requires_arbitrary_precision_storage: bits > i64::MAX as u64,
        }
    }

    pub fn to_bigint(&self) -> BigInt {
        self.decimal
            .parse::<BigInt>()
            .expect("BigIntLiteralIr decimal should parse")
    }

    pub fn wrapping_payload(&self) -> u64 {
        self.low_bits
    }

    pub fn signed_magnitude_u64(&self) -> Option<(i64, u64)> {
        let (sign, limbs) = self.signed_magnitude_limbs();
        if limbs.len() > 1 {
            return None;
        }
        Some((sign, limbs.first().copied().unwrap_or(0)))
    }

    pub fn signed_magnitude_limbs(&self) -> (i64, Vec<u64>) {
        let value = self.to_bigint();
        let (sign, magnitude_bytes) = value.to_bytes_le();
        let sign = match sign {
            Sign::Minus => -1,
            Sign::NoSign => 0,
            Sign::Plus => 1,
        };
        let limbs = magnitude_bytes
            .chunks(8)
            .map(|chunk| {
                let mut bytes = [0; 8];
                bytes[..chunk.len()].copy_from_slice(chunk);
                u64::from_le_bytes(bytes)
            })
            .collect();
        (sign, limbs)
    }

    pub fn negated(&self) -> Self {
        Self::from_bigint(-self.to_bigint())
    }

    pub fn complemented(&self) -> Self {
        Self::from_bigint(!self.to_bigint())
    }

    pub fn added(&self, rhs: &Self) -> Self {
        Self::from_bigint(self.to_bigint() + rhs.to_bigint())
    }

    pub fn pow_u32(&self, exponent: u32) -> Self {
        Self::from_bigint(self.to_bigint().pow(exponent))
    }

    fn low_bits(value: &BigInt) -> u64 {
        let (_, magnitude) = value.to_bytes_le();
        let magnitude = BigUint::from_bytes_le(&magnitude);
        let low_bits_mask = (BigUint::one() << 64_u32) - BigUint::one();
        let low_bits = (magnitude & low_bits_mask).to_u64().unwrap_or(0);
        if value.sign() == Sign::Minus && !value.is_zero() {
            low_bits.wrapping_neg()
        } else {
            low_bits
        }
    }
}

impl ValueKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Symbol => "symbol",
            Self::Object => "object",
            Self::Array => "array",
            Self::Function => "function",
            Self::Arguments => "arguments",
            Self::BigInt => "bigint",
            Self::Dynamic => "dynamic",
        }
    }

    pub const fn known_ecmascript_type(self) -> Option<EcmaLanguageType> {
        match self {
            Self::Undefined => Some(EcmaLanguageType::Undefined),
            Self::Null => Some(EcmaLanguageType::Null),
            Self::Boolean => Some(EcmaLanguageType::Boolean),
            Self::Number => Some(EcmaLanguageType::Number),
            Self::String => Some(EcmaLanguageType::String),
            Self::Symbol => Some(EcmaLanguageType::Symbol),
            Self::BigInt => Some(EcmaLanguageType::BigInt),
            Self::Object | Self::Array | Self::Function | Self::Arguments => {
                Some(EcmaLanguageType::Object)
            }
            Self::Dynamic => None,
        }
    }

    pub const fn tag(self) -> i32 {
        match self {
            Self::Undefined => 0,
            Self::Null => 1,
            Self::Boolean => 2,
            Self::Number => 3,
            Self::String => 4,
            Self::Symbol => 5,
            Self::Object => 6,
            Self::Array => 7,
            Self::Function => 8,
            Self::Arguments => 9,
            Self::BigInt => 10,
            Self::Dynamic => 11,
        }
    }

    pub const fn from_tag(tag: i32) -> Option<Self> {
        match tag {
            0 => Some(Self::Undefined),
            1 => Some(Self::Null),
            2 => Some(Self::Boolean),
            3 => Some(Self::Number),
            4 => Some(Self::String),
            5 => Some(Self::Symbol),
            6 => Some(Self::Object),
            7 => Some(Self::Array),
            8 => Some(Self::Function),
            9 => Some(Self::Arguments),
            10 => Some(Self::BigInt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindSet(pub(crate) u16);

impl KindSet {
    pub const EMPTY: Self = Self(0);

    const UNDEFINED_BIT: u16 = 1 << 0;
    const NULL_BIT: u16 = 1 << 1;
    const BOOLEAN_BIT: u16 = 1 << 2;
    const NUMBER_BIT: u16 = 1 << 3;
    const STRING_BIT: u16 = 1 << 4;
    const SYMBOL_BIT: u16 = 1 << 5;
    const OBJECT_BIT: u16 = 1 << 6;
    const ARRAY_BIT: u16 = 1 << 7;
    const FUNCTION_BIT: u16 = 1 << 8;
    const ARGUMENTS_BIT: u16 = 1 << 9;
    const BIGINT_BIT: u16 = 1 << 10;

    pub const PRIMITIVE_ONLY: Self = Self(
        Self::UNDEFINED_BIT
            | Self::NULL_BIT
            | Self::BOOLEAN_BIT
            | Self::NUMBER_BIT
            | Self::STRING_BIT
            | Self::SYMBOL_BIT
            | Self::BIGINT_BIT,
    );

    pub const HEAP_COERCIBLE_ONLY: Self =
        Self(Self::OBJECT_BIT | Self::ARRAY_BIT | Self::ARGUMENTS_BIT);

    pub const PRIMITIVE_OR_HEAP_COERCIBLE: Self =
        Self(Self::PRIMITIVE_ONLY.0 | Self::HEAP_COERCIBLE_ONLY.0);
    pub const PROPERTY_KEY_COERCIBLE: Self =
        Self(Self::PRIMITIVE_OR_HEAP_COERCIBLE.0 | Self::FUNCTION_BIT);

    pub const NULLISH: Self = Self(Self::UNDEFINED_BIT | Self::NULL_BIT);

    /// The kinds ToNumber maps with no ToPrimitive call, no string parse and no
    /// TypeError. A `String` needs the parse; a `Symbol` or `BigInt` must
    /// throw; anything on the heap needs ToPrimitive first. Skipping the full
    /// conversion for a wider set - `PRIMITIVE_ONLY` minus `String`, say -
    /// silently turns `1n ^ 1` and `Symbol() * 1` into numbers instead of
    /// TypeErrors.
    pub const DIRECT_TO_NUMBER: Self =
        Self(Self::UNDEFINED_BIT | Self::NULL_BIT | Self::BOOLEAN_BIT | Self::NUMBER_BIT);

    pub const fn from_kind(kind: ValueKind) -> Self {
        match kind {
            ValueKind::Undefined => Self(Self::UNDEFINED_BIT),
            ValueKind::Null => Self(Self::NULL_BIT),
            ValueKind::Boolean => Self(Self::BOOLEAN_BIT),
            ValueKind::Number => Self(Self::NUMBER_BIT),
            ValueKind::String => Self(Self::STRING_BIT),
            ValueKind::Symbol => Self(Self::SYMBOL_BIT),
            ValueKind::Object => Self(Self::OBJECT_BIT),
            ValueKind::Array => Self(Self::ARRAY_BIT),
            ValueKind::Function => Self(Self::FUNCTION_BIT),
            ValueKind::Arguments => Self(Self::ARGUMENTS_BIT),
            ValueKind::BigInt => Self(Self::BIGINT_BIT),
            ValueKind::Dynamic => Self::all_runtime_tags(),
        }
    }

    pub const fn all_runtime_tags() -> Self {
        Self(
            Self::UNDEFINED_BIT
                | Self::NULL_BIT
                | Self::BOOLEAN_BIT
                | Self::NUMBER_BIT
                | Self::STRING_BIT
                | Self::SYMBOL_BIT
                | Self::OBJECT_BIT
                | Self::ARRAY_BIT
                | Self::FUNCTION_BIT
                | Self::ARGUMENTS_BIT
                | Self::BIGINT_BIT,
        )
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn contains(self, kind: ValueKind) -> bool {
        self.0 & Self::from_kind(kind).0 != 0
    }

    pub const fn is_singleton(self) -> bool {
        self.0 != 0 && (self.0 & (self.0 - 1)) == 0
    }

    pub const fn is_subset_of(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn without(self, kind: ValueKind) -> Self {
        Self(self.0 & !Self::from_kind(kind).0)
    }

    pub const fn as_value_kind(self) -> ValueKind {
        if self.is_singleton() {
            if self.contains(ValueKind::Undefined) {
                ValueKind::Undefined
            } else if self.contains(ValueKind::Null) {
                ValueKind::Null
            } else if self.contains(ValueKind::Boolean) {
                ValueKind::Boolean
            } else if self.contains(ValueKind::Number) {
                ValueKind::Number
            } else if self.contains(ValueKind::String) {
                ValueKind::String
            } else if self.contains(ValueKind::Symbol) {
                ValueKind::Symbol
            } else if self.contains(ValueKind::Object) {
                ValueKind::Object
            } else if self.contains(ValueKind::Array) {
                ValueKind::Array
            } else if self.contains(ValueKind::Function) {
                ValueKind::Function
            } else if self.contains(ValueKind::Arguments) {
                ValueKind::Arguments
            } else if self.contains(ValueKind::BigInt) {
                ValueKind::BigInt
            } else {
                ValueKind::Dynamic
            }
        } else {
            ValueKind::Dynamic
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueInfo {
    pub kind: ValueKind,
    pub possible_kinds: KindSet,
    pub heap_shape: Option<Box<HeapShape>>,
    pub function_targets: FunctionTargetKnowledge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionTargetKnowledge {
    Exact(BTreeSet<FunctionId>),
    Open(BTreeSet<FunctionId>),
}

impl FunctionTargetKnowledge {
    pub const fn none() -> Self {
        Self::Exact(BTreeSet::new())
    }

    pub const fn unknown() -> Self {
        Self::Open(BTreeSet::new())
    }

    pub fn exact(function_id: FunctionId) -> Self {
        Self::Exact(BTreeSet::from([function_id]))
    }

    pub fn exact_many(function_ids: BTreeSet<FunctionId>) -> Self {
        Self::Exact(function_ids)
    }

    pub fn exact_targets(&self) -> Option<&BTreeSet<FunctionId>> {
        match self {
            Self::Exact(targets) => Some(targets),
            Self::Open(_) => None,
        }
    }

    pub fn exact_single_target(&self) -> Option<&FunctionId> {
        let targets = self.exact_targets()?;
        if targets.len() != 1 {
            return None;
        }
        targets.iter().next()
    }

    pub fn known_targets(&self) -> &BTreeSet<FunctionId> {
        match self {
            Self::Exact(targets) | Self::Open(targets) => targets,
        }
    }

    pub fn join(self, other: Self) -> Self {
        let exact = matches!(self, Self::Exact(_)) && matches!(other, Self::Exact(_));
        let mut targets = match self {
            Self::Exact(targets) | Self::Open(targets) => targets,
        };
        targets.extend(match other {
            Self::Exact(targets) | Self::Open(targets) => targets,
        });
        if exact {
            Self::Exact(targets)
        } else {
            Self::Open(targets)
        }
    }

    pub fn widen_for_possible_replacement(&mut self) {
        *self = match std::mem::replace(self, Self::unknown()) {
            Self::Exact(targets) | Self::Open(targets) => Self::Open(targets),
        };
    }

    pub fn replace_with_unknown(&mut self) {
        *self = Self::unknown();
    }

    pub fn replace_with_no_function(&mut self) {
        *self = Self::none();
    }

    pub fn map_known_targets(self, map: impl FnMut(FunctionId) -> FunctionId) -> Self {
        match self {
            Self::Exact(targets) => Self::Exact(targets.into_iter().map(map).collect()),
            Self::Open(targets) => Self::Open(targets.into_iter().map(map).collect()),
        }
    }
}

impl ValueInfo {
    pub const fn undefined() -> Self {
        Self {
            kind: ValueKind::Undefined,
            possible_kinds: KindSet::from_kind(ValueKind::Undefined),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub const fn new(kind: ValueKind) -> Self {
        Self {
            kind,
            possible_kinds: KindSet::from_kind(kind),
            heap_shape: None,
            function_targets: match kind {
                ValueKind::Dynamic | ValueKind::Function | ValueKind::Object => {
                    FunctionTargetKnowledge::unknown()
                }
                _ => FunctionTargetKnowledge::none(),
            },
        }
    }

    pub fn widen_for_possible_replacement(&mut self) {
        self.kind = ValueKind::Dynamic;
        self.possible_kinds = KindSet::all_runtime_tags();
        self.heap_shape = None;
        self.function_targets.widen_for_possible_replacement();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeapShape {
    Object(ObjectShape),
    Array(ArrayShape),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxedPrimitiveKind {
    Number,
    String,
    Boolean,
    Symbol,
    BigInt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectAccessorShape {
    pub function_id: FunctionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectShapeProperty {
    Data(ValueInfo),
    Accessor {
        getter: Option<ObjectAccessorShape>,
        setter: Option<ObjectAccessorShape>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectShape {
    pub provenance: HeapShapeProvenance,
    pub prototype: Option<Box<HeapShape>>,
    pub properties: BTreeMap<String, ObjectShapeProperty>,
    pub private_brands: BTreeSet<PrivateNameId>,
    pub boxed_primitive: Option<Box<ValueInfo>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArrayShape {
    pub provenance: HeapShapeProvenance,
    pub prototype: Option<Box<HeapShape>>,
    pub properties: BTreeMap<String, ObjectShapeProperty>,
    pub elements: Vec<ValueInfo>,
}

/// A catalogue describes a fresh Realm, not the mutable prototype at the
/// current program point. Property lowering must validate catalogue facts
/// against the live intrinsic before treating its descriptors as authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeapShapeProvenance {
    #[default]
    Program,
    IntrinsicPrototype(StandardBuiltinId),
    /// The intrinsic has no tracked constructor path in the current Realm.
    UntrackedIntrinsicPrototype,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionFlavor {
    Ordinary,
    Arrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionExecutionKind {
    Ordinary,
    Generator,
    Async,
    AsyncGenerator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumableSuspensionKindIr {
    Await,
    Yield,
    ForAwaitNext,
    ForAwaitClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumableResumeEnvironmentIr {
    InvocationOuter,
    SavedLexicalChain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumableSuspensionPointIr {
    pub kind: ResumableSuspensionKindIr,
    pub suspend_state: u32,
    pub resume_state: u32,
    pub resume_environment: ResumableResumeEnvironmentIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumablePlanIr {
    pub entry_state: u32,
    pub state_count: u32,
    pub suspension_points: Vec<ResumableSuspensionPointIr>,
    pub(crate) resume_environment_plan: AsyncGeneratorResumeEnvironmentPlanIr,
}

/// Native scope restoration consumes this checked source certificate. Its
/// private state list cannot be assembled from backend guesses or raw flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncGeneratorResumeEnvironmentPlanIr {
    invocation_resume_states: Vec<u32>,
    enclosing_scope_resume_states: Vec<u32>,
}

impl AsyncGeneratorResumeEnvironmentPlanIr {
    pub fn invocation_resume_states(&self) -> &[u32] {
        &self.invocation_resume_states
    }

    pub fn enclosing_scope_resume_states(&self) -> &[u32] {
        &self.enclosing_scope_resume_states
    }
}

impl ResumablePlanIr {
    pub(crate) fn from_checked_source(
        source: crate::async_generator_source::AsyncGeneratorFunctionSource,
    ) -> Self {
        let (state_count, suspension_points, enclosing_scope_resume_states) = source.into_parts();
        let invocation_resume_states = suspension_points
            .iter()
            .filter(|point| {
                point.resume_environment == ResumableResumeEnvironmentIr::InvocationOuter
            })
            .map(|point| point.resume_state)
            .collect();
        Self {
            entry_state: 0,
            state_count,
            suspension_points,
            resume_environment_plan: AsyncGeneratorResumeEnvironmentPlanIr {
                invocation_resume_states,
                enclosing_scope_resume_states,
            },
        }
    }

    pub fn empty() -> Self {
        Self {
            entry_state: 0,
            state_count: 1,
            suspension_points: Vec::new(),
            resume_environment_plan: AsyncGeneratorResumeEnvironmentPlanIr {
                invocation_resume_states: Vec::new(),
                enclosing_scope_resume_states: Vec::new(),
            },
        }
    }

    pub fn resume_environment_plan(&self) -> &AsyncGeneratorResumeEnvironmentPlanIr {
        &self.resume_environment_plan
    }

    pub(crate) fn matches_resume_environment_plan(&self) -> bool {
        let source_scopes = &self.resume_environment_plan.enclosing_scope_resume_states;
        source_scopes
            .iter()
            .all(|state| *state > self.entry_state && *state < self.state_count)
            && source_scopes.windows(2).all(|pair| pair[0] < pair[1])
            && self
                .suspension_points
                .iter()
                .filter(|point| {
                    point.resume_environment == ResumableResumeEnvironmentIr::InvocationOuter
                })
                .map(|point| point.resume_state)
                .eq(self
                    .resume_environment_plan
                    .invocation_resume_states
                    .iter()
                    .copied())
    }

    pub(crate) fn insert_legacy_branch_exit(
        &mut self,
        next_index: usize,
        branch_end: u32,
    ) -> Result<(), crate::generator_loop_control::GeneratorLoopControlError> {
        use crate::generator_loop_control::{checked_next_state, GeneratorLoopControlError};
        if next_index > self.suspension_points.len() || !self.matches_resume_environment_plan() {
            return Err(GeneratorLoopControlError::InvalidPhases);
        }
        if self.suspension_points[..next_index]
            .iter()
            .any(|point| point.resume_state > branch_end)
            || self.suspension_points[next_index..]
                .iter()
                .any(|point| point.suspend_state < branch_end)
        {
            return Err(GeneratorLoopControlError::InvalidPhases);
        }
        let state_count = checked_next_state(self.state_count)?;
        let consumed_invocations = self.suspension_points[..next_index]
            .iter()
            .filter(|point| {
                point.resume_environment == ResumableResumeEnvironmentIr::InvocationOuter
            })
            .count();
        // Preflight the complete relocation so a rejected insertion preserves
        // both the actual source tape and its private environment certificate.
        for point in &self.suspension_points[next_index..] {
            checked_next_state(point.suspend_state)?;
            checked_next_state(point.resume_state)?;
        }
        for state in &self.resume_environment_plan.invocation_resume_states[consumed_invocations..]
        {
            checked_next_state(*state)?;
        }
        for state in self
            .resume_environment_plan
            .enclosing_scope_resume_states
            .iter()
            .filter(|state| **state > branch_end)
        {
            checked_next_state(*state)?;
        }
        for point in &mut self.suspension_points[next_index..] {
            point.suspend_state += 1;
            point.resume_state += 1;
        }
        for state in
            &mut self.resume_environment_plan.invocation_resume_states[consumed_invocations..]
        {
            *state += 1;
        }
        for state in self
            .resume_environment_plan
            .enclosing_scope_resume_states
            .iter_mut()
            .filter(|state| **state > branch_end)
        {
            *state += 1;
        }
        self.state_count = state_count;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorSuspensionPointIr {
    pub suspend_state: u32,
    pub resume_state: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratorResumeModeIr {
    Ignore,
    Return,
    AssignIdentifier(String),
    AssignGlobal {
        name: String,
        strictness: Strictness,
    },
    AssignProperty(SuspendedPropertyReferenceIr),
}

/// Which of 14.4.14's two yield productions a generator suspension represents.
///
/// Delegation carries the one-inhabitant witness for the iterator protocol it
/// acquires. There is no spelling for `yield*` without that witness and no
/// protocol payload on a plain `yield`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YieldForm {
    /// `yield expr`.
    Plain,
    /// `yield* expr` and the four iterator obligations its delegation emits.
    Delegate(GeneratorDelegationProtocol),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncResumeModeIr {
    Ignore,
    Return,
    AssignIdentifier(String),
    AssignGlobal {
        name: String,
        strictness: Strictness,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorPlanIr {
    pub entry_state: u32,
    pub state_count: u32,
    pub suspension_points: Vec<GeneratorSuspensionPointIr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorTryPlanIr {
    pub entry_state: u32,
    pub try_exit_state: u32,
    pub catch_entry_state: Option<u32>,
    pub catch_exit_state: Option<u32>,
    pub finally_entry_state: Option<u32>,
    pub finally_exit_state: Option<u32>,
    pub exit_state: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncTryPlanIr {
    pub entry_state: u32,
    pub try_exit_state: u32,
    pub catch_entry_state: Option<u32>,
    pub catch_exit_state: Option<u32>,
    pub finally_entry_state: Option<u32>,
    pub finally_exit_state: Option<u32>,
    pub exit_state: u32,
}

impl GeneratorPlanIr {
    pub(crate) const MODULE_INSTANTIATION: GeneratorSuspensionPointIr =
        GeneratorSuspensionPointIr {
            suspend_state: 0,
            resume_state: 1,
        };

    pub(crate) fn module_instantiation() -> Self {
        Self {
            entry_state: Self::MODULE_INSTANTIATION.suspend_state,
            state_count: Self::MODULE_INSTANTIATION.resume_state + 1,
            suspension_points: vec![Self::MODULE_INSTANTIATION],
        }
    }

    #[must_use]
    pub const fn without_suspensions() -> Self {
        Self {
            entry_state: 0,
            state_count: 1,
            suspension_points: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassFunctionKind {
    None,
    Constructor,
    Method,
    Getter,
    Setter,
}

/// The only function roles a class method definition can carry.
///
/// [`ClassFunctionKind`] also describes constructors and functions with no
/// class role, but neither state is meaningful inside
/// [`ClassElementDefinitionIr::PublicMethod`] or
/// [`ClassElementDefinitionIr::PrivateMethod`]. Keeping the narrower domain on
/// those variants makes an invalid method row a compile error instead of a
/// backend rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassMethodKindIr {
    Method,
    Getter,
    Setter,
}

impl ClassMethodKindIr {
    #[must_use]
    pub const fn function_kind(self) -> ClassFunctionKind {
        match self {
            Self::Method => ClassFunctionKind::Method,
            Self::Getter => ClassFunctionKind::Getter,
            Self::Setter => ClassFunctionKind::Setter,
        }
    }

    #[must_use]
    pub const fn is_method(self) -> bool {
        matches!(self, Self::Method)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassElementExecutionKind {
    None,
    InstanceFieldInitializer,
    StaticFieldInitializer,
    StaticBlock,
}

/// Exact function identity for an object-literal method whose materializer
/// must attach the allocated literal as `[[HomeObject]]`.
///
/// The fields are private so a generic function expression cannot be placed in
/// a method property without first proving the corresponding function protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an object-method function must be materialized with its HomeObject"]
pub struct ObjectMethodFunctionIr {
    function_id: FunctionId,
    protocol: FunctionProtocolIr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectMethodProtocolIr {
    Method(FunctionExecutionKind),
    Getter,
    Setter,
}

impl ObjectMethodProtocolIr {
    pub(crate) const fn function_protocol(self) -> FunctionProtocolIr {
        match self {
            Self::Method(execution) => FunctionProtocolIr::ObjectMethod(execution),
            Self::Getter => FunctionProtocolIr::ObjectGetter,
            Self::Setter => FunctionProtocolIr::ObjectSetter,
        }
    }
}

impl ObjectMethodFunctionIr {
    pub(crate) fn new(function_id: FunctionId, protocol: ObjectMethodProtocolIr) -> Self {
        Self {
            function_id,
            protocol: protocol.function_protocol(),
        }
    }

    #[must_use]
    pub fn function_id(&self) -> &FunctionId {
        &self.function_id
    }

    #[must_use]
    pub const fn protocol(&self) -> FunctionProtocolIr {
        self.protocol
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputedPropertyNameInferenceIr {
    None,
    Function,
    Class { key_binding: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectPropertyIr {
    PrototypeSetter {
        value: TypedExpr,
    },
    Spread {
        source: TypedExpr,
    },
    Data {
        key: String,
        value: TypedExpr,
        is_shorthand: bool,
    },
    NonEnumerableData {
        key: String,
        value: TypedExpr,
    },
    ComputedData {
        key: TypedExpr,
        value: TypedExpr,
        name_inference: ComputedPropertyNameInferenceIr,
    },
    ComputedMethod {
        key: TypedExpr,
        function: ObjectMethodFunctionIr,
    },
    ComputedGetter {
        key: TypedExpr,
        function: ObjectMethodFunctionIr,
    },
    ComputedSetter {
        key: TypedExpr,
        function: ObjectMethodFunctionIr,
    },
    Method {
        key: String,
        function: ObjectMethodFunctionIr,
    },
    Getter {
        key: String,
        function: ObjectMethodFunctionIr,
    },
    Setter {
        key: String,
        function: ObjectMethodFunctionIr,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateElementKindIr {
    Field,
    Method,
    Accessor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassMethodPlacementIr {
    Instance,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassHeritageKind {
    None,
    Constructable,
    Null,
}

impl Default for ClassHeritageKind {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassPublicMethodIr {
    pub key: PropertyKeyIr,
    pub function_id: FunctionId,
    pub placement: ClassMethodPlacementIr,
    pub kind: ClassMethodKindIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassPrivateMethodIr {
    pub private_name_id: PrivateNameId,
    pub function_id: FunctionId,
    pub placement: ClassMethodPlacementIr,
    pub kind: ClassMethodKindIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassFieldKeyIr {
    Public(String),
    ComputedPublic(u32),
    Private(PrivateNameId),
}

impl ClassFieldKeyIr {
    pub fn static_name(&self) -> Option<&str> {
        match self {
            Self::Public(name) => Some(name),
            Self::ComputedPublic(_) | Self::Private(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFieldInitIr {
    pub key: ClassFieldKeyIr,
    pub init_function_id: Option<FunctionId>,
}

/// The unspellable private name allocated for one auto-accessor's backing slot.
///
/// This wrapper deliberately cannot be constructed outside `scriptw-ir`: source
/// private-name lookup accepts [`PrivateNameId`], while an auto-accessor backing
/// reaches the backend only through the closed class-element plans below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AutoAccessorBackingNameIr(PrivateNameId);

impl AutoAccessorBackingNameIr {
    pub(crate) const fn new(private_name_id: PrivateNameId) -> Self {
        Self(private_name_id)
    }

    pub const fn private_name_id(self) -> PrivateNameId {
        self.0
    }
}

/// The generated getter/setter identities owned by one auto-accessor.
///
/// Both functions are allocated together and consumers can only project the
/// two required sides; a half-pair is not representable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoAccessorFunctionPairIr {
    getter: FunctionId,
    setter: FunctionId,
}

impl AutoAccessorFunctionPairIr {
    pub(crate) fn new(getter: FunctionId, setter: FunctionId) -> Self {
        Self { getter, setter }
    }

    pub fn getter(&self) -> &FunctionId {
        &self.getter
    }

    pub fn setter(&self) -> &FunctionId {
        &self.setter
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassAutoAccessorIr {
    pub key: ClassFieldKeyIr,
    pub computed_key: Option<PropertyKeyIr>,
    pub backing_name: AutoAccessorBackingNameIr,
    pub functions: AutoAccessorFunctionPairIr,
    pub init_function_id: Option<FunctionId>,
    pub placement: ClassMethodPlacementIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassAutoAccessorBackingInitIr {
    pub backing_name: AutoAccessorBackingNameIr,
    pub init_function_id: Option<FunctionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassInstanceElementIr {
    Field(ClassFieldInitIr),
    AutoAccessorBacking(ClassAutoAccessorBackingInitIr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassInstanceElementPlanIr {
    pub private_method_brands: Vec<PrivateNameId>,
    pub elements: Vec<ClassInstanceElementIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassStaticBlockIr {
    pub function_id: FunctionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassElementDefinitionIr {
    PublicMethod(ClassPublicMethodIr),
    PrivateMethod(ClassPrivateMethodIr),
    ComputedFieldKey { slot: u32, key: PropertyKeyIr },
    AutoAccessor(ClassAutoAccessorIr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassStaticElementIr {
    Field(ClassFieldInitIr),
    AutoAccessorBacking(ClassAutoAccessorBackingInitIr),
    Block(ClassStaticBlockIr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassElementPlanIr {
    pub definitions: Vec<ClassElementDefinitionIr>,
    pub static_elements: Vec<ClassStaticElementIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassNameBindingIr {
    pub storage_name: String,
    pub environment: LexicalEnvironmentIr,
}

/// The allocation facts for one class private environment.
///
/// `slot_count` includes both source-visible private names and hidden
/// auto-accessor backing names. Keeping it next to the class scope prevents a
/// public-only auto-accessor class from masquerading as a class with no private
/// environment merely because its visible-name map is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassPrivateEnvironmentIr {
    class_scope: u32,
    slot_count: u32,
}

impl ClassPrivateEnvironmentIr {
    pub(crate) const fn new(class_scope: u32, slot_count: u32) -> Self {
        assert!(
            slot_count > 0,
            "a class private environment must own a slot"
        );
        Self {
            class_scope,
            slot_count,
        }
    }

    pub const fn class_scope(self) -> u32 {
        self.class_scope
    }

    pub const fn slot_count(self) -> u32 {
        self.slot_count
    }
}

/// The name supplied to anonymous ClassExpression NamedEvaluation.
///
/// A computed object property owns a temporary binding; a class field owns
/// either its static name or one already-normalized key in the initializer's
/// immutable class context. The alternatives cannot be combined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassNameInferenceIr {
    None,
    PropertyKeyBinding(String),
    FieldInitializer(ClassFieldNameIr),
}

/// The original field name, independent of receiver properties and source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassFieldNameIr {
    Static(String),
    Computed(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDefinitionIr {
    pub name: Option<String>,
    pub name_binding: Option<ClassNameBindingIr>,
    pub name_inference: ClassNameInferenceIr,
    pub constructor_function_id: FunctionId,
    pub explicit_constructor: bool,
    pub heritage_kind: ClassHeritageKind,
    pub heritage: Option<Box<TypedExpr>>,
    pub element_plan: ClassElementPlanIr,
    pub private_name_ids: BTreeMap<String, PrivateNameId>,
    pub private_environment: Option<ClassPrivateEnvironmentIr>,
}

/// A class operand's statement prefix and the continuation states it spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassEvaluationPrefixIr {
    statements: Vec<StatementIr>,
    entry_state: u32,
    exit_state: u32,
}

impl ClassEvaluationPrefixIr {
    pub(crate) fn new(statements: Vec<StatementIr>, entry_state: u32, exit_state: u32) -> Self {
        assert!(entry_state <= exit_state);
        Self {
            statements,
            entry_state,
            exit_state,
        }
    }
    pub fn statements(&self) -> &[StatementIr] {
        &self.statements
    }
    pub fn entry_state(&self) -> u32 {
        self.entry_state
    }
    pub fn exit_state(&self) -> u32 {
        self.exit_state
    }
}

/// ClassDefinitionEvaluation owns its environment and prepared constructor
/// across suspension. Keeping this in statement position prevents expression
/// temporaries from being mistaken for activation storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumableClassDefinitionIr {
    expression: Box<TypedExpr>,
    constructor_binding: String,
    name_environment_binding: Option<String>,
    completion_binding: String,
    heritage_prefix: ClassEvaluationPrefixIr,
    element_prefixes: BTreeMap<usize, ClassEvaluationPrefixIr>,
    exit_state: u32,
}

impl ResumableClassDefinitionIr {
    pub(crate) fn new(
        expression: TypedExpr,
        constructor_binding: String,
        name_environment_binding: Option<String>,
        completion_binding: String,
        heritage_prefix: ClassEvaluationPrefixIr,
        element_prefixes: BTreeMap<usize, ClassEvaluationPrefixIr>,
        exit_state: u32,
    ) -> Self {
        let ExprIr::ClassDefinition(class) = &expression.expr else {
            panic!("resumable class evaluation requires a class definition");
        };
        assert_eq!(
            class.name_binding.is_some(),
            name_environment_binding.is_some()
        );
        let mut state = heritage_prefix.exit_state;
        assert!(heritage_prefix.entry_state <= state);
        for (index, prefix) in &element_prefixes {
            assert!(*index < class.element_plan.definitions.len());
            assert_eq!(prefix.entry_state, state);
            assert!(prefix.entry_state <= prefix.exit_state);
            state = prefix.exit_state;
        }
        assert_eq!(state, exit_state);
        assert!(heritage_prefix.entry_state < exit_state);
        Self {
            expression: Box::new(expression),
            constructor_binding,
            name_environment_binding,
            completion_binding,
            heritage_prefix,
            element_prefixes,
            exit_state,
        }
    }

    pub fn constructor_binding(&self) -> &str {
        &self.constructor_binding
    }
    pub fn name_environment_binding(&self) -> Option<&str> {
        self.name_environment_binding.as_deref()
    }
    pub fn completion_binding(&self) -> &str {
        &self.completion_binding
    }
    pub fn heritage_prefix(&self) -> &ClassEvaluationPrefixIr {
        &self.heritage_prefix
    }
    pub fn element_prefix(&self, index: usize) -> Option<&ClassEvaluationPrefixIr> {
        self.element_prefixes.get(&index)
    }
    pub fn exit_state(&self) -> u32 {
        self.exit_state
    }

    pub fn expression(&self) -> &TypedExpr {
        &self.expression
    }

    pub fn class(&self) -> &ClassDefinitionIr {
        match &self.expression.expr {
            ExprIr::ClassDefinition(class) => class,
            _ => unreachable!("constructed only from a class definition"),
        }
    }

    pub fn entry_state(&self) -> u32 {
        self.heritage_prefix.entry_state
    }

    pub fn prefixes(&self) -> impl Iterator<Item = &ClassEvaluationPrefixIr> {
        std::iter::once(&self.heritage_prefix).chain(self.element_prefixes.values())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyKeyIr {
    StaticString(String),
    StringExpr(Box<TypedExpr>),
    ArrayIndex(Box<TypedExpr>),
    ArrayLength,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestructuringPropertyKeyIr {
    Static(String),
    Computed(TypedExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestructuringTargetIr {
    /// A var declaration still contributes its hoisted storage even when
    /// BindingInitialization writes a Reference selected through with/eval.
    ResolvedVarBinding {
        name: String,
        reference: IdentifierWriteReferenceIr,
    },
    Binding {
        mode: BindingMode,
        name: String,
    },
    /// PutValue through an identifier Reference after destructuring has
    /// produced the element value and evaluated any default initializer.
    ///
    /// The closed reference replaces the former `name` plus
    /// `global`/`implicit`/`immutable` flag bag, whose invalid combinations
    /// compiled and whose global arm discarded `[[Strict]]` in the backend.
    AssignmentIdentifier(IdentifierWriteReferenceIr),
    AssignmentProperty {
        target: TypedExpr,
        key: DestructuringPropertyKeyIr,
        /// The `[[Strict]]` of the Reference this destructuring element
        /// writes through.
        ///
        /// 13.15.5.4 DestructuringAssignmentEvaluation routes an
        /// `AssignmentProperty` whose target is a property access through
        /// PutValue, exactly as an ordinary `o.x = v` is — so 3.d's TypeError
        /// on a `[[Set]]` that answered `false` is selected by *this*
        /// Reference's `[[Strict]]`, not by the mode of the Wasm function the
        /// pattern happens to be emitted into. The two differ whenever
        /// lowering hoists the pattern into a generated function.
        ///
        /// `AssignmentIdentifier` carries the same fact inside its typed
        /// Reference only for its global/unresolvable disposition; an
        /// Environment Record disposition discharges PutValue branch 4.c at
        /// lowering time. `AssignmentPrivate` carries none because PrivateSet
        /// throws in both modes. See
        /// [`crate::reference::carried_put_value_failure`].
        strictness: Strictness,
    },
    AssignmentPrivate {
        target: TypedExpr,
        private_name_id: PrivateNameId,
    },
    /// A SuperProperty target (13.15.5.5 step 1.a / 13.15.5.6 step 1.a).
    ///
    /// `capture` evaluates the Reference (`this`, the home object's
    /// `[[Prototype]]` base, the uncoerced referenced name) into its slots
    /// when the target is prepared, i.e. before the element value is
    /// obtained. After the value (and any default) exists, the emitter
    /// stores it into `value_binding` and evaluates `put`, which performs
    /// PutValue through the captured Reference.
    AssignmentSuper {
        capture: Box<TypedExpr>,
        value_binding: String,
        put: Box<TypedExpr>,
    },
    NestedArray(Box<ArrayDestructuringPatternIr>),
    NestedObject(Box<ObjectDestructuringPatternIr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayDestructuringElementIr {
    Elision,
    Target {
        target: DestructuringTargetIr,
        default: Option<TypedExpr>,
    },
    Rest {
        target: DestructuringTargetIr,
    },
}

/// The ECMAScript abstract operation that owns an array destructuring pattern.
///
/// Both operations use the same [`ArrayPatternProtocol`], but they have
/// different result and declaration semantics. There is deliberately no
/// `Default` or boolean conversion: every producer names the operation and
/// every semantic consumer matches it exhaustively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayDestructuringEvaluationIr {
    /// 8.6.3 IteratorBindingInitialization.
    BindingInitialization,
    /// 13.15.5.5 IteratorDestructuringAssignmentEvaluation.
    AssignmentEvaluation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayDestructuringPatternIr {
    pub elements: Vec<ArrayDestructuringElementIr>,
    /// How this pattern's own `GetIterator` discharged the four 7.4
    /// obligations.
    ///
    /// **Per pattern, not per statement.** 8.6.3 IteratorBindingInitialization
    /// and 13.15.5.5 IteratorDestructuringAssignmentEvaluation each acquire a
    /// fresh iterator for *every* ArrayBindingPattern / ArrayAssignmentPattern,
    /// including each one reached through
    /// [`DestructuringTargetIr::NestedArray`]. A field on
    /// `ExprIr::ArrayDestructure` would witness the outermost acquisition and
    /// silently cover none of the nested ones, so the field lives here.
    ///
    /// Non-optional, no `Default`: `ArrayDestructuringPatternIr` is matched
    /// exhaustively nowhere in the workspace and constructed in exactly two
    /// places, both in `lowering.rs`, so an array pattern built without saying
    /// how its iterator was accounted for is `E0063` at those two lines and
    /// byte-neutral everywhere else.
    ///
    /// The type is [`ArrayPatternProtocol`], **not** `IteratorProtocolWitness`.
    /// The bare witness type is the whole witness domain, so both
    /// `IteratorProtocolWitness::NO_ITERATION` and `::SYNC_ITERATOR_PROTOCOL`
    /// compiled at both construction sites and every const assertion still
    /// passed. The newtype has one inhabitant and a private constructor, so any
    /// other witness here is `E0308`: the guarantee is now "the right constant",
    /// not "a constant".
    ///
    /// **The emitter must not read this**, and cannot: every reader of a
    /// witness's contents — including [`ArrayPatternProtocol::witness`] — is
    /// `pub(crate)` to `scriptw-ir`, so a `scriptw-aot-wasm` arm that binds it
    /// and branches on it is `E0624`.
    pub protocol: ArrayPatternProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectDestructuringPropertyIr {
    pub key: DestructuringPropertyKeyIr,
    pub target: DestructuringTargetIr,
    pub default: Option<TypedExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectDestructuringPatternIr {
    pub properties: Vec<ObjectDestructuringPropertyIr>,
    pub rest: Option<DestructuringTargetIr>,
}

impl ObjectDestructuringPatternIr {
    pub fn visit_expressions(&self, visit: &mut impl FnMut(&TypedExpr)) {
        for property in &self.properties {
            if let DestructuringPropertyKeyIr::Computed(key) = &property.key {
                visit(key);
            }
            visit_destructuring_target_expressions(&property.target, visit);
            if let Some(default) = &property.default {
                visit(default);
            }
        }
        if let Some(rest) = &self.rest {
            visit_destructuring_target_expressions(rest, visit);
        }
    }
}

pub(crate) fn visit_destructuring_target_expressions(
    target: &DestructuringTargetIr,
    visit: &mut impl FnMut(&TypedExpr),
) {
    match target {
        DestructuringTargetIr::AssignmentProperty { target, key, .. } => {
            visit(target);
            if let DestructuringPropertyKeyIr::Computed(key) = key {
                visit(key);
            }
        }
        DestructuringTargetIr::AssignmentPrivate { target, .. } => visit(target),
        DestructuringTargetIr::AssignmentSuper { capture, put, .. } => {
            visit(capture);
            visit(put);
        }
        DestructuringTargetIr::NestedArray(pattern) => pattern.visit_expressions(visit),
        DestructuringTargetIr::NestedObject(pattern) => pattern.visit_expressions(visit),
        DestructuringTargetIr::ResolvedVarBinding { reference, .. }
        | DestructuringTargetIr::AssignmentIdentifier(reference) => {
            reference.visit_resolution_expressions(visit);
        }
        DestructuringTargetIr::Binding { .. } => {}
    }
}

impl ArrayDestructuringPatternIr {
    pub fn visit_expressions(&self, visit: &mut impl FnMut(&TypedExpr)) {
        for element in &self.elements {
            let (target, default) = match element {
                ArrayDestructuringElementIr::Elision => continue,
                ArrayDestructuringElementIr::Target { target, default } => {
                    (target, default.as_ref())
                }
                ArrayDestructuringElementIr::Rest { target } => (target, None),
            };
            visit_destructuring_target_expressions(target, visit);
            if let Some(default) = default {
                visit(default);
            }
        }
    }

    pub fn visit_bindings(&self, visit: &mut impl FnMut(BindingMode, &str)) {
        for element in &self.elements {
            let target = match element {
                ArrayDestructuringElementIr::Elision => continue,
                ArrayDestructuringElementIr::Target { target, .. }
                | ArrayDestructuringElementIr::Rest { target } => target,
            };
            visit_destructuring_target_bindings(target, visit);
        }
    }
}

impl ObjectDestructuringPatternIr {
    pub fn visit_bindings(&self, visit: &mut impl FnMut(BindingMode, &str)) {
        for property in &self.properties {
            visit_destructuring_target_bindings(&property.target, visit);
        }
        if let Some(rest) = &self.rest {
            visit_destructuring_target_bindings(rest, visit);
        }
    }
}

pub(crate) fn visit_destructuring_target_bindings(
    target: &DestructuringTargetIr,
    visit: &mut impl FnMut(BindingMode, &str),
) {
    match target {
        DestructuringTargetIr::ResolvedVarBinding { name, .. } => visit(BindingMode::Var, name),
        DestructuringTargetIr::Binding { mode, name } => visit(*mode, name),
        DestructuringTargetIr::NestedArray(pattern) => pattern.visit_bindings(visit),
        DestructuringTargetIr::NestedObject(pattern) => pattern.visit_bindings(visit),
        DestructuringTargetIr::AssignmentIdentifier(..)
        | DestructuringTargetIr::AssignmentProperty { .. }
        | DestructuringTargetIr::AssignmentPrivate { .. }
        | DestructuringTargetIr::AssignmentSuper { .. } => {}
    }
}

impl PropertyKeyIr {
    pub fn static_name(&self) -> Option<&str> {
        match self {
            Self::StaticString(name) => Some(name),
            Self::ArrayLength => Some("length"),
            Self::StringExpr(_) | Self::ArrayIndex(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedExpr {
    pub kind: ValueKind,
    pub possible_kinds: KindSet,
    pub heap_shape: Option<Box<HeapShape>>,
    pub function_targets: FunctionTargetKnowledge,
    pub expr: ExprIr,
}

impl TypedExpr {
    pub const fn undefined() -> Self {
        Self {
            kind: ValueKind::Undefined,
            possible_kinds: KindSet::from_kind(ValueKind::Undefined),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
            expr: ExprIr::Undefined,
        }
    }

    pub fn from_info(info: ValueInfo, expr: ExprIr) -> Self {
        Self {
            kind: info.kind,
            possible_kinds: info.possible_kinds,
            heap_shape: info.heap_shape,
            function_targets: info.function_targets,
            expr,
        }
    }

    pub fn value_info(&self) -> ValueInfo {
        ValueInfo {
            kind: self.kind,
            possible_kinds: self.possible_kinds,
            heap_shape: self.heap_shape.clone(),
            function_targets: self.function_targets.clone(),
        }
    }

    pub fn spec_is_callable(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::IsCallable,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_is_constructor(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::IsConstructor,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_is_property_key(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::IsPropertyKey,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_boolean(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToBoolean,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_primitive(argument: TypedExpr, hint: ToPrimitiveHint) -> Self {
        Self::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::PRIMITIVE_ONLY,
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToPrimitive(hint),
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_numeric(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::BigInt)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToNumeric,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_number(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToNumber,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_bigint(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::BigInt),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToBigInt,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_string(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::String),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToString,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_object(argument: TypedExpr) -> Self {
        let possible_kinds = KindSet::from_kind(ValueKind::Object)
            .union(KindSet::from_kind(ValueKind::Array))
            .union(KindSet::from_kind(ValueKind::Function))
            .union(KindSet::from_kind(ValueKind::Arguments));
        Self::from_info(
            ValueInfo {
                kind: possible_kinds.as_value_kind(),
                possible_kinds,
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToObject,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_to_property_key(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Symbol)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToPropertyKey,
                operands: vec![argument],
            },
        )
    }

    /// **Reachable only from tests as of `091487732`.** Measured: this
    /// constructor has exactly two references workspace-wide besides its own
    /// definition, and both sit inside `#[cfg(test)]` modules — `ir/tests.rs` (`#[cfg(test)]`) and `crates/scriptw-aot-wasm/src/tests.rs` (`#[cfg(test)]`). AGENTS.md wants unreachable-from-product code to
    /// fail to build; deleting this requires editing an aot-wasm test module,
    /// which is outside this area's lane. Ledger **LN4** in
    /// `docs/rust-rewrite/contracts/numeric-conversion-codomains.md`.
    ///
    /// Of the 31 `pub fn spec_*` constructors in this file, these three were the
    /// ones verified line by line. Do not delete this from this lane; do not
    /// add a product call site to make the count look better either — the
    /// codomain of 7.1.5 that a real call site would need is
    /// [`crate::IntegerOrInfinity`], not the `ValueKind::Number` claimed here.
    pub fn spec_to_integer_or_infinity(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToIntegerOrInfinity,
                operands: vec![argument],
            },
        )
    }

    /// **Reachable only from tests as of `091487732`.** Measured: exactly two
    /// references besides this definition, both inside `#[cfg(test)]` modules —
    /// `ir/tests.rs` (`#[cfg(test)]`) and
    /// `crates/scriptw-aot-wasm/src/tests.rs` (`#[cfg(test)]`).
    /// Ledger **LN4**; see `spec_to_integer_or_infinity` above.
    pub fn spec_to_length(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToLength,
                operands: vec![argument],
            },
        )
    }

    /// **Reachable only from tests as of `091487732`.** Measured: exactly two
    /// references besides this definition, both inside `#[cfg(test)]` modules —
    /// `ir/tests.rs` (`#[cfg(test)]`) and
    /// `crates/scriptw-aot-wasm/src/tests.rs` (`#[cfg(test)]`).
    /// Ledger **LN4**; see `spec_to_integer_or_infinity` above. 7.1.22 is the
    /// one *partial* conversion in this area — see
    /// [`crate::ToIndexOutcome`] — and `ValueKind::Number` cannot say so.
    pub fn spec_to_index(argument: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Number),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::ToIndex,
                operands: vec![argument],
            },
        )
    }

    pub fn spec_same_value(lhs: TypedExpr, rhs: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::SameValue,
                operands: vec![lhs, rhs],
            },
        )
    }

    pub fn spec_same_value_zero(lhs: TypedExpr, rhs: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::SameValueZero,
                operands: vec![lhs, rhs],
            },
        )
    }

    pub fn spec_strict_equality_comparison(lhs: TypedExpr, rhs: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::StrictEqualityComparison,
                operands: vec![lhs, rhs],
            },
        )
    }

    pub fn spec_is_loosely_equal(lhs: TypedExpr, rhs: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::IsLooselyEqual,
                operands: vec![lhs, rhs],
            },
        )
    }

    pub fn spec_get_v(target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::spec_get_v_with_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            target,
            property_key,
        )
    }

    pub fn spec_get(target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::spec_get_with_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            target,
            property_key,
        )
    }

    pub fn spec_get_with_info(info: ValueInfo, target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::from_info(
            info,
            ExprIr::SpecOperation {
                operation: SpecOperationIr::Get,
                operands: vec![target, property_key],
            },
        )
    }

    pub fn spec_get_v_with_info(
        info: ValueInfo,
        target: TypedExpr,
        property_key: TypedExpr,
    ) -> Self {
        Self::from_info(
            info,
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands: vec![target, property_key],
            },
        )
    }

    pub fn spec_has_property(target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::HasProperty,
                operands: vec![target, property_key],
            },
        )
    }

    pub fn spec_has_own_property(target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::HasOwnProperty,
                operands: vec![target, property_key],
            },
        )
    }

    pub fn spec_set(target: TypedExpr, property_key: TypedExpr, value: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::Set,
                operands: vec![target, property_key, value],
            },
        )
    }

    pub fn spec_delete_property_or_throw(target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Boolean),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::DeletePropertyOrThrow,
                operands: vec![target, property_key],
            },
        )
    }

    pub fn spec_get_method(target: TypedExpr, property_key: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Undefined)
                    .union(KindSet::from_kind(ValueKind::Function)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetMethod,
                operands: vec![target, property_key],
            },
        )
    }

    pub fn spec_call(callee: TypedExpr, this_arg: TypedExpr, args: Vec<TypedExpr>) -> Self {
        let mut operands = Vec::with_capacity(args.len() + 2);
        operands.push(callee);
        operands.push(this_arg);
        operands.extend(args);
        Self::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::Call,
                operands,
            },
        )
    }

    pub fn spec_construct(callee: TypedExpr, args: Vec<TypedExpr>) -> Self {
        let mut operands = Vec::with_capacity(args.len() + 1);
        operands.push(callee);
        operands.extend(args);
        Self::from_info(
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::Object)
                    .union(KindSet::from_kind(ValueKind::Array))
                    .union(KindSet::from_kind(ValueKind::Function))
                    .union(KindSet::from_kind(ValueKind::Arguments)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::unknown(),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::Construct,
                operands,
            },
        )
    }

    pub fn spec_create_data_property_or_throw(
        target: TypedExpr,
        property_key: TypedExpr,
        value: TypedExpr,
    ) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Undefined),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::CreateDataPropertyOrThrow,
                operands: vec![target, property_key, value],
            },
        )
    }

    pub fn spec_copy_data_properties(target: TypedExpr, source: TypedExpr) -> Self {
        Self::from_info(
            ValueInfo::new(ValueKind::Undefined),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::CopyDataProperties,
                operands: vec![target, source],
            },
        )
    }
}

/// The operand of `...expr` in an argument list, together with the required
/// 13.3.8.1 iterator-protocol discharge.
///
/// Both fields are required and there is no `Default`. The protocol field's
/// type has one inhabitant with a private constructor, so a new spread
/// construction cannot omit the witness or substitute a witness belonging to
/// another iterator consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadArgumentIr {
    pub value: Box<TypedExpr>,
    pub protocol: SpreadArgumentProtocol,
}

/// One spread operand in 13.2.4.1 ArrayAccumulation.
///
/// The one-inhabitant protocol field makes the iterator obligations part of
/// the IR construction rather than a convention in the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArraySpreadIr {
    pub value: Box<TypedExpr>,
    pub protocol: ArraySpreadProtocol,
}

/// One source-ordered contribution to a fresh array literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayAccumulationElementIr {
    Elision,
    Value(TypedExpr),
    Spread(ArraySpreadIr),
}

/// Suspension-owned binding that carries the fresh array itself.
///
/// This is deliberately distinct from the logical-index slot: swapping the
/// two names in staged lowering must be a type error, not a latent runtime bug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayAccumulatorArraySlot(String);

impl ArrayAccumulatorArraySlot {
    pub(crate) fn new(name: String) -> Self {
        Self(name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Suspension-owned binding that carries ArrayAccumulation's exact unsigned
/// 64-bit `nextIndex`, independently of the fresh array's capped `length`.
///
/// This compiler-private slot is an opaque integer carrier, not an ECMAScript
/// Number. The backend rejects the next contribution at `u64::MAX` rather than
/// wrapping. The bound is far above the observable 2^32-1 array-index seam but
/// deliberately does not claim the spec's unbounded mathematical integer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayAccumulatorU64NextIndexSlot(String);

impl ArrayAccumulatorU64NextIndexSlot {
    pub(crate) fn new(name: String) -> Self {
        Self(name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The two non-transposable suspension slots needed by a staged array literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayAccumulatorSlots {
    array: ArrayAccumulatorArraySlot,
    next_index: ArrayAccumulatorU64NextIndexSlot,
}

impl ArrayAccumulatorSlots {
    pub(crate) fn new(
        array: ArrayAccumulatorArraySlot,
        next_index: ArrayAccumulatorU64NextIndexSlot,
    ) -> Self {
        Self { array, next_index }
    }

    pub fn array(&self) -> &ArrayAccumulatorArraySlot {
        &self.array
    }

    pub fn next_index(&self) -> &ArrayAccumulatorU64NextIndexSlot {
        &self.next_index
    }
}

/// Where an ArrayAccumulation expression gets its fresh-array state.
///
/// `Fresh` is a single uninterrupted expression. `SuspensionOwned` names two
/// bindings initialized before the first element and retained across yield;
/// only the lowerer can construct those typed slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayAccumulationTargetIr {
    Fresh,
    SuspensionOwned(ArrayAccumulatorSlots),
}

/// 13.2.4.1 ArrayAccumulation, kept distinct from a plain no-spread
/// `ArrayLiteral` so ordinary literals retain their compact emitter and shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayAccumulationIr {
    target: ArrayAccumulationTargetIr,
    elements: Vec<ArrayAccumulationElementIr>,
}

impl ArrayAccumulationIr {
    pub(crate) fn fresh(elements: Vec<ArrayAccumulationElementIr>) -> Self {
        Self {
            target: ArrayAccumulationTargetIr::Fresh,
            elements,
        }
    }

    pub(crate) fn suspension_owned(
        slots: ArrayAccumulatorSlots,
        elements: Vec<ArrayAccumulationElementIr>,
    ) -> Self {
        Self {
            target: ArrayAccumulationTargetIr::SuspensionOwned(slots),
            elements,
        }
    }

    pub fn target(&self) -> &ArrayAccumulationTargetIr {
        &self.target
    }

    pub fn elements(&self) -> &[ArrayAccumulationElementIr] {
        &self.elements
    }
}

/// Evaluation behavior of the same module namespace exotic representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleNamespaceModeIr {
    Eager,
    Deferred,
}

impl ModuleNamespaceModeIr {
    pub(crate) const fn cell_role(self) -> crate::UnitCellRole {
        match self {
            Self::Eager => crate::UnitCellRole::Namespace,
            Self::Deferred => crate::UnitCellRole::DeferredNamespace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprIr {
    /// A compiler-owned first-class wrapper's declared native scalar call.
    ForeignCall(Box<crate::foreign::ForeignCallIr>),
    Undefined,
    ArrayHole,
    Null,
    Boolean(bool),
    Number(u64),
    BigInt(BigIntLiteralIr),
    /// The canonical agent-wide Symbol, distinct from its description string.
    WellKnownSymbol(crate::WellKnownSymbol),
    Symbol {
        /// Description operand for `Symbol(desc)`. `None` for `Symbol()` /
        /// `Symbol(undefined)` (spec `[[Description]]` = undefined). When
        /// present, the operand has already been coerced via ToString during
        /// lowering.
        description: Option<Box<TypedExpr>>,
    },
    String(String),
    /// An intrinsic RegExp literal creation, independent of the mutable global
    /// `RegExp` constructor.
    RegExpLiteral {
        source: String,
        flags: String,
        static_compilation: Option<StaticRegExpCompilation>,
    },
    FunctionValue(FunctionId),
    /// NamedEvaluation of an anonymous field initializer function. The key is
    /// the original static/private name or retained computed-key slot; no
    /// source key expression or user coercion runs during instance creation.
    ClassFieldNamedFunctionValue {
        function_id: FunctionId,
        name: ClassFieldNameIr,
    },
    /// `import(specifier, options)`.
    ///
    /// Resolved at runtime against the compile-time component registry in
    /// `ModuleGraphIr::components`. Never parses source: a specifier that
    /// matches no component rejects the promise.
    /// ExternalCaller retains a different ProgramInstance's authority; its
    /// unbridged runtime capability path must execute only at this operation.
    DynamicImport {
        specifier: Box<TypedExpr>,
        options: Option<Box<TypedExpr>>,
        phase: ImportPhaseIr,
        referrer: crate::ImportCallSourceOriginIr,
    },
    /// `import.meta` of the enclosing module.
    ImportMeta {
        module: ModuleUnitId,
    },
    /// A namespace's private closure table, produced only from trusted linker
    /// metadata. Its first element is undefined (eager) or an evaluation closure
    /// (deferred), followed by sorted export-name / live-reader pairs. The linker
    /// binds the resulting object once to preserve namespace identity.
    ModuleNamespace {
        mode: ModuleNamespaceModeIr,
        exports: Box<TypedExpr>,
    },
    /// Allocate every private activation and instantiate imports/namespaces before evaluation.
    ModuleExecutionGraph(Box<crate::modules::ModuleExecutionGraphIr>),
    ModuleEntryEvaluation(crate::modules::ModuleEntryEvaluationIr),
    ModuleBindingRead(crate::modules::ModuleCellIr),
    /// Native data evaluation of a genuine JSON synthetic module record.
    JsonModuleValue(Box<crate::modules::JsonModuleValueIr>),
    BytesModuleValue(Box<crate::modules::BytesModuleValueIr>),
    ModuleEvaluate(crate::modules::ModuleEvaluationIr),
    DeferredModuleEvaluate(crate::modules::DeferredModuleEvaluationIr),
    ModuleHasAsyncDependencies(crate::modules::ModuleEvaluationIr),
    ModuleDeferredImportEvaluate(crate::modules::ModuleEvaluationIr),
    ModuleNamespacePublish {
        module: ModuleUnitId,
        mode: ModuleNamespaceModeIr,
        namespace: Box<TypedExpr>,
    },
    This,
    /// The current execution Realm's actual global object, selected by a
    /// compiler-owned Environment Reference or a proven intact initial
    /// `globalThis` data property. This never looks up a source binding.
    ExecutionGlobalObject,
    Arguments,
    ObjectLiteral(Vec<ObjectPropertyIr>),
    ObjectPropertyDefinition(Box<crate::ObjectPropertyDefinitionIr>),
    ObjectDestructuringOperation(Box<crate::ObjectDestructuringOperationIr>),
    ArrayLiteral(Vec<TypedExpr>),
    ArrayAccumulation(ArrayAccumulationIr),
    Identifier(String),
    /// GetValue of an identifier Reference that resolves in the Global
    /// Environment Record, for a name the lowerer saw as a global property.
    /// That sighting carries value knowledge, not existence: the backend
    /// still performs HasBinding, so a deleted global throws a ReferenceError,
    /// and as the operand of `TypeOf` it keeps typeof's unresolvable result.
    GlobalPropertyRead {
        name: String,
    },
    EnvironmentIdentifier(Box<crate::EnvironmentIdentifierIr>),
    GlobalIdentifierRead {
        name: String,
    },
    AssignIdentifier {
        name: String,
        value: Box<TypedExpr>,
    },
    GlobalPropertyWrite {
        name: String,
        value: Box<TypedExpr>,
        /// The lowerer could not prove the property already exists on the
        /// global object, so this write may *create* a global binding.
        ///
        /// **Not** a backend input, and deliberately so: which of PutValue's
        /// two branches applies is a runtime fact. The backend resolves and
        /// retains the global Reference before evaluating `value`, then applies
        /// its strictness through the shared Environment Record Put. This flag's
        /// one consumer is the `implicit_globals` counter in this file's AST-stat visitor, which is
        /// what keeps it from being an unread field — check there before
        /// deleting it.
        implicit: bool,
        /// The `[[Strict]]` of the Reference this write consumes. PutValue
        /// step 2.a requires a ReferenceError when the Reference is
        /// unresolvable and `[[Strict]]` is true, even if the RHS creates that
        /// property. A resolved Object Record rechecks its held binding object
        /// after the RHS; step 3.d then makes a `[[Set]]` that answered `false`
        /// a TypeError.
        strictness: Strictness,
    },
    PropertyRead {
        target: Box<TypedExpr>,
        key: PropertyKeyIr,
    },
    /// An ordered optional chain of property accesses and calls.
    ///
    /// Keys and call arguments remain expressions in the chain so a backend
    /// can defer them until all preceding optional operations have succeeded.
    OptionalPropertyChain {
        target: Box<TypedExpr>,
        chain: Vec<OptionalChainOperationIr>,
    },
    DeleteOptionalPropertyChain(Box<crate::DeleteOptionalPropertyChainIr>),
    PropertyWrite {
        target: Box<TypedExpr>,
        key: PropertyKeyIr,
        value: Box<TypedExpr>,
        /// The `[[Strict]]` of the Reference this write consumes. PutValue
        /// step 3.d: a `[[Set]]` that answered `false` is a TypeError only
        /// when it is true.
        strictness: Strictness,
    },
    OrdinaryPropertyAssignment(OrdinaryPropertyAssignmentIr),
    OrdinaryPropertyLogicalAssignment(OrdinaryPropertyLogicalAssignmentIr),
    OrdinaryPropertyGetCapture(OrdinaryPropertyGetCaptureIr),
    CapturedOrdinaryPropertyWrite(CapturedOrdinaryPropertyWriteIr),
    OrdinaryPropertyNumericUpdate(OrdinaryPropertyNumericUpdateIr),
    OrdinaryPropertyEagerCompoundAssignment(OrdinaryPropertyEagerCompoundAssignmentIr),
    UpdateIdentifier {
        name: String,
        op: NumericUpdateOp,
        return_mode: UpdateReturnMode,
        value_kind: NumericUpdateValueKind,
    },
    CompoundAssignIdentifier {
        name: String,
        op: ArithmeticBinaryOp,
        value: Box<TypedExpr>,
    },
    UnaryPlus {
        expr: Box<TypedExpr>,
    },
    UnaryMinusNumeric {
        expr: Box<TypedExpr>,
    },
    UnaryBitwiseNumeric {
        op: UnaryBitwiseOp,
        expr: Box<TypedExpr>,
    },
    Void {
        expr: Box<TypedExpr>,
    },
    DeleteValue {
        expr: Box<TypedExpr>,
    },
    /// `delete` of an identifier that resolves to a binding which cannot be
    /// deleted: a declarative binding or a non-configurable global. Always
    /// `false`. An identifier that may name a global object property lowers
    /// to [`ExprIr::DeleteGlobalProperty`] instead, because only the runtime
    /// global object knows whether the property exists.
    DeleteIdentifier {
        name: String,
    },
    /// Identifier deletion through the current Global Environment Record.
    /// ResolveBinding precedes DeleteBinding; an unresolvable name returns
    /// true. Source property syntax always retains [`ExprIr::DeleteProperty`].
    DeleteGlobalProperty {
        name: String,
        /// Source identifier deletion is legal only in sloppy code.
        strictness: Strictness,
    },
    DeleteProperty {
        target: Box<TypedExpr>,
        key: PropertyKeyIr,
        /// 13.5.1.2 step 5.e, as above.
        strictness: Strictness,
    },
    TypeOf {
        expr: Box<TypedExpr>,
    },
    NewTarget,
    /// Resolve a global identifier at execution time and complete typeof.
    /// Only an unresolvable Reference bypasses GetValue; HasBinding hooks,
    /// accessors, and uninitialized declarative bindings remain observable.
    TypeOfUnresolvedIdentifier {
        name: String,
    },
    LogicalNot {
        expr: Box<TypedExpr>,
    },
    SpecOperation {
        operation: SpecOperationIr,
        operands: Vec<TypedExpr>,
    },
    BinaryNumber {
        op: ArithmeticBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    CoerciveAdd {
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    CoerciveBinaryNumber {
        op: ArithmeticBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    BitwiseNumeric {
        op: BitwiseBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    StringFromCharCode {
        code: Box<TypedExpr>,
    },
    StringConcat {
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    TemplateObject(TemplateObjectIr),
    CompareNumber {
        op: RelationalBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    CompareValue {
        op: RelationalBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    StrictEquality {
        op: EqualityBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    LooseEquality {
        op: EqualityBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    LogicalShortCircuit {
        op: LogicalBinaryOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    Conditional {
        condition: Box<TypedExpr>,
        then_expr: Box<TypedExpr>,
        else_expr: Box<TypedExpr>,
    },
    Comma {
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    MaterializeBinding {
        name: String,
        value: Box<TypedExpr>,
        body: Box<TypedExpr>,
    },
    ArrayDestructure {
        value: Box<TypedExpr>,
        pattern: ArrayDestructuringPatternIr,
        evaluation: ArrayDestructuringEvaluationIr,
    },
    ObjectDestructure {
        value: Box<TypedExpr>,
        pattern: Box<ObjectDestructuringPatternIr>,
    },
    CallNamed {
        name: String,
        args: Vec<TypedExpr>,
    },
    SpreadArgument(SpreadArgumentIr),
    CaptureArgumentList(crate::ArgumentListCaptureIr),
    CaptureOptionalCallReference(crate::OptionalCallReferenceCaptureIr),
    CapturedArgumentList(crate::CapturedArgumentListIr),
    AssertSameValue {
        actual: Box<TypedExpr>,
        expected: Box<TypedExpr>,
        message: String,
    },
    /// Throw a fresh instance of an error intrinsic, chosen at compile time.
    ///
    /// `name` is a [`NativeErrorKind`] rather than a `&'static str` so that a
    /// misspelt or invented error name is `error[E0308]` at the construction
    /// site instead of a value that falls through the backend's
    /// name-to-prototype table to `%Object.prototype%` — a thrown value for
    /// which `e instanceof TypeError` is `false` and `e.message` is
    /// `undefined`, with no diagnostic.
    RuntimeThrow {
        name: NativeErrorKind,
        message: &'static str,
    },
    CallIndirect {
        /// Original bare `eval` syntax carries caller context; only the runtime
        /// callee's identity can select direct evaluation.
        direct_eval: Option<crate::DirectEvalContextIr>,
        callee: Box<TypedExpr>,
        this_arg: Option<Box<TypedExpr>>,
        args: Vec<TypedExpr>,
        /// The static compilation outcome for a direct, constant `RegExp` call.
        /// This is metadata only: the ordinary call path still observes callee,
        /// receiver, and argument evaluation before applying the outcome.
        static_regexp_compilation: Option<StaticRegExpCompilation>,
    },
    Construct {
        callee: Box<TypedExpr>,
        args: Vec<TypedExpr>,
        /// The static compilation outcome for a direct, constant `new RegExp` call.
        /// This is metadata only: construction still observes callee and argument
        /// evaluation before applying the outcome.
        static_regexp_compilation: Option<StaticRegExpCompilation>,
    },
    ClassDefinition(Box<ClassDefinitionIr>),
    CallMethod {
        receiver: Box<TypedExpr>,
        key: PropertyKeyIr,
        args: Vec<TypedExpr>,
    },
    SuperConstruct {
        args: Vec<TypedExpr>,
    },
    /// GetNewTarget from the original lexical derived constructor activation.
    SuperNewTarget,
    /// GetSuperConstructor from that same original activation.
    SuperConstructor,
    PreparedSuperConstruct(Box<crate::PreparedSuperConstructIr>),
    SuperPropertyRead {
        key: PropertyKeyIr,
        receiver: Box<TypedExpr>,
    },
    SuperPropertyWrite {
        key: PropertyKeyIr,
        /// GetThisValue of the super Reference, evaluated before the RHS and
        /// passed as the Receiver to `superBase.[[Set]]`.
        receiver: Box<TypedExpr>,
        value: Box<TypedExpr>,
        /// PutValue step 3.d.
        strictness: Strictness,
    },
    /// Numeric/eager mutation or an activation-owned capture and consuming Put
    /// through the original Super Property Reference.
    SuperPropertyMutation(SuperPropertyMutationIr),
    PrivateRead {
        target: Box<TypedExpr>,
        private_name_id: PrivateNameId,
    },
    PrivateWrite {
        target: Box<TypedExpr>,
        private_name_id: PrivateNameId,
        value: Box<TypedExpr>,
    },
    PrivateIn {
        private_name_id: PrivateNameId,
        rhs: Box<TypedExpr>,
    },
    InstanceOf {
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    In {
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateObjectIr {
    pub site_id: crate::TemplateSiteId,
    pub cooked: Vec<Option<String>>,
    pub raw: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionalChainOperationIr {
    Property {
        key: PropertyKeyIr,
        /// Whether this operation was introduced by `?.` and therefore
        /// short-circuits the whole chain for a nullish receiver.
        shorted: bool,
    },
    PrivateProperty {
        private_name_id: PrivateNameId,
        /// Whether this operation was introduced by `?.` and therefore
        /// short-circuits the whole chain for a nullish receiver.
        shorted: bool,
    },
    Call {
        args: Vec<TypedExpr>,
        /// How the call's `this` value is recovered from the source Reference.
        receiver: OptionalChainCallReceiverIr,
        /// Whether this operation was introduced by `?.` and therefore
        /// short-circuits the whole chain for a nullish callee.
        shorted: bool,
        /// Whether a parenthesized/grouped expression ended the preceding
        /// short-circuit segment before this call, without discarding a
        /// preceding property Reference used as the call receiver.
        boundary_before: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalChainCallReceiverIr {
    /// Use the base retained by the immediately preceding property operation,
    /// or `undefined` when the callee was not obtained from a property Reference.
    ReferenceOrUndefined,
    /// Use the surrounding function's current `this`, as required when calling
    /// a function obtained from a `super` property Reference.
    CurrentThis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForLexicalInitIr {
    pub mode: BindingMode,
    pub name: String,
    pub init: TypedExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForInitIr {
    Lexical {
        mode: BindingMode,
        name: String,
        init: TypedExpr,
    },
    LexicalBlock(Vec<ForLexicalInitIr>),
    Var(Vec<VarDeclaratorIr>),
    Expression(TypedExpr),
    /// A loop head that binds a pattern (`for (let [a, b] = x; …)`), lowered to
    /// the same statements the equivalent standalone declaration lowers to.
    /// The statements run in the loop's own scope, so the names they bind stay
    /// visible to the test, update and body.
    Statements(Vec<StatementIr>),
    /// A non-empty, declaration-ordered synchronous `using` head.
    ///
    /// The containing `StatementIr::For` owns the loop control targets and its
    /// lexical environment. This variant separately requires the backend to
    /// keep one DisposeCapability active from before the first initializer
    /// until the loop's final completion.
    SyncDisposable(SyncDisposableResourcesIr),
    /// A non-empty async-disposable head whose activation-backed capability
    /// remains live across the whole classic loop.
    AsyncDisposable(AsyncDisposableForInitIr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDeclaratorIr {
    pub name: String,
    pub init: Option<TypedExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchCaseIr {
    pub condition: Option<TypedExpr>,
    pub body: BlockIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParamIr {
    pub name: String,
    pub kind: ValueKind,
    pub default_init: Option<TypedExpr>,
    pub is_rest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedEnvBindingIr {
    pub name: String,
    pub slot: u32,
    /// The real BindingCell's write policy, independently of eval visibility.
    pub mutability: EnvironmentBindingMutabilityIr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentBindingMutabilityIr {
    Mutable,
    /// `strict` is CreateImmutableBinding's S flag, not source strictness.
    Immutable {
        strict: bool,
    },
}

impl EnvironmentBindingMutabilityIr {
    pub const fn lexical(mode: BindingMode) -> Self {
        match mode {
            BindingMode::Var | BindingMode::Let => Self::Mutable,
            BindingMode::Const => Self::Immutable { strict: true },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionBodyBindingValueIr {
    Undefined,
    Parameter { slot: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBodyBindingInitializationIr {
    pub slot: u32,
    pub value: FunctionBodyBindingValueIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexicalEnvironmentInitializationIr {
    Uninitialized,
    FunctionBody {
        bindings: Vec<FunctionBodyBindingInitializationIr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalEnvironmentIr {
    pub initialization: LexicalEnvironmentInitializationIr,
    pub eval_environment: Option<crate::EvalEnvironmentRoleIr>,
    pub bindings: Vec<OwnedEnvBindingIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForLexicalEnvironmentIr {
    pub eval_environment: Option<crate::EvalEnvironmentRoleIr>,
    pub bindings: Vec<OwnedEnvBindingIr>,
    pub per_iteration_slots: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForInOfEnvironmentIr {
    pub tdz_environment: Option<LexicalEnvironmentIr>,
    pub iteration_environment: Option<LexicalEnvironmentIr>,
    pub tdz_binding_names: Vec<String>,
}

/// The assignment performed by an ordinary `for-of` head.
///
/// The generic iterator assignment head uses this type. Resource heads are
/// separate [`ForOfIteratorHeadIr`] variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForOfAssignmentIr {
    pub mode: BindingMode,
    pub name: String,
}

/// One immutable synchronous resource binding owned by a `for-of` iteration.
///
/// The private field and crate-private constructor keep patterns, multiple
/// bindings, mutable modes and async disposal out of this closed capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDisposableForOfHeadIr {
    binding_name: String,
}

impl SyncDisposableForOfHeadIr {
    pub(crate) fn new(binding_name: String) -> Self {
        Self { binding_name }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }
}

/// The activation-backed DisposeCapability for one plain-async-function
/// `for-of` `await using` head.
///
/// This capability is deliberately distinct from a lexical scope or
/// classic-for capability: its finalizer states are reused once per entered
/// iteration, while its exit state belongs to the loop as a whole.
#[must_use = "a plain-async for-of async DisposeCapability must be attached to its head"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncFunctionAsyncDisposableForOfCapabilityIr {
    binding_name: String,
    finalizer: AsyncDisposableFinalizerPlanIr,
}

impl AsyncFunctionAsyncDisposableForOfCapabilityIr {
    pub(crate) fn new(binding_name: String, finalizer: AsyncDisposableFinalizerPlanIr) -> Self {
        Self {
            binding_name,
            finalizer,
        }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }

    pub fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr {
        &self.finalizer
    }
}

/// One immutable async-disposable resource binding owned by every iteration
/// of a synchronous `for-of` iterator walk in a plain async function.
///
/// Private fields and the crate-private constructor make the generic sync
/// protocol, activation-backed iterator record, and repeating async finalizer
/// one indivisible producer obligation.
#[must_use = "an async-disposable for-of head must be attached to its iterator loop"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDisposableForOfHeadIr {
    binding_name: String,
    capability: AsyncFunctionAsyncDisposableForOfCapabilityIr,
    record: IteratorRecordIr,
}

impl AsyncDisposableForOfHeadIr {
    pub(crate) fn new(
        binding_name: String,
        capability: AsyncFunctionAsyncDisposableForOfCapabilityIr,
        record: IteratorRecordIr,
    ) -> Self {
        Self {
            binding_name,
            capability,
            record,
        }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }

    pub fn capability(&self) -> &AsyncFunctionAsyncDisposableForOfCapabilityIr {
        &self.capability
    }

    pub fn record(&self) -> &IteratorRecordIr {
        &self.record
    }
}

/// The exhaustive head domain of the generic iterator protocol path.
///
/// Both resource variants structurally select synchronous generic iteration:
/// only `Assignment` owns an optional async plan and explicit protocol witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForOfIteratorHeadIr {
    Assignment {
        binding: ForOfAssignmentIr,
        async_plan: Option<AsyncForOfIteratorPlanIr>,
        protocol: IteratorProtocolWitness,
    },
    SyncDisposable(SyncDisposableForOfHeadIr),
    AsyncDisposable(AsyncDisposableForOfHeadIr),
}

/// The runtime Environment Record lifecycle owned by a resumable loop.
///
/// This is required on every [`StatementIr::GeneratorLoop`] and derived by
/// [`AsyncFunctionForOfIteratorPlanIr`] so neither a new lowerer nor a backend
/// consumer can silently forget whether the loop needs a fresh record for each
/// iteration. See
/// `docs/rust-rewrite/contracts/resumable-loop-per-iteration-environment.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumableLoopIterationEnvironmentIr {
    /// The loop's carried bindings live entirely in their existing storage.
    StorageOnly,
    /// Allocate this environment once for every entered iteration and preserve
    /// the active record across the loop body's suspension.
    FreshPerIteration(LexicalEnvironmentIr),
}

#[path = "resumable_sync_for_of_binding.rs"]
mod resumable_sync_for_of_binding;
#[cfg(test)]
pub(crate) use resumable_sync_for_of_binding::ResumableSyncForOfBindingError;
pub use resumable_sync_for_of_binding::ResumableSyncForOfBindingStorageIr;
pub(crate) use resumable_sync_for_of_binding::{
    ValidatedResumableSyncForOfBindingIr, ValidatedResumableSyncForOfLexicalPatternIr,
};

/// The lowering-only description of the assignment performed for each value of
/// a resumable synchronous `for-of`.
///
/// [`AsyncFunctionForOfIteratorPlanIr::new`] consumes this input and derives
/// the public storage domain. No backend can select storage independently of
/// the source head shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AsyncFunctionForOfIteratorHeadIr {
    /// The iterator value is the source binding's value.
    Binding {
        source_name: String,
        binding: ForOfAssignmentIr,
    },
    /// The iterator value is consumed by an assignment prefix before the body
    /// can suspend.
    PreparedAssignment { value_name: String },
    /// The iterator value feeds one lexical BindingInitialization before the
    /// body can suspend.
    LexicalPattern {
        mode: BindingMode,
        value_name: String,
        iteration_storage_names: Vec<String>,
        tdz_placeholder_names: Vec<String>,
        initialization: Vec<StatementIr>,
    },
}

/// Where one resumable synchronous `for-of` stores IteratorValue.
///
/// The plan constructor derives this enum from
/// [`AsyncFunctionForOfIteratorHeadIr`]. `EntryLocal` is deliberately missing
/// a `BindingMode`: it is an unspellable compiler sink with fixed mutable,
/// dynamic local storage, not a source binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncFunctionForOfIteratorValueStorageIr {
    Activation(ForOfAssignmentIr),
    IterationEnvironment(ForOfAssignmentIr),
    EntryLocal { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AsyncFunctionForOfIteratorEnvironmentError {
    BindingCountOverflow {
        role: &'static str,
        binding_count: usize,
    },
    DuplicateBindingName {
        role: &'static str,
        name: String,
    },
    DuplicateSlot {
        role: &'static str,
        slot: u32,
        first_name: String,
        second_name: String,
    },
    SlotOutOfRange {
        role: &'static str,
        name: String,
        slot: u32,
        binding_count: u32,
    },
    BindingOutsideExpectedNames {
        role: &'static str,
        name: String,
        expected_names: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AsyncFunctionForOfIteratorInitializationError {
    Missing,
    UnsupportedStatement {
        index: usize,
    },
    MixedForms {
        first_form: &'static str,
        index: usize,
        form: &'static str,
    },
    MultipleDestructuringStatements {
        form: &'static str,
        second_index: usize,
    },
    ArrayAssignmentEvaluation {
        index: usize,
    },
    AssignmentTarget {
        index: usize,
    },
    ModeMismatch {
        name: String,
        expected_mode: BindingMode,
        actual_mode: BindingMode,
    },
    DuplicateBinding {
        name: String,
    },
    BindingNamesMismatch {
        expected_names: Vec<String>,
        actual_names: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AsyncFunctionForOfIteratorPlanError {
    AwaitedBindingHeadRequired,
    AwaitedEntryStateOverflow {
        entry_state: u32,
    },
    AwaitedProtocolStorageAlias {
        name: String,
    },
    InvalidBody(AsyncFunctionForOfBodyError),
    ExitStateOverflow {
        body_exit_state: u32,
    },
    BindingHeadEnvironmentRequired {
        mode: BindingMode,
        name: String,
    },
    VarBindingHasHeadEnvironment {
        name: String,
        tdz_placeholder_names: Vec<String>,
        iteration_storage_names: Vec<String>,
    },
    SingleBindingTdzNameCount {
        name: String,
        tdz_placeholder_names: Vec<String>,
    },
    SingleBindingTdzNameMismatch {
        source_name: String,
        expected_name: String,
        actual_name: String,
    },
    SingleBindingIterationNamesMismatch {
        name: String,
        iteration_storage_names: Vec<String>,
    },
    PreparedAssignmentHasHeadEnvironment {
        value_name: String,
        tdz_placeholder_names: Vec<String>,
        iteration_storage_names: Vec<String>,
    },
    LexicalPatternMode {
        mode: BindingMode,
    },
    LexicalPatternHeadEnvironmentRequired {
        iteration_storage_names: Vec<String>,
        tdz_placeholder_names: Vec<String>,
    },
    LexicalPatternNameCountMismatch {
        iteration_storage_names: Vec<String>,
        tdz_placeholder_names: Vec<String>,
    },
    DuplicateTdzPlaceholderName {
        origin: &'static str,
        name: String,
    },
    DuplicateLexicalPatternIterationStorageName {
        name: String,
    },
    LexicalPatternTdzNamesMismatch {
        expected_names: Vec<String>,
        actual_names: Vec<String>,
    },
    LexicalPatternIterationNamesMismatch {
        expected_names: Vec<String>,
        actual_names: Vec<String>,
    },
    EmptyLexicalPatternHasIterationEnvironment {
        actual_names: Vec<String>,
    },
    LexicalPatternValueNameCollision {
        value_name: String,
        iteration_storage_names: Vec<String>,
    },
    InvalidEnvironmentLayout(AsyncFunctionForOfIteratorEnvironmentError),
    InvalidLexicalPatternInitialization(AsyncFunctionForOfIteratorInitializationError),
    CapturedTdzEnvironment {
        tdz_placeholder_names: Vec<String>,
    },
}

/// Invalid suspension shape or continuation order in a direct await sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwaitSequenceError {
    FirstAwaitRequired,
    NestedSuspension,
    StateMismatch {
        expected_suspend_state: u32,
        suspend_state: u32,
        resume_state: u32,
    },
}

/// Validate a nonempty sequence of direct awaits separated by eager statements,
/// and return the final continuation state owned by the sequence.
/// Shared by loop lowering and async-generator emission preflight.
pub fn direct_await_sequence_resume_state(
    first: &StatementIr,
    after: &[StatementIr],
    entry_state: u32,
) -> Result<u32, AwaitSequenceError> {
    if !matches!(first, StatementIr::AsyncAwait { .. }) {
        return Err(AwaitSequenceError::FirstAwaitRequired);
    }
    let mut state = entry_state;
    for statement in std::iter::once(first).chain(after) {
        match statement {
            StatementIr::AsyncAwait {
                suspend_state,
                resume_state,
                ..
            } => {
                if *suspend_state != state || state.checked_add(1) != Some(*resume_state) {
                    return Err(AwaitSequenceError::StateMismatch {
                        expected_suspend_state: state,
                        suspend_state: *suspend_state,
                        resume_state: *resume_state,
                    });
                }
                state = *resume_state;
            }
            statement if statement_contains_suspension(statement) => {
                return Err(AwaitSequenceError::NestedSuspension);
            }
            _ => {}
        }
    }
    Ok(state)
}

fn duplicate_async_function_for_of_name(names: &[String]) -> Option<String> {
    let mut seen = BTreeSet::new();
    names
        .iter()
        .find(|name| !seen.insert(name.as_str()))
        .cloned()
}

fn async_function_for_of_names_match(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && left.iter().map(String::as_str).collect::<BTreeSet<_>>()
            == right.iter().map(String::as_str).collect::<BTreeSet<_>>()
}

fn async_function_for_of_environment_names(environment: &LexicalEnvironmentIr) -> Vec<String> {
    environment
        .bindings
        .iter()
        .map(|binding| binding.name.clone())
        .collect()
}

fn validate_async_function_for_of_environment(
    role: &'static str,
    environment: &LexicalEnvironmentIr,
    expected_names: &[String],
) -> Result<Vec<String>, AsyncFunctionForOfIteratorEnvironmentError> {
    let binding_count = u32::try_from(environment.bindings.len()).map_err(|_| {
        AsyncFunctionForOfIteratorEnvironmentError::BindingCountOverflow {
            role,
            binding_count: environment.bindings.len(),
        }
    })?;
    let expected_names = expected_names
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut binding_names = BTreeSet::new();
    let mut slots = BTreeMap::new();

    for binding in &environment.bindings {
        if !binding_names.insert(binding.name.as_str()) {
            return Err(
                AsyncFunctionForOfIteratorEnvironmentError::DuplicateBindingName {
                    role,
                    name: binding.name.clone(),
                },
            );
        }
        if binding.slot >= binding_count {
            return Err(AsyncFunctionForOfIteratorEnvironmentError::SlotOutOfRange {
                role,
                name: binding.name.clone(),
                slot: binding.slot,
                binding_count,
            });
        }
        if let Some(first_name) = slots.insert(binding.slot, binding.name.as_str()) {
            return Err(AsyncFunctionForOfIteratorEnvironmentError::DuplicateSlot {
                role,
                slot: binding.slot,
                first_name: first_name.to_string(),
                second_name: binding.name.clone(),
            });
        }
        if !expected_names.contains(binding.name.as_str()) {
            return Err(
                AsyncFunctionForOfIteratorEnvironmentError::BindingOutsideExpectedNames {
                    role,
                    name: binding.name.clone(),
                    expected_names: expected_names
                        .iter()
                        .map(|name| (*name).to_string())
                        .collect(),
                },
            );
        }
    }

    Ok(async_function_for_of_environment_names(environment))
}

fn collect_async_function_for_of_destructuring_target_bindings(
    target: &DestructuringTargetIr,
    bindings: &mut Vec<(BindingMode, String)>,
) -> Result<(), ()> {
    match target {
        DestructuringTargetIr::Binding { mode, name } => {
            bindings.push((*mode, name.clone()));
            Ok(())
        }
        DestructuringTargetIr::NestedArray(pattern) => {
            collect_async_function_for_of_array_bindings(pattern, bindings)
        }
        DestructuringTargetIr::NestedObject(pattern) => {
            collect_async_function_for_of_object_bindings(pattern, bindings)
        }
        DestructuringTargetIr::ResolvedVarBinding { name, .. } => {
            bindings.push((BindingMode::Var, name.clone()));
            Ok(())
        }
        DestructuringTargetIr::AssignmentIdentifier(..)
        | DestructuringTargetIr::AssignmentProperty { .. }
        | DestructuringTargetIr::AssignmentPrivate { .. }
        | DestructuringTargetIr::AssignmentSuper { .. } => Err(()),
    }
}

fn collect_async_function_for_of_array_bindings(
    pattern: &ArrayDestructuringPatternIr,
    bindings: &mut Vec<(BindingMode, String)>,
) -> Result<(), ()> {
    for element in &pattern.elements {
        let target = match element {
            ArrayDestructuringElementIr::Elision => continue,
            ArrayDestructuringElementIr::Target { target, .. }
            | ArrayDestructuringElementIr::Rest { target } => target,
        };
        collect_async_function_for_of_destructuring_target_bindings(target, bindings)?;
    }
    Ok(())
}

fn collect_async_function_for_of_object_bindings(
    pattern: &ObjectDestructuringPatternIr,
    bindings: &mut Vec<(BindingMode, String)>,
) -> Result<(), ()> {
    for property in &pattern.properties {
        collect_async_function_for_of_destructuring_target_bindings(&property.target, bindings)?;
    }
    if let Some(rest) = &pattern.rest {
        collect_async_function_for_of_destructuring_target_bindings(rest, bindings)?;
    }
    Ok(())
}

fn validate_async_function_for_of_initialization(
    expected_mode: BindingMode,
    expected_names: &[String],
    initialization: &[StatementIr],
) -> Result<(), AsyncFunctionForOfIteratorInitializationError> {
    if initialization.is_empty() {
        return Err(AsyncFunctionForOfIteratorInitializationError::Missing);
    }

    let mut first_form = None;
    let mut bindings = Vec::new();
    for (index, statement) in initialization.iter().enumerate() {
        let form = match statement {
            StatementIr::Lexical { mode, name, .. } => {
                bindings.push((*mode, name.clone()));
                "StatementIr::Lexical"
            }
            StatementIr::DeclarationEvaluation(TypedExpr {
                expr:
                    ExprIr::ArrayDestructure {
                        pattern,
                        evaluation,
                        ..
                    },
                ..
            }) => {
                match *evaluation {
                    ArrayDestructuringEvaluationIr::BindingInitialization => {}
                    ArrayDestructuringEvaluationIr::AssignmentEvaluation => {
                        return Err(
                            AsyncFunctionForOfIteratorInitializationError::ArrayAssignmentEvaluation {
                                index,
                            },
                        );
                    }
                }
                collect_async_function_for_of_array_bindings(pattern, &mut bindings).map_err(
                    |()| AsyncFunctionForOfIteratorInitializationError::AssignmentTarget { index },
                )?;
                "Array BindingInitialization"
            }
            StatementIr::DeclarationEvaluation(TypedExpr {
                expr: ExprIr::ObjectDestructure { pattern, .. },
                ..
            }) => {
                collect_async_function_for_of_object_bindings(pattern, &mut bindings).map_err(
                    |()| AsyncFunctionForOfIteratorInitializationError::AssignmentTarget { index },
                )?;
                "ExprIr::ObjectDestructure"
            }
            _ => {
                return Err(
                    AsyncFunctionForOfIteratorInitializationError::UnsupportedStatement { index },
                );
            }
        };

        match first_form {
            None => first_form = Some(form),
            Some(first_form) if first_form != form => {
                return Err(AsyncFunctionForOfIteratorInitializationError::MixedForms {
                    first_form,
                    index,
                    form,
                });
            }
            Some(_) if form != "StatementIr::Lexical" => {
                return Err(
                    AsyncFunctionForOfIteratorInitializationError::MultipleDestructuringStatements {
                        form,
                        second_index: index,
                    },
                );
            }
            Some(_) => {}
        }
    }

    let mut actual_names = Vec::with_capacity(bindings.len());
    let mut unique_names = BTreeSet::new();
    for (actual_mode, name) in bindings {
        if actual_mode != expected_mode {
            return Err(
                AsyncFunctionForOfIteratorInitializationError::ModeMismatch {
                    name,
                    expected_mode,
                    actual_mode,
                },
            );
        }
        if !unique_names.insert(name.clone()) {
            return Err(AsyncFunctionForOfIteratorInitializationError::DuplicateBinding { name });
        }
        actual_names.push(name);
    }
    if !async_function_for_of_names_match(expected_names, &actual_names) {
        return Err(
            AsyncFunctionForOfIteratorInitializationError::BindingNamesMismatch {
                expected_names: expected_names.to_vec(),
                actual_names,
            },
        );
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncForOfIteratorPlanIr {
    pub entry_state: u32,
    pub value_resume_state: u32,
    pub close_resume_state: u32,
    pub exit_state: u32,
    /// `{ [[Iterator]], [[NextMethod]], [[Done]] }` (7.4). These were three
    /// same-typed `String` fields; transposing `[[Iterator]]` and
    /// `[[NextMethod]]` type-checked and miscompiled every `for await`.
    pub record: IteratorRecordIr,
    /// Not an Iterator Record field: the runtime flag for whether the iterable
    /// supplied `@@asyncIterator` rather than a wrapped sync iterator.
    pub async_iterator_binding: String,
    /// Not an Iterator Record field: whether a rejection must run the close.
    pub close_on_rejection_binding: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedBindingIr {
    pub name: String,
    pub source_name: String,
    pub mode: BindingMode,
    pub slot: u32,
    pub hops: u32,
}

/// Compiler-private per-invocation state for a derived constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedConstructorActivationIr {
    pub owner_function_id: FunctionId,
    pub this_binding: String,
    pub this_status_binding: String,
    pub new_target_binding: String,
    pub active_function_binding: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionIr {
    pub template_source: Option<crate::TemplateSourceIr>,
    pub eval_environment: Option<crate::EvalEnvironmentRoleIr>,
    pub id: FunctionId,
    pub name: String,
    pub to_string_representation: CallableToStringRepresentation,
    pub protocol: FunctionProtocolIr,
    pub generator_plan: Option<GeneratorPlanIr>,
    pub resumable_plan: Option<ResumablePlanIr>,
    pub strict: bool,
    pub class_element_execution_kind: ClassElementExecutionKind,
    pub class_heritage_kind: ClassHeritageKind,
    pub is_static_class_member: bool,
    pub is_derived_constructor: bool,
    pub is_synthetic_default_derived_constructor: bool,
    pub class_instance_element_plan: Option<ClassInstanceElementPlanIr>,
    pub super_constructor_target: Option<FunctionId>,
    pub uses_super: bool,
    pub this_before_super: bool,
    pub lexical_derived_activation: Option<DerivedConstructorActivationIr>,
    pub private_name_ids: BTreeMap<String, PrivateNameId>,
    /// Keeps the lexical private environment available to this function and
    /// any nested function values it creates at runtime.
    pub captures_private_environment: bool,
    pub is_nested: bool,
    pub is_expression: bool,
    pub is_named_expression: bool,
    pub captures_lexical_this: bool,
    pub captures_lexical_arguments: bool,
    pub params: Vec<FunctionParamIr>,
    pub body: BlockIr,
    pub return_kind: ValueKind,
    pub return_shape: Option<Box<HeapShape>>,
    pub return_targets: FunctionTargetKnowledge,
    pub constructor_instance: ValueInfo,
    pub owned_env_bindings: Vec<OwnedEnvBindingIr>,
    pub captured_bindings: Vec<CapturedBindingIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementIr {
    Empty,
    ModuleImportBinding(crate::modules::ModuleImportBindingIr),
    ResumableClassDefinition(Box<ResumableClassDefinitionIr>),
    /// Runs module `module`'s hoist or body block exactly once.
    ///
    /// Re-entry is a no-op, which is what makes a cyclic graph and a repeated
    /// `import()` of the same specifier behave.
    ModuleUnitOnce {
        module: ModuleUnitId,
        block: Box<BlockIr>,
    },
    Lexical {
        mode: BindingMode,
        name: String,
        init: TypedExpr,
    },
    AnnexBFunctionCopy {
        source_name: String,
        block_storage_name: String,
        target: AnnexBFunctionCopyTargetIr,
        admission: Option<OwnedEnvBindingIr>,
    },
    LexicalBlock(Vec<StatementIr>),
    /// The actual source declaration or empty statement owns this complete
    /// lowered item, including staging that must not publish a temporary value.
    EmptyStatementCompletion(Box<EmptyStatementCompletionIr>),
    /// A synchronous DisposeCapability with an explicit execution owner.
    ///
    /// `resources` is non-empty and in declaration order. Each entry owns its
    /// binding initialization; the backend acquires and registers its dispose
    /// method before initializing `binding_name`, then evaluates `body` and
    /// disposes every registered entry in reverse on every completion.
    SyncDisposableScope {
        execution: SyncDisposableScopeExecutionIr,
        resources: SyncDisposableResourcesIr,
        body: BlockIr,
    },
    /// An async DisposeCapability with an explicit resumable execution owner.
    ///
    /// This is deliberately distinct from `SyncDisposableScope`: each
    /// registered resource follows the async-dispose protocol and the required
    /// finalizer plan suspends before the saved completion may leave the scope.
    AsyncDisposableScope {
        execution: AsyncDisposableScopeExecutionIr,
        resources: AsyncDisposableResourcesIr,
        body: BlockIr,
    },
    ParameterInitialization {
        parameter_index: usize,
        statements: Vec<StatementIr>,
    },
    Var(Vec<VarDeclaratorIr>),
    /// Evaluates declaration initialization effects with an empty normal
    /// completion, preserving the preceding StatementList value.
    DeclarationEvaluation(TypedExpr),
    Expression(TypedExpr),
    GeneratorYield {
        value: TypedExpr,
        form: YieldForm,
        suspend_state: u32,
        resume_state: u32,
        resume_mode: GeneratorResumeModeIr,
    },
    /// Private Allocate/Instantiate boundary; it neither creates jobs nor settles the body promise.
    AsyncModuleInstantiation,
    AsyncAwait {
        value: TypedExpr,
        suspend_state: u32,
        resume_state: u32,
        resume_mode: AsyncResumeModeIr,
    },
    /// Complete phase/control owner for an ordinary synchronous generator.
    OrdinaryGeneratorLoop(Box<OrdinaryGeneratorLoopIr>),
    /// Complete mixed Await/Yield classic-loop phases.
    AsyncGeneratorLoop(Box<AsyncGeneratorLoopIr>),
    /// Complete branch ranges within an ordinary-generator loop region.
    OrdinaryGeneratorIf(Box<OrdinaryGeneratorIfIr>),
    /// Complete mixed condition and selected branches.
    AsyncGeneratorIf(Box<AsyncGeneratorIfIr>),
    /// Complete CaseBlock selection, fallthrough and persistent completion owner.
    OrdinaryGeneratorSwitch(Box<OrdinaryGeneratorSwitchIr>),
    /// Complete mixed Await/Yield selection and CaseBlock lifetime.
    AsyncGeneratorSwitch(Box<crate::AsyncGeneratorSwitchIr>),
    OrdinaryGeneratorArrayDestructuring(Box<OrdinaryGeneratorArrayDestructuringIr>),
    /// Mixed pattern body with its synchronous IteratorClose owner.
    AsyncGeneratorArrayDestructuring(Box<crate::AsyncGeneratorArrayDestructuringIr>),
    /// Plain Async pattern body with its actual synchronous IteratorClose owner.
    AsyncFunctionArrayDestructuring(Box<crate::AsyncFunctionArrayDestructuringIr>),
    OrdinaryGeneratorWith(Box<crate::OrdinaryGeneratorWithIr>),
    /// Complete mixed head/body ranges with the original Object Environment.
    AsyncGeneratorWith(Box<crate::AsyncGeneratorWithIr>),
    AsyncFunctionWith(Box<crate::AsyncFunctionWithIr>),
    /// Complete mixed head/enumeration/body with the original per-key initializer.
    AsyncGeneratorForIn(Box<crate::AsyncGeneratorForInIr>),
    /// Complete synchronous or awaited iterator head, initializer and body.
    AsyncGeneratorForOf(Box<crate::AsyncGeneratorForOfIr>),
    /// A complete lexical resource scope and its checked registration operations.
    AsyncGeneratorResourceScope(Box<crate::AsyncGeneratorResourceScopeIr>),
    AsyncGeneratorResourceRegistration(Box<crate::AsyncGeneratorResourceRegistrationIr>),
    ArrayDestructuringOperation(Box<crate::ArrayDestructuringOperationIr>),
    /// Existing await-loop execution, consumed only by async owners.
    GeneratorLoop {
        init: Option<ForInitIr>,
        test: Option<TypedExpr>,
        update: Option<TypedExpr>,
        iteration_environment: ResumableLoopIterationEnvironmentIr,
        before_suspension: Vec<StatementIr>,
        suspension_statement: Box<StatementIr>,
        after_suspension: Vec<StatementIr>,
        entry_state: u32,
        /// The final resume state in the iteration's suspension sequence.
        resume_state: u32,
        exit_state: u32,
    },
    GeneratorIf {
        condition: TypedExpr,
        then_before_yield: Vec<StatementIr>,
        then_yield_statement: Option<Box<StatementIr>>,
        then_after_yield: Vec<StatementIr>,
        else_before_yield: Vec<StatementIr>,
        else_yield_statement: Option<Box<StatementIr>>,
        else_after_yield: Vec<StatementIr>,
        entry_state: u32,
        then_resume_state: Option<u32>,
        else_resume_state: Option<u32>,
        exit_state: u32,
    },
    Block(BlockIr),
    If {
        condition: TypedExpr,
        then_branch: Box<StatementIr>,
        else_branch: Option<Box<StatementIr>>,
    },
    AsyncFunctionIf {
        condition: TypedExpr,
        then_branch: Box<StatementIr>,
        else_branch: Option<Box<StatementIr>>,
        plan: AsyncFunctionIfPlanIr,
    },
    AsyncFunctionWhile(AsyncFunctionWhileConditionIr),
    AsyncFunctionSwitch(AsyncFunctionSwitchIr),
    While {
        condition: TypedExpr,
        body: Box<StatementIr>,
    },
    DoWhile {
        body: Box<StatementIr>,
        condition: TypedExpr,
    },
    For {
        init: Option<ForInitIr>,
        test: Option<TypedExpr>,
        update: Option<TypedExpr>,
        body: Box<StatementIr>,
        lexical_environment: Option<ForLexicalEnvironmentIr>,
    },
    ForOfIterator {
        head: ForOfIteratorHeadIr,
        iterable: TypedExpr,
        body: Box<StatementIr>,
        lexical_environment: Option<ForInOfEnvironmentIr>,
    },
    AsyncFunctionForOfIterator {
        iterable: TypedExpr,
        plan: AsyncFunctionForOfIteratorPlanIr,
    },
    GeneratorForOfIterator {
        iterable: TypedExpr,
        plan: GeneratorForOfIteratorPlanIr,
    },
    ForInArray {
        mode: BindingMode,
        name: String,
        target: TypedExpr,
        body: Box<StatementIr>,
        lexical_environment: Option<ForInOfEnvironmentIr>,
    },
    ForInString {
        mode: BindingMode,
        name: String,
        target: TypedExpr,
        body: Box<StatementIr>,
        lexical_environment: Option<ForInOfEnvironmentIr>,
    },
    ForInObject {
        mode: BindingMode,
        name: String,
        target: TypedExpr,
        body: Box<StatementIr>,
        lexical_environment: Option<ForInOfEnvironmentIr>,
    },
    Switch {
        discriminant: TypedExpr,
        lexical_environment: Option<LexicalEnvironmentIr>,
        lexical_declarations: Vec<StatementIr>,
        cases: Vec<SwitchCaseIr>,
    },
    Labelled {
        labels: Vec<String>,
        statement: Box<StatementIr>,
        async_plan: Option<AsyncFunctionLabelledPlanIr>,
    },
    Debugger,
    Throw(TypedExpr),
    TryCatch {
        try_block: BlockIr,
        catch_name: String,
        catch_source_name: String,
        catch_parameter_environment: Option<LexicalEnvironmentIr>,
        catch_block: BlockIr,
        generator_plan: Option<GeneratorTryPlanIr>,
        async_plan: Option<AsyncTryPlanIr>,
    },
    TryFinally {
        try_block: BlockIr,
        finally_block: BlockIr,
        generator_plan: Option<GeneratorTryPlanIr>,
        async_plan: Option<AsyncTryPlanIr>,
    },
    TryCatchFinally {
        try_block: BlockIr,
        catch_name: String,
        catch_source_name: String,
        catch_parameter_environment: Option<LexicalEnvironmentIr>,
        catch_block: BlockIr,
        finally_block: BlockIr,
        generator_plan: Option<GeneratorTryPlanIr>,
        async_plan: Option<AsyncTryPlanIr>,
    },
    Return(TypedExpr),
    Break {
        label: Option<String>,
    },
    Continue {
        label: Option<String>,
    },
}

/// One declarator registered by [`StatementIr::SyncDisposableScope`].
///
/// There is deliberately no disposal-kind flag: this closed node only accepts
/// synchronous `using`. `binding_name` is the immutable lexical binding's IR
/// storage name, and this entry is its sole runtime initialization owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDisposableResourceIr {
    pub binding_name: String,
    pub initializer: TypedExpr,
}

/// Where a synchronous DisposeCapability must remain live.
///
/// The owner is required rather than inferred by backend context: adding a new
/// lifetime requires an exhaustive producer and consumer decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncDisposableScopeExecutionIr {
    /// The scope never suspends, so backend-private locals own its capability.
    Immediate,
    /// The scope may yield and therefore owns activation-backed capability
    /// storage rather than temporary locals.
    PlainGenerator(PlainGeneratorSyncDisposableCapabilityIr),
    /// The scope may await and therefore owns activation-backed capability
    /// storage until the async function settles.
    AsyncFunction(AsyncFunctionSyncDisposableCapabilityIr),
    /// The scope may yield or await and therefore owns activation-backed
    /// capability storage until the async generator completes.
    AsyncGenerator(AsyncGeneratorSyncDisposableCapabilityIr),
}

/// The hidden activation binding for one plain-generator DisposeCapability.
///
/// Fields are private and the sole crate constructor is fed only by lowering's
/// suspension-owned binding allocator. Backend crates can consume the binding
/// identity but cannot manufacture this proof from an arbitrary `String`.
#[must_use = "a plain-generator synchronous DisposeCapability must be attached to its scope"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainGeneratorSyncDisposableCapabilityIr {
    binding_name: String,
}

impl PlainGeneratorSyncDisposableCapabilityIr {
    pub(crate) fn new(binding_name: String) -> Self {
        Self { binding_name }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }
}

/// The hidden activation binding for one plain-async-function
/// DisposeCapability.
///
/// Fields are private and the sole crate constructor is fed only by lowering's
/// suspension-owned binding allocator. Backend crates can consume the binding
/// identity but cannot manufacture this proof from an arbitrary `String`.
#[must_use = "a plain-async-function synchronous DisposeCapability must be attached to its scope"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncFunctionSyncDisposableCapabilityIr {
    binding_name: String,
}

impl AsyncFunctionSyncDisposableCapabilityIr {
    pub(crate) fn new(binding_name: String) -> Self {
        Self { binding_name }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }
}

/// The hidden activation binding for one async-generator DisposeCapability.
///
/// Fields are private and the sole crate constructor is fed only by lowering's
/// suspension-owned binding allocator. Backend crates can consume the binding
/// identity but cannot manufacture this proof from an arbitrary `String`.
#[must_use = "an async-generator synchronous DisposeCapability must be attached to its scope"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncGeneratorSyncDisposableCapabilityIr {
    binding_name: String,
}

impl AsyncGeneratorSyncDisposableCapabilityIr {
    pub(crate) fn new(binding_name: String) -> Self {
        Self { binding_name }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }
}

/// A declaration-ordered, statically non-empty synchronous resource list.
///
/// Fields stay private so a backend consumer can inspect but cannot mint an
/// empty DisposeCapability. Lowering is the sole constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncDisposableResourcesIr {
    first: SyncDisposableResourceIr,
    rest: Vec<SyncDisposableResourceIr>,
}

/// One declarator registered by [`StatementIr::AsyncDisposableScope`].
///
/// Fields are private so only lowering can transfer binding initialization to
/// the async resource protocol. The backend can inspect the entry but cannot
/// manufacture one from a generic lexical initializer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDisposableResourceIr {
    binding_name: String,
    initializer: TypedExpr,
}

impl AsyncDisposableResourceIr {
    pub(crate) fn new(binding_name: String, initializer: TypedExpr) -> Self {
        Self {
            binding_name,
            initializer,
        }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }

    pub fn initializer(&self) -> &TypedExpr {
        &self.initializer
    }
}

/// A declaration-ordered, statically non-empty async-dispose resource list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDisposableResourcesIr {
    first: AsyncDisposableResourceIr,
    rest: Vec<AsyncDisposableResourceIr>,
}

/// The complete async-dispose ownership proof for one classic-for initializer.
///
/// Private fields prevent a backend from pairing resources with a different
/// execution owner or manufacturing an unfinished finalizer. Lowering is the
/// sole constructor and can call it only after the complete loop region has
/// allocated its source suspension states.
#[must_use = "an async-disposable classic-for initializer must be attached to its loop"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDisposableForInitIr {
    capability: AsyncFunctionAsyncDisposableCapabilityIr,
    resources: AsyncDisposableResourcesIr,
}

impl AsyncDisposableForInitIr {
    pub(crate) fn new(
        capability: AsyncFunctionAsyncDisposableCapabilityIr,
        resources: AsyncDisposableResourcesIr,
    ) -> Self {
        Self {
            capability,
            resources,
        }
    }

    pub fn capability(&self) -> &AsyncFunctionAsyncDisposableCapabilityIr {
        &self.capability
    }

    pub fn resources(&self) -> &AsyncDisposableResourcesIr {
        &self.resources
    }
}

impl AsyncDisposableResourcesIr {
    pub(crate) fn new(
        first: AsyncDisposableResourceIr,
        rest: Vec<AsyncDisposableResourceIr>,
    ) -> Self {
        Self { first, rest }
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &AsyncDisposableResourceIr> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    pub fn len(&self) -> usize {
        1 + self.rest.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

/// The closed async state transition required to finalize one lexical
/// async-dispose capability.
#[must_use = "an async-dispose finalizer plan must be attached to its capability"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDisposableFinalizerPlanIr {
    entry_state: u32,
    dispose_state: u32,
    resume_state: u32,
    exit_state: u32,
}

impl AsyncDisposableFinalizerPlanIr {
    /// The disposal walk, its Await resume, and its exit continuation each own
    /// one state beyond the scope's current entry state.
    pub(crate) const IMPLICIT_STATE_COUNT: u32 = 3;

    pub(crate) fn after_source_suffix(entry_state: u32, suffix_end: u32) -> Option<Self> {
        if entry_state > suffix_end {
            return None;
        }
        let dispose_state = suffix_end.checked_add(1)?;
        let resume_state = dispose_state.checked_add(1)?;
        let exit_state = suffix_end.checked_add(Self::IMPLICIT_STATE_COUNT)?;
        Some(Self::new(
            entry_state,
            dispose_state,
            resume_state,
            exit_state,
        ))
    }

    pub(crate) fn new(
        entry_state: u32,
        dispose_state: u32,
        resume_state: u32,
        exit_state: u32,
    ) -> Self {
        assert!(
            entry_state < dispose_state
                && dispose_state < resume_state
                && resume_state < exit_state,
            "async-dispose finalizer states must be strictly ordered"
        );
        Self {
            entry_state,
            dispose_state,
            resume_state,
            exit_state,
        }
    }

    pub fn entry_state(&self) -> u32 {
        self.entry_state
    }

    pub fn dispose_state(&self) -> u32 {
        self.dispose_state
    }

    pub fn resume_state(&self) -> u32 {
        self.resume_state
    }

    pub fn exit_state(&self) -> u32 {
        self.exit_state
    }
}

/// The activation-backed capability for one plain-async-function `await using`
/// scope.
#[must_use = "a plain-async-function async DisposeCapability must be attached to its scope"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncFunctionAsyncDisposableCapabilityIr {
    binding_name: String,
    finalizer: AsyncDisposableFinalizerPlanIr,
}

/// Where an asynchronous DisposeCapability must remain live.
///
/// The required owner proof keeps activation layout and completion routing an
/// exhaustive backend decision. Neither capability can be manufactured by a
/// backend from an arbitrary binding name.
#[must_use = "an async DisposeCapability execution owner must be attached to its scope"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncDisposableScopeExecutionIr {
    AsyncFunction(AsyncFunctionAsyncDisposableCapabilityIr),
    AsyncGenerator(AsyncGeneratorAsyncDisposableCapabilityIr),
}

/// The activation-backed capability for one async-generator `await using`
/// scope.
///
/// Fields are private and the sole crate constructor is fed only by lowering's
/// suspension-owned binding allocator. Backend crates can consume the binding
/// identity and finalizer roles but cannot manufacture this proof.
#[must_use = "an async-generator async DisposeCapability must be attached to its scope"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncGeneratorAsyncDisposableCapabilityIr {
    binding_name: String,
    finalizer: AsyncDisposableFinalizerPlanIr,
}

impl AsyncGeneratorAsyncDisposableCapabilityIr {
    pub(crate) fn new(binding_name: String, finalizer: AsyncDisposableFinalizerPlanIr) -> Self {
        Self {
            binding_name,
            finalizer,
        }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }

    pub fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr {
        &self.finalizer
    }
}

impl AsyncFunctionAsyncDisposableCapabilityIr {
    pub(crate) fn new(binding_name: String, finalizer: AsyncDisposableFinalizerPlanIr) -> Self {
        Self {
            binding_name,
            finalizer,
        }
    }

    pub fn binding_name(&self) -> &str {
        &self.binding_name
    }

    pub fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr {
        &self.finalizer
    }
}

impl SyncDisposableResourcesIr {
    pub(crate) fn new(
        first: SyncDisposableResourceIr,
        rest: Vec<SyncDisposableResourceIr>,
    ) -> Self {
        Self { first, rest }
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &SyncDisposableResourceIr> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    pub fn len(&self) -> usize {
        1 + self.rest.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnexBFunctionCopyTargetIr {
    OwnerBinding { storage_name: String },
    ScriptGlobal { name: String },
    DirectEvalVariable { name: String },
    FunctionVariableEnvironment { name: String },
}

impl StatementIr {
    /// The Completion Record (6.2.4) this statement itself produces, if any.
    ///
    /// Exhaustive with **no catch-all**: the previous `_ => None` absorbed 29
    /// of 33 variants, so "I added a statement that is an abrupt completion and
    /// forgot to say so" answered *not abrupt* silently. It is now `E0004`.
    pub fn abrupt_completion_record(&self) -> Option<CompletionRecordIr<TypedExpr>> {
        match self {
            Self::Throw(value) => Some(CompletionRecordIr::throw(value.clone())),
            Self::Return(value) => Some(CompletionRecordIr::return_(value.clone())),
            Self::Break { label } => Some(CompletionRecordIr::break_(None, label.clone())),
            Self::Continue { label } => Some(CompletionRecordIr::continue_(None, label.clone())),
            Self::Empty
            | Self::ResumableClassDefinition(_)
            | Self::ModuleUnitOnce { .. }
            | Self::ModuleImportBinding(_)
            | Self::Lexical { .. }
            | Self::AnnexBFunctionCopy { .. }
            | Self::LexicalBlock(_)
            | Self::EmptyStatementCompletion(_)
            | Self::SyncDisposableScope { .. }
            | Self::AsyncDisposableScope { .. }
            | Self::ParameterInitialization { .. }
            | Self::Var(_)
            | Self::DeclarationEvaluation(_)
            | Self::Expression(_)
            | Self::GeneratorYield { .. }
            | Self::AsyncModuleInstantiation
            | Self::AsyncAwait { .. }
            | Self::GeneratorLoop { .. }
            | Self::AsyncGeneratorLoop(_)
            | Self::AsyncGeneratorIf(_)
            | Self::AsyncGeneratorWith(_)
            | Self::AsyncGeneratorSwitch(_)
            | Self::AsyncGeneratorArrayDestructuring(_)
            | Self::AsyncGeneratorResourceScope(_)
            | Self::AsyncGeneratorResourceRegistration(_)
            | Self::AsyncGeneratorForOf(_)
            | Self::AsyncGeneratorForIn(_)
            | Self::OrdinaryGeneratorLoop(_)
            | Self::OrdinaryGeneratorIf(_)
            | Self::OrdinaryGeneratorSwitch(_)
            | Self::OrdinaryGeneratorArrayDestructuring(_)
            | Self::AsyncFunctionArrayDestructuring(_)
            | Self::AsyncFunctionWith(_)
            | Self::OrdinaryGeneratorWith(_)
            | Self::ArrayDestructuringOperation(_)
            | Self::GeneratorIf { .. }
            | Self::Block(_)
            | Self::If { .. }
            | Self::AsyncFunctionIf { .. }
            | Self::AsyncFunctionWhile(_)
            | Self::AsyncFunctionSwitch(_)
            | Self::While { .. }
            | Self::DoWhile { .. }
            | Self::For { .. }
            | Self::ForOfIterator { .. }
            | Self::AsyncFunctionForOfIterator { .. }
            | Self::GeneratorForOfIterator { .. }
            | Self::ForInArray { .. }
            | Self::ForInString { .. }
            | Self::ForInObject { .. }
            | Self::Switch { .. }
            | Self::Labelled { .. }
            | Self::Debugger
            | Self::TryCatch { .. }
            | Self::TryFinally { .. }
            | Self::TryCatchFinally { .. } => None,
        }
    }

    pub fn is_abrupt_completion_statement(&self) -> bool {
        self.abrupt_completion_record().is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIr {
    pub statements: Vec<StatementIr>,
    pub result_kind: ValueKind,
    pub lexical_environment: Option<LexicalEnvironmentIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalPropertyInitializerIr {
    Intrinsic,
    Infinity,
    NaN,
    Undefined,
    FreshUndefined,
    SourceFunction(FunctionId),
    ReflectObject,
    MathObject,
    JsonObject,
    AtomicsObject,
    TemporalObject,
    IntlObject,
    BuiltinFunction(StandardBuiltinId),
    HostFunction(HostBuiltinId),
}

impl GlobalPropertyInitializerIr {
    pub const fn writable(&self) -> bool {
        !matches!(self, Self::Infinity | Self::NaN | Self::Undefined)
    }

    pub const fn enumerable(&self) -> bool {
        matches!(self, Self::FreshUndefined | Self::SourceFunction(_))
    }

    pub const fn configurable(&self) -> bool {
        !matches!(
            self,
            Self::Infinity
                | Self::NaN
                | Self::Undefined
                | Self::FreshUndefined
                | Self::SourceFunction(_)
        )
    }

    pub const fn permits_global_function_declaration(&self) -> bool {
        self.configurable() || (self.writable() && self.enumerable())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalDeclarationSetIr {
    None,
    Var,
    Function,
    FunctionAndVar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum GlobalFunctionDeclarationDispositionIr {
    Installed,
    RestrictedExistingProperty,
}

impl GlobalDeclarationSetIr {
    pub const fn with_var(self) -> Self {
        match self {
            Self::None | Self::Var => Self::Var,
            Self::Function | Self::FunctionAndVar => Self::FunctionAndVar,
        }
    }

    pub const fn with_function(self) -> Self {
        match self {
            Self::None | Self::Function => Self::Function,
            Self::Var | Self::FunctionAndVar => Self::FunctionAndVar,
        }
    }

    pub const fn is_declared(self) -> bool {
        !matches!(self, Self::None)
    }

    pub const fn needs_main_frame_write_storage(self) -> bool {
        matches!(self, Self::Var | Self::FunctionAndVar)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptGlobalBindingIr {
    pub name: String,
    pub initializer: GlobalPropertyInitializerIr,
    pub declarations: GlobalDeclarationSetIr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalLexicalBindingModeIr {
    Mutable,
    Immutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalBindingPlan {
    object_bindings: BTreeMap<String, ScriptGlobalBindingIr>,
    lexical_bindings: BTreeMap<String, GlobalLexicalBindingModeIr>,
}

impl GlobalBindingPlan {
    pub fn from_parts(
        object_bindings: impl IntoIterator<Item = ScriptGlobalBindingIr>,
        lexical_bindings: impl IntoIterator<Item = (String, GlobalLexicalBindingModeIr)>,
    ) -> Self {
        let mut plan = Self {
            object_bindings: BTreeMap::new(),
            lexical_bindings: lexical_bindings.into_iter().collect(),
        };
        for binding in object_bindings {
            plan.insert_initial(binding);
        }
        plan
    }

    pub fn insert_initial(&mut self, binding: ScriptGlobalBindingIr) {
        let previous = self.object_bindings.insert(binding.name.clone(), binding);
        assert!(
            previous.is_none(),
            "global binding plan names must be unique"
        );
    }

    pub(crate) fn require_host_global(&mut self, builtin: HostBuiltinId) {
        let Some(name) = builtin.global_name() else {
            return;
        };
        match self.object_bindings.get_mut(name) {
            Some(binding) => {
                // Prepared sources are compiled after entry declarations. A bare
                // var reuses the host property; a source function overrides it.
                if binding.initializer == GlobalPropertyInitializerIr::FreshUndefined {
                    binding.initializer = GlobalPropertyInitializerIr::HostFunction(builtin);
                }
            }
            None => self.insert_initial(ScriptGlobalBindingIr {
                name: name.to_owned(),
                initializer: GlobalPropertyInitializerIr::HostFunction(builtin),
                declarations: GlobalDeclarationSetIr::None,
            }),
        }
    }

    pub fn record_var(&mut self, name: String) {
        assert!(
            !self.lexical_bindings.contains_key(&name),
            "a global var name cannot collide with a global lexical name"
        );
        match self.object_bindings.get_mut(&name) {
            Some(binding) => binding.declarations = binding.declarations.with_var(),
            None => self.insert_initial(ScriptGlobalBindingIr {
                name,
                initializer: GlobalPropertyInitializerIr::FreshUndefined,
                declarations: GlobalDeclarationSetIr::Var,
            }),
        }
    }

    pub fn record_annex_b_var(&mut self, name: String) {
        self.record_var(name);
    }

    pub fn record_function(
        &mut self,
        name: String,
        function: FunctionId,
    ) -> GlobalFunctionDeclarationDispositionIr {
        assert!(
            !self.lexical_bindings.contains_key(&name),
            "a global function name cannot collide with a global lexical name"
        );
        let (initializer, declarations, disposition) = match self.object_bindings.get(&name) {
            Some(binding) if !binding.initializer.permits_global_function_declaration() => (
                binding.initializer.clone(),
                binding.declarations.with_function(),
                GlobalFunctionDeclarationDispositionIr::RestrictedExistingProperty,
            ),
            Some(binding) => (
                GlobalPropertyInitializerIr::SourceFunction(function),
                binding.declarations.with_function(),
                GlobalFunctionDeclarationDispositionIr::Installed,
            ),
            None => (
                GlobalPropertyInitializerIr::SourceFunction(function),
                GlobalDeclarationSetIr::Function,
                GlobalFunctionDeclarationDispositionIr::Installed,
            ),
        };
        self.object_bindings.insert(
            name.clone(),
            ScriptGlobalBindingIr {
                name,
                initializer,
                declarations,
            },
        );
        disposition
    }

    pub fn get(&self, name: &str) -> Option<&ScriptGlobalBindingIr> {
        self.object_bindings.get(name)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ScriptGlobalBindingIr> {
        self.object_bindings.values()
    }

    /// Script-global bindings with frame storage for declaration writes.
    ///
    /// Publishers write these slots before mirroring them to the global object.
    /// Source reads use the global object and need no eager property reads.
    pub fn main_frame_write_bindings(&self) -> impl Iterator<Item = &ScriptGlobalBindingIr> {
        self.object_bindings
            .values()
            .filter(|binding| binding.declarations.needs_main_frame_write_storage())
    }

    pub fn len(&self) -> usize {
        self.object_bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.object_bindings.is_empty()
    }

    pub fn lexical_names(&self) -> impl ExactSizeIterator<Item = &String> {
        self.lexical_bindings.keys()
    }

    /// Even an otherwise scalar Script must execute the restricted-property
    /// admission check and construct its intrinsic SyntaxError through R.
    pub fn has_restricted_lexical_declarations(&self) -> bool {
        self.lexical_names().any(|name| {
            self.get(name)
                .is_some_and(|binding| !binding.initializer.configurable())
        })
    }

    pub fn lexical_bindings(&self) -> &BTreeMap<String, GlobalLexicalBindingModeIr> {
        &self.lexical_bindings
    }
}

impl<'a> IntoIterator for &'a GlobalBindingPlan {
    type Item = &'a ScriptGlobalBindingIr;
    type IntoIter = std::collections::btree_map::Values<'a, String, ScriptGlobalBindingIr>;

    fn into_iter(self) -> Self::IntoIter {
        self.object_bindings.values()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptIr {
    /// Exact linked capabilities; unused compilation declarations are omitted.
    pub foreign_modules: crate::foreign::ForeignModuleRegistry,
    /// Exact compile-time language semantics retained by this P and its prepared code.
    pub language_features: crate::LanguageFeaturePolicy,
    pub template_source: Option<crate::TemplateSourceIr>,
    pub eval_environment: Option<crate::EvalEnvironmentRoleIr>,
    /// A separate global Script evaluated before the Script or Module entry.
    pub module_prelude: Option<PreparedScriptUnit>,
    /// Independently instantiated global Script after the earlier global prelude.
    pub script_entry_after_prelude: Option<crate::GlobalScriptEntryIr>,
    pub prepared_scripts: Vec<PreparedScript>,
    pub runtime_declarations: RuntimeGlobalDeclarationPlan,
    pub prepared_dynamic_functions: Vec<PreparedDynamicFunction>,
    pub strict: bool,
    pub functions: Vec<FunctionIr>,
    pub body: BlockIr,
    pub owned_env_bindings: Vec<OwnedEnvBindingIr>,
    pub global_bindings: GlobalBindingPlan,
    pub host_builtins: Vec<HostBuiltinId>,
    pub builtin_ctor_calls: usize,
    pub builtin_static_calls: usize,
    pub error_builtin_calls: usize,
    pub aggregate_errors: usize,
    pub function_proto_calls: usize,
    pub function_proto_applies: usize,
    pub function_proto_binds: usize,
    pub function_proto_to_strings: usize,
    pub bound_functions: usize,
    pub bound_function_constructs: usize,
    pub boxed_builtin_calls: usize,
    pub boxed_builtin_constructs: usize,
    pub boxed_receiver_adaptations: usize,
    pub top_level_this_uses: usize,
    pub host_builtin_calls: usize,
    pub error_proto_to_strings: usize,
}

impl ScriptIr {
    pub fn module_entry_evaluation(&self) -> Option<&ModuleEntryEvaluationIr> {
        ModuleEntryEvaluationIr::in_root_block(&self.body)
    }

    pub fn prepared_script_units(&self) -> impl Iterator<Item = &PreparedScriptUnit> {
        self.module_prelude
            .iter()
            .chain(
                self.script_entry_after_prelude
                    .iter()
                    .map(|entry| entry.unit()),
            )
            .chain(
                self.prepared_scripts
                    .iter()
                    .filter_map(|prepared| match &prepared.outcome {
                        PreparedScriptOutcome::Executable(unit) => Some(unit),
                        PreparedScriptOutcome::DeferredSyntaxError { .. } => None,
                    }),
            )
    }

    pub fn executable_script_bodies(&self) -> impl Iterator<Item = &BlockIr> {
        std::iter::once(&self.body).chain(self.prepared_script_units().map(|unit| &unit.body))
    }

    /// Number exponentiation emitted by the executable IR, including the
    /// Number arm of coercive operators. A typed BigInt-only binary operation
    /// uses its separate arithmetic helper.
    /// Whether any module body of the program declares an import binding:
    /// only those cells are ever linked to another module's cell.
    pub fn declares_module_import_bindings(&self) -> bool {
        let mut counts = IrSummaryCounts::default();
        for body in self.executable_script_bodies() {
            counts.visit_block(body);
        }
        for function in &self.functions {
            counts.visit_function(function);
        }
        counts.module_import_bindings != 0
    }

    pub fn has_scalar_exponentiation(&self) -> bool {
        let mut counts = IrSummaryCounts {
            scalar_exponentiation: Some(false),
            ..Default::default()
        };
        for body in self.executable_script_bodies() {
            counts.visit_block(body);
        }
        for function in &self.functions {
            counts.visit_function(function);
        }
        counts.scalar_exponentiation == Some(true)
    }

    pub const fn result_kind(&self) -> ValueKind {
        self.body.result_kind
    }
}

/// Closed compiler architecture guarantees carried by every lowered program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoweringInvariant {
    DirectJsToWasm,
    NoShippedInterpreter,
    SpecIrAuthority,
}
impl LoweringInvariant {
    pub const ALL: [Self; 3] = [
        Self::DirectJsToWasm,
        Self::NoShippedInterpreter,
        Self::SpecIrAuthority,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectJsToWasm => "direct-js-to-wasm-only",
            Self::NoShippedInterpreter => "no-shipped-interpreter-in-wasm",
            Self::SpecIrAuthority => "spec-ir-is-semantic-source-of-truth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramIr {
    pub goal: ParseGoal,
    pub stages: Vec<LoweringStage>,
    /// Bytes in the retained SourceUnit source text, including the lossless
    /// encoding for UTF-16 input. Used as a conservative capacity bound.
    pub source_len: usize,
    pub invariants: Vec<LoweringInvariant>,
    pub diagnostics: Vec<IrDiagnostic>,
    pub script: Option<ScriptIr>,
    /// Present iff `goal == ParseGoal::Module`.
    ///
    /// `script` is the graph linked into one unit; this keeps the spec records
    /// addressable so the artifact strategy can evolve to several linked Wasm
    /// modules without re-deriving them.
    pub modules: Option<ModuleGraphIr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IrBlockSummary {
    pub super_uses: usize,
    pub this_reads: usize,
}

pub(crate) fn summarize_block(block: &BlockIr) -> IrBlockSummary {
    let mut counts = IrSummaryCounts::default();
    counts.visit_block(block);
    IrBlockSummary {
        super_uses: counts.super_uses,
        this_reads: counts.this_reads,
    }
}

pub(crate) fn statement_contains_suspension(statement: &StatementIr) -> bool {
    let mut counts = IrSummaryCounts::default();
    counts.visit_statement(statement);
    counts.suspensions != 0
}

/// Inspect the current activation's statement tree. Function values name a
/// separate function body and the exhaustive summary visitor does not enter it.
pub(crate) fn statement_contains_async_while_condition(statement: &StatementIr) -> bool {
    let mut counts = IrSummaryCounts::default();
    counts.visit_statement(statement);
    counts.async_while_conditions != 0
}

pub(crate) fn statement_contains_async_switch(statement: &StatementIr) -> bool {
    let mut counts = IrSummaryCounts::default();
    counts.visit_statement(statement);
    counts.async_switch_owners != 0
}

pub(crate) fn statement_contains_async_with(statement: &StatementIr) -> bool {
    let mut counts = IrSummaryCounts::default();
    counts.visit_statement(statement);
    counts.async_with_owners != 0
}

pub(crate) fn statement_contains_async_for_in(statement: &StatementIr) -> bool {
    let mut counts = IrSummaryCounts::default();
    counts.visit_statement(statement);
    counts.async_for_in_owners != 0
}

impl ProgramIr {
    pub fn language_features(&self) -> crate::LanguageFeaturePolicy {
        self.script
            .as_ref()
            .map_or_else(Default::default, |script| script.language_features)
    }

    /// Select semantics before emission; all prepared bodies share this P owner.
    pub fn with_language_features(mut self, policy: crate::LanguageFeaturePolicy) -> Self {
        if let Some(script) = &mut self.script {
            script.language_features = policy;
        }
        self
    }

    /// The first diagnostic that prevents this program from reaching Wasm
    /// emission.
    ///
    /// This is the single ordering and classification rule shared by the
    /// backend preflight and its engine-facing error transport. Keeping the
    /// diagnostic intact lets consumers retain closed capability identities
    /// instead of reconstructing them from the display message.
    pub fn wasm_blocking_diagnostic(&self) -> Option<&IrDiagnostic> {
        let blocking = self.diagnostics.iter().find(|diagnostic| {
            matches!(
                diagnostic.kind(),
                IrDiagnosticKind::Unsupported
                    | IrDiagnosticKind::EarlyError
                    | IrDiagnosticKind::LinkError
            )
        });
        // A program without a lowered script has no artifact to emit, so
        // whatever diagnostic explains that blocks, whatever its kind.
        blocking.or_else(|| {
            self.script
                .is_none()
                .then(|| self.diagnostics.first())
                .flatten()
        })
    }

    pub fn is_wasm_supported(&self) -> bool {
        self.script.is_some() && self.wasm_blocking_diagnostic().is_none()
    }

    pub fn ir_summary(&self) -> String {
        match &self.script {
            Some(script) => {
                let mut counts = IrSummaryCounts::default();
                counts.functions += script.functions.len();
                for function in &script.functions {
                    counts.visit_function(function);
                }
                counts.visit_block(&script.body);
                format!(
                    "script statements={} result={} strict={} functions={} nested_functions={} function_exprs={} arrow_functions={} named_function_exprs={} closures={} captures={} lexical_this_captures={} lexical_arguments_captures={} environment_identifier_reads={} environment_identifier_calls={} default_params={} rest_params={} arguments_uses={} calls={} indirect_calls={} method_calls={} constructs={} classes={} class_exprs={} class_extends={} null_heritage_classes={} class_fields={} private_elements={} static_blocks={} super_uses={} null_heritage_super_uses={} private_in_checks={} throws={} try_catches={} try_finallys={} returns={} lets={} consts={} vars={} global_bindings={} builtin_globals={} boxed_builtin_globals={} global_this_uses={} top_level_this_uses={} global_default_this_calls={} global_property_reads={} global_property_writes={} implicit_globals={} host_globals={} host_builtin_calls={} builtin_ctor_calls={} builtin_static_calls={} error_builtin_calls={} aggregate_errors={} function_proto_calls={} function_proto_applies={} function_proto_binds={} function_proto_to_strings={} bound_functions={} bound_function_constructs={} boxed_builtin_calls={} boxed_builtin_constructs={} boxed_receiver_adaptations={} error_proto_to_strings={} blocks={} ifs={} whiles={} do_whiles={} fors={} switches={} labels={} debuggers={} breaks={} continues={} objects={} object_shorthands={} object_methods={} object_getters={} object_setters={} arrays={} property_reads={} property_writes={} array_lengths={} heap_shapes={} function_values={} this_reads={} new_target_uses={} assigns={} prefix_updates={} postfix_updates={} compound_assigns={} string_concats={} loose_equalities={} coercive_numeric_ops={} coercive_relational_ops={} typeof_uses={} void_uses={} deletes={} identifier_deletes={} global_deletes={} comma_ops={} nullish_ops={} spec_operations={} kind_unions={} heap_to_primitives={} heap_loose_equalities={} heap_coercions={} instanceofs={} in_ops={} prototype_reads={} prototype_writes={}",
                    counts.statements,
                    script.result_kind().as_str(),
                    script.strict,
                    counts.functions,
                    counts.nested_functions,
                    counts.function_exprs,
                    counts.arrow_functions,
                    counts.named_function_exprs,
                    counts.closures,
                    counts.captures,
                    counts.lexical_this_captures,
                    counts.lexical_arguments_captures,
                    counts.environment_identifier_reads,
                    counts.environment_identifier_calls,
                    counts.default_params,
                    counts.rest_params,
                    counts.arguments_uses,
                    counts.calls,
                    counts.indirect_calls,
                    counts.method_calls,
                    counts.constructs,
                    counts.classes,
                    counts.class_exprs,
                    counts.class_extends,
                    counts.null_heritage_classes,
                    counts.class_fields,
                    counts.private_elements,
                    counts.static_blocks,
                    counts.super_uses,
                    counts.null_heritage_super_uses,
                    counts.private_in_checks,
                    counts.throws,
                    counts.try_catches,
                    counts.try_finallys,
                    counts.returns,
                    counts.lets,
                    counts.consts,
                    counts.vars,
                    script.global_bindings.len(),
                    script
                        .global_bindings
                        .iter()
                        .filter(|binding| matches!(
                            &binding.initializer,
                            GlobalPropertyInitializerIr::BuiltinFunction(_)
                        ))
                        .count(),
                    script
                        .global_bindings
                        .iter()
                        .filter(|binding| {
                            matches!(
                                &binding.initializer,
                                GlobalPropertyInitializerIr::BuiltinFunction(builtin)
                                    if builtin.is_boxed_primitive_constructor()
                            )
                        })
                        .count(),
                    counts.global_this_uses,
                    script.top_level_this_uses,
                    counts.global_default_this_calls,
                    counts.global_property_reads,
                    counts.global_property_writes,
                    counts.implicit_globals,
                    script.host_builtins.len(),
                    script.host_builtin_calls,
                    script.builtin_ctor_calls,
                    script.builtin_static_calls,
                    script.error_builtin_calls,
                    script.aggregate_errors,
                    script.function_proto_calls,
                    script.function_proto_applies,
                    script.function_proto_binds,
                    script.function_proto_to_strings,
                    script.bound_functions,
                    script.bound_function_constructs,
                    script.boxed_builtin_calls,
                    script.boxed_builtin_constructs,
                    script.boxed_receiver_adaptations,
                    script.error_proto_to_strings,
                    counts.blocks,
                    counts.ifs,
                    counts.whiles,
                    counts.do_whiles,
                    counts.fors,
                    counts.switches,
                    counts.labels,
                    counts.debuggers,
                    counts.breaks,
                    counts.continues,
                    counts.objects,
                    counts.object_shorthands,
                    counts.object_methods,
                    counts.object_getters,
                    counts.object_setters,
                    counts.arrays,
                    counts.property_reads,
                    counts.property_writes,
                    counts.array_lengths,
                    counts.heap_shapes,
                    counts.function_values,
                    counts.this_reads,
                    counts.new_target_uses,
                    counts.assignments,
                    counts.prefix_updates,
                    counts.postfix_updates,
                    counts.compound_assignments,
                    counts.string_concats,
                    counts.loose_equalities,
                    counts.coercive_numeric_ops,
                    counts.coercive_relational_ops,
                    counts.typeof_uses,
                    counts.void_uses,
                    counts.deletes,
                    counts.identifier_deletes,
                    counts.global_deletes,
                    counts.comma_ops,
                    counts.nullish_ops,
                    counts.spec_operations,
                    counts.kind_unions,
                    counts.heap_to_primitives,
                    counts.heap_loose_equalities,
                    counts.heap_coercions,
                    counts.instanceofs,
                    counts.in_ops,
                    counts.prototype_reads,
                    counts.prototype_writes
                )
            }
            None => "no script ir".to_string(),
        }
    }
}

/// Sole consumed storage query for the checked generator entry-local head.
/// Reuses the exhaustive summary traversal; ordinary summaries keep their
/// existing counter traversal when no storage name is tracked.
pub(crate) fn statements_reference_storage(statements: &[StatementIr], name: &str) -> bool {
    let mut counts = IrSummaryCounts {
        tracked_storage: Some(name.to_string()),
        ..Default::default()
    };
    for statement in statements {
        counts.visit_statement(statement);
    }
    counts.storage_seen
}

/// Validate the entry-only IteratorValue reads in a prepared eager pattern.
/// Every retained name/write uses the same existing exhaustive storage census.
pub(crate) fn entry_local_storage_has_only_dynamic_reads(
    statements: &[StatementIr],
    name: &str,
) -> bool {
    let mut counts = IrSummaryCounts {
        tracked_storage: Some(name.to_string()),
        ..Default::default()
    };
    for statement in statements {
        counts.visit_statement(statement);
    }
    counts.storage_read_seen && !counts.storage_non_read_seen && !counts.storage_invalid_read
}

#[derive(Default)]
struct IrSummaryCounts {
    tracked_storage: Option<String>,
    storage_seen: bool,
    storage_read_seen: bool,
    storage_non_read_seen: bool,
    storage_invalid_read: bool,
    suspensions: usize,
    async_while_conditions: usize,
    async_switch_owners: usize,
    async_with_owners: usize,
    async_for_in_owners: usize,
    module_import_bindings: usize,
    statements: usize,
    functions: usize,
    nested_functions: usize,
    function_exprs: usize,
    arrow_functions: usize,
    named_function_exprs: usize,
    closures: usize,
    captures: usize,
    lexical_this_captures: usize,
    lexical_arguments_captures: usize,
    environment_identifier_reads: usize,
    environment_identifier_calls: usize,
    default_params: usize,
    rest_params: usize,
    arguments_uses: usize,
    calls: usize,
    indirect_calls: usize,
    method_calls: usize,
    constructs: usize,
    classes: usize,
    class_exprs: usize,
    class_extends: usize,
    null_heritage_classes: usize,
    class_fields: usize,
    private_elements: usize,
    static_blocks: usize,
    super_uses: usize,
    null_heritage_super_uses: usize,
    private_in_checks: usize,
    throws: usize,
    try_catches: usize,
    try_finallys: usize,
    returns: usize,
    lets: usize,
    consts: usize,
    vars: usize,
    global_this_uses: usize,
    global_default_this_calls: usize,
    global_property_reads: usize,
    global_property_writes: usize,
    implicit_globals: usize,
    blocks: usize,
    ifs: usize,
    whiles: usize,
    do_whiles: usize,
    fors: usize,
    switches: usize,
    labels: usize,
    debuggers: usize,
    breaks: usize,
    continues: usize,
    objects: usize,
    object_shorthands: usize,
    object_methods: usize,
    object_getters: usize,
    object_setters: usize,
    arrays: usize,
    property_reads: usize,
    property_writes: usize,
    array_lengths: usize,
    heap_shapes: usize,
    function_values: usize,
    this_reads: usize,
    new_target_uses: usize,
    assignments: usize,
    prefix_updates: usize,
    postfix_updates: usize,
    compound_assignments: usize,
    string_concats: usize,
    loose_equalities: usize,
    coercive_numeric_ops: usize,
    coercive_relational_ops: usize,
    scalar_exponentiation: Option<bool>,
    typeof_uses: usize,
    void_uses: usize,
    deletes: usize,
    identifier_deletes: usize,
    global_deletes: usize,
    comma_ops: usize,
    nullish_ops: usize,
    spec_operations: usize,
    kind_unions: usize,
    heap_to_primitives: usize,
    heap_loose_equalities: usize,
    heap_coercions: usize,
    instanceofs: usize,
    in_ops: usize,
    prototype_reads: usize,
    prototype_writes: usize,
}

impl IrSummaryCounts {
    fn track_storage(&mut self, name: &str) {
        let matches = self.tracked_storage.as_deref() == Some(name);
        self.storage_seen |= matches;
        self.storage_non_read_seen |= matches;
    }

    fn track_environment(&mut self, environment: Option<&LexicalEnvironmentIr>) {
        if self.tracked_storage.is_some() {
            if let Some(environment) = environment {
                for binding in &environment.bindings {
                    self.track_storage(&binding.name);
                }
            }
        }
    }

    fn track_for_environment(&mut self, environment: Option<&ForInOfEnvironmentIr>) {
        if let Some(environment) = environment {
            self.track_environment(environment.tdz_environment.as_ref());
            self.track_environment(environment.iteration_environment.as_ref());
            for name in &environment.tdz_binding_names {
                self.track_storage(name);
            }
        }
    }

    fn track_iteration_environment(&mut self, environment: &ResumableLoopIterationEnvironmentIr) {
        match environment {
            ResumableLoopIterationEnvironmentIr::StorageOnly => {}
            ResumableLoopIterationEnvironmentIr::FreshPerIteration(environment) => {
                self.track_environment(Some(environment))
            }
        }
    }

    fn track_record(&mut self, record: &IteratorRecordIr) {
        self.track_storage(record.iterator().as_str());
        self.track_storage(record.next_method().as_str());
        self.track_storage(record.done().as_str());
    }

    fn track_destructuring_target(&mut self, target: &DestructuringTargetIr) {
        match target {
            DestructuringTargetIr::Binding { name, .. } => self.track_storage(name),
            DestructuringTargetIr::ResolvedVarBinding { name, reference } => {
                self.track_storage(name);
                self.track_storage(reference.name());
            }
            DestructuringTargetIr::AssignmentIdentifier(reference) => {
                self.track_storage(reference.name())
            }
            DestructuringTargetIr::NestedArray(pattern) => self.track_array_pattern(pattern),
            DestructuringTargetIr::NestedObject(pattern) => self.track_object_pattern(pattern),
            DestructuringTargetIr::AssignmentSuper { value_binding, .. } => {
                self.track_storage(value_binding)
            }
            DestructuringTargetIr::AssignmentProperty { .. }
            | DestructuringTargetIr::AssignmentPrivate { .. } => {}
        }
    }

    fn track_array_pattern(&mut self, pattern: &ArrayDestructuringPatternIr) {
        if self.tracked_storage.is_none() {
            return;
        }
        for element in &pattern.elements {
            match element {
                ArrayDestructuringElementIr::Elision => {}
                ArrayDestructuringElementIr::Target { target, .. }
                | ArrayDestructuringElementIr::Rest { target } => {
                    self.track_destructuring_target(target)
                }
            }
        }
    }

    fn track_object_pattern(&mut self, pattern: &ObjectDestructuringPatternIr) {
        if self.tracked_storage.is_none() {
            return;
        }
        for property in &pattern.properties {
            self.track_destructuring_target(&property.target);
        }
        if let Some(rest) = &pattern.rest {
            self.track_destructuring_target(rest);
        }
    }

    fn visit_function(&mut self, function: &FunctionIr) {
        if let Some(plan) = &function.class_instance_element_plan {
            self.class_fields += plan.elements.len();
            self.private_elements += plan
                .elements
                .iter()
                .filter(|element| match element {
                    ClassInstanceElementIr::Field(field) => {
                        matches!(&field.key, ClassFieldKeyIr::Private(_))
                    }
                    ClassInstanceElementIr::AutoAccessorBacking(_) => true,
                })
                .count();
        }
        if function.is_nested {
            self.nested_functions += 1;
        }
        if function.is_expression {
            self.function_exprs += 1;
        }
        if function.protocol.flavor() == FunctionFlavor::Arrow {
            self.arrow_functions += 1;
        }
        if function.is_named_expression {
            self.named_function_exprs += 1;
        }
        if !function.captured_bindings.is_empty() || !function.owned_env_bindings.is_empty() {
            self.closures += 1;
        }
        self.captures += function.captured_bindings.len();
        if function.captures_lexical_this {
            self.lexical_this_captures += 1;
        }
        if function.captures_lexical_arguments {
            self.lexical_arguments_captures += 1;
        }
        for param in &function.params {
            if let Some(default_init) = &param.default_init {
                self.default_params += 1;
                self.visit_expr(default_init);
            }
            if param.is_rest {
                self.rest_params += 1;
            }
        }
        if function.class_heritage_kind == ClassHeritageKind::Null && function.uses_super {
            self.null_heritage_super_uses += 1;
        }
        self.visit_block(&function.body);
    }

    fn visit_block(&mut self, block: &BlockIr) {
        self.track_environment(block.lexical_environment.as_ref());
        for statement in &block.statements {
            self.visit_statement(statement);
        }
    }

    fn visit_statement(&mut self, statement: &StatementIr) {
        self.statements += 1;
        if matches!(
            statement,
            StatementIr::GeneratorYield { .. }
                | StatementIr::AsyncDisposableScope { .. }
                | StatementIr::GeneratorLoop { .. }
                | StatementIr::AsyncGeneratorLoop(_)
                | StatementIr::AsyncGeneratorIf(_)
                | StatementIr::AsyncGeneratorWith(_)
                | StatementIr::AsyncGeneratorSwitch(_)
                | StatementIr::AsyncGeneratorArrayDestructuring(_)
                | StatementIr::AsyncGeneratorResourceScope(_)
                | StatementIr::AsyncGeneratorResourceRegistration(_)
                | StatementIr::AsyncGeneratorForOf(_)
                | StatementIr::AsyncGeneratorForIn(_)
                | StatementIr::OrdinaryGeneratorLoop(_)
                | StatementIr::OrdinaryGeneratorIf(_)
                | StatementIr::OrdinaryGeneratorSwitch(_)
                | StatementIr::OrdinaryGeneratorArrayDestructuring(_)
                | StatementIr::AsyncFunctionArrayDestructuring(_)
                | StatementIr::AsyncFunctionWith(_)
                | StatementIr::OrdinaryGeneratorWith(_)
                | StatementIr::GeneratorForOfIterator { .. }
                | StatementIr::GeneratorIf { .. }
                | StatementIr::AsyncFunctionIf { .. }
                | StatementIr::AsyncFunctionWhile(_)
                | StatementIr::AsyncFunctionSwitch(_)
                | StatementIr::ForOfIterator {
                    head: ForOfIteratorHeadIr::Assignment {
                        async_plan: Some(_),
                        ..
                    } | ForOfIteratorHeadIr::AsyncDisposable(_),
                    ..
                }
                | StatementIr::For {
                    init: Some(ForInitIr::AsyncDisposable(_)),
                    ..
                }
        ) {
            self.suspensions += 1;
        }
        match statement {
            StatementIr::EmptyStatementCompletion(item) => self.visit_statement(item.statement()),
            StatementIr::ResumableClassDefinition(plan) => {
                self.visit_expr(plan.expression());
                for statement in plan.prefixes().flat_map(|prefix| prefix.statements()) {
                    self.visit_statement(statement);
                }
            }
            StatementIr::AsyncModuleInstantiation | StatementIr::Empty => {}
            StatementIr::AnnexBFunctionCopy {
                block_storage_name,
                target,
                admission,
                ..
            } => {
                self.track_storage(block_storage_name);
                match target {
                    AnnexBFunctionCopyTargetIr::OwnerBinding { storage_name } => {
                        self.track_storage(storage_name)
                    }
                    AnnexBFunctionCopyTargetIr::ScriptGlobal { .. }
                    | AnnexBFunctionCopyTargetIr::DirectEvalVariable { .. }
                    | AnnexBFunctionCopyTargetIr::FunctionVariableEnvironment { .. } => {}
                }
                if let Some(binding) = admission {
                    self.track_storage(&binding.name);
                }
            }
            StatementIr::ModuleImportBinding(binding) => {
                self.module_import_bindings += 1;
                self.track_storage(&binding.name)
            }
            StatementIr::ModuleUnitOnce { block, .. } => self.visit_block(block),
            StatementIr::Lexical { mode, name, init } => {
                self.track_storage(name);
                match mode {
                    BindingMode::Let => self.lets += 1,
                    BindingMode::Const => self.consts += 1,
                    BindingMode::Var => self.vars += 1,
                }
                self.visit_expr(init);
            }
            StatementIr::LexicalBlock(statements)
            | StatementIr::ParameterInitialization { statements, .. } => {
                for statement in statements {
                    self.visit_statement(statement);
                }
            }
            StatementIr::SyncDisposableScope {
                execution,
                resources,
                body,
            } => {
                match execution {
                    SyncDisposableScopeExecutionIr::Immediate => {}
                    SyncDisposableScopeExecutionIr::PlainGenerator(capability) => {
                        self.track_storage(capability.binding_name())
                    }
                    SyncDisposableScopeExecutionIr::AsyncFunction(capability) => {
                        self.track_storage(capability.binding_name())
                    }
                    SyncDisposableScopeExecutionIr::AsyncGenerator(capability) => {
                        self.track_storage(capability.binding_name())
                    }
                }
                for resource in resources.iter() {
                    self.track_storage(&resource.binding_name);
                    self.visit_expr(&resource.initializer);
                }
                self.visit_block(body);
            }
            StatementIr::AsyncDisposableScope {
                execution,
                resources,
                body,
            } => {
                match execution {
                    AsyncDisposableScopeExecutionIr::AsyncFunction(capability) => {
                        self.track_storage(capability.binding_name())
                    }
                    AsyncDisposableScopeExecutionIr::AsyncGenerator(capability) => {
                        self.track_storage(capability.binding_name())
                    }
                }
                for resource in resources.iter() {
                    self.track_storage(resource.binding_name());
                    self.visit_expr(resource.initializer());
                }
                self.visit_block(body);
            }
            StatementIr::Var(declarators) => {
                self.vars += declarators.len();
                for declarator in declarators {
                    self.track_storage(&declarator.name);
                    if let Some(init) = &declarator.init {
                        self.visit_expr(init);
                    }
                }
            }
            StatementIr::DeclarationEvaluation(expr) | StatementIr::Expression(expr) => {
                self.visit_expr(expr)
            }
            StatementIr::GeneratorYield {
                value, resume_mode, ..
            } => {
                if let GeneratorResumeModeIr::AssignIdentifier(name) = resume_mode {
                    self.track_storage(name);
                }
                if let GeneratorResumeModeIr::AssignProperty(reference) = resume_mode {
                    match reference.use_view() {
                        SuspendedPropertyReferenceUse::Ordinary {
                            base_and_receiver,
                            key,
                            strictness: _,
                        } => {
                            self.visit_expr(base_and_receiver);
                            match key {
                                PropertyKeyIr::StringExpr(expr)
                                | PropertyKeyIr::ArrayIndex(expr) => self.visit_expr(expr),
                                PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => {}
                            }
                        }
                    }
                }
                self.visit_expr(value);
            }
            StatementIr::AsyncAwait {
                value, resume_mode, ..
            } => {
                if let AsyncResumeModeIr::AssignIdentifier(name) = resume_mode {
                    self.track_storage(name);
                }
                self.suspensions += 1;
                self.visit_expr(value);
            }
            StatementIr::Block(block) => {
                self.blocks += 1;
                self.visit_block(block);
            }
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
                self.ifs += 1;
                self.visit_expr(condition);
                self.visit_statement(then_branch);
                if let Some(else_branch) = else_branch {
                    self.visit_statement(else_branch);
                }
            }
            StatementIr::AsyncFunctionWhile(plan) => {
                self.async_while_conditions += 1;
                self.whiles += 1;
                for statement in plan.condition_prefix() {
                    self.visit_statement(statement);
                }
                self.visit_expr(plan.condition());
                self.visit_statement(plan.body());
            }
            StatementIr::While { condition, body } => {
                self.whiles += 1;
                self.visit_expr(condition);
                self.visit_statement(body);
            }
            StatementIr::DoWhile { body, condition } => {
                self.do_whiles += 1;
                self.visit_statement(body);
                self.visit_expr(condition);
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                lexical_environment,
            } => {
                if let Some(environment) = lexical_environment {
                    for binding in &environment.bindings {
                        self.track_storage(&binding.name);
                    }
                }
                self.fors += 1;
                if let Some(init) = init {
                    self.visit_for_init(init);
                }
                if let Some(test) = test {
                    self.visit_expr(test);
                }
                if let Some(update) = update {
                    self.visit_expr(update);
                }
                self.visit_statement(body);
            }
            StatementIr::OrdinaryGeneratorLoop(plan) => {
                self.track_storage(plan.value_binding_name());
                if let Some(environment) = plan.lexical_environment() {
                    for binding in &environment.bindings {
                        self.track_storage(&binding.name);
                    }
                }
                match plan.kind() {
                    GeneratorLoopKindIr::For => self.fors += 1,
                    GeneratorLoopKindIr::While => self.whiles += 1,
                    GeneratorLoopKindIr::DoWhile => self.do_whiles += 1,
                }
                for region in plan.regions() {
                    self.visit_block(region.block());
                }
                for expression in plan.expressions() {
                    self.visit_expr(expression);
                }
            }
            StatementIr::AsyncGeneratorLoop(plan) => {
                self.track_storage(plan.value_binding_name());
                if let Some(resource) = plan.resource() {
                    self.track_storage(&resource.capability_binding().name);
                }
                if let Some(environment) = plan.lexical_environment() {
                    for binding in &environment.bindings {
                        self.track_storage(&binding.name);
                    }
                }
                match plan.kind() {
                    GeneratorLoopKindIr::For => self.fors += 1,
                    GeneratorLoopKindIr::While => self.whiles += 1,
                    GeneratorLoopKindIr::DoWhile => self.do_whiles += 1,
                }
                for region in plan.regions() {
                    self.visit_block(region.block());
                }
                for expression in plan.expressions() {
                    self.visit_expr(expression);
                }
            }
            StatementIr::OrdinaryGeneratorIf(plan) => {
                self.ifs += 1;
                self.visit_expr(plan.condition());
                self.visit_block(plan.then_branch().block());
                self.visit_block(plan.else_branch().block());
            }
            StatementIr::AsyncGeneratorIf(plan) => {
                self.ifs += 1;
                self.visit_block(plan.condition().region().block());
                self.visit_expr(plan.condition().value());
                self.visit_block(plan.then_branch().block());
                self.visit_block(plan.else_branch().block());
            }
            StatementIr::OrdinaryGeneratorSwitch(plan) => {
                self.switches += 1;
                self.track_storage(&plan.discriminant_binding().name);
                self.track_storage(&plan.value_binding().name);
                self.visit_block(plan.discriminant().region().block());
                self.visit_expr(plan.discriminant().value());
                self.track_environment(plan.lexical_environment());
                for declaration in plan.lexical_declarations() {
                    self.visit_statement(declaration);
                }
                for case in plan.cases() {
                    if let Some(selector) = case.selector() {
                        self.visit_block(selector.region().block());
                        self.visit_expr(selector.value());
                    }
                    self.visit_block(case.body().block());
                }
            }
            StatementIr::AsyncGeneratorSwitch(plan) => {
                self.switches += 1;
                if let Some(resource) = plan.resource() {
                    self.track_storage(&resource.capability_binding().name);
                }
                self.track_storage(&plan.discriminant_binding().name);
                self.track_storage(&plan.value_binding().name);
                self.visit_block(plan.discriminant().region().block());
                self.visit_expr(plan.discriminant().value());
                self.track_environment(plan.lexical_environment());
                for declaration in plan.lexical_declarations() {
                    self.visit_statement(declaration);
                }
                for case in plan.cases() {
                    if let Some(selector) = case.selector() {
                        self.visit_block(selector.region().block());
                        self.visit_expr(selector.value());
                    }
                    self.visit_block(case.body().block());
                }
            }
            StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
                self.track_storage(&plan.storage().binding().name);
                self.visit_expr(plan.raw_source());
                self.visit_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorResourceScope(plan) => {
                self.track_storage(&plan.capability_binding().name);
                self.visit_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorResourceRegistration(operation) => {
                self.track_storage(operation.binding_name());
                self.visit_expr(operation.initializer());
            }
            StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
                self.track_storage(&plan.storage().binding().name);
                self.visit_expr(plan.raw_source());
                self.visit_block(plan.body().block());
            }
            StatementIr::AsyncFunctionArrayDestructuring(plan) => {
                self.track_storage(&plan.storage().binding().name);
                self.visit_expr(plan.raw_source());
                self.visit_block(plan.body());
            }
            StatementIr::AsyncGeneratorForIn(plan) => {
                self.fors += 1;
                if plan.execution() == crate::ResumableRegionProtocolIr::Async {
                    self.async_for_in_owners += 1;
                }
                for binding in [
                    plan.head_binding(),
                    plan.enumerator_binding(),
                    plan.key_binding(),
                    plan.value_binding(),
                ] {
                    self.track_storage(&binding.name);
                }
                self.visit_block(plan.head().region().block());
                self.visit_expr(plan.head().value());
                self.track_for_environment(plan.lexical_environment());
                self.visit_block(plan.initialization());
                self.visit_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorForOf(plan) => {
                self.fors += 1;
                if let Some(resource) = plan.resource() {
                    self.track_storage(&resource.capability_binding().name);
                }
                for binding in [
                    plan.head_binding(),
                    plan.incoming_binding(),
                    plan.value_binding(),
                ] {
                    self.track_storage(&binding.name);
                }
                self.visit_block(plan.head().region().block());
                self.visit_expr(plan.head().value());
                self.track_for_environment(plan.lexical_environment());
                self.visit_block(plan.initialization().block());
                self.visit_block(plan.body().block());
            }
            StatementIr::OrdinaryGeneratorWith(plan) => {
                self.visit_block(plan.head().region().block());
                self.visit_expr(plan.head().value());
                self.track_environment(Some(plan.lexical_environment()));
                self.visit_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorWith(plan) => {
                self.track_storage(&plan.head_binding().name);
                self.visit_block(plan.head().region().block());
                self.visit_expr(plan.head().value());
                self.track_environment(Some(plan.lexical_environment()));
                self.visit_block(plan.body().block());
            }
            StatementIr::AsyncFunctionWith(plan) => {
                self.async_with_owners += 1;
                self.track_storage(&plan.head_binding().name);
                self.visit_block(plan.head());
                self.visit_expr(plan.head_value());
                self.track_environment(Some(plan.lexical_environment()));
                self.visit_block(plan.body());
            }
            StatementIr::ArrayDestructuringOperation(operation) => {
                self.track_storage(&operation.storage().binding().name);
                match operation.use_view() {
                    crate::ArrayDestructuringOperationView::RestArray(_) => self.arrays += 1,
                    crate::ArrayDestructuringOperationView::StepValue(_)
                    | crate::ArrayDestructuringOperationView::Elision(_) => {}
                }
            }
            StatementIr::GeneratorLoop {
                init,
                test,
                update,
                before_suspension,
                suspension_statement,
                after_suspension,
                iteration_environment,
                ..
            } => {
                self.track_iteration_environment(iteration_environment);
                self.fors += 1;
                if let Some(init) = init {
                    self.visit_for_init(init);
                }
                if let Some(test) = test {
                    self.visit_expr(test);
                }
                if let Some(update) = update {
                    self.visit_expr(update);
                }
                for statement in before_suspension {
                    self.visit_statement(statement);
                }
                self.visit_statement(suspension_statement);
                for statement in after_suspension {
                    self.visit_statement(statement);
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
                self.ifs += 1;
                self.visit_expr(condition);
                for statement in then_before_yield
                    .iter()
                    .chain(then_yield_statement.as_deref())
                    .chain(then_after_yield)
                    .chain(else_before_yield)
                    .chain(else_yield_statement.as_deref())
                    .chain(else_after_yield)
                {
                    self.visit_statement(statement);
                }
            }
            StatementIr::AsyncFunctionForOfIterator { iterable, plan } => {
                self.track_storage(plan.value_name());
                self.track_record(plan.record());
                match plan.execution() {
                    AsyncFunctionForOfIteratorExecutionIr::Synchronous(_) => {}
                    AsyncFunctionForOfIteratorExecutionIr::Awaited(awaited) => {
                        self.track_storage(awaited.async_iterator_binding());
                        self.track_storage(awaited.close_on_rejection_binding());
                    }
                }
                self.track_for_environment(plan.head_environment());
                self.track_iteration_environment(plan.iteration_environment());
                self.fors += 1;
                self.visit_expr(iterable);
                for statement in plan.body().statements() {
                    self.visit_statement(statement);
                }
            }
            StatementIr::GeneratorForOfIterator { iterable, plan } => {
                self.track_storage(plan.value_name());
                self.track_record(plan.record());
                self.track_for_environment(plan.head_environment());
                self.track_iteration_environment(plan.iteration_environment());
                self.fors += 1;
                self.visit_expr(iterable);
                for statement in plan.body().statements() {
                    self.visit_statement(statement);
                }
            }
            StatementIr::ForOfIterator {
                head,
                iterable,
                body,
                lexical_environment,
            } => {
                self.track_for_environment(lexical_environment.as_ref());
                match head {
                    ForOfIteratorHeadIr::Assignment {
                        binding,
                        async_plan,
                        ..
                    } => {
                        self.track_storage(&binding.name);
                        if let Some(plan) = async_plan {
                            self.track_record(&plan.record);
                            self.track_storage(&plan.async_iterator_binding);
                            self.track_storage(&plan.close_on_rejection_binding);
                        }
                    }
                    ForOfIteratorHeadIr::SyncDisposable(head) => {
                        self.track_storage(head.binding_name())
                    }
                    ForOfIteratorHeadIr::AsyncDisposable(head) => {
                        self.track_storage(head.binding_name());
                        self.track_storage(head.capability().binding_name());
                        self.track_record(head.record());
                    }
                }
                self.fors += 1;
                self.visit_expr(iterable);
                self.visit_statement(body);
            }
            StatementIr::ForInArray {
                name,
                target,
                body,
                lexical_environment,
                ..
            }
            | StatementIr::ForInString {
                name,
                target,
                body,
                lexical_environment,
                ..
            }
            | StatementIr::ForInObject {
                name,
                target,
                body,
                lexical_environment,
                ..
            } => {
                self.track_storage(name);
                self.track_for_environment(lexical_environment.as_ref());
                self.fors += 1;
                self.visit_expr(target);
                self.visit_statement(body);
            }
            StatementIr::AsyncFunctionSwitch(plan) => {
                self.async_switch_owners += 1;
                self.switches += 1;
                self.visit_expr(plan.discriminant());
                for declaration in plan.lexical_declarations() {
                    self.visit_statement(declaration);
                }
                for case in plan.cases() {
                    for statement in case.condition_prefix() {
                        self.visit_statement(statement);
                    }
                    if let Some(condition) = case.condition() {
                        self.visit_expr(condition);
                    }
                    self.visit_block(case.body());
                }
            }
            StatementIr::Switch {
                discriminant,
                lexical_declarations,
                cases,
                lexical_environment,
            } => {
                self.track_environment(lexical_environment.as_ref());
                self.switches += 1;
                self.visit_expr(discriminant);
                for declaration in lexical_declarations {
                    self.visit_statement(declaration);
                }
                for case in cases {
                    if let Some(condition) = &case.condition {
                        self.visit_expr(condition);
                    }
                    self.visit_block(&case.body);
                }
            }
            StatementIr::Labelled {
                labels, statement, ..
            } => {
                self.labels += labels.len();
                self.visit_statement(statement);
            }
            StatementIr::Debugger => self.debuggers += 1,
            StatementIr::Throw(expr) => {
                self.throws += 1;
                self.visit_expr(expr);
            }
            StatementIr::TryCatch {
                try_block,
                catch_block,
                catch_name,
                catch_parameter_environment,
                ..
            } => {
                self.track_storage(catch_name);
                self.track_environment(catch_parameter_environment.as_ref());
                self.try_catches += 1;
                self.visit_block(try_block);
                self.visit_block(catch_block);
            }
            StatementIr::TryFinally {
                try_block,
                finally_block,
                ..
            } => {
                self.try_finallys += 1;
                self.visit_block(try_block);
                self.visit_block(finally_block);
            }
            StatementIr::TryCatchFinally {
                try_block,
                catch_block,
                finally_block,
                catch_name,
                catch_parameter_environment,
                ..
            } => {
                self.track_storage(catch_name);
                self.track_environment(catch_parameter_environment.as_ref());
                self.try_finallys += 1;
                self.visit_block(try_block);
                self.visit_block(catch_block);
                self.visit_block(finally_block);
            }
            StatementIr::Return(expr) => {
                self.returns += 1;
                self.visit_expr(expr);
            }
            StatementIr::Break { .. } => self.breaks += 1,
            StatementIr::Continue { .. } => self.continues += 1,
        }
    }

    fn visit_for_init(&mut self, init: &ForInitIr) {
        match init {
            ForInitIr::Lexical { mode, name, init } => {
                self.track_storage(name);
                match mode {
                    BindingMode::Let => self.lets += 1,
                    BindingMode::Const => self.consts += 1,
                    BindingMode::Var => self.vars += 1,
                }
                self.visit_expr(init);
            }
            ForInitIr::LexicalBlock(bindings) => {
                for binding in bindings {
                    self.track_storage(&binding.name);
                    match binding.mode {
                        BindingMode::Let => self.lets += 1,
                        BindingMode::Const => self.consts += 1,
                        BindingMode::Var => self.vars += 1,
                    }
                    self.visit_expr(&binding.init);
                }
            }
            ForInitIr::Var(declarators) => {
                self.vars += declarators.len();
                for declarator in declarators {
                    self.track_storage(&declarator.name);
                    if let Some(init) = &declarator.init {
                        self.visit_expr(init);
                    }
                }
            }
            ForInitIr::Expression(expr) => self.visit_expr(expr),
            ForInitIr::Statements(statements) => {
                for statement in statements {
                    self.visit_statement(statement);
                }
            }
            ForInitIr::SyncDisposable(resources) => {
                self.consts += resources.len();
                for resource in resources.iter() {
                    self.track_storage(&resource.binding_name);
                    self.visit_expr(&resource.initializer);
                }
            }
            ForInitIr::AsyncDisposable(init) => {
                self.consts += init.resources().len();
                for resource in init.resources().iter() {
                    self.track_storage(resource.binding_name());
                    self.visit_expr(resource.initializer());
                }
            }
        }
    }

    fn visit_object_property(&mut self, property: &ObjectPropertyIr) {
        match property {
            ObjectPropertyIr::PrototypeSetter { value }
            | ObjectPropertyIr::Spread { source: value }
            | ObjectPropertyIr::NonEnumerableData { value, .. } => self.visit_expr(value),
            ObjectPropertyIr::Data {
                value,
                is_shorthand,
                ..
            } => {
                self.object_shorthands += usize::from(*is_shorthand);
                self.visit_expr(value);
            }
            ObjectPropertyIr::ComputedData { key, value, .. } => {
                self.visit_expr(key);
                self.visit_expr(value);
            }
            ObjectPropertyIr::ComputedMethod { key, .. } => {
                self.object_methods += 1;
                self.function_values += 1;
                self.visit_expr(key);
            }
            ObjectPropertyIr::Method { .. } => {
                self.object_methods += 1;
                self.function_values += 1;
            }
            ObjectPropertyIr::ComputedGetter { key, .. } => {
                self.object_getters += 1;
                self.function_values += 1;
                self.visit_expr(key);
            }
            ObjectPropertyIr::Getter { .. } => {
                self.object_getters += 1;
                self.function_values += 1;
            }
            ObjectPropertyIr::ComputedSetter { key, .. } => {
                self.object_setters += 1;
                self.function_values += 1;
                self.visit_expr(key);
            }
            ObjectPropertyIr::Setter { .. } => {
                self.object_setters += 1;
                self.function_values += 1;
            }
        }
    }

    fn visit_expr(&mut self, expr: &TypedExpr) {
        if let Some(required) = &mut self.scalar_exponentiation {
            *required |= match &expr.expr {
                ExprIr::BinaryNumber {
                    op: ArithmeticBinaryOp::Exp,
                    ..
                } => expr.kind != ValueKind::BigInt,
                ExprIr::CoerciveBinaryNumber {
                    op: ArithmeticBinaryOp::Exp,
                    ..
                }
                | ExprIr::CompoundAssignIdentifier {
                    op: ArithmeticBinaryOp::Exp,
                    ..
                } => true,
                ExprIr::EnvironmentIdentifier(identifier) => matches!(
                    &identifier.operation,
                    crate::EnvironmentIdentifierOperationIr::EagerCompound {
                        operation: crate::EnvironmentCompoundOperationIr::Arithmetic(
                            ArithmeticBinaryOp::Exp
                        ),
                        ..
                    }
                ),
                _ => false,
            };
        }
        if expr.heap_shape.is_some() {
            self.heap_shapes += 1;
        }
        if !expr.possible_kinds.is_singleton() {
            self.kind_unions += 1;
        }
        if expr.kind == ValueKind::Function {
            self.function_values += 1;
        }
        match &expr.expr {
            ExprIr::ForeignCall(call) => {
                self.calls += 1;
                for operand in call.operands() {
                    self.visit_expr(operand);
                }
            }
            ExprIr::EnvironmentIdentifier(identifier) => {
                self.track_storage(&identifier.name);
                match &identifier.operation {
                    crate::EnvironmentIdentifierOperationIr::Read => {
                        self.environment_identifier_reads += 1;
                    }
                    crate::EnvironmentIdentifierOperationIr::Typeof
                    | crate::EnvironmentIdentifierOperationIr::CaptureCallReference { .. }
                    | crate::EnvironmentIdentifierOperationIr::CaptureAssignmentReference {
                        ..
                    }
                    | crate::EnvironmentIdentifierOperationIr::ReleaseCapturedReference {
                        ..
                    }
                    | crate::EnvironmentIdentifierOperationIr::Delete => {}
                    crate::EnvironmentIdentifierOperationIr::Assign { .. }
                    | crate::EnvironmentIdentifierOperationIr::AssignWithGlobalFallback {
                        ..
                    }
                    | crate::EnvironmentIdentifierOperationIr::PutCapturedReference { .. } => {
                        self.assignments += 1
                    }
                    crate::EnvironmentIdentifierOperationIr::Update { return_mode, .. } => {
                        match return_mode {
                            UpdateReturnMode::Prefix => self.prefix_updates += 1,
                            UpdateReturnMode::Postfix => self.postfix_updates += 1,
                        }
                    }
                    crate::EnvironmentIdentifierOperationIr::EagerCompound { .. }
                    | crate::EnvironmentIdentifierOperationIr::LogicalCompound { .. } => {
                        self.compound_assignments += 1
                    }
                    crate::EnvironmentIdentifierOperationIr::Call { .. } => {
                        self.environment_identifier_calls += 1;
                        self.calls += 1;
                        self.indirect_calls += 1;
                    }
                }
                for operand in identifier.operation.operands() {
                    self.visit_expr(operand);
                }
            }
            ExprIr::ImportMeta { .. } => {}
            ExprIr::ModuleEntryEvaluation(entry) => self.visit_expr(entry.evaluation()),
            ExprIr::ModuleExecutionGraph(_)
            | ExprIr::ModuleBindingRead(_)
            | ExprIr::JsonModuleValue(_)
            | ExprIr::BytesModuleValue(_)
            | ExprIr::ModuleEvaluate(_)
            | ExprIr::DeferredModuleEvaluate(_)
            | ExprIr::ModuleHasAsyncDependencies(_)
            | ExprIr::ModuleDeferredImportEvaluate(_) => {}
            ExprIr::ModuleNamespacePublish { namespace, .. } => self.visit_expr(namespace),
            ExprIr::ModuleNamespace { exports, .. } => self.visit_expr(exports),
            ExprIr::DynamicImport {
                specifier, options, ..
            } => {
                self.visit_expr(specifier);
                if let Some(options) = options {
                    self.visit_expr(options);
                }
            }
            ExprIr::AssignIdentifier { name, value } => {
                self.track_storage(name);
                self.assignments += 1;
                self.visit_expr(value);
            }
            ExprIr::GlobalPropertyRead { .. } | ExprIr::GlobalIdentifierRead { .. } => {
                self.global_property_reads += 1;
            }
            ExprIr::GlobalPropertyWrite {
                value, implicit, ..
            } => {
                self.assignments += 1;
                self.global_property_writes += 1;
                self.implicit_globals += usize::from(*implicit);
                self.visit_expr(value);
            }
            ExprIr::ObjectLiteral(properties) => {
                self.objects += 1;
                for property in properties {
                    self.visit_object_property(property);
                }
            }
            ExprIr::ObjectPropertyDefinition(definition) => {
                self.visit_expr(definition.target());
                self.visit_object_property(definition.property());
            }
            ExprIr::RegExpLiteral { .. } => {
                self.objects += 1;
            }
            ExprIr::ArrayLiteral(elements) => {
                self.arrays += 1;
                for element in elements {
                    self.visit_expr(element);
                }
            }
            ExprIr::ArrayAccumulation(accumulation) => {
                self.arrays += 1;
                for element in accumulation.elements() {
                    match element {
                        ArrayAccumulationElementIr::Elision => {}
                        ArrayAccumulationElementIr::Value(value) => self.visit_expr(value),
                        ArrayAccumulationElementIr::Spread(spread) => {
                            self.visit_expr(&spread.value)
                        }
                    }
                }
            }
            ExprIr::TemplateObject(_) => {
                self.arrays += 2;
            }
            ExprIr::CaptureOptionalCallReference(capture) => {
                for operand in capture.operands() {
                    self.visit_expr(operand);
                }
            }
            ExprIr::CaptureArgumentList(capture) => {
                self.arrays += 1;
                for argument in capture.arguments() {
                    self.visit_expr(argument);
                }
            }
            ExprIr::CapturedArgumentList(list) => self.visit_expr(list.binding()),
            ExprIr::SpreadArgument(spread) => {
                self.visit_expr(&spread.value);
            }
            ExprIr::PropertyRead { target, key } => {
                self.property_reads += 1;
                if matches!(key, PropertyKeyIr::ArrayLength) {
                    self.array_lengths += 1;
                }
                if matches!(key, PropertyKeyIr::StaticString(name) if name == "prototype") {
                    self.prototype_reads += 1;
                }
                self.visit_expr(target);
                self.visit_property_key(key);
            }
            ExprIr::OptionalPropertyChain { target, chain } => {
                self.visit_expr(target);
                let mut previous_was_property = false;
                for operation in chain {
                    match operation {
                        OptionalChainOperationIr::Property { key, .. } => {
                            self.property_reads += 1;
                            if matches!(key, PropertyKeyIr::ArrayLength) {
                                self.array_lengths += 1;
                            }
                            if matches!(key, PropertyKeyIr::StaticString(name) if name == "prototype")
                            {
                                self.prototype_reads += 1;
                            }
                            self.visit_property_key(key);
                            previous_was_property = true;
                        }
                        OptionalChainOperationIr::PrivateProperty { .. } => {
                            self.private_elements += 1;
                            previous_was_property = true;
                        }
                        OptionalChainOperationIr::Call { args, receiver, .. } => {
                            self.calls += 1;
                            self.indirect_calls += 1;
                            self.method_calls += usize::from(
                                previous_was_property
                                    || *receiver == OptionalChainCallReceiverIr::CurrentThis,
                            );
                            for arg in args {
                                self.visit_expr(arg);
                            }
                            previous_was_property = false;
                        }
                    }
                }
            }
            ExprIr::DeleteOptionalPropertyChain(deletion) => {
                let target = deletion.target();
                let chain = deletion.prefix();
                self.visit_expr(target);
                let mut previous_was_property = false;
                for operation in chain {
                    match operation {
                        OptionalChainOperationIr::Property { key, .. } => {
                            self.property_reads += 1;
                            if matches!(key, PropertyKeyIr::ArrayLength) {
                                self.array_lengths += 1;
                            }
                            if matches!(key, PropertyKeyIr::StaticString(name) if name == "prototype")
                            {
                                self.prototype_reads += 1;
                            }
                            self.visit_property_key(key);
                            previous_was_property = true;
                        }
                        OptionalChainOperationIr::PrivateProperty { .. } => {
                            self.private_elements += 1;
                            previous_was_property = true;
                        }
                        OptionalChainOperationIr::Call { args, receiver, .. } => {
                            self.calls += 1;
                            self.indirect_calls += 1;
                            self.method_calls += usize::from(
                                previous_was_property
                                    || *receiver == OptionalChainCallReceiverIr::CurrentThis,
                            );
                            for arg in args {
                                self.visit_expr(arg);
                            }
                            previous_was_property = false;
                        }
                    }
                }

                self.deletes += 1;
                self.visit_property_key(deletion.key());
            }
            ExprIr::PropertyWrite {
                target, key, value, ..
            } => {
                self.property_writes += 1;
                if matches!(key, PropertyKeyIr::StaticString(name) if name == "prototype") {
                    self.prototype_writes += 1;
                }
                self.visit_expr(target);
                self.visit_property_key(key);
                self.visit_expr(value);
            }
            ExprIr::OrdinaryPropertyAssignment(assignment) => {
                self.property_writes += 1;
                if matches!(assignment.referenced_name(), PropertyKeyIr::StaticString(name) if name == "prototype")
                {
                    self.prototype_writes += 1;
                }
                self.visit_expr(assignment.base_and_receiver());
                self.visit_property_key(assignment.referenced_name());
                self.visit_expr(assignment.rhs());
            }
            ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {
                self.property_reads += 1;
                self.property_writes += 1;
                self.compound_assignments += 1;
                if matches!(assignment.referenced_name(), PropertyKeyIr::StaticString(name) if name == "prototype")
                {
                    self.prototype_writes += 1;
                }
                self.visit_expr(assignment.base_and_receiver());
                self.visit_property_key(assignment.referenced_name());
                self.visit_expr(assignment.rhs());
            }
            ExprIr::OrdinaryPropertyGetCapture(capture) => {
                self.track_storage(capture.receiver_storage_name());
                self.track_storage(capture.target_storage_name());
                self.track_storage(capture.key_storage_name());
                self.property_reads += 1;
                self.visit_expr(capture.base_and_receiver());
                self.visit_property_key(capture.referenced_name());
            }
            ExprIr::CapturedOrdinaryPropertyWrite(write) => {
                self.track_storage(write.receiver_storage_name());
                self.track_storage(write.target_storage_name());
                self.track_storage(write.key_storage_name());
                self.property_writes += 1;
                self.compound_assignments += 1;
                if write.static_key() == Some("prototype") {
                    self.prototype_writes += 1;
                }
                self.visit_expr(write.rhs());
            }
            ExprIr::OrdinaryPropertyNumericUpdate(update) => {
                self.property_reads += 1;
                self.property_writes += 1;
                match update.return_mode() {
                    UpdateReturnMode::Prefix => self.prefix_updates += 1,
                    UpdateReturnMode::Postfix => self.postfix_updates += 1,
                }
                self.visit_expr(update.base_and_receiver());
                self.visit_property_key(update.referenced_name());
            }
            ExprIr::OrdinaryPropertyEagerCompoundAssignment(assignment) => {
                self.track_storage(assignment.old_value_binding());
                self.property_reads += 1;
                self.property_writes += 1;
                self.compound_assignments += 1;
                self.visit_expr(assignment.base_and_receiver());
                self.visit_property_key(assignment.referenced_name());
                self.visit_expr(assignment.result());
            }
            ExprIr::UpdateIdentifier {
                name, return_mode, ..
            } => {
                self.track_storage(name);
                match return_mode {
                    UpdateReturnMode::Prefix => self.prefix_updates += 1,
                    UpdateReturnMode::Postfix => self.postfix_updates += 1,
                }
            }
            ExprIr::CompoundAssignIdentifier { name, value, .. } => {
                self.track_storage(name);
                self.compound_assignments += 1;
                self.visit_expr(value);
            }
            ExprIr::UnaryPlus { expr }
            | ExprIr::UnaryMinusNumeric { expr }
            | ExprIr::UnaryBitwiseNumeric { expr, .. }
            | ExprIr::StringFromCharCode { code: expr } => {
                self.visit_expr(expr);
            }
            ExprIr::LogicalNot { expr } => {
                self.visit_expr(expr);
            }
            ExprIr::SpecOperation { operands, .. } => {
                self.spec_operations += 1;
                if let ExprIr::SpecOperation { operation, .. } = &expr.expr {
                    match operation {
                        SpecOperationIr::Get | SpecOperationIr::GetV => {
                            self.property_reads += 1;
                            if matches!(operands.get(1).map(|operand| &operand.expr), Some(ExprIr::String(name)) if name == "prototype")
                            {
                                self.prototype_reads += 1;
                            }
                        }
                        SpecOperationIr::GetMethod => {
                            self.property_reads += 1;
                        }
                        SpecOperationIr::CreateDataPropertyOrThrow => {
                            self.property_writes += 1;
                        }
                        SpecOperationIr::CopyDataProperties => {
                            self.property_reads += 1;
                            self.property_writes += 1;
                        }
                        SpecOperationIr::Set => {
                            self.property_writes += 1;
                        }
                        SpecOperationIr::DeletePropertyOrThrow => {
                            self.deletes += 1;
                        }
                        SpecOperationIr::Call => {
                            self.calls += 1;
                            self.indirect_calls += 1;
                        }
                        SpecOperationIr::Construct => {
                            self.constructs += 1;
                            self.indirect_calls += 1;
                        }
                        SpecOperationIr::IsLooselyEqual => {
                            self.loose_equalities += 1;
                            if let [lhs, rhs] = operands.as_slice() {
                                if !lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                                    || !rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                                {
                                    self.heap_loose_equalities += 1;
                                    self.heap_coercions += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                for operand in operands {
                    self.visit_expr(operand);
                }
            }
            ExprIr::Void { expr } => {
                self.void_uses += 1;
                self.visit_expr(expr);
            }
            ExprIr::DeleteValue { expr } => {
                self.deletes += 1;
                self.visit_expr(expr);
            }
            ExprIr::DeleteIdentifier { name, .. } => {
                self.track_storage(name);
                self.deletes += 1;
                self.identifier_deletes += 1;
            }
            ExprIr::DeleteGlobalProperty { .. } => {
                self.deletes += 1;
                self.global_deletes += 1;
            }
            ExprIr::DeleteProperty { target, key, .. } => {
                self.deletes += 1;
                self.visit_expr(target);
                self.visit_property_key(key);
            }
            ExprIr::TypeOf { expr } => {
                self.typeof_uses += 1;
                self.visit_expr(expr);
            }
            ExprIr::TypeOfUnresolvedIdentifier { .. } => {
                self.typeof_uses += 1;
            }
            ExprIr::BinaryNumber { lhs, rhs, .. }
            | ExprIr::CoerciveAdd { lhs, rhs }
            | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
            | ExprIr::BitwiseNumeric { lhs, rhs, .. }
            | ExprIr::CompareNumber { lhs, rhs, .. }
            | ExprIr::CompareValue { lhs, rhs, .. }
            | ExprIr::StrictEquality { lhs, rhs, .. }
            | ExprIr::LooseEquality { lhs, rhs, .. }
            | ExprIr::AssertSameValue {
                actual: lhs,
                expected: rhs,
                ..
            }
            | ExprIr::StringConcat { lhs, rhs }
            | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
            | ExprIr::Comma { lhs, rhs } => {
                if matches!(&expr.expr, ExprIr::StringConcat { .. }) {
                    self.string_concats += 1;
                }
                if matches!(&expr.expr, ExprIr::CoerciveAdd { .. }) {
                    self.heap_to_primitives += 1;
                    self.heap_coercions += 1;
                }
                if matches!(&expr.expr, ExprIr::LooseEquality { .. }) {
                    self.loose_equalities += 1;
                    if !lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                        || !rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    {
                        self.heap_loose_equalities += 1;
                        self.heap_coercions += 1;
                    }
                }
                if matches!(&expr.expr, ExprIr::CoerciveBinaryNumber { .. }) {
                    self.coercive_numeric_ops += 1;
                    if !lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                        || !rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    {
                        self.heap_to_primitives += 1;
                        self.heap_coercions += 1;
                    }
                }
                if matches!(&expr.expr, ExprIr::CompareValue { .. }) {
                    self.coercive_relational_ops += 1;
                    if !lhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                        || !rhs.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)
                    {
                        self.heap_to_primitives += 1;
                        self.heap_coercions += 1;
                    }
                }
                if matches!(
                    &expr.expr,
                    ExprIr::LogicalShortCircuit {
                        op: LogicalBinaryOp::Coalesce,
                        ..
                    }
                ) {
                    self.nullish_ops += 1;
                }
                if matches!(&expr.expr, ExprIr::Comma { .. }) {
                    self.comma_ops += 1;
                }
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                self.track_storage(name);
                self.visit_expr(value);
                self.visit_expr(body);
            }
            ExprIr::ArrayDestructure { value, pattern, .. } => {
                self.track_array_pattern(pattern);
                self.assignments += 1;
                self.visit_expr(value);
                pattern.visit_expressions(&mut |expr| self.visit_expr(expr));
            }
            ExprIr::ObjectDestructure { value, pattern } => {
                self.track_object_pattern(pattern);
                self.assignments += 1;
                self.visit_expr(value);
                pattern.visit_expressions(&mut |expr| self.visit_expr(expr));
            }
            ExprIr::ObjectDestructuringOperation(operation) => {
                match operation.use_view() {
                    crate::ObjectDestructuringOperationView::GetV { .. } => {
                        self.property_reads += 1
                    }
                    crate::ObjectDestructuringOperationView::Rest { .. } => self.objects += 1,
                    crate::ObjectDestructuringOperationView::PutTarget { target, .. } => {
                        self.track_destructuring_target(target);
                        self.assignments += 1;
                    }
                }
                operation.visit_expressions(&mut |expr| self.visit_expr(expr));
            }
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.visit_expr(condition);
                self.visit_expr(then_expr);
                self.visit_expr(else_expr);
            }
            ExprIr::CallNamed { args, .. } => {
                self.calls += 1;
                self.global_default_this_calls += 1;
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprIr::RuntimeThrow { .. } => {}
            ExprIr::Arguments => {
                self.arguments_uses += 1;
            }
            ExprIr::CallIndirect {
                callee,
                this_arg,
                args,
                ..
            } => {
                // The summary historically counts callee/arguments only; the
                // storage and scalar-exponentiation queries also inspect the
                // retained receiver without changing historical summary counts.
                if self.tracked_storage.is_some() || self.scalar_exponentiation.is_some() {
                    if let Some(receiver) = this_arg {
                        self.visit_expr(receiver);
                    }
                }
                self.calls += 1;
                self.indirect_calls += 1;
                self.global_default_this_calls += 1;
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprIr::Construct { callee, args, .. } => {
                self.calls += 1;
                self.indirect_calls += 1;
                self.constructs += 1;
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprIr::ClassDefinition(class) => {
                if let Some(binding) = &class.name_binding {
                    self.track_storage(&binding.storage_name);
                    self.track_environment(Some(&binding.environment));
                }
                match &class.name_inference {
                    ClassNameInferenceIr::PropertyKeyBinding(name) => self.track_storage(name),
                    ClassNameInferenceIr::None | ClassNameInferenceIr::FieldInitializer(_) => {}
                }
                self.constructs += 1;
                self.classes += 1;
                self.class_exprs += 1;
                self.class_extends += usize::from(class.heritage_kind != ClassHeritageKind::None);
                self.null_heritage_classes +=
                    usize::from(class.heritage_kind == ClassHeritageKind::Null);
                for static_element in &class.element_plan.static_elements {
                    match static_element {
                        ClassStaticElementIr::Field(field) => {
                            self.class_fields += 1;
                            self.private_elements +=
                                usize::from(matches!(&field.key, ClassFieldKeyIr::Private(_)));
                        }
                        ClassStaticElementIr::AutoAccessorBacking(_) => {
                            self.class_fields += 1;
                            self.private_elements += 1;
                        }
                        ClassStaticElementIr::Block(_) => self.static_blocks += 1,
                    }
                }
                self.private_elements += class
                    .element_plan
                    .definitions
                    .iter()
                    .filter(|definition| {
                        matches!(definition, ClassElementDefinitionIr::PrivateMethod(_))
                    })
                    .count();
                if let Some(heritage) = &class.heritage {
                    self.visit_expr(heritage);
                }
                for definition in &class.element_plan.definitions {
                    match definition {
                        ClassElementDefinitionIr::PublicMethod(method) => {
                            self.visit_property_key(&method.key);
                        }
                        ClassElementDefinitionIr::AutoAccessor(accessor) => {
                            if let Some(key) = &accessor.computed_key {
                                self.visit_property_key(key);
                            }
                            self.private_elements +=
                                usize::from(matches!(accessor.key, ClassFieldKeyIr::Private(_)));
                        }
                        ClassElementDefinitionIr::ComputedFieldKey { key, .. } => {
                            if self.scalar_exponentiation.is_some() {
                                self.visit_property_key(key);
                            }
                        }
                        ClassElementDefinitionIr::PrivateMethod(_) => {}
                    }
                }
            }
            ExprIr::CallMethod {
                receiver,
                key,
                args,
            } => {
                self.calls += 1;
                self.indirect_calls += 1;
                self.method_calls += 1;
                self.visit_expr(receiver);
                self.visit_property_key(key);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprIr::This => {
                self.this_reads += 1;
            }
            ExprIr::ExecutionGlobalObject => {
                self.global_this_uses += 1;
            }
            ExprIr::NewTarget => {
                self.new_target_uses += 1;
            }
            ExprIr::SuperConstruct { args } => {
                self.super_uses += 1;
                self.calls += 1;
                self.indirect_calls += 1;
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprIr::SuperNewTarget | ExprIr::SuperConstructor => {
                self.super_uses += 1;
            }
            ExprIr::PreparedSuperConstruct(prepared) => {
                self.super_uses += 1;
                self.calls += 1;
                self.indirect_calls += 1;
                for operand in prepared.operands() {
                    self.visit_expr(operand);
                }
            }
            ExprIr::SuperPropertyRead { key, receiver } => {
                self.super_uses += 1;
                self.visit_property_key(key);
                self.visit_expr(receiver);
            }
            ExprIr::SuperPropertyWrite {
                key,
                receiver,
                value,
                ..
            } => {
                self.super_uses += 1;
                self.visit_property_key(key);
                self.visit_expr(receiver);
                self.visit_expr(value);
            }
            ExprIr::SuperPropertyMutation(mutation) => {
                self.super_uses += 1;
                self.visit_property_key(mutation.referenced_name());
                self.visit_expr(mutation.receiver());
                match mutation.operation() {
                    SuperPropertyMutationOperationIr::Capture(capture) => {
                        if capture.mode() == SuperPropertyCaptureMode::ReadBeforeRhs {
                            self.property_reads += 1;
                        }
                        self.track_storage(capture.receiver_storage_name());
                        self.track_storage(capture.base_storage_name());
                        self.track_storage(capture.referenced_name_storage_name());
                    }
                    SuperPropertyMutationOperationIr::PutCaptured { capture, value } => {
                        self.property_writes += 1;
                        self.track_storage(capture.receiver_storage_name());
                        self.track_storage(capture.base_storage_name());
                        self.track_storage(capture.referenced_name_storage_name());
                        self.visit_expr(value);
                    }
                    SuperPropertyMutationOperationIr::NumericUpdate { return_mode, .. } => {
                        self.property_reads += 1;
                        self.property_writes += 1;
                        match return_mode {
                            UpdateReturnMode::Prefix => self.prefix_updates += 1,
                            UpdateReturnMode::Postfix => self.postfix_updates += 1,
                        }
                    }
                    SuperPropertyMutationOperationIr::EagerCompound {
                        old_value_binding,
                        result,
                    } => {
                        self.property_reads += 1;
                        self.property_writes += 1;
                        self.track_storage(old_value_binding);
                        self.compound_assignments += 1;
                        self.visit_expr(result);
                    }
                }
            }
            ExprIr::PrivateRead {
                target,
                private_name_id: _,
            } => {
                self.private_elements += 1;
                self.visit_expr(target);
            }
            ExprIr::PrivateWrite {
                target,
                private_name_id: _,
                value,
            } => {
                self.private_elements += 1;
                self.visit_expr(target);
                self.visit_expr(value);
            }
            ExprIr::PrivateIn {
                private_name_id: _,
                rhs,
            } => {
                self.private_in_checks += 1;
                self.visit_expr(rhs);
            }
            ExprIr::InstanceOf { lhs, rhs } => {
                self.instanceofs += 1;
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            ExprIr::In { lhs, rhs } => {
                self.in_ops += 1;
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            ExprIr::Symbol { description } => {
                if let Some(description) = description {
                    self.visit_expr(description);
                }
            }
            ExprIr::Undefined
            | ExprIr::ArrayHole
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::BigInt(_)
            | ExprIr::WellKnownSymbol(_)
            | ExprIr::String(_)
            | ExprIr::FunctionValue(_)
            | ExprIr::ClassFieldNamedFunctionValue { .. }
            | ExprIr::Identifier(_) => {
                if let ExprIr::Identifier(name) = &expr.expr {
                    let matches = self.tracked_storage.as_deref() == Some(name);
                    self.storage_seen |= matches;
                    self.storage_read_seen |= matches;
                    self.storage_invalid_read |= matches
                        && (expr.kind != ValueKind::Dynamic
                            || expr.possible_kinds != KindSet::all_runtime_tags());
                }
            }
        }
    }

    fn visit_property_key(&mut self, key: &PropertyKeyIr) {
        match key {
            PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => {}
            PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
                self.visit_expr(expr)
            }
        }
    }
}

/// Prefix marking an [`ObjectShape::properties`] entry whose real key is a
/// well-known *symbol* rather than a string. The map is otherwise a string-key
/// map consulted by string-keyed property reads, so a symbol-keyed entry has to
/// live under a name no string-keyed read or write ever resolves — see
/// [`shape_property_name_is_symbol_keyed`], which every string-key path filters
/// on. Static analyses that legitimately want a symbol hook (ToPrimitive
/// inference, for one) ask for the prefixed name explicitly.
pub const SYMBOL_SHAPE_PROPERTY_PREFIX: &str = "@@";

/// Whether `name` addresses a symbol-keyed shape entry, i.e. one that a
/// string-keyed read or write must never see.
pub fn shape_property_name_is_symbol_keyed(name: &str) -> bool {
    name.starts_with(SYMBOL_SHAPE_PROPERTY_PREFIX)
}

pub(crate) fn read_heap_shape_property(
    shape: &HeapShape,
    key: &str,
) -> Option<ObjectShapeProperty> {
    if shape_property_name_is_symbol_keyed(key) {
        return None;
    }
    match shape {
        HeapShape::Object(object) => object.properties.get(key).cloned().or_else(|| {
            object
                .prototype
                .as_deref()
                .and_then(|proto| read_heap_shape_property(proto, key))
        }),
        HeapShape::Array(array) => array.properties.get(key).cloned().or_else(|| {
            array
                .prototype
                .as_deref()
                .and_then(|proto| read_heap_shape_property(proto, key))
        }),
    }
}

#[cfg(test)]
mod tests;
