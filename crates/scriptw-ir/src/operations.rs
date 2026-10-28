//! Spec-operation vocabulary and the catalog of ECMAScript abstract operations.
//!
//! The catalog is **evidence**, not documentation. A row that claims an
//! operation is implemented can only be produced from something that implements
//! it: a [`SpecOperationIr`] variant (which the shared expression emitter arm
//! in `scriptw-aot-wasm/src/operations.rs` must handle, or the match there fails
//! to build), a [`BackendSpecOperation`] variant joined to its backend emitter,
//! or an [`EmissionSite`] naming a statement-shaped emitter arm (which
//! `scriptw-aot-wasm/src/emission_sites.rs` joins to a real function path).
//! Everything else is a [`TrackedGapRow`], whose type has no field capable of
//! holding an implementation status.
//!
//! See `docs/rust-rewrite/contracts/spec-operations.md`.

use crate::{
    iterator_obligations::{site_emits, EmissionSite, IteratorObligation},
    ConformanceOwner, ValueKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingMode {
    Let,
    Const,
    Var,
}

/// The unary bitwise operations whose result follows `ToNumeric`'s closed
/// Number-or-BigInt domain.
///
/// Keeping complement distinct from [`BitwiseBinaryOp::Xor`] prevents lowering
/// from manufacturing a Number `-1` operand, which would turn every BigInt
/// complement into the binary operator's mixed-numeric TypeError.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryBitwiseOp {
    Complement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitwiseBinaryOp {
    And,
    Or,
    Xor,
    Shl,
    Shr,
    UShr,
}

/// The bitwise operators which have a BigInt semantic operation.
///
/// Unsigned right shift is deliberately absent: ECMA-262 defines `>>>` only
/// for Numbers, so a BigInt helper cannot accidentally be selected for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BigIntBitwiseOp {
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

impl BitwiseBinaryOp {
    pub const fn bigint_op(self) -> Option<BigIntBitwiseOp> {
        match self {
            Self::And => Some(BigIntBitwiseOp::And),
            Self::Or => Some(BigIntBitwiseOp::Or),
            Self::Xor => Some(BigIntBitwiseOp::Xor),
            Self::Shl => Some(BigIntBitwiseOp::Shl),
            Self::Shr => Some(BigIntBitwiseOp::Shr),
            Self::UShr => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationalBinaryOp {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualityBinaryOp {
    StrictEqual,
    StrictNotEqual,
    LooseEqual,
    LooseNotEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToPrimitiveHint {
    Default,
    Number,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalBinaryOp {
    And,
    Or,
    Coalesce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericUpdateOp {
    Increment,
    Decrement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericUpdateValueKind {
    Number,
    BigInt,
    Dynamic,
}

impl NumericUpdateValueKind {
    #[must_use]
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::Number => ValueKind::Number,
            Self::BigInt => ValueKind::BigInt,
            Self::Dynamic => ValueKind::Dynamic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateReturnMode {
    Prefix,
    Postfix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpecOperationFamily {
    TypeQuery,
    Conversion,
    Comparison,
    Environment,
    Object,
    Invocation,
    Iterator,
    Completion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EcmaLanguageType {
    Undefined,
    Null,
    Boolean,
    String,
    Symbol,
    Number,
    BigInt,
    Object,
}

impl EcmaLanguageType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Undefined => "Undefined",
            Self::Null => "Null",
            Self::Boolean => "Boolean",
            Self::String => "String",
            Self::Symbol => "Symbol",
            Self::Number => "Number",
            Self::BigInt => "BigInt",
            Self::Object => "Object",
        }
    }

    pub const fn is_primitive(self) -> bool {
        match self {
            Self::Undefined
            | Self::Null
            | Self::Boolean
            | Self::String
            | Self::Symbol
            | Self::Number
            | Self::BigInt => true,
            Self::Object => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Iterator Records (7.4)
// ---------------------------------------------------------------------------

/// `[[Iterator]]` — the name of the binding holding the iterator object.
///
/// The three slots of an Iterator Record are all binding names, i.e. all
/// `String`. Spelling them as three distinct newtypes moves the transposition
/// inward; it does not by itself kill it, because the construction site used to
/// read `IteratorSlot::new(iterator_binding), NextMethodSlot::new(next_binding)`
/// and swapping the two *strings* type-checked just as well as the original
/// three-`String` struct did.
///
/// So `new` is `pub(crate)` and the only callers are
/// `ScriptLowerer::alloc_iterator_slot` / `alloc_next_method_slot` /
/// `alloc_done_slot`, each of which allocates the binding it names. A slot can
/// therefore only be obtained from the allocation that names it, there is no
/// `String` left in the expression to transpose, and swapping two already-wrapped
/// values is `E0308`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IteratorSlot(String);

/// `[[NextMethod]]` — the name of the binding holding the once-read `next`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextMethodSlot(String);

/// `[[Done]]` — the name of the suspension slot holding the done flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoneSlot(String);

impl IteratorSlot {
    pub(crate) fn new(binding: String) -> Self {
        Self(binding)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl NextMethodSlot {
    pub(crate) fn new(binding: String) -> Self {
        Self(binding)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl DoneSlot {
    pub(crate) fn new(binding: String) -> Self {
        Self(binding)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An Iterator Record (7.4): `{ [[Iterator]], [[NextMethod]], [[Done]] }`.
///
/// `[[Done]]` at IR level is the *name of a suspension slot*, not a
/// compile-time boolean: the compiler never knows whether an iterator is
/// exhausted, only where the emitted code keeps that flag. There is therefore
/// no `is_done`/`mark_done` here, and there is no type parameter — the only
/// thing an IR-level record can hold is binding names.
///
/// There is also no `kind: Sync | Async`. The contract proposed one on the
/// grounds that "`kind` and the plan's existence cannot disagree", but a field
/// set by the single reachable constructor is a constant, and a constant cannot
/// disagree with anything — it is decoration by the same argument the contract
/// uses to delete `OperationLoweringStatus::SharedRustModel`. The sync/async
/// distinction is carried by *which plan owns the record*:
/// [`crate::AsyncFunctionForOfIteratorPlanIr`] and
/// [`crate::GeneratorForOfIteratorPlanIr`] own the resumable synchronous
/// record, while [`crate::AsyncForOfIteratorPlanIr`] owns the async-protocol
/// record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IteratorRecordIr {
    iterator: IteratorSlot,
    next_method: NextMethodSlot,
    done: DoneSlot,
}

impl IteratorRecordIr {
    pub fn new(iterator: IteratorSlot, next_method: NextMethodSlot, done: DoneSlot) -> Self {
        Self {
            iterator,
            next_method,
            done,
        }
    }

    pub fn iterator(&self) -> &IteratorSlot {
        &self.iterator
    }

    pub fn next_method(&self) -> &NextMethodSlot {
        &self.next_method
    }

    pub fn done(&self) -> &DoneSlot {
        &self.done
    }
}

// ---------------------------------------------------------------------------
// Completion Records (6.2.4)
// ---------------------------------------------------------------------------

macro_rules! completion_abrupt_kinds {
    ($( $variant:ident ),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum CompletionAbruptKind {
            $( $variant, )+
        }

        impl CompletionAbruptKind {
            /// Every abrupt completion kind, in 6.2.4 order.
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )+ ];
        }
    };
}

completion_abrupt_kinds! { Throw, Return, Break, Continue }

macro_rules! completion_kinds {
    ($(
        $variant:ident => {
            name: $name:literal,
            code: $code:literal,
            carries_value: $carries_value:literal,
            carries_target: $carries_target:literal,
        }
    );+ $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum CompletionKindIr {
            $( $variant, )+
        }

        impl CompletionKindIr {
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )+ ];

            pub const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $name, )+
                }
            }

            pub const fn abi_code(self) -> i64 {
                match self {
                    $( Self::$variant => $code, )+
                }
            }

            pub const fn carries_value(self) -> bool {
                match self {
                    $( Self::$variant => $carries_value, )+
                }
            }

            pub const fn carries_target(self) -> bool {
                match self {
                    $( Self::$variant => $carries_target, )+
                }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct CompletionAbiSlot(CompletionKindIr);

        impl CompletionAbiSlot {
            const fn new(kind: CompletionKindIr) -> Self {
                Self(kind)
            }

            pub const fn kind(self) -> CompletionKindIr {
                self.0
            }

            pub const fn name(self) -> &'static str {
                self.0.name()
            }

            pub const fn code(self) -> i64 {
                self.0.abi_code()
            }

            pub const fn carries_value(self) -> bool {
                self.0.carries_value()
            }

            pub const fn carries_target(self) -> bool {
                self.0.carries_target()
            }
        }

        pub const COMPLETION_ABI_SLOTS: &[CompletionAbiSlot] = &[
            $( completion_abi_slot(CompletionKindIr::$variant), )+
        ];

        pub const fn completion_abi_slot(kind: CompletionKindIr) -> CompletionAbiSlot {
            CompletionAbiSlot::new(kind)
        }

        const _: () = {
            let mut index = 0;
            while index < CompletionKindIr::ALL.len() {
                assert!(CompletionKindIr::ALL[index].abi_code() == index as i64);
                index += 1;
            }
        };
    };
}

completion_kinds! {
    Normal => {
        name: "normal",
        code: 0,
        carries_value: true,
        carries_target: false,
    };
    Throw => {
        name: "throw",
        code: 1,
        carries_value: true,
        carries_target: false,
    };
    Return => {
        name: "return",
        code: 2,
        carries_value: true,
        carries_target: false,
    };
    Break => {
        name: "break",
        code: 3,
        carries_value: true,
        carries_target: true,
    };
    Continue => {
        name: "continue",
        code: 4,
        carries_value: true,
        carries_target: true,
    };
    Empty => {
        name: "empty",
        code: 5,
        carries_value: false,
        carries_target: false,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionRecordIr<T> {
    Normal {
        value: Option<T>,
    },
    Throw {
        value: T,
    },
    Return {
        value: T,
    },
    Break {
        value: Option<T>,
        target: Option<String>,
    },
    Continue {
        value: Option<T>,
        target: Option<String>,
    },
    Empty,
}

impl<T> CompletionRecordIr<T> {
    pub const fn normal(value: T) -> Self {
        Self::Normal { value: Some(value) }
    }

    pub const fn empty_normal() -> Self {
        Self::Normal { value: None }
    }

    pub const fn throw(value: T) -> Self {
        Self::Throw { value }
    }

    pub const fn return_(value: T) -> Self {
        Self::Return { value }
    }

    pub fn break_(value: Option<T>, target: Option<String>) -> Self {
        Self::Break { value, target }
    }

    pub fn continue_(value: Option<T>, target: Option<String>) -> Self {
        Self::Continue { value, target }
    }

    pub const fn empty() -> Self {
        Self::Empty
    }

    pub const fn kind(&self) -> CompletionKindIr {
        match self {
            Self::Normal { .. } => CompletionKindIr::Normal,
            Self::Throw { .. } => CompletionKindIr::Throw,
            Self::Return { .. } => CompletionKindIr::Return,
            Self::Break { .. } => CompletionKindIr::Break,
            Self::Continue { .. } => CompletionKindIr::Continue,
            Self::Empty => CompletionKindIr::Empty,
        }
    }

    pub const fn is_abrupt(&self) -> bool {
        match self {
            Self::Normal { .. } | Self::Empty => false,
            Self::Throw { .. }
            | Self::Return { .. }
            | Self::Break { .. }
            | Self::Continue { .. } => true,
        }
    }

    pub const fn value(&self) -> Option<&T> {
        match self {
            Self::Normal { value } | Self::Break { value, .. } | Self::Continue { value, .. } => {
                value.as_ref()
            }
            Self::Throw { value } | Self::Return { value } => Some(value),
            Self::Empty => None,
        }
    }

    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Break { target, .. } | Self::Continue { target, .. } => target.as_deref(),
            Self::Normal { .. } | Self::Throw { .. } | Self::Return { .. } | Self::Empty => None,
        }
    }

    pub fn update_empty(self, replacement: T) -> Self {
        match self {
            Self::Normal { value: None } => Self::Normal {
                value: Some(replacement),
            },
            Self::Break {
                value: None,
                target,
            } => Self::Break {
                value: Some(replacement),
                target,
            },
            Self::Continue {
                value: None,
                target,
            } => Self::Continue {
                value: Some(replacement),
                target,
            },
            Self::Empty => Self::Normal {
                value: Some(replacement),
            },
            completion => completion,
        }
    }

    pub fn map_value<U>(self, mut map: impl FnMut(T) -> U) -> CompletionRecordIr<U> {
        match self {
            Self::Normal { value } => CompletionRecordIr::Normal {
                value: value.map(map),
            },
            Self::Throw { value } => CompletionRecordIr::Throw { value: map(value) },
            Self::Return { value } => CompletionRecordIr::Return { value: map(value) },
            Self::Break { value, target } => CompletionRecordIr::Break {
                value: value.map(map),
                target,
            },
            Self::Continue { value, target } => CompletionRecordIr::Continue {
                value: value.map(map),
                target,
            },
            Self::Empty => CompletionRecordIr::Empty,
        }
    }
}

// ---------------------------------------------------------------------------
// Part A — the catalog as evidence
// ---------------------------------------------------------------------------

/// Proof that a [`SpecOperationIr`] variant stands behind a catalog row.
///
/// The field is private and there is no public constructor: outside this module
/// the only way to obtain one is [`SpecOperationIr::emitter_evidence`], so
/// `OperationLoweringStatus::SharedWasmEmitter(..)` cannot be forged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitterEvidence {
    operation: SpecOperationIr,
}

impl EmitterEvidence {
    pub const fn operation(self) -> SpecOperationIr {
        self.operation
    }
}

/// Proof that a [`BackendSpecOperation`] variant stands behind a catalog row.
///
/// The field is private and there is no public constructor. The backend joins
/// every operation in this closed domain to a concrete emitter function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendEmitterEvidence {
    operation: BackendSpecOperation,
}

impl BackendEmitterEvidence {
    pub const fn operation(self) -> BackendSpecOperation {
        self.operation
    }
}

/// Why an operation has no implementation. Closed; no free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackedGapReason {
    /// No [`SpecOperationIr`] or [`BackendSpecOperation`] variant and no
    /// emitter arm implements it.
    NoImplementation,
    /// A Rust model type exists in `scriptw-ir` but nothing on the product path
    /// constructs it.
    ModelWithoutCallSite,
}

impl TrackedGapReason {
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoImplementation => "no implementation",
            Self::ModelWithoutCallSite => "model without call site",
        }
    }
}

/// How an operation reaches emitted Wasm — or the honest statement that it does
/// not.
///
/// Four variants, and every implementation-claiming variant carries evidence
/// that cannot be produced by hand:
///
/// - `SharedWasmEmitter` needs an [`EmitterEvidence`], whose only constructor is
///   [`SpecOperationIr::emitter_evidence`]. What that evidence proves is
///   name resolution and match exhaustiveness, **not** that the arm emits
///   anything: an arm may satisfy both backend matches with
///   `Err(EmitError::unsupported(..))`. Closing that is ledger **L2**.
/// - `SharedBackendEmitter` needs [`BackendEmitterEvidence`], whose only
///   constructor is [`BackendSpecOperation::emitter_evidence`]. The backend
///   resolves every operation in that closed domain to a concrete emitter.
/// - `StatementEmission` needs at least one [`EmissionSite`], every variant of
///   which is joined to a real function path by
///   `scriptw-aot-wasm/src/emission_sites.rs::emission_sites_are_backed`. It is
///   a *slice* because the truth is a set: `GetIterator` is emitted by the sync
///   for-of arm, the async for-of arm and the array-destructuring arm. A single
///   `site` made the catalog read as if `for await` performed no `GetIterator`,
///   which is a fresh instance of the false-claim defect this area removes.
///
/// There is deliberately no `CatalogOnly` (a variant whose only semantics was
/// "must never occur" — that is a missing variant, not a runtime check) and no
/// `SharedRustModel` (measured: zero rows for which it is true; reintroduce it
/// in the same patch that gives some model type its first product call site).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationLoweringStatus {
    SharedWasmEmitter(EmitterEvidence),
    SharedBackendEmitter(BackendEmitterEvidence),
    StatementEmission(&'static [EmissionSite]),
    TrackedGap {
        reason: TrackedGapReason,
        owner: ConformanceOwner,
    },
}

/// The normal codomain of an abstract operation. Closed: exactly the shapes the
/// catalog's rows use.
///
/// This exists so that the old `assert!(!entry.normal_result.is_empty())` has
/// nothing to check — emptiness is unrepresentable — and so that a typo in a
/// codomain is `E0599` rather than a string nobody reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NormalResult {
    Unused,
    Boolean,
    BooleanOrUndefined,
    String,
    Number,
    NumberOrBigInt,
    BigInt,
    Integer,
    Object,
    ObjectOrUndefined,
    ObjectOrFalse,
    Constructor,
    CallableOrUndefined,
    PropertyKey,
    PropertyDescriptor,
    LanguageValue,
    LanguageType,
    IteratorRecord,
    CompletionRecord,
}

impl NormalResult {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unused => "Unused",
            Self::Boolean => "Boolean",
            Self::BooleanOrUndefined => "Boolean or Undefined",
            Self::String => "String",
            Self::Number => "Number",
            Self::NumberOrBigInt => "Number or BigInt",
            Self::BigInt => "BigInt",
            Self::Integer => "Integer",
            Self::Object => "Object",
            Self::ObjectOrUndefined => "Object or Undefined",
            Self::ObjectOrFalse => "Object or false",
            Self::Constructor => "Constructor",
            Self::CallableOrUndefined => "Callable or Undefined",
            Self::PropertyKey => "PropertyKey",
            Self::PropertyDescriptor => "PropertyDescriptor",
            Self::LanguageValue => "ECMAScript language value",
            Self::LanguageType => "Type",
            Self::IteratorRecord => "IteratorRecord",
            Self::CompletionRecord => "Completion Record",
        }
    }
}

/// The operand domain of a shared abstract operation.
///
/// This is deliberately more specific than an integer arity. `Get` and an
/// equality comparison both take two operands, but interchanging their rows in
/// the catalog would still be a lie. The domain names the spec roles and owns
/// the arity check used by the backend, so a new operation cannot acquire an
/// emitter arm without choosing both its operand shape and its variadic rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationDomain {
    Value,
    ValuePair,
    ValueAndInteger,
    ObjectAndPropertyKey,
    WithBindingObjectAndName,
    ValueAndPropertyKey,
    ObjectPropertyKeyValue,
    ObjectAndSourceValue,
    CallableThisAndArguments,
    ConstructorAndArguments,
}

impl OperationDomain {
    pub const fn accepts(self, operand_count: usize) -> bool {
        match self {
            Self::Value => operand_count == 1,
            Self::ValuePair
            | Self::ValueAndInteger
            | Self::ObjectAndPropertyKey
            | Self::WithBindingObjectAndName
            | Self::ValueAndPropertyKey
            | Self::ObjectAndSourceValue => operand_count == 2,
            Self::ObjectPropertyKeyValue => operand_count == 3,
            Self::CallableThisAndArguments => operand_count >= 2,
            Self::ConstructorAndArguments => operand_count >= 1,
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Value => "1 value",
            Self::ValuePair => "2 values",
            Self::ValueAndInteger => "a value and an integer",
            Self::ObjectAndPropertyKey => "an object and a property key",
            Self::WithBindingObjectAndName => "a with binding object and a String binding name",
            Self::ValueAndPropertyKey => "a value and a property key",
            Self::ObjectPropertyKeyValue => "an object, a property key, and a value",
            Self::ObjectAndSourceValue => "a target object and a source value",
            Self::CallableThisAndArguments => "a callable, this value, and zero or more arguments",
            Self::ConstructorAndArguments => "a constructor and zero or more arguments",
        }
    }
}

/// The abrupt-completion capability of a shared operation.
///
/// Shared value-producing operations either cannot leave abruptly or may throw.
/// Return/break/continue belong to statement-shaped completion rows and remain
/// represented by [`CompletionAbruptKind`]. Keeping this distinction as an
/// enum, rather than a boolean or a hand-written slice, gives the backend a
/// closed routing contract and makes a newly introduced capability an
/// exhaustive-match error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AbruptCapability {
    Infallible,
    MayThrow,
}

impl AbruptCapability {
    pub const fn completions(self) -> &'static [CompletionAbruptKind] {
        match self {
            Self::Infallible => NO_ABRUPT,
            Self::MayThrow => MAY_THROW,
        }
    }

    pub const fn may_throw(self) -> bool {
        match self {
            Self::Infallible => false,
            Self::MayThrow => true,
        }
    }
}

/// The complete, typed signature of one shared-emitter catalog row.
///
/// Values are produced only by the `spec_operations!` and
/// `backend_spec_operations!` declarations below. Within each domain, the enum
/// variants, canonical enumeration, names, families, operand domains, normal
/// results, and abrupt capabilities therefore have one source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationDescriptor {
    name: &'static str,
    family: SpecOperationFamily,
    domain: OperationDomain,
    normal_result: NormalResult,
    abrupt: AbruptCapability,
}

impl OperationDescriptor {
    const fn new(
        name: &'static str,
        family: SpecOperationFamily,
        domain: OperationDomain,
        normal_result: NormalResult,
        abrupt: AbruptCapability,
    ) -> Self {
        Self {
            name,
            family,
            domain,
            normal_result,
            abrupt,
        }
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn family(self) -> SpecOperationFamily {
        self.family
    }

    pub const fn domain(self) -> OperationDomain {
        self.domain
    }

    pub const fn normal_result(self) -> NormalResult {
        self.normal_result
    }

    pub const fn abrupt(self) -> AbruptCapability {
        self.abrupt
    }
}

/// Declares [`SpecOperationIr`], its enumeration and its complete descriptor
/// from **one** row list.
///
/// Ledger **L1** used to record "you added a variant and forgot to add it to
/// `ALL`" as the one drift this area could not turn into a compile error, and
/// the test that was supposed to catch it could not: its final assertion was
/// `ALL.len() == 29`, which is precisely the number an omission preserves. With
/// the enum and `ALL` being two expansions of the same `$(...)+` sequence, the
/// omission is not expressible, the test is deleted, and L1 is discharged by
/// construction rather than by a runtime check.
///
/// A payload-carrying variant contributes exactly one row, because the catalog
/// is keyed by operation *name* and `ToPrimitive(hint)` is one operation with
/// one signature. Every descriptor column is mandatory macro syntax: adding a
/// variant while omitting its domain or abrupt capability is a compile error,
/// not a catalog row that silently inherits a default.
macro_rules! spec_operations {
    ($(
        $variant:ident $(( $payload:ty ))? => {
            canonical: $canonical:expr,
            name: $name:literal,
            family: $family:ident,
            domain: $domain:ident,
            result: $result:ident,
            abrupt: $abrupt:ident,
        }
    );+ $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum SpecOperationIr {
            $( $variant $(( $payload ))? , )+
        }

        impl SpecOperationIr {
            /// Every operation, in catalog order — one entry per operation, not
            /// per variant: payload-carrying variants collapse to their
            /// canonical expression.
            pub const ALL: &'static [SpecOperationIr] = &[ $( $canonical , )+ ];

            pub const fn descriptor(self) -> OperationDescriptor {
                match self {
                    $(
                        SpecOperationIr::$variant { .. } => OperationDescriptor::new(
                            $name,
                            SpecOperationFamily::$family,
                            OperationDomain::$domain,
                            NormalResult::$result,
                            AbruptCapability::$abrupt,
                        ),
                    )+
                }
            }
        }
    };
}

spec_operations! {
    IsCallable => {
        canonical: SpecOperationIr::IsCallable,
        name: "IsCallable",
        family: TypeQuery,
        domain: Value,
        result: Boolean,
        abrupt: Infallible,
    };
    IsConstructor => {
        canonical: SpecOperationIr::IsConstructor,
        name: "IsConstructor",
        family: TypeQuery,
        domain: Value,
        result: Boolean,
        abrupt: Infallible,
    };
    IsPropertyKey => {
        canonical: SpecOperationIr::IsPropertyKey,
        name: "IsPropertyKey",
        family: TypeQuery,
        domain: Value,
        result: Boolean,
        abrupt: Infallible,
    };
    ToPrimitive(ToPrimitiveHint) => {
        canonical: SpecOperationIr::ToPrimitive(ToPrimitiveHint::Default),
        name: "ToPrimitive",
        family: Conversion,
        domain: Value,
        result: LanguageValue,
        abrupt: MayThrow,
    };
    ToBoolean => {
        canonical: SpecOperationIr::ToBoolean,
        name: "ToBoolean",
        family: Conversion,
        domain: Value,
        result: Boolean,
        abrupt: Infallible,
    };
    ToNumeric => {
        canonical: SpecOperationIr::ToNumeric,
        name: "ToNumeric",
        family: Conversion,
        domain: Value,
        result: NumberOrBigInt,
        abrupt: MayThrow,
    };
    ToNumber => {
        canonical: SpecOperationIr::ToNumber,
        name: "ToNumber",
        family: Conversion,
        domain: Value,
        result: Number,
        abrupt: MayThrow,
    };
    ToBigInt => {
        canonical: SpecOperationIr::ToBigInt,
        name: "ToBigInt",
        family: Conversion,
        domain: Value,
        result: BigInt,
        abrupt: MayThrow,
    };
    ToString => {
        canonical: SpecOperationIr::ToString,
        name: "ToString",
        family: Conversion,
        domain: Value,
        result: String,
        abrupt: MayThrow,
    };
    ToObject => {
        canonical: SpecOperationIr::ToObject,
        name: "ToObject",
        family: Conversion,
        domain: Value,
        result: Object,
        abrupt: MayThrow,
    };
    ToPropertyKey => {
        canonical: SpecOperationIr::ToPropertyKey,
        name: "ToPropertyKey",
        family: Conversion,
        domain: Value,
        result: PropertyKey,
        abrupt: MayThrow,
    };
    // 7.1.5's codomain is ℤ ∪ {+∞, −∞}; 7.1.20 and 7.1.22 land in
    // [0, 2^53−1] ∩ ℤ. The descriptor reflects exactly that, and a
    // `const _` at the end of this file asserts it. See
    // `docs/rust-rewrite/contracts/numeric-conversion-codomains.md` §2.6.
    ToIntegerOrInfinity => {
        canonical: SpecOperationIr::ToIntegerOrInfinity,
        name: "ToIntegerOrInfinity",
        family: Conversion,
        domain: Value,
        result: Number,
        abrupt: MayThrow,
    };
    ToLength => {
        canonical: SpecOperationIr::ToLength,
        name: "ToLength",
        family: Conversion,
        domain: Value,
        result: Integer,
        abrupt: MayThrow,
    };
    ToIndex => {
        canonical: SpecOperationIr::ToIndex,
        name: "ToIndex",
        family: Conversion,
        domain: Value,
        result: Integer,
        abrupt: MayThrow,
    };
    SameValue => {
        canonical: SpecOperationIr::SameValue,
        name: "SameValue",
        family: Comparison,
        domain: ValuePair,
        result: Boolean,
        abrupt: Infallible,
    };
    SameValueZero => {
        canonical: SpecOperationIr::SameValueZero,
        name: "SameValueZero",
        family: Comparison,
        domain: ValuePair,
        result: Boolean,
        abrupt: Infallible,
    };
    StrictEqualityComparison => {
        canonical: SpecOperationIr::StrictEqualityComparison,
        name: "StrictEqualityComparison",
        family: Comparison,
        domain: ValuePair,
        result: Boolean,
        abrupt: Infallible,
    };
    IsLooselyEqual => {
        canonical: SpecOperationIr::IsLooselyEqual,
        name: "IsLooselyEqual",
        family: Comparison,
        domain: ValuePair,
        result: Boolean,
        abrupt: MayThrow,
    };
    Get => {
        canonical: SpecOperationIr::Get,
        name: "Get",
        family: Object,
        domain: ObjectAndPropertyKey,
        result: LanguageValue,
        abrupt: MayThrow,
    };
    GetV => {
        canonical: SpecOperationIr::GetV,
        name: "GetV",
        family: Object,
        domain: ValueAndPropertyKey,
        result: LanguageValue,
        abrupt: MayThrow,
    };
    Set => {
        canonical: SpecOperationIr::Set,
        name: "Set",
        family: Object,
        domain: ObjectPropertyKeyValue,
        result: Boolean,
        abrupt: MayThrow,
    };
    HasProperty => {
        canonical: SpecOperationIr::HasProperty,
        name: "HasProperty",
        family: Object,
        domain: ObjectAndPropertyKey,
        result: Boolean,
        abrupt: MayThrow,
    };
    WithEnvironmentHasBinding => {
        canonical: SpecOperationIr::WithEnvironmentHasBinding,
        name: "WithEnvironmentHasBinding",
        family: Environment,
        domain: WithBindingObjectAndName,
        result: Boolean,
        abrupt: MayThrow,
    };
    WithEnvironmentGetBindingValueSloppy => {
        canonical: SpecOperationIr::WithEnvironmentGetBindingValueSloppy,
        name: "WithEnvironmentGetBindingValueSloppy",
        family: Environment,
        domain: WithBindingObjectAndName,
        result: LanguageValue,
        abrupt: MayThrow,
    };
    WithEnvironmentGetBindingValueStrict => {
        canonical: SpecOperationIr::WithEnvironmentGetBindingValueStrict,
        name: "WithEnvironmentGetBindingValueStrict",
        family: Environment,
        domain: WithBindingObjectAndName,
        result: LanguageValue,
        abrupt: MayThrow,
    };
    HasOwnProperty => {
        canonical: SpecOperationIr::HasOwnProperty,
        name: "HasOwnProperty",
        family: Object,
        domain: ObjectAndPropertyKey,
        result: Boolean,
        abrupt: MayThrow,
    };
    DeletePropertyOrThrow => {
        canonical: SpecOperationIr::DeletePropertyOrThrow,
        name: "DeletePropertyOrThrow",
        family: Object,
        domain: ObjectAndPropertyKey,
        result: Boolean,
        abrupt: MayThrow,
    };
    CreateDataPropertyOrThrow => {
        canonical: SpecOperationIr::CreateDataPropertyOrThrow,
        name: "CreateDataPropertyOrThrow",
        family: Object,
        domain: ObjectPropertyKeyValue,
        result: Unused,
        abrupt: MayThrow,
    };
    CopyDataProperties => {
        canonical: SpecOperationIr::CopyDataProperties,
        name: "CopyDataProperties",
        family: Object,
        domain: ObjectAndSourceValue,
        result: Unused,
        abrupt: MayThrow,
    };
    GetMethod => {
        canonical: SpecOperationIr::GetMethod,
        name: "GetMethod",
        family: Invocation,
        domain: ValueAndPropertyKey,
        result: CallableOrUndefined,
        abrupt: MayThrow,
    };
    Call => {
        canonical: SpecOperationIr::Call,
        name: "Call",
        family: Invocation,
        domain: CallableThisAndArguments,
        result: LanguageValue,
        abrupt: MayThrow,
    };
    Construct => {
        canonical: SpecOperationIr::Construct,
        name: "Construct",
        family: Invocation,
        domain: ConstructorAndArguments,
        result: Object,
        abrupt: MayThrow,
    };
}

/// Declares the closed set of catalog operations emitted by dedicated backend
/// functions rather than the shared [`SpecOperationIr`] expression emitter.
///
/// The enum, its canonical enumeration, and every descriptor column come from
/// the same row list. Adding an operation without describing its typed contract
/// therefore fails to build.
macro_rules! backend_spec_operations {
    ($(
        $variant:ident => {
            name: $name:literal,
            family: $family:ident,
            domain: $domain:ident,
            result: $result:ident,
            abrupt: $abrupt:ident,
        }
    );+ $(;)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum BackendSpecOperation {
            $( $variant, )+
        }

        impl BackendSpecOperation {
            pub const ALL: &'static [BackendSpecOperation] = &[
                $( BackendSpecOperation::$variant, )+
            ];

            pub const fn descriptor(self) -> OperationDescriptor {
                match self {
                    $(
                        BackendSpecOperation::$variant => OperationDescriptor::new(
                            $name,
                            SpecOperationFamily::$family,
                            OperationDomain::$domain,
                            NormalResult::$result,
                            AbruptCapability::$abrupt,
                        ),
                    )+
                }
            }
        }
    };
}

backend_spec_operations! {
    ArraySpeciesCreate => {
        name: "ArraySpeciesCreate",
        family: Invocation,
        domain: ValueAndInteger,
        result: Object,
        abrupt: MayThrow,
    };
    ToPropertyDescriptor => {
        name: "ToPropertyDescriptor",
        family: Object,
        domain: Value,
        result: PropertyDescriptor,
        abrupt: MayThrow,
    };
}

const NO_ABRUPT: &[CompletionAbruptKind] = &[];
const MAY_THROW: &[CompletionAbruptKind] = &[CompletionAbruptKind::Throw];
/// All four abrupt kinds, as a row value. Defined *from*
/// [`CompletionAbruptKind::ALL`] rather than beside it: two hand-written lists
/// of the same four kinds is the drift shape this area exists to remove.
const CONTROL_COMPLETIONS: &[CompletionAbruptKind] = CompletionAbruptKind::ALL;

impl SpecOperationIr {
    pub const fn name(self) -> &'static str {
        self.descriptor().name()
    }

    pub const fn family(self) -> SpecOperationFamily {
        self.descriptor().family()
    }

    pub const fn domain(self) -> OperationDomain {
        self.descriptor().domain()
    }

    pub const fn accepts_operand_count(self, operand_count: usize) -> bool {
        self.domain().accepts(operand_count)
    }

    pub const fn operand_domain_description(self) -> &'static str {
        self.domain().description()
    }

    pub const fn normal_result(self) -> NormalResult {
        self.descriptor().normal_result()
    }

    pub const fn abrupt_capability(self) -> AbruptCapability {
        self.descriptor().abrupt()
    }

    pub const fn may_throw(self) -> bool {
        self.abrupt_capability().may_throw()
    }

    /// The abrupt completions this operation may return.
    ///
    /// This is a **total function of the variant**, not a per-row argument.
    /// Commit `ca09433c1` (ToPrimitive abrupt completions compiled as if they
    /// could not escape) is the shape of the defect a free `abrupt` field
    /// allows; here there is no parameter to get wrong.
    ///
    /// Whether these sets agree with what the emitter arms actually emit is
    /// **ledger L2** — `scriptw-ir` cannot see `scriptw-aot-wasm`.
    pub const fn abrupt(self) -> &'static [CompletionAbruptKind] {
        self.abrupt_capability().completions()
    }

    /// The only constructor of [`EmitterEvidence`].
    ///
    /// Total on the variant, and therefore weaker than it looks: it proves that
    /// this name is a variant of the enum the two `scriptw-aot-wasm` matches
    /// must handle exhaustively, not that the arm emits anything. See ledger
    /// **L2**.
    pub const fn emitter_evidence(self) -> EmitterEvidence {
        EmitterEvidence { operation: self }
    }

    /// The row. Not a table entry that happens to match the variant — the row
    /// *is* the variant, so a variant without a row is not expressible, and a
    /// row whose signature disagrees with the variant is not expressible
    /// either. The row's `name` is taken from the evidence's own operation, so
    /// it cannot name one operation while carrying another's evidence.
    pub const fn catalog_entry(self) -> SpecOperationCatalogEntry {
        let evidence = self.emitter_evidence();
        SpecOperationCatalogEntry {
            name: evidence.operation().name(),
            family: self.family(),
            normal_result: self.normal_result(),
            abrupt: self.abrupt(),
            lowering_status: OperationLoweringStatus::SharedWasmEmitter(evidence),
            source: RowSource::DerivedFromOperation,
        }
    }
}

impl BackendSpecOperation {
    pub const fn name(self) -> &'static str {
        self.descriptor().name()
    }

    /// The only constructor of [`BackendEmitterEvidence`].
    pub const fn emitter_evidence(self) -> BackendEmitterEvidence {
        BackendEmitterEvidence { operation: self }
    }

    pub const fn catalog_entry(self) -> SpecOperationCatalogEntry {
        let descriptor = self.descriptor();
        let evidence = self.emitter_evidence();
        SpecOperationCatalogEntry {
            name: evidence.operation().name(),
            family: descriptor.family(),
            normal_result: descriptor.normal_result(),
            abrupt: descriptor.abrupt().completions(),
            lowering_status: OperationLoweringStatus::SharedBackendEmitter(evidence),
            source: RowSource::DerivedFromBackendOperation,
        }
    }
}

/// Which row constructor produced a [`SpecOperationCatalogEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowSource {
    /// [`SpecOperationIr::catalog_entry`] — the row *is* the variant.
    DerivedFromOperation,
    /// [`BackendSpecOperation::catalog_entry`] — the row *is* the variant.
    DerivedFromBackendOperation,
    /// The crate-private `StatementEmissionRow::into_entry` constructor.
    StatementEmissionTable,
    /// The crate-private `TrackedGapRow::into_entry` constructor.
    TrackedGapTable,
}

/// One row of [`SPEC_OPERATION_CATALOG`].
///
/// **Every field is private and there is no public constructor.** The evidence
/// design protects `EmitterEvidence`, `BackendEmitterEvidence`, and
/// `EmissionSite`, but that is worth nothing while the entry those produce can
/// be written out by hand: with `pub` fields this compiled, and forged an
/// implementation claim for an operation nobody implemented —
///
/// ```compile_fail
/// use scriptw_ir::{SpecOperationCatalogEntry, SpecOperationIr};
/// let _ = SpecOperationCatalogEntry {
///     name: "SpeciesConstructor",
///     ..SpecOperationIr::ToString.catalog_entry()
/// };
/// ```
///
/// The private `source` field closes both halves of that hole: the struct is
/// unconstructible outside this module, so the only producers are
/// [`SpecOperationIr::catalog_entry`], [`BackendSpecOperation::catalog_entry`],
/// `StatementEmissionRow::into_entry`, and `TrackedGapRow::into_entry`; and the
/// derived constructors take `name` from the evidence rather than from a free
/// field, so evidence and name cannot disagree (const assert J6 checks that for
/// every assembled row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecOperationCatalogEntry {
    name: &'static str,
    family: SpecOperationFamily,
    normal_result: NormalResult,
    abrupt: &'static [CompletionAbruptKind],
    lowering_status: OperationLoweringStatus,
    source: RowSource,
}

impl SpecOperationCatalogEntry {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn family(&self) -> SpecOperationFamily {
        self.family
    }

    pub const fn normal_result(&self) -> NormalResult {
        self.normal_result
    }

    pub const fn abrupt(&self) -> &'static [CompletionAbruptKind] {
        self.abrupt
    }

    pub const fn lowering_status(&self) -> OperationLoweringStatus {
        self.lowering_status
    }

    pub const fn source(&self) -> RowSource {
        self.source
    }
}

/// How an emitter arm gets an abrupt completion out of itself.
///
/// This is the column that makes [`StatementEmissionRow::abrupt`] mean
/// something. Before it, `abrupt` had **zero** readers anywhere in the
/// workspace: a machine-readable statement of which operations may throw, kept
/// alive by `pub`. A row that says "this operation may throw" must also say
/// what happens to the throw.
///
/// Closed, and each variant names emitted code that was read rather than
/// assumed. What the const assertions below prove is a **table** claim — that a
/// row states a discipline consistent with its `abrupt` set. They do *not*
/// prove that the named emitter body implements that discipline: `scriptw-ir`
/// cannot see `scriptw-aot-wasm`, the dependency runs the other way. That
/// boundary is ledger **L2**, restated as **IC-1** in the iterator-close
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AbruptDiscipline {
    /// The operation has no abrupt completion at all; `abrupt` must be empty.
    NoAbruptExit,
    /// Abrupt completions leave through the shared current-completion channel
    /// and close nothing: any iterator in scope belongs to the caller, and
    /// 7.4.8/7.4.9 have already set `[[Done]]` on every path that can fail.
    /// (`emit_propagate_throw_from_locals_if_needed`,
    /// `emit_propagate_current_completion_if_throw`.)
    PropagateWithoutClose,
    /// The arm closes the acquired iterator and routes the two completion
    /// classes 7.4.11 step 4 distinguishes to the two helpers that implement
    /// them: `emit_iterator_close_preserving_current_throw` when a `throw` is
    /// already in flight (step 5 — the original completion wins and the close's
    /// own error is swallowed), and `emit_iterator_close` otherwise (steps 6-8 —
    /// the close's error is *not* swallowed).
    ///
    /// It does **not** claim that every site exercises the `break`/`return`
    /// branch. `compile_for_of_iterator` does: `control_flow.rs` branches on
    /// `saved_completion == THROW` and its else-arm really is the
    /// break/return/continue close. `compile_array_destructure_from_value_locals`
    /// does not: there the `emit_iterator_close` call is the **normal**-completion
    /// close 8.6.3 step 5 requires, and the abrupt arm is unconditionally the
    /// step-5 helper for every abrupt kind. Both are step 4's two halves; only
    /// one site has a third case.
    CloseOnAbruptExitWithStep4Precedence,
}

impl AbruptDiscipline {
    /// The rendered name, and — more usefully — the exhaustive match that makes
    /// the domain closed in practice.
    ///
    /// J12 reads `discipline` through `matches!`, which has an implicit
    /// catch-all and so would silently accept a fourth variant. This arm list
    /// has none, so adding a discipline without saying what it is called is
    /// `E0004` rather than a variant that no assertion constrains. That is the
    /// same reason `CompletionKindIr`, `IteratorObligation` and
    /// `IntactnessPremise` each carry one.
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoAbruptExit => "no abrupt exit",
            Self::PropagateWithoutClose => "propagate without close",
            Self::CloseOnAbruptExitWithStep4Precedence => "close, 7.4.11 step 4 precedence",
        }
    }

    /// Every discipline, in declaration order. Quantified over by the
    /// distinctness assertion below, which is `name`'s `const` consumer.
    const ALL: [Self; 3] = [
        Self::NoAbruptExit,
        Self::PropagateWithoutClose,
        Self::CloseOnAbruptExitWithStep4Precedence,
    ];
}

// The three discipline names are pairwise distinct.
//
// `name()` is given the same treatment K4 gives `EmissionSite::name` twenty
// lines away in the sibling file: a renderer nobody reads is the "survival by
// `pub`" shape, and this is a `const` reader that catches a real mistake — a
// copy-pasted arm whose string was not changed makes two disciplines
// indistinguishable in every rendered table the column exists to feed, while
// the enum still looks closed.
const _: () = {
    let mut i = 0;
    while i < AbruptDiscipline::ALL.len() {
        let mut j = i + 1;
        while j < AbruptDiscipline::ALL.len() {
            assert!(
                !str_eq(
                    AbruptDiscipline::ALL[i].name(),
                    AbruptDiscipline::ALL[j].name()
                ),
                "two AbruptDiscipline variants render the same name"
            );
            j += 1;
        }
        i += 1;
    }
};

/// A row for an operation emitted by a statement-shaped emitter arm rather than
/// by a `SpecOperationIr` arm.
///
/// It cannot claim the shared emitter: there is no field that can hold an
/// [`EmitterEvidence`]. What it *can* claim is that one or more named functions
/// emit it, and those names are checked by
/// `scriptw-aot-wasm/src/emission_sites.rs::emission_sites_are_backed`.
///
/// `sites` is a slice, not a single site, because an operation of 7.4 is
/// emitted by every arm that runs the protocol. Const assert J7 rejects an
/// empty slice, so "statement-emitted, by nothing" has no spelling.
///
/// Crate-private on purpose: downstream consumers inspect the assembled
/// [`SPEC_OPERATION_CATALOG`], not the hand-written evidence rows that feed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatementEmissionRow {
    pub name: &'static str,
    pub family: SpecOperationFamily,
    pub normal_result: NormalResult,
    /// Which of the four 7.4 obligations this row's operation *is*.
    ///
    /// Required, so that J10 can ask whether each credited site emits **this**
    /// operation rather than merely "some operation". Without it a site could
    /// be credited on the `IteratorClose` row because it emits `GetIterator`
    /// there — which is exactly the mistake a `SYNC_CLOSE_SITES` split exists
    /// to prevent, and which convention alone was preventing.
    ///
    /// `AsyncIteratorClose` (7.4.12) maps to
    /// [`IteratorObligation::IteratorClose`]: the witness domain does not
    /// separate the sync and async closes, and the site does.
    pub obligation: IteratorObligation,
    pub abrupt: &'static [CompletionAbruptKind],
    /// What happens to an abrupt completion of this operation. Required, with
    /// no `Default`: a new row that declares an abrupt set and says nothing
    /// about how the abruption leaves is `E0063`, and a declared discipline
    /// that disagrees with the set is `E0080` (J12).
    pub discipline: AbruptDiscipline,
    /// The abstract operations this row's own spec definition invokes,
    /// transcribed from the spec text cited beside each row.
    ///
    /// This is the reader that consumes [`SpecOperationIr::abrupt`]: const
    /// assert J13 requires the caller's `abrupt` set to contain every abrupt
    /// completion each callee may return. Nothing in this crate can read
    /// ECMA-262, so under-listing a callee only *weakens* J13 and can never
    /// forge a containment — ledger **IC-2** bounds it. Every entry is a real
    /// `SpecOperationIr` variant, which is type-checked.
    pub calls: &'static [SpecOperationIr],
    pub sites: &'static [EmissionSite],
}

impl StatementEmissionRow {
    /// `pub(crate)`, and that is load-bearing. Every field of
    /// [`SpecOperationCatalogEntry`] is private so that the only producers are
    /// the four constructors in this module; a `pub` `into_entry` on a struct
    /// whose own fields are all `pub` handed that guarantee straight back, since
    /// any consumer crate could mint an entry claiming `StatementEmission` for
    /// an unimplemented operation and bypass J7, J10, J12 and J13 — which
    /// quantify only over [`STATEMENT_EMISSION_ROWS`].
    pub(crate) const fn into_entry(self) -> SpecOperationCatalogEntry {
        SpecOperationCatalogEntry {
            name: self.name,
            family: self.family,
            normal_result: self.normal_result,
            abrupt: self.abrupt,
            lowering_status: OperationLoweringStatus::StatementEmission(self.sites),
            source: RowSource::StatementEmissionTable,
        }
    }
}

/// A row for an operation we have *not* implemented. By construction it cannot
/// carry `SharedWasmEmitter`, `SharedBackendEmitter`, or `StatementEmission`:
/// there is no field for one. The raw row is crate-private; the assembled
/// catalog is the public surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrackedGapRow {
    pub name: &'static str,
    pub family: SpecOperationFamily,
    pub normal_result: NormalResult,
    pub abrupt: &'static [CompletionAbruptKind],
    pub reason: TrackedGapReason,
    pub owner: ConformanceOwner,
}

impl TrackedGapRow {
    /// `pub(crate)` for the same reason as
    /// [`StatementEmissionRow::into_entry`].
    pub(crate) const fn into_entry(self) -> SpecOperationCatalogEntry {
        SpecOperationCatalogEntry {
            name: self.name,
            family: self.family,
            normal_result: self.normal_result,
            abrupt: self.abrupt,
            lowering_status: OperationLoweringStatus::TrackedGap {
                reason: self.reason,
                owner: self.owner,
            },
            source: RowSource::TrackedGapTable,
        }
    }
}

const T04: ConformanceOwner = ConformanceOwner::T04;

/// The 7.4 obligations plus `AsyncIteratorClose`, emitted by statement/expression
/// arms rather than by a `SpecOperationIr` arm.
///
/// Evidence, read rather than assumed: `compile_for_of_iterator`
/// (`control_flow.rs:6874`) emits the `@@iterator` `Get`, the `Call`, the
/// once-only `Get` of `"next"`, the per-step `next()` call and the
/// `"done"`/`"value"` reads, and routes exits through
/// `emit_iterator_close_condition_i32` (`:8451`) into `emit_iterator_close`
/// (`:8479`) or `emit_iterator_close_preserving_current_throw` (`:8622`).
/// `compile_resumable_sync_for_of_iterator` emits the same synchronous
/// protocol while keeping its Iterator Record in the async activation.
/// `compile_async_for_of_iterator` (`control_flow.rs:5577`) open-codes the async
/// close. `compile_array_destructure_from_value_locals` (`control_flow.rs:7656`)
/// runs the same four operations for every array destructuring — it has no array
/// fast path — via `emit_get_iterator_from_value_locals` (`:7687`),
/// `emit_destructuring_iterator_step` (`:8099`), `emit_iterator_close` (`:7710`)
/// and `emit_iterator_close_preserving_current_throw` (`:7729`), the last two
/// under the `[[Done]]` guard 8.6.3 step 5 requires.
/// `GeneratorDelegation` names both the sync and async `yield*` emitters: they
/// run the same four protocol obligations, with the async member additionally
/// implementing `AsyncIteratorClose`.
/// `CallArgumentSpread` and `ArrayLiteralSpread` emit acquisition, stepping and
/// value extraction but deliberately do not appear in the close row: their
/// algorithms propagate those abrupt completions without `IteratorClose`.
///
/// Every entry is checked twice: J10 requires some `IteratorProtocolWitness`
/// constant to discharge an obligation by emission at it (so no arm is credited
/// that no IR construct admits to running), and J11 requires every
/// `EmissionSite` to appear in some row here (so no variant exists only to
/// satisfy J10's converse).
const SYNC_PROTOCOL_SITES: &[EmissionSite] = &[
    EmissionSite::SyncForOfIterator,
    EmissionSite::ResumableSyncForOfIterator,
    EmissionSite::AsyncForOfIterator,
    EmissionSite::ArrayDestructuring,
    EmissionSite::CallArgumentSpread,
    EmissionSite::ArrayLiteralSpread,
    EmissionSite::GeneratorDelegation,
];
const SYNC_CLOSE_SITES: &[EmissionSite] = &[
    EmissionSite::SyncForOfIterator,
    EmissionSite::ResumableSyncForOfIterator,
    EmissionSite::AsyncForOfIterator,
    EmissionSite::ArrayDestructuring,
    EmissionSite::GeneratorDelegation,
];
const ASYNC_CLOSE_SITES: &[EmissionSite] = &[
    EmissionSite::AsyncForOfIterator,
    EmissionSite::GeneratorDelegation,
];

pub(crate) const STATEMENT_EMISSION_ROWS: &[StatementEmissionRow] = &[
    StatementEmissionRow {
        name: "GetIterator",
        family: SpecOperationFamily::Iterator,
        normal_result: NormalResult::IteratorRecord,
        obligation: IteratorObligation::GetIterator,
        abrupt: MAY_THROW,
        // 7.4.2 steps 3-5: `GetMethod(obj, @@iterator)`, `Call(method, obj)`,
        // and the once-only `Get(iterator, "next")`.
        calls: &[
            SpecOperationIr::GetMethod,
            SpecOperationIr::Call,
            SpecOperationIr::Get,
        ],
        // Nothing has been acquired yet on any failing path, so there is
        // nothing this operation could close.
        discipline: AbruptDiscipline::PropagateWithoutClose,
        sites: SYNC_PROTOCOL_SITES,
    },
    StatementEmissionRow {
        name: "IteratorStep",
        family: SpecOperationFamily::Iterator,
        normal_result: NormalResult::ObjectOrFalse,
        obligation: IteratorObligation::IteratorStep,
        abrupt: MAY_THROW,
        // 7.4.8 via IteratorNext (`Call(next, iterator)`, object check) and
        // IteratorComplete (`ToBoolean(Get(result, "done"))`).
        calls: &[
            SpecOperationIr::Call,
            SpecOperationIr::Get,
            SpecOperationIr::ToBoolean,
        ],
        // The iterator belongs to the caller, and ES2025's `IteratorStepValue`
        // sets `[[Done]]` on every abrupt path it has, so 7.4.11 is not owed by
        // this operation. Emitting a close here would be an observable extra
        // call to `return`.
        discipline: AbruptDiscipline::PropagateWithoutClose,
        sites: SYNC_PROTOCOL_SITES,
    },
    StatementEmissionRow {
        name: "IteratorValue",
        family: SpecOperationFamily::Iterator,
        normal_result: NormalResult::LanguageValue,
        obligation: IteratorObligation::IteratorValue,
        abrupt: MAY_THROW,
        // 7.4.9: `Get(result, "value")`, read after `"done"`.
        calls: &[SpecOperationIr::Get],
        discipline: AbruptDiscipline::PropagateWithoutClose,
        sites: SYNC_PROTOCOL_SITES,
    },
    StatementEmissionRow {
        name: "IteratorClose",
        family: SpecOperationFamily::Iterator,
        normal_result: NormalResult::CompletionRecord,
        obligation: IteratorObligation::IteratorClose,
        abrupt: CONTROL_COMPLETIONS,
        // 7.4.11 step 3 (`GetMethod(iterator, "return")`) and step 4.c
        // (`Call(return, iterator)`).
        calls: &[SpecOperationIr::GetMethod, SpecOperationIr::Call],
        // Steps 5 and 6, in that order, are the whole content of the asymmetry:
        // a `throw` completion wins over the close's own error, a
        // `break`/`continue`/`return` completion does not.
        discipline: AbruptDiscipline::CloseOnAbruptExitWithStep4Precedence,
        sites: SYNC_CLOSE_SITES,
    },
    StatementEmissionRow {
        name: "AsyncIteratorClose",
        family: SpecOperationFamily::Iterator,
        normal_result: NormalResult::CompletionRecord,
        // 7.4.12 is the async spelling of the same obligation; the witness
        // domain does not separate them and the site does.
        obligation: IteratorObligation::IteratorClose,
        abrupt: CONTROL_COMPLETIONS,
        // 7.4.12 steps 3 and 4.c. `Await` is not a `SpecOperationIr` variant
        // and contributes no row.
        calls: &[SpecOperationIr::GetMethod, SpecOperationIr::Call],
        discipline: AbruptDiscipline::CloseOnAbruptExitWithStep4Precedence,
        sites: ASYNC_CLOSE_SITES,
    },
];

/// Operations with no implementation on the product path.
///
/// `ModelWithoutCallSite` marks the three whose Rust model exists and is
/// correct but has zero product call sites (measured); `NoImplementation` marks
/// the seven whose model type was deleted along with the false row, because a
/// type with no call site is a claim and this area exists because claims were
/// being read as implementations.
pub(crate) const TRACKED_GAP_ROWS: &[TrackedGapRow] = &[
    TrackedGapRow {
        name: "Type",
        family: SpecOperationFamily::TypeQuery,
        normal_result: NormalResult::LanguageType,
        abrupt: NO_ABRUPT,
        reason: TrackedGapReason::ModelWithoutCallSite,
        owner: T04,
    },
    TrackedGapRow {
        name: "IntegerIndexedConversion",
        family: SpecOperationFamily::Conversion,
        normal_result: NormalResult::Integer,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "IsLessThan",
        family: SpecOperationFamily::Comparison,
        normal_result: NormalResult::BooleanOrUndefined,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "CreateDataProperty",
        family: SpecOperationFamily::Object,
        normal_result: NormalResult::Boolean,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "DefinePropertyOrThrow",
        family: SpecOperationFamily::Object,
        normal_result: NormalResult::Unused,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "FromPropertyDescriptor",
        family: SpecOperationFamily::Object,
        normal_result: NormalResult::ObjectOrUndefined,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "OrdinaryCreateFromConstructor",
        family: SpecOperationFamily::Invocation,
        normal_result: NormalResult::Object,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "SpeciesConstructor",
        family: SpecOperationFamily::Invocation,
        normal_result: NormalResult::Constructor,
        abrupt: MAY_THROW,
        reason: TrackedGapReason::NoImplementation,
        owner: T04,
    },
    TrackedGapRow {
        name: "Completion",
        family: SpecOperationFamily::Completion,
        normal_result: NormalResult::CompletionRecord,
        abrupt: CONTROL_COMPLETIONS,
        reason: TrackedGapReason::ModelWithoutCallSite,
        owner: T04,
    },
    TrackedGapRow {
        name: "UpdateEmpty",
        family: SpecOperationFamily::Completion,
        normal_result: NormalResult::CompletionRecord,
        abrupt: CONTROL_COMPLETIONS,
        reason: TrackedGapReason::ModelWithoutCallSite,
        owner: T04,
    },
];

pub const SPEC_OPERATION_ROW_COUNT: usize = SpecOperationIr::ALL.len()
    + BackendSpecOperation::ALL.len()
    + STATEMENT_EMISSION_ROWS.len()
    + TRACKED_GAP_ROWS.len();

const CATALOG_PLACEHOLDER: SpecOperationCatalogEntry = SpecOperationIr::IsCallable.catalog_entry();

const fn build_catalog() -> [SpecOperationCatalogEntry; SPEC_OPERATION_ROW_COUNT] {
    let mut rows = [CATALOG_PLACEHOLDER; SPEC_OPERATION_ROW_COUNT];
    let mut next = 0;

    let mut i = 0;
    while i < SpecOperationIr::ALL.len() {
        rows[next] = SpecOperationIr::ALL[i].catalog_entry();
        next += 1;
        i += 1;
    }

    let mut j = 0;
    while j < BackendSpecOperation::ALL.len() {
        rows[next] = BackendSpecOperation::ALL[j].catalog_entry();
        next += 1;
        j += 1;
    }

    let mut k = 0;
    while k < STATEMENT_EMISSION_ROWS.len() {
        rows[next] = STATEMENT_EMISSION_ROWS[k].into_entry();
        next += 1;
        k += 1;
    }

    let mut m = 0;
    while m < TRACKED_GAP_ROWS.len() {
        rows[next] = TRACKED_GAP_ROWS[m].into_entry();
        next += 1;
        m += 1;
    }

    rows
}

const CATALOG: [SpecOperationCatalogEntry; SPEC_OPERATION_ROW_COUNT] = build_catalog();

/// The catalog. A `static` rather than a `const` so that `.iter()` yields
/// `&'static` entries; `CATALOG` is the `const` the assertions below evaluate.
pub static SPEC_OPERATION_CATALOG: [SpecOperationCatalogEntry; SPEC_OPERATION_ROW_COUNT] = CATALOG;

/// Byte-wise `str` equality, usable in `const`.
///
/// `pub(crate)` so that `iterator_obligations`'s K4 uses this one rather than
/// keeping a second copy — one const reference implementation, not two that can
/// drift.
pub(crate) const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

// (J1) Every catalog name is distinct. Replaces the runtime test
//      `operations_catalog_names_are_unique`. Because emitter rows are derived
//      from closed operation enums and every hand-written row is a
//      `StatementEmissionRow` or `TrackedGapRow`, this also subsumes (J2): a
//      gap row can never shadow an implemented operation.
const _: () = {
    let mut i = 0;
    while i < SPEC_OPERATION_ROW_COUNT {
        let mut j = i + 1;
        while j < SPEC_OPERATION_ROW_COUNT {
            assert!(
                !str_eq(CATALOG[i].name, CATALOG[j].name),
                "duplicate spec operation name"
            );
            j += 1;
        }
        i += 1;
    }
};

// (J3) was "catalog_index is dense over ALL". Deleted with `catalog_index`
//      itself: each `ALL` and its enum are now one macro expansion, so density
//      is definitional, and an assertion that cannot fail is decoration by
//      AGENTS.md's own test.

// (J4) The census the contract states, tied to the tables that produce it:
//      32 shared-expression rows + 2 shared-backend rows + 5 statement-emission
//      rows + 10 tracked gaps. Changing any table without restating the census
//      is a compile error, which is the point — the row counts are the claim
//      this area exists to keep honest.
const _: () = {
    assert!(SpecOperationIr::ALL.len() == 32);
    assert!(BackendSpecOperation::ALL.len() == 2);
    assert!(STATEMENT_EMISSION_ROWS.len() == 5);
    assert!(TRACKED_GAP_ROWS.len() == 10);
    assert!(SPEC_OPERATION_ROW_COUNT == 49);
};

// (J5) Row order: `build_catalog` lays each derived domain down first, in its
//      `ALL` order. This is a real check on `build_catalog`, not on either
//      enumeration.
const _: () = {
    let mut i = 0;
    while i < SpecOperationIr::ALL.len() {
        assert!(
            str_eq(CATALOG[i].name, SpecOperationIr::ALL[i].name()),
            "build_catalog does not lay the derived rows down first, in ALL order"
        );
        i += 1;
    }

    let mut j = 0;
    while j < BackendSpecOperation::ALL.len() {
        assert!(
            str_eq(
                CATALOG[SpecOperationIr::ALL.len() + j].name,
                BackendSpecOperation::ALL[j].name()
            ),
            "build_catalog does not lay backend-derived rows down in ALL order"
        );
        j += 1;
    }
};

// (J6) A shared-emitter row names the operation its evidence is for. Both
//      evidence types are unforgeable, but before the entry's fields became
//      private legitimately obtained evidence could still be attached to any
//      name. Each `catalog_entry` derives `name` from its evidence; this pins
//      that for every assembled row rather than for the constructors alone.
const _: () = {
    let mut i = 0;
    while i < SPEC_OPERATION_ROW_COUNT {
        match CATALOG[i].lowering_status {
            OperationLoweringStatus::SharedWasmEmitter(evidence) => {
                assert!(
                    str_eq(CATALOG[i].name, evidence.operation().name()),
                    "a shared-emitter row names an operation its evidence is not for"
                );
            }
            OperationLoweringStatus::SharedBackendEmitter(evidence) => {
                assert!(
                    str_eq(CATALOG[i].name, evidence.operation().name()),
                    "a shared-backend row names an operation its evidence is not for"
                );
            }
            OperationLoweringStatus::StatementEmission(_)
            | OperationLoweringStatus::TrackedGap { .. } => {}
        }
        i += 1;
    }
};

// (J7) A `StatementEmission` row names at least one emitter arm. "Emitted by a
//      statement arm, by no statement arm" is the false claim in miniature.
const _: () = {
    let mut i = 0;
    while i < STATEMENT_EMISSION_ROWS.len() {
        assert!(
            !STATEMENT_EMISSION_ROWS[i].sites.is_empty(),
            "a statement-emission row names no emitter arm"
        );
        i += 1;
    }
};

// (J10) The catalog may not credit an emitter arm that no acquisition has
//       accepted **for this row's own operation**. J7 established that a row
//       names *some* site; this establishes that the site is one an
//       `IteratorProtocolWitness` constant discharges by emission *of the
//       obligation the row is about* — i.e. that some IR construct admits to
//       causing that arm to run that operation.
//
//       Before `ARRAY_DESTRUCTURING_PROTOCOL` existed this failed:
//       `SYNC_PROTOCOL_SITES` had credited `EmissionSite::ArrayDestructuring`
//       since the catalog was written, and no witness named it. The attribution
//       was *true* — `compile_array_destructure_from_value_locals` really does
//       emit all four obligations — but nothing in the IR had said so, and
//       nothing could see the asymmetry.
//
//       The per-obligation form matters for a partial-protocol site. A site
//       that runs 7.4.2/7.4.8/7.4.9 and owes no 7.4.11 —
//       13.3.8.1's argument-list spread is the worked example — must appear on
//       the first three rows and not on the `IteratorClose` row. Asking only
//       "is this site witnessed for *some* obligation" would accept it on all
//       five, which is precisely the mistake a split site list exists to
//       prevent, enforced by convention rather than by `cargo check`.
const _: () = {
    let mut i = 0;
    while i < STATEMENT_EMISSION_ROWS.len() {
        let row = STATEMENT_EMISSION_ROWS[i];
        let mut j = 0;
        while j < row.sites.len() {
            assert!(
                site_emits(row.sites[j], row.obligation),
                "a statement-emission row credits a site that no witness constant discharges by \
                 emission of that row's own operation"
            );
            j += 1;
        }
        i += 1;
    }
};

// (J11) ...and the converse: every `EmissionSite` must be credited by some
//       catalog row, so a variant cannot exist purely to satisfy K1 in
//       `iterator_obligations.rs`. Together with K1 (site ⇒ witness), J10
//       (catalog ⇒ site) and `emission_sites_are_backed` (site ⇒ real function)
//       this closes the triangle: a new site cannot land without its witness and
//       its row in the same patch.
const _: () = {
    let mut i = 0;
    while i < EmissionSite::ALL.len() {
        let mut found = false;
        let mut j = 0;
        while j < STATEMENT_EMISSION_ROWS.len() {
            let sites = STATEMENT_EMISSION_ROWS[j].sites;
            let mut k = 0;
            while k < sites.len() {
                if sites[k] as u8 == EmissionSite::ALL[i] as u8 {
                    found = true;
                }
                k += 1;
            }
            j += 1;
        }
        assert!(found, "an EmissionSite is named by no catalog row");
        i += 1;
    }
};

// (J12) `abrupt` and `discipline` agree, in both directions.
//
//   (a) An empty `abrupt` set and `NoAbruptExit` imply one another. This is the
//       sentence the `discipline` column exists to make true: a row that says
//       the operation may throw and declares no propagation discipline is
//       unrepresentable, because the only discipline compatible with an empty
//       `abrupt` is the one that says there is no abrupt exit.
//
//   (b) A row claiming 7.4.11 step-4 precedence must carry the completions
//       step 4 distinguishes. The asymmetry between step 5 and step 6 has no
//       content unless `break`/`continue`/`return` are possible alongside
//       `throw`; claiming the asymmetry with only one side is claiming nothing.
//
//       This is a **membership scan**, not `abrupt.len() == 4`. The length test
//       accepted `&[Throw, Return, Break, Break]`, in which `Continue` — the
//       kind whose outer-label carve-out is the subtlest part of 14.7.1.1 — is
//       silently absent, and writing the slice out by hand instead of reusing
//       the `CONTROL_COMPLETIONS` alias is exactly the plausible mistake.
//       J13 already contains this idiom; it is reused rather than re-derived.
const _: () = {
    let mut i = 0;
    while i < STATEMENT_EMISSION_ROWS.len() {
        let row = STATEMENT_EMISSION_ROWS[i];
        let is_none = matches!(row.discipline, AbruptDiscipline::NoAbruptExit);
        assert!(
            row.abrupt.is_empty() == is_none,
            "a statement-emission row's abrupt set and propagation discipline disagree"
        );
        if matches!(
            row.discipline,
            AbruptDiscipline::CloseOnAbruptExitWithStep4Precedence
        ) {
            let mut k = 0;
            while k < CompletionAbruptKind::ALL.len() {
                let mut found = false;
                let mut m = 0;
                while m < row.abrupt.len() {
                    if row.abrupt[m] as u8 == CompletionAbruptKind::ALL[k] as u8 {
                        found = true;
                    }
                    m += 1;
                }
                assert!(
                    found,
                    "a row claiming 7.4.11 step 4 precedence must admit all four abrupt kinds"
                );
                k += 1;
            }
        }
        i += 1;
    }
};

// (J13) Callee containment: every abrupt completion a callee may return must
//       appear in the caller's `abrupt` set.
//
//       This is the first and only reader of `SpecOperationIr::abrupt()` on the
//       product path — before it, the whole column was a machine-readable claim
//       nothing consumed. Marking `Get` as `NO_ABRUPT` — the shape of commit
//       `ca09433c1`, where abrupt completions were compiled as if they could not
//       escape — now fails the build at the `IteratorValue` row rather than
//       waiting for a conformance run.
//
//       This is a guard, not a repair: it passes on today's values. Ledger
//       **IC-2** bounds what it cannot see — a row that lists fewer callees than
//       the spec text gives only weakens the check, and can never forge a
//       containment.
//
//       Containment alone does **not** catch the mistake the paragraph above
//       names, and that gap is what the second assertion below closes. Marking
//       `Get` as `NO_ABRUPT` makes `row.calls[j].abrupt()` an *empty* slice, so
//       the `while k < callee.len()` body never runs, nothing is asserted, and
//       the build stays green — containment is monotone in the wrong direction
//       for a weakened callee. So each row that claims an abrupt exit must also
//       name at least one callee that can produce one. `IteratorValue`'s only
//       callee is `Get`, so weakening `Get` fails the build at that row, which
//       is what the paragraph above claims.
const _: () = {
    let mut i = 0;
    while i < STATEMENT_EMISSION_ROWS.len() {
        let row = STATEMENT_EMISSION_ROWS[i];
        assert!(
            !row.calls.is_empty(),
            "a statement-emission row names no callee; 7.4's operations all invoke something"
        );
        // The justification check: an abrupt exit this row claims must have a
        // callee that can produce one.
        let mut justified = false;
        let mut c = 0;
        while c < row.calls.len() {
            if !row.calls[c].abrupt().is_empty() {
                justified = true;
            }
            c += 1;
        }
        assert!(
            row.abrupt.is_empty() || justified,
            "a statement-emission row claims an abrupt exit no callee it names can produce"
        );
        let mut j = 0;
        while j < row.calls.len() {
            let callee = row.calls[j].abrupt();
            let mut k = 0;
            while k < callee.len() {
                let mut found = false;
                let mut m = 0;
                while m < row.abrupt.len() {
                    if row.abrupt[m] as u8 == callee[k] as u8 {
                        found = true;
                    }
                    m += 1;
                }
                assert!(
                    found,
                    "a statement-emission row omits an abrupt completion its callee may return"
                );
                k += 1;
            }
            j += 1;
        }
        i += 1;
    }
};

pub fn spec_operation_catalog() -> &'static [SpecOperationCatalogEntry] {
    &SPEC_OPERATION_CATALOG
}

pub fn find_spec_operation(name: &str) -> Option<&'static SpecOperationCatalogEntry> {
    SPEC_OPERATION_CATALOG
        .iter()
        .find(|entry| entry.name() == name)
}

pub fn completion_abi_slots() -> &'static [CompletionAbiSlot] {
    COMPLETION_ABI_SLOTS
}

/// Ties the catalog's descriptive [`NormalResult`] rows to the codomain types in
/// [`crate::numeric_conversions`], so the two cannot drift.
///
/// [`NormalResult`] is a *descriptive tag* for the catalog's rendered "returns"
/// column, not a codomain — nothing constructs a value of it — and this
/// assertion deliberately does not promote it to one. What it pins is the
/// claim each row makes.
///
/// `ToIntegerOrInfinity` is `Number`, **not** `Integer`, because 7.1.5's
/// codomain is ℤ ∪ {+∞, −∞} and `±∞` are not integers — the same fact that
/// forces [`crate::IntegerOrInfinity`] to have three variants rather than
/// wrapping an `i32`. If someone "tidies" this row to `Integer` on the grounds
/// that the operation has "Integer" in its name, this assertion fails the
/// build. `ToLength` and `ToIndex` are `Integer` because 7.1.20 step 3 and
/// 7.1.22 step 3 both land in `[0, 2^53−1] ∩ ℤ`.
///
/// See `docs/rust-rewrite/contracts/numeric-conversion-codomains.md` §2.6.
const _: () = {
    assert!(matches!(
        SpecOperationIr::ToIntegerOrInfinity.normal_result(),
        NormalResult::Number
    ));
    assert!(matches!(
        SpecOperationIr::ToLength.normal_result(),
        NormalResult::Integer
    ));
    assert!(matches!(
        SpecOperationIr::ToIndex.normal_result(),
        NormalResult::Integer
    ));
};

#[cfg(test)]
mod tests {
    use super::*;

    // `spec_operation_all_is_complete_and_dense` is deleted. It named ledger L1
    // in its failure message but could not detect L1's failure: a 30th variant
    // with all four exhaustive arms and no `ALL` row passed J1, J3, J4 and J5,
    // and passed the test itself, whose final assertion was the hardcoded
    // `ALL.len() == 29` that the omission preserves. `ALL` is now generated
    // from the same macro rows as the enum, so the omission is unrepresentable
    // and there is nothing left for a test to check.

    /// Not a test of the type's own contents: it pins that each slot reads back
    /// through its own accessor, which is the property a transposition at the
    /// construction site would break. The transposition itself is `E0308` and
    /// therefore has no test.
    #[test]
    fn operations_iterator_record_slots_read_back_through_their_own_accessor() {
        let record = IteratorRecordIr::new(
            IteratorSlot::new("async.forof.iterator.0".to_string()),
            NextMethodSlot::new("async.forof.next.0".to_string()),
            DoneSlot::new("async.forof.done.0".to_string()),
        );

        assert_eq!(record.iterator().as_str(), "async.forof.iterator.0");
        assert_eq!(record.next_method().as_str(), "async.forof.next.0");
        assert_eq!(record.done().as_str(), "async.forof.done.0");
    }

    #[test]
    fn operations_ecmascript_language_type_reports_names_and_primitive_status() {
        for primitive in [
            EcmaLanguageType::Undefined,
            EcmaLanguageType::Null,
            EcmaLanguageType::Boolean,
            EcmaLanguageType::String,
            EcmaLanguageType::Symbol,
            EcmaLanguageType::Number,
            EcmaLanguageType::BigInt,
        ] {
            assert!(!primitive.name().is_empty());
            assert!(primitive.is_primitive());
        }

        assert_eq!(EcmaLanguageType::Object.name(), "Object");
        assert!(!EcmaLanguageType::Object.is_primitive());
    }

    #[test]
    fn operations_completion_abi_slots_are_stable_and_dense() {
        for (expected, slot) in completion_abi_slots().iter().enumerate() {
            assert_eq!(
                slot.code(),
                expected as i64,
                "completion kind {} should stay at {}",
                slot.name(),
                expected
            );
            assert_eq!(slot.name(), slot.kind().name());
            assert_eq!(slot.carries_value(), slot.kind().carries_value());
            assert_eq!(slot.carries_target(), slot.kind().carries_target());
        }
    }

    #[test]
    fn operations_completion_abi_documents_values_and_targets() {
        let value_slots = completion_abi_slots()
            .iter()
            .filter(|slot| slot.carries_value())
            .map(|slot| slot.name())
            .collect::<Vec<_>>();
        assert_eq!(
            value_slots,
            vec!["normal", "throw", "return", "break", "continue"]
        );

        let target_slots = completion_abi_slots()
            .iter()
            .filter(|slot| slot.carries_target())
            .map(|slot| slot.name())
            .collect::<Vec<_>>();
        assert_eq!(target_slots, vec!["break", "continue"]);
    }

    #[test]
    fn operations_completion_abi_slot_construction_is_total() {
        for slot in COMPLETION_ABI_SLOTS {
            assert_eq!(completion_abi_slot(slot.kind()), *slot);
        }
    }

    #[test]
    fn operations_completion_record_reports_kind_value_target_and_abruptness() {
        let normal = CompletionRecordIr::normal("ok");
        assert_eq!(normal.kind(), CompletionKindIr::Normal);
        assert_eq!(normal.value(), Some(&"ok"));
        assert_eq!(normal.target(), None);
        assert!(!normal.is_abrupt());

        let throw = CompletionRecordIr::throw("boom");
        assert_eq!(throw.kind(), CompletionKindIr::Throw);
        assert_eq!(throw.value(), Some(&"boom"));
        assert!(throw.is_abrupt());

        let break_ = CompletionRecordIr::break_(None::<&str>, Some("outer".to_string()));
        assert_eq!(break_.kind(), CompletionKindIr::Break);
        assert_eq!(break_.value(), None);
        assert_eq!(break_.target(), Some("outer"));
        assert!(break_.is_abrupt());
    }

    #[test]
    fn operations_completion_record_update_empty_preserves_abrupt_kind() {
        assert_eq!(
            CompletionRecordIr::empty_normal().update_empty("fallback"),
            CompletionRecordIr::normal("fallback")
        );
        assert_eq!(
            CompletionRecordIr::empty().update_empty("fallback"),
            CompletionRecordIr::normal("fallback")
        );
        assert_eq!(
            CompletionRecordIr::break_(None, Some("outer".to_string())).update_empty("fallback"),
            CompletionRecordIr::break_(Some("fallback"), Some("outer".to_string()))
        );
        assert_eq!(
            CompletionRecordIr::throw("boom").update_empty("fallback"),
            CompletionRecordIr::throw("boom")
        );
    }

    #[test]
    fn operations_completion_record_map_value_keeps_empty_slots_empty() {
        let mapped = CompletionRecordIr::continue_(Some(2), None).map_value(|value| value * 3);
        assert_eq!(mapped, CompletionRecordIr::continue_(Some(6), None));

        let empty: CompletionRecordIr<i32> = CompletionRecordIr::empty();
        assert_eq!(
            empty.map_value(|value| value * 3),
            CompletionRecordIr::Empty
        );
    }
}
