//! The module binding-name domains: `[[LocalName]]`, `[[ExportName]]` and the
//! merged storage name.
//!
//! ECMA-262 gives an ImportEntry Record (16.2.1.4) and an ExportEntry Record
//! (16.2.1.5) *different* name fields with different definitions:
//! `[[LocalName]]` is "the name that is used to locally access the imported
//! value from within the importing module", while `[[ExportName]]` is "the name
//! used to export this binding by this module". `export { a as b }` is the
//! production that makes them differ, and `ResolveExport` (16.2.1.6.2) matches
//! on one and returns the other.
//!
//! This compiler adds a third domain the specification does not have. The
//! linker assembles every unit's *source text* into one generated Script, in
//! which each module body is its own private activation owner (an async arrow
//! of the module graph table); cross-module reads go through trusted cells the
//! linker resolves, not through shared names. The name a binding is spelled by
//! in that generated text — inside its own unit's activation — is the
//! **merged name**. Two units may therefore use the same merged spelling for
//! different cells: cells are addressed by `(unit, merged name)`.
//!
//! ```text
//! D1  [[LocalName]]   LocalName    a name in one module's own environment
//! D2  [[ExportName]]  ExportName   a name in a module's export table
//! D3  merged name     MergedName   a name in the generated Script text
//! ```
//!
//! `[[ImportName]]` is **not** a fourth domain: 16.2.1.4 defines it as "the name
//! under which the desired binding is exported by the module identified by
//! `[[ModuleRequest]]`", i.e. a `[[ExportName]]` read from the other side, and
//! `ResolveExport(target, importName)` passes it straight in as `exportName`.
//! So [`ImportNameIr::Name`](crate::ImportNameIr::Name) carries an
//! [`ExportName`].
//!
//! D3 has exactly three generators and no others:
//!
//! * [`LocalName::merged_in`] for a source binding. It applies **no** prefix,
//!   so it is the identity except for `*default*`, which becomes the unit's
//!   [`UnitLinkerSpelling::anonymous_default`].
//! * [`UnitLinkerSpelling`] for the two cells generated *inside* a unit's
//!   activation — the anonymous default (`$d<tag>$`) and the `import.meta`
//!   object (`$m<tag>$meta`). Source code of that unit can name anything in
//!   its activation, so these spellings are chosen per unit to occur nowhere
//!   among the graph's identifiers, and the linker removes both from the
//!   eval-visible name table of the unit's activation. User code can then
//!   neither read, assign, `typeof`, shadow nor `eval` its way to them, and a
//!   free reference spelled like one falls through to the global object, as
//!   it must for the unspellable 8.2.2 `*default*` binding.
//! * [`MergedName::minted`] for a compiler-owned per-unit cell, whose role
//!   ranges over the closed set [`UnitCellRole`] rather than over any source
//!   name. Those cells are read only by linker-generated dispatcher text and
//!   are resolved to trusted cells, never through a unit's activation.
//!
//! There is deliberately **no** `MergedName::prefixed(unit, &LocalName)`: no
//! function anywhere takes a `[[LocalName]]` and returns a `$m{unit}$`-prefixed
//! name, so "apply the prefix twice" is not an expression that can be written.
//!
//! Deliberately absent from all three name types, and deliberately never to be
//! added: `Display`, `AsRef<str>`, `Deref<Target = str>`, `FromStr`, `Default`,
//! and `From<…> for String`. A stringification must name `as_str()`,
//! `spec_name()` or `merged_in(..)` at the call site — `format!("{name}")` would
//! otherwise quietly reintroduce the `String` domain these types replace, and
//! here it would reintroduce it *with the wrong answer*, since `spec_name()` and
//! `merged_in()` differ exactly on the case that matters.
//!
//! See `docs/rust-rewrite/contracts/module-binding-names.md`.

use std::collections::BTreeSet;

use crate::{
    IrDiagnostic, ModuleUnitId, MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME, MODULE_DEFAULT_EXPORT_NAME,
};

// -- const helpers --------------------------------------------------------
//
// Private on purpose: a `pub const fn str_eq` would be workspace surface with
// no product call site, which is the shape AGENTS.md names as having shipped
// here before. `native_error.rs` and `well_known.rs` keep their own copies for
// the same reason.

/// Byte-wise `&str` equality usable in a `const` initializer.
const fn str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut index = 0;
    while index < a.len() {
        if a[index] != b[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Number of decimal digits `value` prints as.
const fn decimal_len(value: ModuleUnitId) -> usize {
    let mut digits = 1;
    let mut remaining = value;
    while remaining >= 10 {
        remaining /= 10;
        digits += 1;
    }
    digits
}

/// `true` when every byte of `text` is legal in an ASCII `IdentifierPart`.
///
/// The *body* only: a minted name always begins with `$`, which
/// [`MergedName::minted`] supplies, so a suffix never has to start one.
const fn is_identifier_body_ascii(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let legal = byte == b'$' || byte == b'_' || byte.is_ascii_alphanumeric();
        if !legal {
            return false;
        }
        index += 1;
    }
    true
}

/// `true` when `text` begins with `head`.
const fn begins_with(text: &str, head: &str) -> bool {
    let text = text.as_bytes();
    let head = head.as_bytes();
    if head.len() > text.len() {
        return false;
    }
    let mut index = 0;
    while index < head.len() {
        if text[index] != head[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// `true` when `text` ends with `tail`.
const fn ends_with(text: &str, tail: &str) -> bool {
    let text = text.as_bytes();
    let tail = tail.as_bytes();
    if tail.len() > text.len() {
        return false;
    }
    let offset = text.len() - tail.len();
    let mut index = 0;
    while index < tail.len() {
        if text[offset + index] != tail[index] {
            return false;
        }
        index += 1;
    }
    true
}

// -- spellings ------------------------------------------------------------

/// Prefix of every compiler-owned per-unit cell name, and of the `import.meta`
/// cell's [`UnitLinkerSpelling`].
///
/// `$` is a legal `IdentifierStart`, so source text can spell a name in this
/// range: the ranges are **not** disjoint from source names by construction.
/// The two cells generated inside a unit's activation are therefore spelled per
/// unit by [`UnitLinkerSpelling::choose`], away from the identifiers of every
/// unit of the graph (lowering recognizes the cells by name in all of them).
const MINTED_PREFIX: &str = "$m";
/// Prefix of the merged spelling of an anonymous `export default`.
const ANONYMOUS_DEFAULT_PREFIX: &str = "$d";
/// Separator between a unit id (or spelling tag) and whatever follows it.
const UNIT_ID_TERMINATOR: &str = "$";
/// Suffix of the `import.meta` cell, after `$m<tag>$`.
const IMPORT_META_SUFFIX: &str = "meta";
/// Characters a spelling tag is drawn from, in ascending ASCII order so that
/// enumerating tags by length and then by these indices is shortlex order.
const TAG_ALPHABET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz";
/// Longest spelling tag. Derived from budgets B1 and B2, see
/// [`MAX_LINKABLE_MODULE_UNIT_ID`] and assertions V2/V4.
const MAX_TAG_LEN: usize = 4;

/// Width of the parsed `export` keyword, reserved for generated binding heads.
pub(crate) const EXPORT_KEYWORD: &str = "export";
/// Width of the parsed `default` keyword, reserved for generated binding heads.
pub(crate) const DEFAULT_KEYWORD: &str = "default";
/// Declaration head `modules::source` writes for a non-hoistable anonymous
/// `export default`.
pub(crate) const DEFAULT_BINDING_LET: &str = "let ";
/// Declaration head `modules::source` writes for a hoistable one.
pub(crate) const DEFAULT_BINDING_VAR: &str = "var ";
/// The `=` that closes the rewritten declaration head.
pub(crate) const DEFAULT_BINDING_ASSIGN: &str = "=";

/// The `import.meta` meta-property, as it is written.
pub(crate) const IMPORT_META_TEXT: &str = "import.meta";
/// The keyword `import.meta` opens with, as `modules::record` checks it.
pub(crate) const IMPORT_META_HEAD: &str = "import";
/// The property name `import.meta` closes with, as `modules::record` checks it.
pub(crate) const IMPORT_META_TAIL: &str = "meta";

/// Bytes every valid `export default` pair leaves available for generated code
/// after its line terminators are reserved: the two keywords themselves.
///
/// Inline whitespace makes the source span at least one byte wider, but the
/// narrowest split spelling is `export\ndefault`, whose separator byte must be
/// preserved rather than spent on the replacement head.
const EXPORT_DEFAULT_MIN_CODE_WIDTH: usize = EXPORT_KEYWORD.len() + DEFAULT_KEYWORD.len();

/// Largest module unit id the source-text linker can name.
///
/// Derived, not chosen. Two in-place rewrites must not change a unit's byte
/// length, and each caps the width of a [`UnitLinkerSpelling`] tag:
///
/// * **B1** — `modules::source` rewrites `export default` into
///   `keyword ++ name ++ padding ++ "="` while preserving every line
///   terminator in the erased span. The two keywords guarantee 13 non-terminator
///   bytes even in `export\ndefault`, so
///   `len(UnitLinkerSpelling::anonymous_default()) <= 13 - 4 - 1 = 8`, i.e.
///   `len(tag) <= 5`.
/// * **B2** — `modules::record::rewrite_import_meta` replaces `import.meta` in
///   place, and its narrowest span is 11 bytes, so
///   `len(UnitLinkerSpelling::import_meta()) <= 11`, i.e. `len(tag) <= 4`.
///
/// B2 is strictly tighter, so tags are at most four bytes, and this cap is the
/// largest id whose decimal spelling is itself a tag: that decimal tag is
/// always the first candidate, which keeps `$d<unit>$` and `$m<unit>$meta` the
/// spellings of every unit whose source does not already use them. At a
/// four-byte tag the `import.meta` replacement is 11 bytes with *zero* padding,
/// which is what makes assertion V4 below worth writing: it is tight rather
/// than slack.
pub const MAX_LINKABLE_MODULE_UNIT_ID: ModuleUnitId = 9_999;

// -- D1: `[[LocalName]]` --------------------------------------------------

/// A `BindingIdentifier` as written, resolved out of the interner.
///
/// The only constructor rejects the one name 8.2.2 reserves, so
/// [`LocalName::Source`] and [`LocalName::AnonymousDefault`] cannot alias.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceName(String);

impl SourceName {
    /// The only way to build one.
    ///
    /// `None` for `*default*` and for nothing else — invariant L2. 8.2.2 mints
    /// that spelling precisely so no `BindingIdentifier` can produce it, and
    /// the specification then *relies* on the disjointness; this constructor is
    /// the single place the sentinel is compared against anything.
    ///
    /// It is not a spellability check. Whether a name can be written as an
    /// `IdentifierReference` in generated Script text is a property of the
    /// emitter, not of the domain. The interner has already decoded Unicode
    /// escapes, and generated text can spell astral and combining characters.
    /// `modules::namespace::is_binding_identifier` shares the lexer's character
    /// authority; it does not add another name domain (contract ledger R1).
    #[must_use]
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        (name != MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME).then_some(Self(name))
    }

    /// The name as source wrote it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `[[LocalName]]` (16.2.1.4, 16.2.1.5). Exactly two shapes — invariant L1.
///
/// This generalises [`ImportNameIr`](crate::ImportNameIr), which already
/// distinguishes `namespace-object` from a String by variant rather than by a
/// sentinel value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LocalName {
    /// A name the module's own text spells.
    Source(SourceName),
    /// `*default*` (8.2.2). No source text can produce it.
    AnonymousDefault,
}

impl LocalName {
    /// Classifies a name drawn from BoundNames (8.2.2).
    ///
    /// Total, and the single decision point for invariant L2: `*default*` is
    /// the one name no `BindingIdentifier` can spell, so a bound name equal to
    /// it can only be the anonymous-default binding, and every other spelling
    /// is source-spelled.
    #[must_use]
    pub fn from_bound_name(name: impl Into<String>) -> Self {
        match SourceName::new(name) {
            Some(source) => Self::Source(source),
            None => Self::AnonymousDefault,
        }
    }

    /// The spec spelling. `"*default*"` for [`Self::AnonymousDefault`].
    ///
    /// For diagnostics and for the 16.2 ExportedBindings check only. It is
    /// deliberately **not** what an emitter writes — see [`Self::merged_in`].
    #[must_use]
    pub fn spec_name(&self) -> &str {
        match self {
            Self::Source(name) => name.as_str(),
            Self::AnonymousDefault => MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME,
        }
    }

    /// Domain D1 -> D3, applied exactly once.
    ///
    /// Total (invariant M1): every `[[LocalName]]`, including `*default*`, has
    /// a merged spelling, so no call site can forget a failure case because
    /// there is none. No prefix is applied to a source-spelled name — that is
    /// the whole point of the source-text merge, which shares an exporter's
    /// cell with its importers by name.
    ///
    /// The two-arm `match` has no catch-all, so adding a third `[[LocalName]]`
    /// shape is `E0004` at the one place the mapping lives.
    ///
    /// `spelling` must be the owning unit's: the anonymous default is spelled
    /// by it, and source names cannot collide with it because it was chosen
    /// away from them.
    #[must_use]
    pub fn merged_in(&self, spelling: &UnitLinkerSpelling) -> MergedName {
        match self {
            Self::Source(name) => MergedName(name.as_str().to_string()),
            Self::AnonymousDefault => spelling.anonymous_default(),
        }
    }
}

// -- D2: `[[ExportName]]` -------------------------------------------------

/// `[[ExportName]]` (16.2.1.5), and — read from the requested module's side —
/// `[[ImportName]]` (16.2.1.4). One domain, two viewpoints.
///
/// Not validated, and deliberately so: `ModuleExportName` (16.2.3.1) admits an
/// arbitrary `StringLiteral`, as in `export { a as "any \u{10000} text" }`,
/// including one carrying unpaired surrogates, which this compiler round-trips
/// on purpose. Validating would be *false*. The type earns its place by
/// separation instead: swapping it with a [`LocalName`] is `E0308`, which is
/// the mistake this domain actually suffers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExportName(String);

impl ExportName {
    /// `"default"` (16.2.3.7).
    ///
    /// Spellable from source — `export { x as default }` produces it — so this
    /// is **not** the analogue of `*default*`, and the two must never be given
    /// a common type. It is defined from
    /// [`MODULE_DEFAULT_EXPORT_NAME`](crate::MODULE_DEFAULT_EXPORT_NAME) so
    /// there is one literal.
    pub const DEFAULT: &'static str = MODULE_DEFAULT_EXPORT_NAME;

    /// Wraps an export name. Total, because the domain is open.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The `[[ExportName]]` every `export default` form carries.
    #[must_use]
    pub fn default_export() -> Self {
        Self::new(Self::DEFAULT)
    }

    /// The name as the export table spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for the one name `export *` never re-exports (16.2.1.6.1 step 7).
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.0 == Self::DEFAULT
    }
}

// -- D3: the merged name --------------------------------------------------

/// Declares [`UnitCellRole`], its `ALL` enumeration and its `suffix` renderer
/// from **one** row list.
///
/// Assertion V6 could not do the job it claimed. `ALL`'s declared type
/// `[UnitCellRole; 5]` hardcodes its own length, so `assert!(ALL.len() == 5)`
/// compared 5 against 5 by construction. A sixth variant forced an edit to
/// `suffix()` (`E0004`) but **not** to `ALL`: the tree still compiled with a
/// five-element `ALL`, V6 still passed, and the new role's suffix was checked by
/// neither V3 (identifier-legality, invariant M4) nor V5 (distinctness) — which
/// is precisely the regression mistake class K4 exists to prevent, since
/// `import.meta` and `component.completion` are the two suffixes that were
/// deleted for failing V3.
///
/// With the enum and `ALL` being two expansions of one `$(...)+` sequence, `ALL`
/// cannot be short, long or out of order, and V3/V5 quantify over the whole
/// domain by construction.
macro_rules! unit_cell_roles {
    ($(
        $(#[$meta:meta])*
        $variant:ident => $suffix:literal;
    )+) => {
        /// The compiler-owned per-unit cells of the merged scope.
        ///
        /// Closed by the linker's design, not an open string namespace: every
        /// one of these is emitted into generated Script text as a declaration
        /// or a read, and each exists because the linker needs exactly one of it
        /// per unit. A new cell is a new row above, which const assertions
        /// V3/V5 then check over the whole of `ALL`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u8)]
        pub enum UnitCellRole {
            $(
                $(#[$meta])*
                $variant,
            )+
        }

        impl UnitCellRole {
            /// Every role, in declaration order — generated from the same rows
            /// as the enum, so "added a variant, forgot `ALL`" has no spelling.
            pub const ALL: &'static [UnitCellRole] = &[$(UnitCellRole::$variant,)+];

            /// The part of the minted name that follows `$m{unit}$`.
            ///
            /// Must be an ASCII `IdentifierPart` sequence, because every minted
            /// name is emitted as an `IdentifierReference` — assertion V3.
            #[must_use]
            pub const fn suffix(self) -> &'static str {
                match self {
                    $(UnitCellRole::$variant => $suffix,)+
                }
            }
        }
    };
}

unit_cell_roles! {
    /// Identity-cached namespace exotic object (16.2.1.10).
    Namespace => "namespace";
    /// Host-owned source object, independent of either namespace identity.
    ModuleSource => "source";
    DeferredNamespace => "deferred$namespace";
    /// `import defer` export table, published before the body executes.
    DeferCells => "defer$cells";
    /// `import defer` evaluator and its retained completion.
    DeferEvaluate => "defer$evaluate";
    DeferState => "defer$state";
    DeferError => "defer$error";
    DeferCaughtError => "defer$caught$error";
    /// Separate declaration scope inside the evaluator's exception boundary.
    DeferExecute => "defer$execute";
}

/// A name in the generated Script text.
///
/// Three generators and no other constructor: [`LocalName::merged_in`] for a
/// source binding, [`UnitLinkerSpelling`] for the two cells inside a unit's
/// activation, and [`MergedName::minted`] for a compiler-owned dispatcher cell.
/// There is no `From<String>`, so a bare `String` cannot become one, and no
/// constructor takes a `MergedName`, so the map cannot be applied twice.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MergedName(String);

impl MergedName {
    /// `$m{unit}${suffix}` — a compiler-owned per-unit cell.
    ///
    /// The `$m…$` prefix appears in exactly this one `format!` in the crate.
    #[must_use]
    pub fn minted(unit: ModuleUnitId, role: UnitCellRole) -> Self {
        Self(format!(
            "{MINTED_PREFIX}{unit}{UNIT_ID_TERMINATOR}{}",
            role.suffix()
        ))
    }

    /// The name as the merged script spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// -- per-unit spellings of the activation cells -----------------------------

/// The per-unit spelling of the two cells the linker declares *inside* a
/// unit's activation: the anonymous `export default` binding and the
/// `import.meta` object.
///
/// Invariant: neither [`Self::anonymous_default`] nor [`Self::import_meta`] is
/// any identifier in `reserved` — for a linked unit, every identifier of every
/// unit of the graph (references, bindings at any depth, top-level bound names
/// and import locals). [`Self::choose`] is the only constructor and
/// establishes it; the tag is 1–4 bytes of [`TAG_ALPHABET`], so budgets B1/B2
/// hold for every value of the type. Graph-wide, so that lowering may
/// recognize a cell reference by name in any unit.
///
/// Strict module code reaches its activation only through its own source text
/// or a direct eval with that environment; the linker removes both spellings
/// from that activation's eval-visible name table, so no user reference can
/// read, write, `typeof` or shadow either cell.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitLinkerSpelling {
    unit: ModuleUnitId,
    tag: String,
}

/// Every candidate tag was taken by one of the unit's identifiers.
///
/// Needs millions of distinct `$d…$`/`$m…$meta` identifiers in one module;
/// reported, never silently resolved by sharing a name with user code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkerSpellingExhausted {
    /// The unit that could not be spelled.
    pub unit: ModuleUnitId,
}

impl LinkerSpellingExhausted {
    /// The `unsupported` diagnostic for module `key`.
    #[must_use]
    pub fn to_diagnostic(self, key: &str) -> IrDiagnostic {
        IrDiagnostic::unsupported(format!(
            "unsupported in scriptw wasm-aot: module {key}: every linker spelling for its \
             anonymous default and `import.meta` cells (`{ANONYMOUS_DEFAULT_PREFIX}<tag>$`, \
             `{MINTED_PREFIX}<tag>${IMPORT_META_SUFFIX}` with tags of up to {MAX_TAG_LEN} \
             characters) is already an identifier of the module"
        ))
    }
}

impl UnitLinkerSpelling {
    /// Chooses `unit`'s spelling away from `reserved`, the graph's identifiers.
    ///
    /// Candidates are the decimal spelling of `unit` first — so a unit whose
    /// source does not use `$d<unit>$` or `$m<unit>$meta` keeps exactly those
    /// names — and then every other tag in shortlex order. A candidate is taken
    /// when either of its two names is in `reserved`.
    ///
    /// # Errors
    /// [`LinkerSpellingExhausted`] when every candidate is taken.
    pub fn choose(
        unit: ModuleUnitId,
        reserved: &BTreeSet<String>,
    ) -> Result<Self, LinkerSpellingExhausted> {
        candidate_tags(unit)
            .map(|tag| Self { unit, tag })
            .find(|spelling| {
                !reserved.contains(spelling.anonymous_default().as_str())
                    && !reserved.contains(spelling.import_meta().as_str())
            })
            .ok_or(LinkerSpellingExhausted { unit })
    }

    /// `choose` with nothing reserved: the decimal spelling of an admitted unit.
    #[cfg(test)]
    pub(crate) fn unreserved(unit: ModuleUnitId) -> Self {
        Self::choose(unit, &BTreeSet::new()).expect("an empty reservation leaves every tag free")
    }

    /// The unit this spelling belongs to.
    #[must_use]
    pub(crate) const fn unit(&self) -> ModuleUnitId {
        self.unit
    }

    /// `$d<tag>$` — the merged spelling of this unit's anonymous `export default`.
    ///
    /// Its byte length is capped by B1; assertion V2 checks that.
    #[must_use]
    pub fn anonymous_default(&self) -> MergedName {
        MergedName(format!(
            "{ANONYMOUS_DEFAULT_PREFIX}{}{UNIT_ID_TERMINATOR}",
            self.tag
        ))
    }

    /// `$m<tag>$meta` — this unit's `import.meta` object (13.3.12, 16.2.1.9).
    ///
    /// Its byte length is capped by B2; assertion V4 checks that.
    #[must_use]
    pub fn import_meta(&self) -> MergedName {
        MergedName(format!(
            "{MINTED_PREFIX}{}{UNIT_ID_TERMINATOR}{IMPORT_META_SUFFIX}",
            self.tag
        ))
    }

    /// Whether `name` could equal either spelling of *some* unit, so a
    /// reservation scan need keep only these identifiers.
    #[must_use]
    pub(crate) fn could_collide(name: &str) -> bool {
        name.starts_with(ANONYMOUS_DEFAULT_PREFIX) || name.starts_with(MINTED_PREFIX)
    }
}

/// `true` for a 1–4 byte string of [`TAG_ALPHABET`].
fn is_tag(text: &str) -> bool {
    (1..=MAX_TAG_LEN).contains(&text.len())
        && text
            .bytes()
            .all(|byte| TAG_ALPHABET.as_bytes().contains(&byte))
}

/// Every tag, decimal spelling of `unit` first, each exactly once.
fn candidate_tags(unit: ModuleUnitId) -> impl Iterator<Item = String> {
    let decimal = Some(unit.to_string()).filter(|tag| is_tag(tag));
    let skip = decimal.clone();
    decimal.into_iter().chain(
        (1..=MAX_TAG_LEN)
            .flat_map(|len| {
                let count = TAG_ALPHABET
                    .len()
                    .pow(u32::try_from(len).expect("tag length"));
                (0..count).map(move |mut index| {
                    let mut tag = vec![0u8; len];
                    for slot in tag.iter_mut().rev() {
                        *slot = TAG_ALPHABET.as_bytes()[index % TAG_ALPHABET.len()];
                        index /= TAG_ALPHABET.len();
                    }
                    String::from_utf8(tag).expect("the tag alphabet is ASCII")
                })
            })
            .filter(move |tag| skip.as_ref() != Some(tag)),
    )
}

// -- const assertions -----------------------------------------------------

/// Byte length of the widest `UnitLinkerSpelling::anonymous_default()`.
const ANONYMOUS_DEFAULT_MAX_LEN: usize =
    ANONYMOUS_DEFAULT_PREFIX.len() + MAX_TAG_LEN + UNIT_ID_TERMINATOR.len();

/// Byte length of the widest `UnitLinkerSpelling::import_meta()`.
const IMPORT_META_MAX_LEN: usize =
    MINTED_PREFIX.len() + MAX_TAG_LEN + UNIT_ID_TERMINATOR.len() + IMPORT_META_SUFFIX.len();

const fn no_suffix_is_the_import_meta_suffix() -> bool {
    let mut index = 0;
    while index < UnitCellRole::ALL.len() {
        if str_eq(UnitCellRole::ALL[index].suffix(), IMPORT_META_SUFFIX) {
            return false;
        }
        index += 1;
    }
    true
}

const fn tag_alphabet_is_ascending_identifier_parts() -> bool {
    let bytes = TAG_ALPHABET.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'$' || (index > 0 && bytes[index - 1] >= bytes[index]) {
            return false;
        }
        index += 1;
    }
    is_identifier_body_ascii(TAG_ALPHABET)
}

const fn every_suffix_is_an_identifier_body() -> bool {
    let mut index = 0;
    while index < UnitCellRole::ALL.len() {
        if !is_identifier_body_ascii(UnitCellRole::ALL[index].suffix()) {
            return false;
        }
        index += 1;
    }
    true
}

const fn every_suffix_is_distinct() -> bool {
    let mut left = 0;
    while left < UnitCellRole::ALL.len() {
        let mut right = left + 1;
        while right < UnitCellRole::ALL.len() {
            if str_eq(
                UnitCellRole::ALL[left].suffix(),
                UnitCellRole::ALL[right].suffix(),
            ) {
                return false;
            }
            right += 1;
        }
        left += 1;
    }
    true
}

// **V1.** The two rewrite heads must be the same width, or B1 would depend on
// whether the default declaration is hoistable.
const _: () = assert!(
    DEFAULT_BINDING_LET.len() == DEFAULT_BINDING_VAR.len(),
    "the `let ` and `var ` rewrite heads must be the same width"
);

// **V2 (budget B1).** The whole rewritten declaration head must fit in the
// code budget left by the narrowest split `export default` span after its line
// terminator is reserved.
const _: () = assert!(
    DEFAULT_BINDING_LET.len() + ANONYMOUS_DEFAULT_MAX_LEN + DEFAULT_BINDING_ASSIGN.len()
        <= EXPORT_DEFAULT_MIN_CODE_WIDTH,
    "the anonymous `export default` binding no longer fits the keywords it replaces"
);

// **V3 (invariant M4).** Every minted cell name is emitted as an
// `IdentifierReference`, so no suffix may contain anything else. This is what
// would have rejected the two deleted minters, whose suffixes were
// `import.meta` and `component.completion`.
const _: () = assert!(
    every_suffix_is_an_identifier_body(),
    "a unit-cell suffix is not an ASCII IdentifierPart sequence"
);

// **V4 (budget B2).** Only the `import.meta` cell is written into a
// fixed-width span; it is spelled by `UnitLinkerSpelling`, not by a
// `UnitCellRole`, and its widest tag must still fit.
const _: () = assert!(
    IMPORT_META_MAX_LEN <= IMPORT_META_TEXT.len(),
    "the `import.meta` cell name no longer fits the meta-property it replaces"
);

// **V5.** Two roles sharing a suffix would share a cell, and a role suffixed
// like the `import.meta` cell would share that one with a unit whose tag is its
// decimal id.
const _: () = assert!(
    every_suffix_is_distinct() && no_suffix_is_the_import_meta_suffix(),
    "two unit-cell names share a suffix"
);

// **V8.** Every admitted unit's decimal spelling is a candidate tag, so units
// whose source does not use them keep `$d<unit>$` and `$m<unit>$meta`.
const _: () = assert!(
    decimal_len(MAX_LINKABLE_MODULE_UNIT_ID) <= MAX_TAG_LEN,
    "an admitted unit id no longer spells as a linker tag"
);

// **V9.** Tags are identifier parts without `$` (so `$d<tag>$` ends at its
// terminator) and the alphabet is ascending, so candidate order is shortlex.
const _: () = assert!(
    tag_alphabet_is_ascending_identifier_parts(),
    "the linker tag alphabet is not ascending `$`-free identifier parts"
);

// **V6.** Deleted. `ALL` is generated from the same macro rows as the enum, so
// "short, long or out of order" is unrepresentable and an assertion about it
// could not fail. What V6 actually used to check — `5 == 5`, because `ALL`'s
// array type carried its own length — is recorded in the macro's doc comment so
// the next reader does not reinstate it. `all_is_in_declaration_order` goes with
// it: `#[repr(u8)]` assigns discriminants in declaration order and `ALL` is now
// generated in that same order, so it too cannot fail.

// **V7.** The two fragments `modules::record` matches an `import.meta` span
// against must together *be* the meta-property whose width V4 budgets, or V4
// would be tied to nothing.
const _: () = assert!(
    begins_with(IMPORT_META_TEXT, IMPORT_META_HEAD)
        && ends_with(IMPORT_META_TEXT, IMPORT_META_TAIL)
        && IMPORT_META_HEAD.len() + 1 + IMPORT_META_TAIL.len() == IMPORT_META_TEXT.len(),
    "the `import.meta` fragments no longer spell the meta-property"
);

#[cfg(test)]
mod tests {
    use super::*;

    /// L2, the one rejection: `*default*` is not a source name, and nothing
    /// else is rejected — spellability is the emitter's question, not the
    /// domain's.
    #[test]
    fn source_name_rejects_only_the_reserved_spelling() {
        assert!(SourceName::new("x").is_some());
        assert!(SourceName::new("*not a binding*").is_some());
        assert!(SourceName::new(MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME).is_none());
    }

    /// The single L2 decision point is total and classifies both ways.
    #[test]
    fn bound_names_classify_into_the_two_local_name_shapes() {
        assert_eq!(
            LocalName::from_bound_name("x"),
            LocalName::Source(SourceName::new("x").expect("`x` is a source name"))
        );
        assert_eq!(
            LocalName::from_bound_name(MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME),
            LocalName::AnonymousDefault
        );
    }

    /// D1 -> D3 applies no prefix to a source name: the merge shares the
    /// exporter's cell with its importers by name, which is what makes an
    /// imported binding live.
    #[test]
    fn merging_a_source_name_is_the_identity() {
        for unit in [0, 3] {
            let spelling = UnitLinkerSpelling::unreserved(unit);
            assert_eq!(
                LocalName::from_bound_name("x")
                    .merged_in(&spelling)
                    .as_str(),
                "x"
            );
        }
    }

    /// The one name 8.2.2 makes unspellable gets a per-unit merged spelling, so
    /// two units with an anonymous `export default` do not collide.
    #[test]
    fn the_anonymous_default_gets_a_per_unit_merged_name() {
        let zero = UnitLinkerSpelling::unreserved(0);
        let one = UnitLinkerSpelling::unreserved(1);
        assert_eq!(
            LocalName::AnonymousDefault.merged_in(&zero).as_str(),
            "$d0$"
        );
        assert_eq!(LocalName::AnonymousDefault.merged_in(&one).as_str(), "$d1$");
        assert_eq!(one.import_meta().as_str(), "$m1$meta");
        assert_eq!(
            LocalName::AnonymousDefault.spec_name(),
            MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME
        );
    }

    /// The two D3 generators have disjoint ranges, and each minted name carries
    /// its unit id.
    #[test]
    fn minted_names_are_per_unit_and_disjoint_from_merged_locals() {
        assert_eq!(
            MergedName::minted(0, UnitCellRole::Namespace).as_str(),
            "$m0$namespace"
        );
        assert_ne!(
            MergedName::minted(0, UnitCellRole::Namespace),
            MergedName::minted(1, UnitCellRole::Namespace)
        );
        assert_ne!(
            MergedName::minted(0, UnitCellRole::DeferCells),
            MergedName::minted(0, UnitCellRole::DeferEvaluate)
        );
        let spelling = UnitLinkerSpelling::unreserved(0);
        for role in UnitCellRole::ALL.iter().copied() {
            assert_ne!(
                MergedName::minted(0, role),
                LocalName::AnonymousDefault.merged_in(&spelling)
            );
            assert_ne!(MergedName::minted(0, role), spelling.import_meta());
        }
    }

    /// E4: `default` is spellable from source and `*default*` is not, so the
    /// two constants are not the same thing wearing different names.
    #[test]
    fn the_default_export_name_is_not_the_anonymous_default_local_name() {
        assert_ne!(ExportName::DEFAULT, MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME);
        assert!(ExportName::default_export().is_default());
        assert!(!ExportName::new("d").is_default());
        // `export { x as default }` really does produce it from source.
        assert!(SourceName::new(ExportName::DEFAULT).is_some());
    }

    /// The budgets V2 and V4 assert at compile time, checked here against the
    /// values the two rewriters actually measure so a change to either side is
    /// visible in one place. B2 is tight at a four-byte tag — the cap's decimal
    /// tag and the widest fallback alike; B1 has one byte of slack.
    #[test]
    fn the_byte_budgets_hold_at_the_widest_tag() {
        let decimal = UnitLinkerSpelling::unreserved(MAX_LINKABLE_MODULE_UNIT_ID);
        let fallback = UnitLinkerSpelling {
            unit: 0,
            tag: "zzzz".into(),
        };
        assert!(is_tag(&fallback.tag));
        for spelling in [decimal, fallback] {
            assert_eq!(
                spelling.import_meta().as_str().len(),
                IMPORT_META_TEXT.len()
            );
            let default = LocalName::AnonymousDefault.merged_in(&spelling);
            assert_eq!(
                DEFAULT_BINDING_LET.len() + default.as_str().len() + DEFAULT_BINDING_ASSIGN.len(),
                12
            );
            assert!(
                DEFAULT_BINDING_LET.len() + default.as_str().len() + DEFAULT_BINDING_ASSIGN.len()
                    <= EXPORT_DEFAULT_MIN_CODE_WIDTH
            );
        }
    }

    fn reserved(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    /// The decimal tag comes first, so every existing spelling is unchanged
    /// while the unit's source does not use it.
    #[test]
    fn a_unit_keeps_its_decimal_spelling_when_its_source_does_not_use_it() {
        let spelling = UnitLinkerSpelling::choose(7, &reserved(&["$d0$", "$m1$meta", "x"]))
            .expect("tag is free");
        assert_eq!(spelling.anonymous_default().as_str(), "$d7$");
        assert_eq!(spelling.import_meta().as_str(), "$m7$meta");
        assert_eq!(spelling.unit, 7);
    }

    /// Either name of a candidate takes the whole candidate; fallbacks run in
    /// shortlex order and never repeat the skipped decimal tag.
    #[test]
    fn a_taken_spelling_falls_back_in_shortlex_order() {
        let default_taken = UnitLinkerSpelling::choose(1, &reserved(&["$d1$"])).unwrap();
        assert_eq!(default_taken.anonymous_default().as_str(), "$d0$");
        assert_eq!(default_taken.import_meta().as_str(), "$m0$meta");

        let meta_taken = UnitLinkerSpelling::choose(1, &reserved(&["$m1$meta", "$d0$"])).unwrap();
        assert_eq!(meta_taken.anonymous_default().as_str(), "$d2$");

        let mut taken = vec!["$d5$".to_string()];
        taken.extend(TAG_ALPHABET.chars().map(|tag| format!("$m{tag}$meta")));
        let wide = UnitLinkerSpelling::choose(5, &taken.into_iter().collect()).unwrap();
        assert_eq!(wide.anonymous_default().as_str(), "$d00$");

        let mut tags = candidate_tags(5);
        assert_eq!(tags.next().as_deref(), Some("5"));
        let rest: Vec<String> = tags.take(TAG_ALPHABET.len() + 1).collect();
        assert!(!rest.contains(&"5".to_string()));
        assert_eq!(rest[0], "0");
        assert_eq!(rest[TAG_ALPHABET.len() - 2], "z");
        assert_eq!(rest[TAG_ALPHABET.len() - 1], "00");
        assert!(rest.iter().all(|tag| is_tag(tag)));
    }

    /// A unit past the decimal-tag cap has no decimal candidate but is still
    /// spelled inside the budgets.
    #[test]
    fn a_unit_without_a_decimal_tag_still_has_a_spelling() {
        let spelling = UnitLinkerSpelling::unreserved(MAX_LINKABLE_MODULE_UNIT_ID + 1);
        assert_eq!(spelling.anonymous_default().as_str(), "$d0$");
    }

    #[test]
    fn exhaustion_is_an_unsupported_diagnostic_naming_the_module() {
        let diagnostic = LinkerSpellingExhausted { unit: 3 }.to_diagnostic("m.mjs");
        assert!(
            diagnostic.message.contains("module m.mjs"),
            "{diagnostic:?}"
        );
        assert!(diagnostic.message.contains("unsupported"), "{diagnostic:?}");
    }
}
