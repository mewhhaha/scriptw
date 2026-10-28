//! The iterator-protocol obligations of ECMA-262 7.4, and the witnesses that
//! iterator-consuming IR carries to say how each obligation is discharged.
//!
//! ## Why this exists
//!
//! This module makes every discharge a named value in the IR, so that
//!
//! - relying on one is a construction that a reviewer sees, not a silence;
//! - adding another specialization cannot compile until its author says, for
//!   all four obligations, whether the specialization *emits* the operation or
//!   *assumes it away*, and on which premise;
//! - transposing two obligations is `E0308` rather than a plausible-looking
//!   wrong witness — including *inside this module*, because the four discharge
//!   newtypes are what [`IteratorProtocolWitness::new`] takes.
//!
//! ## What a premise is, and what it is not
//!
//! - [`PremiseKind::ImplementationFact`] — a proposition about *our emitter*,
//!   established by reading it. Recorded so the reader knows no guard is owed.
//! - [`PremiseKind::Vacuous`] — there is nothing to discharge.
//!
//! ## The emitter must not read this
//!
//! No `scriptw-aot-wasm` arm consults the witness, and now it cannot: every
//! reader of a witness's contents is `pub(crate)`, so a backend arm that binds
//! `protocol` and branches on `is_fully_emitted()` is `E0624` at `cargo check`
//! rather than a ten-minute rung-G diff. The type stays `pub` only because it
//! is the type of a `pub` enum variant's field.
//!
//! See `docs/rust-rewrite/contracts/spec-operations.md`.

/// The four 7.4 obligations a for-of head incurs. Closed by the spec.
///
/// ES2025 fuses `IteratorStep` and `IteratorValue` into `IteratorStepValue`;
/// the four-way decomposition is kept because it is what the catalog names,
/// what the emitted code separates, and what the corpus tests key on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IteratorObligation {
    /// 7.4.2. `GetMethod(obj, @@iterator)`, the `Call`, the object check, and
    /// the **once-only** `Get(iterator, "next")`.
    GetIterator,
    /// 7.4.8. `Call(next, iterator)`, the object check, then
    /// `ToBoolean(Get(result, "done"))`.
    IteratorStep,
    /// 7.4.9. `Get(result, "value")` — read after `"done"`, and not read at all
    /// on the exhausting step.
    IteratorValue,
    /// 7.4.11. `GetMethod(iterator, "return")` and the call, with step 4's
    /// asymmetry: an original `throw` completion wins and swallows an error
    /// raised by the close, while a `break`/`return`/`continue` completion does
    /// not.
    IteratorClose,
}

impl IteratorObligation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::GetIterator => "GetIterator",
            Self::IteratorStep => "IteratorStep",
            Self::IteratorValue => "IteratorValue",
            Self::IteratorClose => "IteratorClose",
        }
    }
}

/// Declares [`EmissionSite`], its `ALL` enumeration and its `name` renderer
/// from **one** row list.
///
/// This is `spec_operations!`'s shape (`operations.rs`), applied to the site
/// domain for the same reason. The site↔witness and site↔catalog ties below
/// quantify over [`EmissionSite::ALL`]; a hand-written `ALL` would reintroduce
/// ledger **L1** exactly — a variant absent from the list is invisible to every
/// `const` expression that is supposed to constrain it, and the omission is
/// precisely what a length assertion preserves. With the enum, `ALL` and
/// `name()` being three expansions of the same `$(...)+` sequence, the omission
/// is not expressible.
///
/// A row is `[docs] Variant => "emitter function name"`. The name is the one
/// `scriptw-aot-wasm/src/emission_sites.rs::emission_sites_are_backed` must
/// resolve.
macro_rules! emission_sites {
    ($(
        $( #[$meta:meta] )*
        $variant:ident => $name:literal
    ),+ $(,)?) => {
        /// Which emitter arm performs the operation.
        ///
        /// Closed, and each variant is joined to a real function by
        /// `scriptw-aot-wasm/src/emission_sites.rs::emission_sites_are_backed`,
        /// an uncalled function whose exhaustive `match` names each arm's path.
        /// Renaming or deleting an emitter arm that a variant claims is
        /// therefore `E0599`, and adding a variant is a compile error until it
        /// names something real. The guarantee is name resolution, not
        /// signature — stated so nobody over-reads it.
        ///
        /// Three further ties close the triangle, so a variant cannot exist
        /// without an owner on either side: **K1** (below) rejects a site no
        /// witness constant discharges by emission, and **J10**/**J11** in
        /// `operations.rs` reject a catalog row naming an unwitnessed site and a
        /// site named by no catalog row.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum EmissionSite {
            $( $( #[$meta] )* $variant , )+
        }

        impl EmissionSite {
            /// Every site, in declaration order. Generated from the same rows as
            /// the enum, so it cannot be partial.
            pub const ALL: &'static [EmissionSite] = &[ $( EmissionSite::$variant , )+ ];

            pub const fn name(self) -> &'static str {
                match self { $( EmissionSite::$variant => $name , )+ }
            }
        }
    };
}

emission_sites! {
    /// `FunctionBuilder::compile_for_of_iterator` (`control_flow.rs:6874`).
    SyncForOfIterator => "compile_for_of_iterator",
    /// `FunctionBuilder::compile_async_function_for_of_iterator`.
    ResumableSyncForOfIterator => "compile_resumable_sync_for_of_iterator",
    /// `FunctionBuilder::compile_async_for_of_iterator` (`control_flow.rs:5577`).
    AsyncForOfIterator => "compile_async_for_of_iterator",
    /// `FunctionBuilder::compile_array_destructure_from_value_locals`
    /// (`control_flow.rs:7656`). Array destructuring runs the same protocol:
    /// the `@@iterator` read, `finish_get_iterator_from_method`'s callability
    /// and object checks and the once-only `next` cache (`:7891`),
    /// `emit_destructuring_iterator_step` per element (`:8099`), and **both**
    /// halves of 7.4.11 step 4 under the `[[Done]]` guard — `emit_iterator_close`
    /// on the normal path (`:7710`) and
    /// `emit_iterator_close_preserving_current_throw` on the abrupt path
    /// (`:7729`). There is no array fast path here, so every array destructuring
    /// pays the real protocol.
    ArrayDestructuring => "compile_array_destructure_from_value_locals",
    /// `FunctionBuilder::emit_call_args_vector` (`functions.rs`). Argument-list
    /// spread emits acquisition, stepping and value extraction, but no close:
    /// 13.3.8.1 propagates its iterator-operation abrupt completions directly.
    CallArgumentSpread => "emit_call_args_vector",
    /// `FunctionBuilder::compile_array_accumulation_payload`
    /// (`builtins/array.rs`). ArrayAccumulation always performs the sync
    /// iterator protocol for a spread element. Its algorithm propagates an
    /// abrupt iterator operation directly and does not invoke IteratorClose.
    ArrayLiteralSpread => "compile_array_accumulation_payload",
    /// `FunctionBuilder::{compile_generator_delegation,
    /// compile_async_generator_delegation}` (`generator_delegation.rs`). The
    /// lowerer cannot know which member runs because the backend selects the
    /// function execution kind after reading the shared `GeneratorYield` IR.
    GeneratorDelegation => "compile_generator_delegation",
}

/// What kind of claim an [`IntactnessPremise`] is.
///
/// The distinction prevents facts about an emitter from being confused with a
/// vacuous obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PremiseKind {
    /// A proposition about *our emitter*, established by reading it. No guard
    /// is owed; the citation in the variant's doc comment is the evidence.
    ImplementationFact,
    /// There is nothing to discharge — the obligation does not arise.
    Vacuous,
}

/// The premises a specialization may rely on when it declines to emit an
/// obligation.
///
/// Closed: a lowering that needs a premise not in this list must add a variant,
/// which is a diff a reviewer sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntactnessPremise {
    /// The head did not lower to an iteration at all — an unsupported form was
    /// reported and `StatementIr::Empty` was emitted — so no 7.4 operation is
    /// owed by the statement that was produced.
    NoIterationLowered,
    /// No abrupt exit of argument-list spread leaves an iterator that 13.3.8.1
    /// owes a close for. Failures while acquiring the iterator propagate before
    /// the caller holds an Iterator Record; step/value failures propagate from
    /// the loop without an `IteratorClose` operation.
    ///
    /// This is an implementation fact about `emit_call_args_vector`'s control
    /// flow, not a property of the operand or realm.
    SpreadCloseOwedOnlyAfterAcquisition,
    /// ArrayAccumulation's SpreadElement algorithm does not perform
    /// IteratorClose. Abrupt completion of GetIterator, IteratorStep or
    /// IteratorValue propagates directly from the accumulation loop.
    ///
    /// This is an implementation fact about
    /// `compile_array_accumulation_payload`, not a premise about the operand.
    ArrayAccumulationDoesNotClose,
}

impl IntactnessPremise {
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoIterationLowered => "NoIterationLowered",
            Self::SpreadCloseOwedOnlyAfterAcquisition => "SpreadCloseOwedOnlyAfterAcquisition",
            Self::ArrayAccumulationDoesNotClose => "ArrayAccumulationDoesNotClose",
        }
    }

    /// Which kind of claim this premise is. Exhaustive and without a catch-all:
    /// a new premise must state whether it owes a guard.
    pub const fn kind(self) -> PremiseKind {
        match self {
            Self::SpreadCloseOwedOnlyAfterAcquisition | Self::ArrayAccumulationDoesNotClose => {
                PremiseKind::ImplementationFact
            }
            Self::NoIterationLowered => PremiseKind::Vacuous,
        }
    }
}

/// How one obligation was accounted for.
///
/// Two cases, both carrying payload: there is no "unknown", no `Default`, and
/// no unit variant, so "I did not think about this one" has no spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationDischarge {
    ByEmission(EmissionSite),
    ByAssumption(IntactnessPremise),
}

impl ObligationDischarge {
    pub(crate) const fn is_emitted(self) -> bool {
        match self {
            Self::ByEmission(_) => true,
            Self::ByAssumption(_) => false,
        }
    }
}

// Four distinct newtypes, deliberately not one generic wrapper: transposing
// "what I assumed about `next`" with "what I assumed about `return`" must be
// `E0308`, not a witness that type-checks and lies. That only holds if the
// *constructors* are per-obligation too — `assumed(a, b, c, d)` over four
// same-typed `IntactnessPremise` arguments would have moved the transposition
// inside this module rather than removing it — so each newtype carries its own
// `assumed`/`emitted` pair and `IteratorProtocolWitness::new` takes the four
// newtypes. Every constructor is private to this module, so outside
// `iterator_obligations` the only available values are the named constants
// generated below.

/// How a specialization discharged `GetIterator` (7.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetIteratorDischarge(ObligationDischarge);

/// How a specialization discharged `IteratorStep` (7.4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IteratorStepDischarge(ObligationDischarge);

/// How a specialization discharged `IteratorValue` (7.4.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IteratorValueDischarge(ObligationDischarge);

/// How a specialization discharged `IteratorClose` (7.4.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IteratorCloseDischarge(ObligationDischarge);

impl GetIteratorDischarge {
    const fn assumed(premise: IntactnessPremise) -> Self {
        Self(ObligationDischarge::ByAssumption(premise))
    }

    const fn emitted(site: EmissionSite) -> Self {
        Self(ObligationDischarge::ByEmission(site))
    }

    pub(crate) const fn get(self) -> ObligationDischarge {
        self.0
    }
}

impl IteratorStepDischarge {
    const fn assumed(premise: IntactnessPremise) -> Self {
        Self(ObligationDischarge::ByAssumption(premise))
    }

    const fn emitted(site: EmissionSite) -> Self {
        Self(ObligationDischarge::ByEmission(site))
    }

    pub(crate) const fn get(self) -> ObligationDischarge {
        self.0
    }
}

impl IteratorValueDischarge {
    const fn assumed(premise: IntactnessPremise) -> Self {
        Self(ObligationDischarge::ByAssumption(premise))
    }

    const fn emitted(site: EmissionSite) -> Self {
        Self(ObligationDischarge::ByEmission(site))
    }

    pub(crate) const fn get(self) -> ObligationDischarge {
        self.0
    }
}

impl IteratorCloseDischarge {
    const fn assumed(premise: IntactnessPremise) -> Self {
        Self(ObligationDischarge::ByAssumption(premise))
    }

    const fn emitted(site: EmissionSite) -> Self {
        Self(ObligationDischarge::ByEmission(site))
    }

    pub(crate) const fn get(self) -> ObligationDischarge {
        self.0
    }
}

/// The obligation witness an iterator-consuming IR construct carries.
///
/// Non-defaultable, non-optional, all fields private, and **`new` is private to
/// this module**. Outside `iterator_obligations`, the only available values of
/// this type are the named constants below. That turns "these constants are the
/// only witnesses lowering may use" from a rule in a document into a property
/// of the type: a new specialization cannot invent a witness at its construction
/// site; it must add a constant here, next to the premises it is claiming.
///
/// `Copy`, and free of `String`, so putting it on a `StatementIr` variant costs
/// no allocation and preserves `PartialEq`/`Clone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IteratorProtocolWitness {
    get_iterator: GetIteratorDischarge,
    iterator_step: IteratorStepDischarge,
    iterator_value: IteratorValueDischarge,
    iterator_close: IteratorCloseDischarge,
}

impl IteratorProtocolWitness {
    const fn new(
        get_iterator: GetIteratorDischarge,
        iterator_step: IteratorStepDischarge,
        iterator_value: IteratorValueDischarge,
        iterator_close: IteratorCloseDischarge,
    ) -> Self {
        Self {
            get_iterator,
            iterator_step,
            iterator_value,
            iterator_close,
        }
    }

    const fn emitted_by(site: EmissionSite) -> Self {
        Self::new(
            GetIteratorDischarge::emitted(site),
            IteratorStepDischarge::emitted(site),
            IteratorValueDischarge::emitted(site),
            IteratorCloseDischarge::emitted(site),
        )
    }

    pub(crate) const fn get_iterator(self) -> GetIteratorDischarge {
        self.get_iterator
    }

    pub(crate) const fn iterator_step(self) -> IteratorStepDischarge {
        self.iterator_step
    }

    pub(crate) const fn iterator_value(self) -> IteratorValueDischarge {
        self.iterator_value
    }

    pub(crate) const fn iterator_close(self) -> IteratorCloseDischarge {
        self.iterator_close
    }

    /// Read one obligation's discharge, going through the typed accessor for
    /// that slot rather than the field. The four accessors return four
    /// different newtypes, so an arm that reached into the wrong slot would be
    /// `E0308` here rather than a witness that type-checks and lies.
    pub(crate) const fn discharge(self, obligation: IteratorObligation) -> ObligationDischarge {
        match obligation {
            IteratorObligation::GetIterator => self.get_iterator().get(),
            IteratorObligation::IteratorStep => self.iterator_step().get(),
            IteratorObligation::IteratorValue => self.iterator_value().get(),
            IteratorObligation::IteratorClose => self.iterator_close().get(),
        }
    }

    /// True when every obligation is discharged by real emitted code.
    ///
    /// `false` does not imply a premise about the user's program: argument-list
    /// spread's close slot is an implementation fact, and the no-iteration
    /// witness is vacuous. Callers that need that distinction must inspect the
    /// closed [`PremiseKind`] domain instead.
    pub(crate) const fn is_fully_emitted(self) -> bool {
        self.get_iterator.get().is_emitted()
            && self.iterator_step.get().is_emitted()
            && self.iterator_value.get().is_emitted()
            && self.iterator_close.get().is_emitted()
    }
}

/// Declares every [`IteratorProtocolWitness`] constant **and** the
/// [`ALL_WITNESSES`] census from one row list.
///
/// This is `emission_sites!`'s shape, applied to the witness domain. The
/// previous round recorded (ledger **IC-4**) that no type could carry the
/// census because "each constant's body is a different four-argument
/// expression, not a row". That is not a barrier: a `macro_rules!` row can
/// carry an expression fragment, so the constants and the census are two
/// expansions of the same `$(...)+` sequence and "added a constant, forgot the
/// census" is not expressible. The length assertion K3 that guarded the census
/// by hand is retired with it — a `len() == 7` check is exactly what forgetting
/// a row preserves (ledger **L1**'s shape), and an assertion that cannot detect
/// its own omission is decoration.
///
macro_rules! iterator_witnesses {
    ($(
        $( #[$meta:meta] )*
        $name:ident => $body:expr
    ),+ $(,)?) => {
        impl IteratorProtocolWitness {
            // The type is spelled out rather than written `Self`: the row bodies
            // are captured at the invocation site, which is module scope, and
            // `Self` there has no impl to resolve against.
            $( $( #[$meta] )* pub const $name: IteratorProtocolWitness = $body; )+
        }

        /// Every witness constant, by name.
        ///
        /// Generated from the same rows as the constants themselves, so it
        /// cannot be partial.
        ///
        /// `pub(crate)`, like every other reader of a witness's contents: a
        /// `scriptw-aot-wasm` arm that reached for this list would be `E0603`,
        /// which is the same prohibition the accessors carry.
        pub(crate) const ALL_WITNESSES: &[IteratorProtocolWitness] =
            &[ $( IteratorProtocolWitness::$name , )+ ];
    };
}

iterator_witnesses! {
    /// `StatementIr::ForOfIterator`, sync. Every obligation is really emitted;
    /// the close predicate in `emit_iterator_close_condition_i32`
    /// (`control_flow.rs`) is exactly `¬LoopContinues`.
    SYNC_ITERATOR_PROTOCOL => IteratorProtocolWitness::emitted_by(EmissionSite::SyncForOfIterator),

    /// `StatementIr::{AsyncFunctionForOfIterator, GeneratorForOfIterator}`, the activation-backed
    /// synchronous iterator walk used when an ordinary `for-of` body awaits.
    RESUMABLE_SYNC_ITERATOR_PROTOCOL => IteratorProtocolWitness::emitted_by(
        EmissionSite::ResumableSyncForOfIterator,
    ),

    /// `StatementIr::ForOfIterator` with an async plan (`for await`).
    ASYNC_ITERATOR_PROTOCOL => IteratorProtocolWitness::emitted_by(EmissionSite::AsyncForOfIterator),

    /// `ArrayDestructuringPatternIr` — the iterator one ArrayBindingPattern
    /// (8.6.3 IteratorBindingInitialization) or ArrayAssignmentPattern
    /// (13.15.5.5 IteratorDestructuringAssignmentEvaluation) acquires.
    ///
    /// One witness per *pattern*, not per statement: both operations perform a
    /// fresh `GetIterator` for every array pattern, nested ones included, which
    /// is why the field lives on [`crate::ArrayDestructuringPatternIr`] rather
    /// than on `ExprIr::ArrayDestructure`.
    ///
    /// Every obligation is really emitted, verified step by step in
    /// `compile_array_destructure_from_value_locals`:
    /// `emit_get_iterator_from_value_locals`, `emit_destructuring_iterator_step`
    /// per element, and **both** halves of 7.4.11 step 4 —
    /// `emit_iterator_close` under the `[[Done]]` guard on the normal path and
    /// `emit_iterator_close_preserving_current_throw` under the same guard on
    /// the abrupt path. That guard is 8.6.3 step 5's "if
    /// `iteratorRecord.[[Done]]` is false", which is what
    /// `array-elem-iter-nrml-close-skip.js` pins. There is no array fast path
    /// here, so every array destructuring pays the real protocol and the
    /// discharge is honestly `ByEmission` in all four slots.
    ///
    /// Reachable at the IR field only through
    /// [`ArrayPatternProtocol::ARRAY_DESTRUCTURING`].
    ARRAY_DESTRUCTURING_PROTOCOL => IteratorProtocolWitness::emitted_by(EmissionSite::ArrayDestructuring),

    /// `ExprIr::SpreadArgument` — 13.3.8.1 ArgumentListEvaluation.
    ///
    /// `emit_call_args_vector` performs the real `GetIterator`, `IteratorStep`
    /// and `IteratorValue` work. Argument-list spread does not perform
    /// `IteratorClose`; its abrupt exits are accounted for by the implementation
    /// fact named in the fourth slot.
    ///
    /// Reachable at the IR field only through
    /// [`SpreadArgumentProtocol::ARGUMENT_LIST`].
    CALL_ARGUMENT_SPREAD_PROTOCOL => IteratorProtocolWitness::new(
        GetIteratorDischarge::emitted(EmissionSite::CallArgumentSpread),
        IteratorStepDischarge::emitted(EmissionSite::CallArgumentSpread),
        IteratorValueDischarge::emitted(EmissionSite::CallArgumentSpread),
        IteratorCloseDischarge::assumed(IntactnessPremise::SpreadCloseOwedOnlyAfterAcquisition),
    ),

    /// `ExprIr::ArrayAccumulation` — 13.2.4.1 ArrayAccumulation for a
    /// SpreadElement.
    ///
    /// `compile_array_accumulation_payload` performs the real GetIterator,
    /// IteratorStep and IteratorValue operations for every spread. The
    /// ArrayAccumulation algorithm has no IteratorClose step, so the fourth
    /// slot names that implementation fact rather than claiming an emission.
    ///
    /// Reachable at the IR field only through
    /// [`ArraySpreadProtocol::ARRAY_ACCUMULATION`].
    ARRAY_LITERAL_SPREAD_PROTOCOL => IteratorProtocolWitness::new(
        GetIteratorDischarge::emitted(EmissionSite::ArrayLiteralSpread),
        IteratorStepDischarge::emitted(EmissionSite::ArrayLiteralSpread),
        IteratorValueDischarge::emitted(EmissionSite::ArrayLiteralSpread),
        IteratorCloseDischarge::assumed(IntactnessPremise::ArrayAccumulationDoesNotClose),
    ),

    /// `StatementIr::GeneratorYield` in its `yield*` form (14.4.14).
    ///
    /// Both the sync and async generator-delegation emitters perform the
    /// iterator acquisition, step and value operations. They also implement
    /// delegation's close path; on the sync throw-resume path the iterator is
    /// closed before the required "yield* iterator has no throw method"
    /// `TypeError` is created.
    ///
    /// Reachable at the IR field only through
    /// [`GeneratorDelegationProtocol::YIELD_STAR`].
    YIELD_STAR_DELEGATION_PROTOCOL => IteratorProtocolWitness::emitted_by(EmissionSite::GeneratorDelegation),

    /// The for-of head produced no iteration: an unsupported form was reported
    /// and the statement is `StatementIr::Empty`. Every obligation is vacuous
    /// because nothing runs.
    ///
    /// This exists so that a bail-out path is still a construction a reviewer
    /// sees, rather than the one shape in which a for-of head can escape
    /// without saying anything.
    NO_ITERATION => IteratorProtocolWitness::new(
        GetIteratorDischarge::assumed(IntactnessPremise::NoIterationLowered),
        IteratorStepDischarge::assumed(IntactnessPremise::NoIterationLowered),
        IteratorValueDischarge::assumed(IntactnessPremise::NoIterationLowered),
        IteratorCloseDischarge::assumed(IntactnessPremise::NoIterationLowered),
    ),
}

/// The witness slot on [`crate::ArrayDestructuringPatternIr`].
///
/// A newtype rather than a bare [`IteratorProtocolWitness`], because the bare
/// field's type was the **whole witness domain**: both
/// `IteratorProtocolWitness::NO_ITERATION` and `::SYNC_ITERATOR_PROTOCOL`
/// compiled at both construction sites in
/// `lowering.rs`, and every const assertion still passed. K2 pins what the
/// constant *contains*; nothing pinned which field may hold it. The guarantee
/// the round-1 ledger claimed ("the author must name a constant that lives
/// beside its premises") was therefore "a constant", not "the right constant".
///
/// There is exactly one inhabitant and its constructor is private, so any other
/// witness at that field is `E0308`. The accessor is `pub(crate)`, so the
/// prohibition on `scriptw-aot-wasm` reading a witness (P2) is unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayPatternProtocol(IteratorProtocolWitness);

impl ArrayPatternProtocol {
    /// The only inhabitant: 8.6.3 / 13.15.5.5 acquire a real iterator and
    /// `compile_array_destructure_from_value_locals` emits all four 7.4
    /// obligations for it.
    pub const ARRAY_DESTRUCTURING: Self =
        Self(IteratorProtocolWitness::ARRAY_DESTRUCTURING_PROTOCOL);

    pub(crate) const fn witness(self) -> IteratorProtocolWitness {
        self.0
    }
}

/// The protocol witness carried by `yield*`.
///
/// This wrapper has one inhabitant and a private constructor. Consequently,
/// [`crate::YieldForm::Delegate`] cannot carry an unrelated iterator witness:
/// asking the backend to delegate always states the four obligations that the
/// generator-delegation emitters discharge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorDelegationProtocol(IteratorProtocolWitness);

impl GeneratorDelegationProtocol {
    /// The only inhabitant: 14.4.14's delegated yield runs the real iterator
    /// protocol and its close path.
    pub const YIELD_STAR: Self = Self(IteratorProtocolWitness::YIELD_STAR_DELEGATION_PROTOCOL);

    pub(crate) const fn witness(self) -> IteratorProtocolWitness {
        self.0
    }
}

/// The protocol witness carried by an argument-list spread operand.
///
/// This wrapper has one inhabitant and a private constructor. Consequently a
/// new `ExprIr::SpreadArgument` cannot omit the protocol discharge or carry an
/// unrelated for-of/destructuring witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpreadArgumentProtocol(IteratorProtocolWitness);

impl SpreadArgumentProtocol {
    /// The only inhabitant: 13.3.8.1 consumes a real sync iterator, while its
    /// abrupt paths do not invoke `IteratorClose`.
    pub const ARGUMENT_LIST: Self = Self(IteratorProtocolWitness::CALL_ARGUMENT_SPREAD_PROTOCOL);

    pub(crate) const fn witness(self) -> IteratorProtocolWitness {
        self.0
    }
}

/// The protocol witness carried by a spread element in ArrayAccumulation.
///
/// This wrapper has one inhabitant and a private constructor. A new array
/// spread therefore cannot omit the protocol discharge or substitute the
/// witness belonging to argument-list spread or destructuring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArraySpreadProtocol(IteratorProtocolWitness);

impl ArraySpreadProtocol {
    /// The only inhabitant: 13.2.4.1 performs the real sync iterator protocol
    /// and deliberately has no IteratorClose step.
    pub const ARRAY_ACCUMULATION: Self =
        Self(IteratorProtocolWitness::ARRAY_LITERAL_SPREAD_PROTOCOL);

    pub(crate) const fn witness(self) -> IteratorProtocolWitness {
        self.0
    }
}

// ---------------------------------------------------------------------------
// What each witness claims, checked by `cargo check`
// ---------------------------------------------------------------------------
//
// Every reader above is a `const fn`, so "this constant says what its doc
// comment says" is checkable at compile time and does not need a test run to
// discover. That matters twice here:
//
//  1. It is the campaign's standard — an invariant the compiler enforces beats
//     one a test discovers.
//  2. §13.12 narrowed every reader to `pub(crate)` so that a `scriptw-aot-wasm`
//     arm binding `protocol` and branching on it is `E0624`. That narrowing left
//     the readers with no caller outside `#[cfg(test)]`, i.e. `dead_code` in the
//     product build — the "survival by `pub`" defect this area exists to delete,
//     reappearing one visibility level down. These assertions are their callers,
//     so the emitter still cannot read a witness and the readers still earn
//     their place.
//
// The mistake actually being caught: editing a witness so it *claims* emission
// it does not perform (or renaming a premise into a weaker one) is the exact lie
// that makes the index walk look discharged. That is now `E0080` at build time.

const fn assumes(
    witness: IteratorProtocolWitness,
    obligation: IteratorObligation,
    premise: IntactnessPremise,
) -> bool {
    match witness.discharge(obligation) {
        ObligationDischarge::ByAssumption(actual) => actual as u8 == premise as u8,
        ObligationDischarge::ByEmission(_) => false,
    }
}

const fn emits(
    witness: IteratorProtocolWitness,
    obligation: IteratorObligation,
    site: EmissionSite,
) -> bool {
    match witness.discharge(obligation) {
        ObligationDischarge::ByEmission(actual) => actual as u8 == site as u8,
        ObligationDischarge::ByAssumption(_) => false,
    }
}

const fn assumes_kind(
    witness: IteratorProtocolWitness,
    obligation: IteratorObligation,
    kind: PremiseKind,
) -> bool {
    match witness.discharge(obligation) {
        ObligationDischarge::ByAssumption(premise) => premise.kind() as u8 == kind as u8,
        ObligationDischarge::ByEmission(_) => false,
    }
}

const fn emits_every_obligation(witness: IteratorProtocolWitness, site: EmissionSite) -> bool {
    witness.is_fully_emitted()
        && emits(witness, IteratorObligation::GetIterator, site)
        && emits(witness, IteratorObligation::IteratorStep, site)
        && emits(witness, IteratorObligation::IteratorValue, site)
        && emits(witness, IteratorObligation::IteratorClose, site)
}

/// The four obligations, as a value to quantify over.
///
/// Promoted out of `#[cfg(test)]`: [`site_is_witnessed`] is a product-path
/// caller, so this is the opposite of the "survival by `pub`" shape — it is a
/// const with a const consumer, and the test module below reads the same one
/// rather than keeping a second copy that could drift from it.
pub(crate) const ALL_OBLIGATIONS: [IteratorObligation; 4] = [
    IteratorObligation::GetIterator,
    IteratorObligation::IteratorStep,
    IteratorObligation::IteratorValue,
    IteratorObligation::IteratorClose,
];

/// True when some witness constant discharges **`obligation`** by emission at
/// `site` — i.e. some IR construct has accepted responsibility for that emitter
/// arm running *that* operation.
///
/// This is the direction the catalog could not state, at the resolution the
/// catalog needs. `EmissionSite` already guarantees that a variant *names a
/// real function*; what it could not guarantee is that any construct in the IR
/// admits to causing that function to run. `SYNC_PROTOCOL_SITES` credited
/// `ArrayDestructuring` while no witness mentioned it, and nothing could see
/// the asymmetry.
///
/// Per-obligation rather than per-site, because a site may run 7.4.2/7.4.8/7.4.9
/// and owe no 7.4.11 (13.3.8.1's argument-list spread is the worked example).
/// J10 in `operations.rs` asks this question of each row's own operation, so
/// crediting such a site on the `IteratorClose` row is a build failure rather
/// than a convention.
pub(crate) const fn site_emits(site: EmissionSite, obligation: IteratorObligation) -> bool {
    let mut i = 0;
    while i < ALL_WITNESSES.len() {
        // `match` and `as u8`, not `if let` and `==`: this is the shape the
        // `assumes`/`emits` helpers above already use, because `EmissionSite`
        // has no `const` `PartialEq`.
        match ALL_WITNESSES[i].discharge(obligation) {
            ObligationDischarge::ByEmission(actual) => {
                if actual as u8 == site as u8 {
                    return true;
                }
            }
            ObligationDischarge::ByAssumption(_) => {}
        }
        i += 1;
    }
    false
}

/// True when some witness constant discharges *some* obligation by emission at
/// `site`. K1's question, derived from [`site_emits`] rather than re-deriving
/// the scan.
pub(crate) const fn site_is_witnessed(site: EmissionSite) -> bool {
    let mut j = 0;
    while j < ALL_OBLIGATIONS.len() {
        if site_emits(site, ALL_OBLIGATIONS[j]) {
            return true;
        }
        j += 1;
    }
    false
}

// (K1) Every `EmissionSite` is witnessed: no emitter arm may be named by the
//      site domain without some IR construct's witness accepting responsibility
//      for it.
//
//      This assertion **failed on the tree that preceded
//      `ARRAY_DESTRUCTURING_PROTOCOL`**: `SYNC_PROTOCOL_SITES` credited
//      `EmissionSite::ArrayDestructuring` and the six witness constants named
//      only the two for-of sites. That is the point — a tie that passes before
//      and after the change it is supposed to force is decoration. Reverting
//      `ARRAY_DESTRUCTURING_PROTOCOL` in a scratch copy reproduces the failure.
const _: () = {
    let mut i = 0;
    while i < EmissionSite::ALL.len() {
        assert!(
            site_is_witnessed(EmissionSite::ALL[i]),
            "an EmissionSite names an emitter arm that no IR construct's witness has accepted"
        );
        i += 1;
    }
};

// Array-literal spread emits acquisition, stepping and value extraction, but
// ArrayAccumulation deliberately has no IteratorClose operation. Ask through
// the one-inhabitant wrapper so a transposed witness field cannot compile while
// leaving the underlying constant intact.
const _: () = {
    let witness = ArraySpreadProtocol::ARRAY_ACCUMULATION.witness();
    assert!(
        emits(
            witness,
            IteratorObligation::GetIterator,
            EmissionSite::ArrayLiteralSpread,
        ) && emits(
            witness,
            IteratorObligation::IteratorStep,
            EmissionSite::ArrayLiteralSpread,
        ) && emits(
            witness,
            IteratorObligation::IteratorValue,
            EmissionSite::ArrayLiteralSpread,
        ) && assumes(
            witness,
            IteratorObligation::IteratorClose,
            IntactnessPremise::ArrayAccumulationDoesNotClose,
        ),
        "ArraySpreadProtocol::ARRAY_ACCUMULATION must emit acquisition/step/value and must not claim a close"
    );
};

// (K2) The array-destructuring constant says what its doc comment says, in the
//      same shape as the two `emits_every_obligation` assertions below — and it
//      is asked of the value reachable *through the IR field's type*, so this is
//      also `ArrayPatternProtocol::witness`'s `const` consumer. A newtype whose
//      only inhabitant were the wrong witness would fail here.
const _: () = assert!(
    emits_every_obligation(
        ArrayPatternProtocol::ARRAY_DESTRUCTURING.witness(),
        EmissionSite::ArrayDestructuring,
    ),
    "ArrayPatternProtocol::ARRAY_DESTRUCTURING must emit all four 7.4 obligations at \
     compile_array_destructure_from_value_locals"
);

// Argument-list spread emits the first three operations and deliberately does
// not claim an `IteratorClose` emission. Asking through the one-inhabitant IR
// wrapper catches both a wrongly wired field and a transposed witness slot.
const _: () = {
    let witness = SpreadArgumentProtocol::ARGUMENT_LIST.witness();
    assert!(
        emits(
            witness,
            IteratorObligation::GetIterator,
            EmissionSite::CallArgumentSpread,
        ) && emits(
            witness,
            IteratorObligation::IteratorStep,
            EmissionSite::CallArgumentSpread,
        ) && emits(
            witness,
            IteratorObligation::IteratorValue,
            EmissionSite::CallArgumentSpread,
        ) && assumes(
            witness,
            IteratorObligation::IteratorClose,
            IntactnessPremise::SpreadCloseOwedOnlyAfterAcquisition,
        ),
        "SpreadArgumentProtocol::ARGUMENT_LIST must emit acquisition/step/value and must not claim a close"
    );
};

// The delegated-yield newtype's only inhabitant must discharge every 7.4
// obligation at the generator-delegation emitter family. Asking through the
// wrapper is important: it catches a wrapper accidentally pointing at a
// different otherwise-valid witness.
const _: () = assert!(
    emits_every_obligation(
        GeneratorDelegationProtocol::YIELD_STAR.witness(),
        EmissionSite::GeneratorDelegation,
    ),
    "GeneratorDelegationProtocol::YIELD_STAR must emit all four 7.4 obligations at \
     the generator-delegation emitter family"
);

// (K3) is retired. It asserted `ALL_WITNESSES.len() == 7`, which is the
//      `ALL.len() == 29` shape ledger **L1** showed cannot detect its own
//      omission: the count is exactly what forgetting a row preserves. The
//      census is now generated from the same rows as the constants by
//      `iterator_witnesses!`, so completeness is definitional and K1 is total
//      rather than conditional on a hand-maintained list. Ledger **IC-4** is
//      closed by that macro, not by a length check.

// (K4) Two sites may not name the same emitter function.
//
//      `emission_sites_are_backed` proves each variant names *a* real function;
//      it cannot notice that two variants name the *same* one. A copy-pasted row
//      whose string was not changed would pass name resolution, pass K1, pass
//      J10 and J11, and leave the catalog crediting "two arms" that are one arm
//      — with `AbruptDiscipline` and the witness census then attributing one
//      emitter's obligations twice. This is also `EmissionSite::name`'s only
//      consumer, which is the point: a renderer nobody reads is the "survival by
//      `pub`" shape, and this is a `const` reader that catches a real mistake.
const _: () = {
    let mut i = 0;
    while i < EmissionSite::ALL.len() {
        let mut j = i + 1;
        while j < EmissionSite::ALL.len() {
            assert!(
                !crate::operations::str_eq(
                    EmissionSite::ALL[i].name(),
                    EmissionSite::ALL[j].name()
                ),
                "two EmissionSite variants name the same emitter function"
            );
            j += 1;
        }
        i += 1;
    }
};

/// Every for-of protocol path emits each operation at its own backend site.
const _: () = assert!(
    emits_every_obligation(
        IteratorProtocolWitness::SYNC_ITERATOR_PROTOCOL,
        EmissionSite::SyncForOfIterator,
    ),
    "SYNC_ITERATOR_PROTOCOL must emit all four 7.4 obligations at compile_for_of_iterator"
);

const _: () = assert!(
    emits_every_obligation(
        IteratorProtocolWitness::RESUMABLE_SYNC_ITERATOR_PROTOCOL,
        EmissionSite::ResumableSyncForOfIterator,
    ),
    "RESUMABLE_SYNC_ITERATOR_PROTOCOL must emit all four 7.4 obligations at \
     compile_async_function_for_of_iterator"
);

const _: () = assert!(
    emits_every_obligation(
        IteratorProtocolWitness::ASYNC_ITERATOR_PROTOCOL,
        EmissionSite::AsyncForOfIterator,
    ),
    "ASYNC_ITERATOR_PROTOCOL must emit all four 7.4 obligations at compile_async_for_of_iterator"
);

/// The bail-out witness assumes nothing about the program: every obligation is
/// vacuous because no iteration was lowered.
const _: () = assert!(
    assumes_kind(
        IteratorProtocolWitness::NO_ITERATION,
        IteratorObligation::GetIterator,
        PremiseKind::Vacuous,
    ) && assumes_kind(
        IteratorProtocolWitness::NO_ITERATION,
        IteratorObligation::IteratorStep,
        PremiseKind::Vacuous,
    ) && assumes_kind(
        IteratorProtocolWitness::NO_ITERATION,
        IteratorObligation::IteratorValue,
        PremiseKind::Vacuous,
    ) && assumes_kind(
        IteratorProtocolWitness::NO_ITERATION,
        IteratorObligation::IteratorClose,
        PremiseKind::Vacuous,
    ),
    "NO_ITERATION must be vacuous in all four obligations"
);
