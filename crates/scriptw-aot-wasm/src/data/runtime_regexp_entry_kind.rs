use super::*;

/// The closed domain the three `RUNTIME_REGEXP_ENTRY_KIND_*` words spell.
///
/// # Why this exists on top of [`RuntimeRegExpEntry`]
///
/// [`RuntimeRegExpEntry`] closes the **writer**: a fourth outcome is
/// `error[E0004]` at `append_runtime_regexp_program_table`. That bought nothing
/// on the **reader** side, which compared a raw `u64` against two of the three
/// constants. A fourth `RUNTIME_REGEXP_ENTRY_KIND_FOO = 3` would have compiled
/// cleanly next to its siblings and fallen through both comparisons in
/// `emit_regexp_program_slots` as a miss — reinstating, one level down,
/// the exact silent-skip class this table exists to remove.
///
/// So the *decision* the emitter makes is stated here, once, as an exhaustive
/// match ([`Self::throws_syntax_error`]), and the emitter builds its comparison
/// chain by iterating [`Self::ALL`]. Adding a variant is then a compile error at
/// two exhaustive matches in this file, and the emitted comparison follows
/// automatically rather than being one more transcription.
///
/// Residual, stated rather than papered over: [`Self::ALL`] is hand-written.
/// The compiler cannot enumerate a Rust enum, so the trigger to extend it is
/// the `error[E0004]` a new variant produces at the two matches below.
pub(crate) enum RuntimeRegExpEntryKind {
    Program,
    Rejected,
    Unsupported,
}

impl RuntimeRegExpEntryKind {
    /// Every kind. See the type's doc for why this is hand-written and what
    /// forces it to be kept honest.
    pub(crate) const ALL: [Self; 3] = [Self::Program, Self::Rejected, Self::Unsupported];

    /// The discriminant word written into
    /// [`RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD`].
    pub(crate) const fn word(&self) -> u64 {
        match self {
            Self::Program => RUNTIME_REGEXP_ENTRY_KIND_PROGRAM,
            Self::Rejected => RUNTIME_REGEXP_ENTRY_KIND_REJECTED,
            Self::Unsupported => RUNTIME_REGEXP_ENTRY_KIND_UNSUPPORTED,
        }
    }

    /// Does a run-time hit on a row of this kind throw `SyntaxError`?
    ///
    /// This is the whole policy, and it is deliberately not `!= Program`:
    /// `Unsupported` means the pattern is legal ECMAScript that Scriptw cannot
    /// compile statically, so it follows the same emitted compiler route as a
    /// total miss. An explicit runtime capability failure exits through the
    /// typed T19 semantic rejection outside JavaScript completion.
    pub(crate) const fn throws_syntax_error(&self) -> bool {
        match self {
            Self::Program | Self::Unsupported => false,
            Self::Rejected => true,
        }
    }
}
