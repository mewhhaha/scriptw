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
//! This compiler adds a third domain the specification does not have. `modules::link`
//! merges every unit's *source text* into one Script and lowers it once, so all
//! units' top-level bindings share one activation environment (the three reasons
//! are stated in that module, and the load-bearing one — a cross-module read
//! becoming an ordinary read of the exporter's own cell — is what makes an
//! imported binding live without runtime indirection). The name a binding is
//! spelled by in that one merged scope is the **merged name**.
//!
//! ```text
//! D1  [[LocalName]]   LocalName    a name in one module's own environment
//! D2  [[ExportName]]  ExportName   a name in a module's export table
//! D3  merged name     MergedName   a name in the single merged Script scope
//! ```
//!
//! `[[ImportName]]` is **not** a fourth domain: 16.2.1.4 defines it as "the name
//! under which the desired binding is exported by the module identified by
//! `[[ModuleRequest]]`", i.e. a `[[ExportName]]` read from the other side, and
//! `ResolveExport(target, importName)` passes it straight in as `exportName`.
//! So [`ImportNameIr::Name`](crate::ImportNameIr::Name) carries an
//! [`ExportName`].
//!
//! D3 has exactly two generators and no others:
//!
//! * [`LocalName::merged_in`] for a source binding. It applies **no** prefix —
//!   the merge shares cells by name on purpose — so it is the identity except
//!   for `*default*`, which becomes `$d{unit}$`.
//! * [`MergedName::minted`] for a compiler-owned per-unit cell, whose role
//!   ranges over the closed set [`UnitCellRole`] rather than over any source
//!   name.
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

use crate::{ModuleUnitId, MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME, MODULE_DEFAULT_EXPORT_NAME};

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

/// Prefix of every compiler-owned per-unit cell name.
///
/// `$` is not a byte this compiler mints at the start of a source-spelled
/// binding — but it *is* a legal `IdentifierStart`, so source text can spell a
/// name in this range and the two ranges are **not** disjoint by construction.
/// [`MergedName::is_minted_shaped`] is the predicate that closes the gap, and
/// `check_dynamic_import_linkable` is where it is enforced.
const MINTED_PREFIX: &str = "$m";
/// Prefix of the merged spelling of an anonymous `export default`.
const ANONYMOUS_DEFAULT_PREFIX: &str = "$d";
/// Separator between a unit id and whatever follows it in a minted name.
const UNIT_ID_TERMINATOR: &str = "$";

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
/// length, and each caps the decimal width of a unit id:
///
/// * **B1** — `modules::source` rewrites `export default` into
///   `keyword ++ name ++ padding ++ "="` while preserving every line
///   terminator in the erased span. The two keywords guarantee 13 non-terminator
///   bytes even in `export\ndefault`, so
///   `len(MergedName::anonymous_default(u)) <= 13 - 4 - 1 = 8`, i.e.
///   `len(dec(u)) <= 5`.
/// * **B2** — `modules::record::rewrite_import_meta` replaces `import.meta` in
///   place, and its narrowest span is 11 bytes, so
///   `len(MergedName::minted(u, ImportMeta)) <= 11`, i.e. `len(dec(u)) <= 4`.
///
/// B2 is strictly tighter. At exactly this cap the `import.meta` replacement is
/// 11 bytes with *zero* padding, which is what makes assertion V4 below worth
/// writing: it is tight rather than slack.
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
    #[must_use]
    pub fn merged_in(&self, unit: ModuleUnitId) -> MergedName {
        match self {
            Self::Source(name) => MergedName(name.as_str().to_string()),
            Self::AnonymousDefault => MergedName::anonymous_default(unit),
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
    /// `import.meta` object (13.3.12, 16.2.1.9).
    ImportMeta => "meta";
}

/// A name in the single merged Script top-level scope.
///
/// Two disjoint generators and no other constructor: [`LocalName::merged_in`]
/// for a source binding and [`MergedName::minted`] for a compiler-owned cell.
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

    /// `$d{unit}$` — the merged spelling of an anonymous `export default`.
    ///
    /// `pub(crate)`, and reached from exactly two places: the
    /// [`LocalName::AnonymousDefault`] arm of [`LocalName::merged_in`], and
    /// `modules::link` building the `DefaultExportRewrite::Bind` the source
    /// rewriter needs. Those two must agree, and they do by construction.
    ///
    /// Its byte length is capped by B1; assertion V2 checks that.
    pub(crate) fn anonymous_default(unit: ModuleUnitId) -> Self {
        Self(format!(
            "{ANONYMOUS_DEFAULT_PREFIX}{unit}{UNIT_ID_TERMINATOR}"
        ))
    }

    /// The name as the merged script spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this merged name lies in the *minted* range — `$m<digits>$…` or
    /// `$d<digits>$`.
    ///
    /// The two generators' ranges are **not** disjoint from the source range by
    /// construction, contrary to what `MINTED_PREFIX`'s doc used to say. That
    /// quantifier ranges over what the compiler mints, not over what source text
    /// may spell, and `$m0$namespace`, `$m1$meta` and `$d0$` are all legal
    /// `BindingIdentifier`s. `merged_in` is the identity on a source name, so a
    /// module that declares one collides with the prelude's own cell, and a
    /// module that merely *reads* one has the read captured by it.
    ///
    /// Derived from `MINTED_PREFIX`/`ANONYMOUS_DEFAULT_PREFIX`/
    /// `UNIT_ID_TERMINATOR` rather than from a literal, so this predicate and
    /// the generators cannot drift apart.
    #[must_use]
    pub fn is_minted_shaped(name: &str) -> bool {
        let rest = match name
            .strip_prefix(MINTED_PREFIX)
            .or_else(|| name.strip_prefix(ANONYMOUS_DEFAULT_PREFIX))
        {
            Some(rest) => rest,
            None => return false,
        };
        let digits_end = rest
            .find(|byte: char| !byte.is_ascii_digit())
            .unwrap_or(rest.len());
        digits_end > 0 && rest[digits_end..].starts_with(UNIT_ID_TERMINATOR)
    }
}

// -- const assertions -----------------------------------------------------

/// Byte length of `MergedName::anonymous_default(unit)`.
const fn anonymous_default_len(unit: ModuleUnitId) -> usize {
    ANONYMOUS_DEFAULT_PREFIX.len() + decimal_len(unit) + UNIT_ID_TERMINATOR.len()
}

/// Byte length of `MergedName::minted(unit, role)`.
const fn minted_len(unit: ModuleUnitId, role: UnitCellRole) -> usize {
    MINTED_PREFIX.len() + decimal_len(unit) + UNIT_ID_TERMINATOR.len() + role.suffix().len()
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
    DEFAULT_BINDING_LET.len()
        + anonymous_default_len(MAX_LINKABLE_MODULE_UNIT_ID)
        + DEFAULT_BINDING_ASSIGN.len()
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

// **V4 (budget B2).** Named for [`UnitCellRole::ImportMeta`] alone and
// deliberately *not* quantified over all roles: `DeferEvaluate`'s suffix is 14
// bytes and would fail, correctly, because only the `import.meta` cell is
// written into a fixed-width span. A second span-constrained role means
// editing this assertion, which is the point.
const _: () = assert!(
    minted_len(MAX_LINKABLE_MODULE_UNIT_ID, UnitCellRole::ImportMeta) <= IMPORT_META_TEXT.len(),
    "the `import.meta` cell name no longer fits the meta-property it replaces"
);

// **V5.** Two roles sharing a suffix would share a cell.
const _: () = assert!(
    every_suffix_is_distinct(),
    "two unit-cell roles share a suffix"
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
        assert_eq!(LocalName::from_bound_name("x").merged_in(3).as_str(), "x");
        assert_eq!(LocalName::from_bound_name("x").merged_in(0).as_str(), "x");
    }

    /// The one name 8.2.2 makes unspellable gets a per-unit merged spelling, so
    /// two units with an anonymous `export default` do not collide.
    #[test]
    fn the_anonymous_default_gets_a_per_unit_merged_name() {
        assert_eq!(LocalName::AnonymousDefault.merged_in(0).as_str(), "$d0$");
        assert_eq!(LocalName::AnonymousDefault.merged_in(1).as_str(), "$d1$");
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
        assert_eq!(
            MergedName::minted(7, UnitCellRole::ImportMeta).as_str(),
            "$m7$meta"
        );
        assert_ne!(
            MergedName::minted(0, UnitCellRole::Namespace),
            MergedName::minted(1, UnitCellRole::Namespace)
        );
        assert_ne!(
            MergedName::minted(0, UnitCellRole::DeferCells),
            MergedName::minted(0, UnitCellRole::DeferEvaluate)
        );
        for role in UnitCellRole::ALL.iter().copied() {
            assert_ne!(
                MergedName::minted(0, role),
                LocalName::AnonymousDefault.merged_in(0)
            );
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
    /// visible in one place. B2 is tight at the cap; B1 has one byte of slack.
    #[test]
    fn the_byte_budgets_hold_at_the_cap() {
        let meta = MergedName::minted(MAX_LINKABLE_MODULE_UNIT_ID, UnitCellRole::ImportMeta);
        assert_eq!(meta.as_str().len(), IMPORT_META_TEXT.len());
        let default = LocalName::AnonymousDefault.merged_in(MAX_LINKABLE_MODULE_UNIT_ID);
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
