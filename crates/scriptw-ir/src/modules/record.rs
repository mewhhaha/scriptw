//! Shared static tables for Source Text, JSON synthetic and Script graph records.
//!
//! Owns `ParseModule` (16.2.1.6.1), the admitted JSON record factory and static tables:
//! `ImportEntries` (16.2.2.3), `ExportEntries` (16.2.3.2), `ModuleRequests`,
//! and the module `[[Environment]]` shape (9.1.1.5) that
//! `InitializeEnvironment` (16.2.1.6.4) would create.
//!
//! Module bodies keep their original byte offsets so span-derived
//! `FunctionId`s and `Function.prototype.toString` slices stay exact. The one
//! rewrite that lives here — [`rewrite_import_meta`] — is length-preserving and
//! line-preserving for exactly that reason.

use crate::*;

use super::early::module_early_errors;
use super::module_key::ANONYMOUS_MODULE_KEY;

use boa_ast::declaration::{ImportKind, ReExportKind};
use boa_ast::expression::{ImportCall, ImportMeta};
use boa_ast::operations::{bound_names, var_declared_names};
use boa_ast::{Module, Position, Span};
use boa_interner::Sym;
use scriptw_front::SourceSpan;

/// Index of a module in [`ModuleGraphIr::units`](crate::ModuleGraphIr::units).
pub type ModuleUnitId = u32;

/// One `with { key: value }` import attribute.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportAttributeIr {
    /// Attribute key, as written.
    pub key: String,
    /// Attribute value, as written.
    pub value: String,
}

/// A repeated key refused by [`ModuleRequestAttributesIr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateImportAttributeKeyIr {
    key: String,
}

impl DuplicateImportAttributeKeyIr {
    /// The duplicated attribute key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl core::fmt::Display for DuplicateImportAttributeKeyIr {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "duplicate import attribute key: {}", self.key)
    }
}

impl std::error::Error for DuplicateImportAttributeKeyIr {}

/// The canonical `[[Attributes]]` list of a ModuleRequest Record.
///
/// Keys are unique and sorted by UTF-16 code-unit order. The storage is
/// private and only an immutable slice escapes, so derived request identity
/// cannot drift after a request becomes a graph-map key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ModuleRequestAttributesIr(Vec<ImportAttributeIr>);

impl ModuleRequestAttributesIr {
    /// Validates and canonicalizes one import-attribute list.
    ///
    /// # Errors
    /// Returns the duplicated key when the input contains two records with the
    /// same key. Static import syntax rejects that shape, and dynamic import
    /// obtains keys from an object's unique own-property keys.
    pub fn try_new(
        attributes: impl IntoIterator<Item = ImportAttributeIr>,
    ) -> Result<Self, DuplicateImportAttributeKeyIr> {
        let mut attributes: Vec<_> = attributes.into_iter().collect();
        attributes.sort_by(|left, right| left.key.encode_utf16().cmp(right.key.encode_utf16()));
        if let Some(pair) = attributes
            .windows(2)
            .find(|pair| pair[0].key == pair[1].key)
        {
            return Err(DuplicateImportAttributeKeyIr {
                key: pair[0].key.clone(),
            });
        }
        Ok(Self(attributes))
    }

    /// The empty attribute list.
    #[must_use]
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    /// Canonical attributes in UTF-16 key order.
    #[must_use]
    pub fn as_slice(&self) -> &[ImportAttributeIr] {
        &self.0
    }
}

/// What graph discovery can know about an `import()` call's `with` object.
///
/// `Known` includes the empty list. `Runtime` means evaluating the options
/// object, its `with` property, or its enumerable properties is required to
/// learn the list. Keeping that distinction closed prevents an absent options
/// expression from being conflated with an expression whose eventual request
/// is not statically knowable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicImportAttributesIr {
    /// The exact attributes are known and sorted by UTF-16 key order.
    ///
    /// ```compile_fail
    /// use scriptw_ir::DynamicImportAttributesIr;
    ///
    /// let _ = DynamicImportAttributesIr::Known(Vec::new());
    /// ```
    Known(ModuleRequestAttributesIr),
    /// Runtime evaluation is required to obtain the attributes.
    Runtime,
}

/// The phase-free identity of a `ModuleRequest` Record.
///
/// `ModuleRequestsEqual` compares only `[[Specifier]]` and `[[Attributes]]`.
/// Keeping both fields private makes the canonical form immutable after this
/// value becomes a host-resolution or module-map key.
///
/// ```compile_fail
/// use scriptw_ir::{ModuleRequestAttributesIr, ModuleRequestKeyIr};
///
/// let _ = ModuleRequestKeyIr {
///     specifier: "./m.js".to_string(),
///     attributes: ModuleRequestAttributesIr::empty(),
/// };
/// ```
///
/// ```compile_fail
/// use scriptw_ir::ModuleRequestKeyIr;
///
/// let mut request = ModuleRequestKeyIr::plain("./m.js");
/// request.specifier = "./other.js".to_string();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleRequestKeyIr {
    /// `[[Specifier]]`, exactly as written.
    specifier: String,
    /// `[[Attributes]]`, sorted by key.
    attributes: ModuleRequestAttributesIr,
}

impl ModuleRequestKeyIr {
    /// Creates a key from an already canonical attribute list.
    #[must_use]
    pub fn new(specifier: impl Into<String>, attributes: ModuleRequestAttributesIr) -> Self {
        Self {
            specifier: specifier.into(),
            attributes,
        }
    }

    /// Validates raw attributes and creates their key in one operation.
    ///
    /// # Errors
    /// Returns the duplicated key when `attributes` contains one.
    pub fn try_new(
        specifier: impl Into<String>,
        attributes: impl IntoIterator<Item = ImportAttributeIr>,
    ) -> Result<Self, DuplicateImportAttributeKeyIr> {
        Ok(Self::new(
            specifier,
            ModuleRequestAttributesIr::try_new(attributes)?,
        ))
    }

    /// A request key with no attributes.
    #[must_use]
    pub fn plain(specifier: impl Into<String>) -> Self {
        Self::new(specifier, ModuleRequestAttributesIr::empty())
    }

    /// `[[Specifier]]`, exactly as written.
    #[must_use]
    pub fn specifier(&self) -> &str {
        &self.specifier
    }

    /// Canonical `[[Attributes]]` in UTF-16 key order.
    #[must_use]
    pub fn attributes(&self) -> &[ImportAttributeIr] {
        self.attributes.as_slice()
    }
}

/// One phaseful occurrence of a `ModuleRequest` Record.
///
/// Phase is needed for import dispatch and evaluation classification, but is
/// deliberately outside [`ModuleRequestKeyIr`]. Derived equality here means
/// occurrence equality; host resolution and `ModuleRequestsEqual` must use
/// [`Self::key`] instead.
///
/// ```compile_fail
/// use scriptw_ir::{ImportPhaseIr, ModuleRequestIr, ModuleRequestKeyIr};
///
/// let _ = ModuleRequestIr {
///     key: ModuleRequestKeyIr::plain("./m.js"),
///     phase: ImportPhaseIr::Evaluation,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleRequestIr {
    key: ModuleRequestKeyIr,
    phase: ImportPhaseIr,
}

impl ModuleRequestIr {
    /// Creates an occurrence from an already canonical attribute list.
    #[must_use]
    pub fn new(
        specifier: impl Into<String>,
        phase: ImportPhaseIr,
        attributes: ModuleRequestAttributesIr,
    ) -> Self {
        Self::from_key(ModuleRequestKeyIr::new(specifier, attributes), phase)
    }

    /// Creates an occurrence from its phase-free identity and phase.
    #[must_use]
    pub const fn from_key(key: ModuleRequestKeyIr, phase: ImportPhaseIr) -> Self {
        Self { key, phase }
    }

    /// Validates raw attributes and creates their occurrence in one operation.
    ///
    /// # Errors
    /// Returns the duplicated key when `attributes` contains one.
    pub fn try_new(
        specifier: impl Into<String>,
        phase: ImportPhaseIr,
        attributes: impl IntoIterator<Item = ImportAttributeIr>,
    ) -> Result<Self, DuplicateImportAttributeKeyIr> {
        Ok(Self::from_key(
            ModuleRequestKeyIr::try_new(specifier, attributes)?,
            phase,
        ))
    }

    /// A plain evaluation-phase occurrence with no attributes.
    #[must_use]
    pub fn plain(specifier: impl Into<String>) -> Self {
        Self::from_key(
            ModuleRequestKeyIr::plain(specifier),
            ImportPhaseIr::Evaluation,
        )
    }

    /// The phase-free identity used by host resolution and module maps.
    #[must_use]
    pub const fn key(&self) -> &ModuleRequestKeyIr {
        &self.key
    }

    /// `[[Specifier]]`, exactly as written.
    #[must_use]
    pub fn specifier(&self) -> &str {
        self.key.specifier()
    }

    /// `[[Phase]]` for this occurrence.
    #[must_use]
    pub const fn phase(&self) -> ImportPhaseIr {
        self.phase
    }

    /// Canonical `[[Attributes]]` in UTF-16 key order.
    #[must_use]
    pub fn attributes(&self) -> &[ImportAttributeIr] {
        self.key.attributes()
    }
}

/// A resolved request that participates in module evaluation.
///
/// The target field is private and the only constructor checks the request's
/// phase exhaustively, so evaluation-order and async-evaluation code cannot
/// accidentally accept a defer- or source-phase edge after resolution has
/// erased the request context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ModuleEvaluationDependencyIr(ModuleUnitId);

impl ModuleEvaluationDependencyIr {
    /// Retains a resolved target only when the request evaluates it.
    #[must_use]
    pub(super) const fn from_resolved_request(
        request: &ModuleRequestIr,
        target: ModuleUnitId,
    ) -> Option<Self> {
        match request.phase() {
            ImportPhaseIr::Evaluation => Some(Self(target)),
            ImportPhaseIr::Defer | ImportPhaseIr::Source => None,
        }
    }

    /// The resolved unit that must evaluate before its referrer.
    #[must_use]
    pub(super) const fn target(self) -> ModuleUnitId {
        self.0
    }
}

/// `[[ImportName]]` of an import or indirect-export entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImportNameIr {
    /// The namespace exotic object of the requested module.
    ///
    /// Produced by `import * as ns from "m"` and `export * as ns from "m"`.
    Namespace,
    /// A named export of the requested module.
    ///
    /// An [`ExportName`], not a [`LocalName`]: 16.2.1.4 defines `[[ImportName]]`
    /// as "the name under which the desired binding is exported by the module
    /// identified by `[[ModuleRequest]]`", and `ResolveExport` takes it as its
    /// `exportName` argument. It is the requested module's D2, read from this
    /// side.
    Name(ExportName),
}

/// An `ImportEntry` Record (16.2.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntryIr {
    /// `[[ModuleRequest]]`.
    pub request: ModuleRequestIr,
    /// `[[ImportName]]`.
    pub import_name: ImportNameIr,
    /// `[[LocalName]]`, the immutable indirect binding the import creates.
    pub local_name: LocalName,
    /// Span of the declaration, for diagnostics.
    pub span: Option<SourceSpan>,
}

/// A `LocalExportEntry` Record.
///
/// The two fields are the whole reason this area has a contract: 16.2.1.5 gives
/// them different definitions and `export { a as b }` makes them differ, yet
/// `export let x` makes them coincide — which is exactly the case that hides a
/// swap. They are different types, so the swap is `E0308`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalExportEntryIr {
    /// `[[LocalName]]`; [`LocalName::AnonymousDefault`] for an anonymous
    /// `export default`.
    pub local_name: LocalName,
    /// `[[ExportName]]`.
    pub export_name: ExportName,
}

/// An `IndirectExportEntry` Record.
///
/// Carries no `[[LocalName]]` at all: `export { x } from "m"` binds nothing in
/// this module, which is why ExportedBindings excludes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndirectExportEntryIr {
    /// `[[ModuleRequest]]`.
    pub request: ModuleRequestIr,
    /// `[[ImportName]]`.
    pub import_name: ImportNameIr,
    /// `[[ExportName]]`.
    pub export_name: ExportName,
}

/// A `StarExportEntry` Record: `export * from "m"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarExportEntryIr {
    /// `[[ModuleRequest]]`.
    pub request: ModuleRequestIr,
}

/// How a module-environment binding was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleBindingKindIr {
    /// Created by `CreateImportBinding` (9.1.1.5.5): immutable and indirect.
    Import,
    /// `var`, including `var`s nested in blocks of the module body.
    Var,
    /// `let`.
    Let,
    /// `const`.
    Const,
    /// A hoistable declaration, initialized before the module body runs.
    Function,
    /// `class`.
    Class,
}

/// One binding of the module `[[Environment]]` (9.1.1.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleEnvBindingIr {
    /// Spec binding name ([`LocalName::AnonymousDefault`] for an anonymous
    /// `export default`).
    pub name: LocalName,
    /// How the binding was declared.
    pub kind: ModuleBindingKindIr,
    /// `false` for `const` and for import bindings.
    pub mutable: bool,
    /// `true` when `InitializeEnvironment` initializes it before evaluation.
    pub initialized_before_evaluation: bool,
    /// `true` when a read before the module body initializes it is a
    /// `ReferenceError` (`let`/`const`/`class`/`export default <expr>`).
    pub in_tdz_until_evaluated: bool,
    /// Set only for import bindings: `[[Module]]`/`[[BindingName]]`.
    pub indirect: Option<(ModuleRequestIr, ImportNameIr)>,
}

/// A scalar host catalog cannot represent an unpaired UTF-16 surrogate.
/// This proof is minted from the actual interned source value, never Display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogEligibility {
    Eligible,
    Unrepresentable,
}

impl CatalogEligibility {
    fn from_symbol(interner: &Interner, symbol: Sym) -> Self {
        match interner.resolve_expect(symbol).utf8() {
            Some(_) => Self::Eligible,
            None => Self::Unrepresentable,
        }
    }

    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Eligible, Self::Eligible) => Self::Eligible,
            (Self::Eligible, Self::Unrepresentable)
            | (Self::Unrepresentable, Self::Eligible)
            | (Self::Unrepresentable, Self::Unrepresentable) => Self::Unrepresentable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DynamicImportSiteOrigin {
    RetainedSource,
    PreparedScript,
}

/// One `import(...)` call site found in a module body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicImportSiteIr {
    /// Specifier when the argument is a string literal, so the target can be
    /// compiled into the artifact. `None` for computed specifiers.
    pub static_specifier: Option<String>,
    /// `[[Phase]]` of the call.
    pub phase: ImportPhaseIr,
    /// Compile-time knowledge of the request's `[[Attributes]]`.
    pub attributes: DynamicImportAttributesIr,
    catalog_eligibility: CatalogEligibility,
    origin: DynamicImportSiteOrigin,
}

impl DynamicImportSiteIr {
    pub(super) fn as_prepared_script_site(mut self) -> Self {
        self.origin = DynamicImportSiteOrigin::PreparedScript;
        self
    }

    pub(super) fn has_retained_source_head(&self) -> bool {
        self.origin == DynamicImportSiteOrigin::RetainedSource
    }

    /// Whether this retained call site can select a declared host key.
    /// This preserves exact known attributes and scalar-string eligibility;
    /// runtime attributes remain unknown until the emitted import evaluates.
    #[must_use]
    pub fn permits_catalog_key(&self, key: &ModuleRequestKeyIr) -> bool {
        self.catalog_occurrence(key).is_some()
    }

    /// Projects one declared host key to this call site's occurrence.
    /// Computed operands can select every declared key they have not ruled out;
    /// phase remains owned by the actual call site, never by host resolution.
    pub(super) fn catalog_occurrence(&self, key: &ModuleRequestKeyIr) -> Option<ModuleRequestIr> {
        match self.catalog_eligibility {
            CatalogEligibility::Eligible => {}
            CatalogEligibility::Unrepresentable => return None,
        }
        if self
            .static_specifier
            .as_ref()
            .is_some_and(|specifier| specifier != key.specifier())
        {
            return None;
        }
        match &self.attributes {
            DynamicImportAttributesIr::Known(attributes)
                if attributes.as_slice() != key.attributes() =>
            {
                None
            }
            DynamicImportAttributesIr::Known(_) | DynamicImportAttributesIr::Runtime => {
                Some(ModuleRequestIr::from_key(key.clone(), self.phase))
            }
        }
    }

    /// The full occurrence whose key graph discovery can ask the host to resolve.
    ///
    /// Runtime attributes deliberately discover the empty request variant.
    /// The emitted dispatcher still evaluates and validates their actual
    /// values, and will only match an exact request variant present in the AOT
    /// component registry. This can over-compile the attribute-free target but
    /// cannot make a differently attributed request resolve to it.
    pub(crate) fn discovery_request(&self) -> Option<ModuleRequestIr> {
        let specifier = self.static_specifier.clone()?;
        let attributes = match &self.attributes {
            DynamicImportAttributesIr::Known(attributes) => attributes.clone(),
            DynamicImportAttributesIr::Runtime => ModuleRequestAttributesIr::empty(),
        };
        Some(ModuleRequestIr::new(specifier, self.phase, attributes))
    }
}

/// Shared linking tables of a genuine Source Text, JSON synthetic or Script record.
///
/// [spec]: https://tc39.es/ecma262/#sec-source-text-module-records
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModuleRecordDataIr {
    ModuleSource { source_tag: String },
    SourceText(super::source::ModuleSyntax),
    Script,
    Json(scriptw_front::ParsedJson),
    Bytes(Vec<u8>),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRecordIr {
    data: ModuleRecordDataIr,
    /// Index of this record in the graph.
    pub id: ModuleUnitId,
    /// Host-normalized resolution key this record is registered under.
    pub key: ModuleKey,
    /// Byte length of the module source text.
    pub source_len: usize,
    /// `[[HasTLA]]`: the module body contains a top-level `await`.
    pub has_top_level_await: bool,
    /// Strictness of a script entry's own code.
    ///
    /// Modules are always strict and never consult this; the linker reads it
    /// only for `entry_is_script` graphs, where the entry body is emitted
    /// after the module wrapper and its own directive prologue would otherwise
    /// be buried mid-script, silently compiling strict entry code as sloppy.
    pub script_entry_strict: bool,
    /// `[[RequestedModules]]`, source order, deduplicated by canonical key and
    /// phase.
    ///
    /// `ModuleRequestsEqual` itself is phase-free; the static-semantics list
    /// retains one occurrence for each phase of that key. Link and evaluation
    /// consume this list directly so an earlier source/defer occurrence cannot
    /// move a later evaluation occurrence ahead of another dependency.
    pub requested_modules: Vec<ModuleRequestIr>,
    /// Phase-free projection used only for host discovery and resolution.
    ///
    /// First-key order is retained for deterministic loading, but this is not
    /// the specification's phaseful `[[RequestedModules]]` list.
    pub module_resolution_requests: Vec<ModuleRequestKeyIr>,
    /// `[[ImportEntries]]`.
    pub import_entries: Vec<ImportEntryIr>,
    /// `[[LocalExportEntries]]`.
    pub local_export_entries: Vec<LocalExportEntryIr>,
    /// `[[IndirectExportEntries]]`.
    pub indirect_export_entries: Vec<IndirectExportEntryIr>,
    /// `[[StarExportEntries]]`.
    pub star_export_entries: Vec<StarExportEntryIr>,
    /// Shape of the module `[[Environment]]`.
    pub environment: Vec<ModuleEnvBindingIr>,
    /// Byte span of every `import.meta` reference in the module body, in
    /// ascending order, indexing this module's own source text.
    ///
    /// A span, not a count: the linker rewrites `import.meta` out of the body
    /// (Script text cannot spell it) and needs to know where each one is. See
    /// [`rewrite_import_meta`].
    pub import_meta_sites: Vec<SourceSpan>,
    /// Every `import(...)` call site in the module body.
    pub dynamic_import_sites: Vec<DynamicImportSiteIr>,
    /// Parser-owned heads, in source order, addressed against the original unit.
    pub(super) dynamic_import_heads: Vec<DynamicImportHead>,
    // Only the AST factory can mint these denied keys. Display collisions do
    // not become host identities, including when diagnostic attrs are empty.
    catalog_unrepresentable_requests: BTreeSet<ModuleRequestKeyIr>,
    /// Every identifier of this unit's source that could spell a
    /// [`UnitLinkerSpelling`](crate::UnitLinkerSpelling) name: references and
    /// bindings at any depth, top-level bound names and import locals.
    /// Synthetic records have no source identifiers and keep it empty.
    linker_reserved_names: BTreeSet<String>,
}

/// Shape of a module's `export default`, from the merged script's point of
/// view. See [`ModuleRecordIr::default_export_form`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultExportFormIr {
    /// The module has no `export default`.
    Absent,
    /// `export default function f() {}` / `export default class C {}`: the
    /// declaration binds `f` / `C` itself.
    Named,
    /// Every other form. `hoisted` distinguishes an anonymous
    /// `HoistableDeclaration`, which is initialized before the body runs, from
    /// a `ClassDeclaration` or `AssignmentExpression`, which is in TDZ until
    /// its own statement is reached.
    Anonymous {
        /// The declaration is a hoistable one.
        hoisted: bool,
    },
}

impl ModuleRecordIr {
    pub const fn kind(&self) -> ModuleKindIr {
        match &self.data {
            ModuleRecordDataIr::SourceText(_) => ModuleKindIr::SourceText,
            ModuleRecordDataIr::Script => ModuleKindIr::Script,
            ModuleRecordDataIr::Json(_) => ModuleKindIr::Json,
            ModuleRecordDataIr::Bytes(_) => ModuleKindIr::Bytes,
            ModuleRecordDataIr::Text(_) => ModuleKindIr::Text,
            ModuleRecordDataIr::ModuleSource { .. } => ModuleKindIr::ModuleSource,
        }
    }

    pub(super) fn json_source(&self) -> Option<&scriptw_front::ParsedJson> {
        match &self.data {
            ModuleRecordDataIr::Json(source) => Some(source),
            ModuleRecordDataIr::SourceText(_)
            | ModuleRecordDataIr::ModuleSource { .. }
            | ModuleRecordDataIr::Script
            | ModuleRecordDataIr::Bytes(_)
            | ModuleRecordDataIr::Text(_) => None,
        }
    }

    pub(super) fn bytes_source(&self) -> Option<&[u8]> {
        match &self.data {
            ModuleRecordDataIr::Bytes(bytes) => Some(bytes),
            ModuleRecordDataIr::SourceText(_)
            | ModuleRecordDataIr::ModuleSource { .. }
            | ModuleRecordDataIr::Script
            | ModuleRecordDataIr::Json(_)
            | ModuleRecordDataIr::Text(_) => None,
        }
    }

    pub(super) fn source_syntax(&self) -> Option<&super::source::ModuleSyntax> {
        match &self.data {
            ModuleRecordDataIr::SourceText(syntax) => Some(syntax),
            ModuleRecordDataIr::Script
            | ModuleRecordDataIr::ModuleSource { .. }
            | ModuleRecordDataIr::Json(_)
            | ModuleRecordDataIr::Bytes(_)
            | ModuleRecordDataIr::Text(_) => None,
        }
    }

    pub(super) fn module_source_tag(&self) -> Option<&str> {
        match &self.data {
            ModuleRecordDataIr::ModuleSource { source_tag } => Some(source_tag),
            ModuleRecordDataIr::SourceText(_)
            | ModuleRecordDataIr::Script
            | ModuleRecordDataIr::Json(_)
            | ModuleRecordDataIr::Bytes(_)
            | ModuleRecordDataIr::Text(_) => None,
        }
    }

    pub(super) fn module_source(source_tag: &str, id: ModuleUnitId, key: ModuleKey) -> Self {
        let mut record = Self::synthetic(
            ModuleRecordDataIr::ModuleSource {
                source_tag: source_tag.to_owned(),
            },
            source_tag.len(),
            id,
            key,
        );
        // A native host source has no ECMAScript body, environment, or exports.
        record.local_export_entries.clear();
        record.environment.clear();
        record
    }

    /// JSON has exactly one mutable default cell, initialized to undefined at linking.
    pub(super) fn json(
        source: &scriptw_front::ParsedJson,
        id: ModuleUnitId,
        key: ModuleKey,
    ) -> Self {
        Self::synthetic(
            ModuleRecordDataIr::Json(source.clone()),
            source.source_text().len(),
            id,
            key,
        )
    }

    pub(super) fn bytes(bytes: &[u8], id: ModuleUnitId, key: ModuleKey) -> Self {
        Self::synthetic(
            ModuleRecordDataIr::Bytes(bytes.to_vec()),
            bytes.len(),
            id,
            key,
        )
    }

    pub(super) fn text_source(&self) -> Option<&str> {
        match &self.data {
            ModuleRecordDataIr::Text(text) => Some(text),
            ModuleRecordDataIr::SourceText(_)
            | ModuleRecordDataIr::ModuleSource { .. }
            | ModuleRecordDataIr::Script
            | ModuleRecordDataIr::Json(_)
            | ModuleRecordDataIr::Bytes(_) => None,
        }
    }

    pub(super) fn text(text: &str, id: ModuleUnitId, key: ModuleKey) -> Self {
        Self::synthetic(
            ModuleRecordDataIr::Text(text.to_owned()),
            text.len(),
            id,
            key,
        )
    }

    fn synthetic(
        data: ModuleRecordDataIr,
        source_len: usize,
        id: ModuleUnitId,
        key: ModuleKey,
    ) -> Self {
        Self {
            data,
            id,
            key,
            source_len,
            has_top_level_await: false,
            script_entry_strict: true,
            requested_modules: Vec::new(),
            module_resolution_requests: Vec::new(),
            import_entries: Vec::new(),
            local_export_entries: vec![LocalExportEntryIr {
                local_name: LocalName::AnonymousDefault,
                export_name: ExportName::new("default"),
            }],
            indirect_export_entries: Vec::new(),
            star_export_entries: Vec::new(),
            environment: vec![ModuleEnvBindingIr {
                name: LocalName::AnonymousDefault,
                kind: ModuleBindingKindIr::Let,
                mutable: true,
                initialized_before_evaluation: true,
                in_tdz_until_evaluated: false,
                indirect: None,
            }],
            import_meta_sites: Vec::new(),
            dynamic_import_sites: Vec::new(),
            dynamic_import_heads: Vec::new(),
            catalog_unrepresentable_requests: BTreeSet::new(),
            linker_reserved_names: BTreeSet::new(),
        }
    }
    pub(super) fn catalog_key_is_eligible(&self, key: &ModuleRequestKeyIr) -> bool {
        !self.catalog_unrepresentable_requests.contains(key)
    }

    /// The identifiers [`UnitLinkerSpelling::choose`](crate::UnitLinkerSpelling::choose)
    /// must avoid for this unit.
    pub(super) const fn linker_reserved_names(&self) -> &BTreeSet<String> {
        &self.linker_reserved_names
    }

    /// Number of `import.meta` references in the module body.
    ///
    /// `import.meta` is created lazily and at most once per module (13.3.12
    /// step 4), so a module with zero uses needs no object at all and the
    /// linker declares none.
    #[must_use]
    pub fn import_meta_uses(&self) -> usize {
        self.import_meta_sites.len()
    }

    /// Export names declared without consulting the graph (no `export *`).
    #[must_use]
    pub fn own_exported_names(&self) -> Vec<ExportName> {
        let mut names = Vec::new();
        for entry in &self.local_export_entries {
            push_unique_name(&mut names, &entry.export_name);
        }
        for entry in &self.indirect_export_entries {
            push_unique_name(&mut names, &entry.export_name);
        }
        names
    }

    /// `true` when the module needs no other source unit to be linked.
    #[must_use]
    pub fn is_self_contained(&self) -> bool {
        self.requested_modules.is_empty()
    }

    /// How this module's `export default`, if any, has to be rewritten for the
    /// merged script (16.2.3.7).
    ///
    /// The distinction the linker needs is not the grammar production but
    /// whether the declaration already binds a name the merged scope can spell.
    /// `export default function f() {}` and `export default class C {}` do, so
    /// deleting the two keywords leaves a declaration that binds exactly what
    /// the export entry names. Every other form has `[[LocalName]]`
    /// [`LocalName::AnonymousDefault`], which nothing can spell, so the
    /// keywords have to become a declaration of a minted name instead.
    ///
    /// The `[[ExportName]]` test and the `[[LocalName]]` test below are asked
    /// of different types, because they are different domains: `default` is
    /// spellable from source (`export { x as default }`) while `*default*`
    /// never is.
    #[must_use]
    pub fn default_export_form(&self) -> DefaultExportFormIr {
        let Some(entry) = self
            .local_export_entries
            .iter()
            .find(|entry| entry.export_name.is_default())
        else {
            return DefaultExportFormIr::Absent;
        };
        if entry.local_name != LocalName::AnonymousDefault {
            return DefaultExportFormIr::Named;
        }
        // A hoistable declaration is initialized before the body runs, so it
        // must not be given the TDZ a `let` would.
        let hoisted = self
            .environment
            .iter()
            .find(|binding| binding.name == LocalName::AnonymousDefault)
            .is_some_and(|binding| binding.kind == ModuleBindingKindIr::Function);
        DefaultExportFormIr::Anonymous { hoisted }
    }

    /// Duplicate `[[ExportName]]`s, which are an early error (16.2.3.1).
    #[must_use]
    pub fn duplicate_export_names(&self) -> Vec<ExportName> {
        let mut seen = BTreeSet::new();
        let mut duplicates = Vec::new();
        let names = self
            .local_export_entries
            .iter()
            .map(|entry| &entry.export_name)
            .chain(
                self.indirect_export_entries
                    .iter()
                    .map(|entry| &entry.export_name),
            );
        for name in names {
            if !seen.insert(name.clone()) {
                push_unique_name(&mut duplicates, name);
            }
        }
        duplicates
    }
}

// -- `import.meta` (13.3.12, 16.2.1.9) ------------------------------------
//
// `import.meta` is an ordinary object, created at most once per module, that
// the host populates. Three properties have to hold and each one falls out of a
// different part of the shape below:
//
// * *identity-stable within a module* — `import.meta === import.meta`. One
//   `const` binding per module, evaluated once, so every reference reads the
//   same cell.
// * *distinct per module* — two modules never share the object. The binding is
//   named from the unit id.
// * *ordinary and extensible* — an object literal is extensible and its
//   properties are writable, enumerable and configurable, which is what
//   `CreateDataPropertyOrThrow` gives them in step 4.c. The prototype is
//   `null`: step 4.a is `OrdinaryObjectCreate(null)`, not
//   `OrdinaryObjectCreate(%Object.prototype%)`.
//
// `url` is this host's whole `HostGetImportMetaProperties` result (16.2.1.9).
// It is a normative-optional hook, so a host is free to define exactly one
// property, and `url` is the one every other host agrees on.

/// The `import.meta` object of one module, as Script source text.
///
/// Source text rather than IR because the linker merges units by concatenating
/// their bodies and lowering once — see `modules::link`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportMetaBindingIr {
    /// Binding that holds the object, and that every rewritten `import.meta`
    /// in this unit reads.
    ///
    /// The unit's [`UnitLinkerSpelling::import_meta`], so it cannot collide
    /// with any identifier of the unit's own source — and so its byte length is
    /// bounded by const assertion V4, which is what [`rewrite_import_meta`]
    /// depends on.
    pub name: MergedName,
    /// The declaration that creates it, terminated by `;`.
    ///
    /// The linker emits this once, before any unit body runs.
    pub declaration: String,
}

/// The `import.meta` object of the unit `spelling` belongs to, with the host
/// properties 16.2.1.9 defines for it.
///
/// `meta_url` is [`ModuleUnitIr::meta_url`](crate::ModuleUnitIr::meta_url): the
/// host resolved and normalized it while loading, and nothing here re-derives
/// it.
#[must_use]
pub fn import_meta_binding(spelling: &UnitLinkerSpelling, meta_url: &str) -> ImportMetaBindingIr {
    let name = spelling.import_meta();
    // `__proto__:` in an object literal is the prototype setter (13.2.5.5), and
    // the lowerer recognises a `null` value as "no prototype"
    // (`lowering::lower_object_literal`), so this is `OrdinaryObjectCreate(null)`
    // without needing `Object.create` — which a unit could have shadowed by
    // declaring its own top-level `Object`.
    let declaration = format!(
        "const {} = {{ __proto__: null, url: {} }};",
        name.as_str(),
        js_string_literal(meta_url)
    );
    ImportMetaBindingIr { name, declaration }
}

/// Why a module body's `import.meta` references could not be rewritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportMetaRewriteError {
    /// Human-readable reason, already phrased as a diagnostic message body.
    pub reason: String,
}

impl ImportMetaRewriteError {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// Replaces every `import.meta` in `source` with a read of the module's
/// [`UnitLinkerSpelling::import_meta`] cell; `spelling` is that module's.
///
/// Script text cannot spell `import.meta` at all — boa rejects it outright
/// outside a module — so a linker that merges units into one Script *must*
/// rewrite it, and rewriting it into a binding read is also what makes the
/// object identity-stable for free.
///
/// `source` must retain the unit's original byte coordinates: both this pass
/// and module-syntax removal consume ranges from the same parsed source.
/// Run this first, then erase module syntax, then insert any anonymous default
/// declaration terminator. Later dynamic-import processing sees the final text.
///
/// The rewrite preserves the source's byte length and its line terminators, so
/// span-derived `FunctionId`s and reported line numbers do not move. The
/// replacement identifier is shorter than `import.meta`, and the slack is
/// filled with the line terminators the span contained followed by spaces.
///
/// # Errors
/// Returns [`ImportMetaRewriteError`] when a site does not address the text it
/// is supposed to (which would mean `source` is not this unit's), or when the
/// binding name does not fit in the span it replaces.
///
/// The second failure is unreachable: the narrowest `import.meta` span is 11
/// bytes and const assertion V4 holds every `UnitLinkerSpelling::import_meta`
/// to that width. It stays checked because the *span* is data from boa, not a
/// constant.
/// A HashbangComment (12.5) is legal only at the very start of a Script or
/// Module. Linked units are embedded inside one synthesized Script, so the
/// comment is rewritten to a SingleLineComment of the same length: every
/// recorded source span in the unit stays valid.
pub(crate) fn embeddable_unit_source(source: &str) -> std::borrow::Cow<'_, str> {
    match source.strip_prefix("#!") {
        Some(rest) => std::borrow::Cow::Owned(format!("//{rest}")),
        None => std::borrow::Cow::Borrowed(source),
    }
}

pub fn rewrite_import_meta(
    source: &str,
    record: &ModuleRecordIr,
    spelling: &UnitLinkerSpelling,
) -> Result<String, ImportMetaRewriteError> {
    if record.import_meta_sites.is_empty() {
        return Ok(source.to_string());
    }

    let cell = spelling.import_meta();
    let name = cell.as_str();
    let mut rewritten = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for site in &record.import_meta_sites {
        let replaced = source
            .get(site.start..site.end)
            .filter(|_| site.start >= cursor)
            .ok_or_else(|| {
                ImportMetaRewriteError::new(format!(
                    "`import.meta` at {}..{} is not a span of this module's source text",
                    site.start, site.end
                ))
            })?;
        // A cheap proof that the span really is the meta-property and not some
        // unrelated range: the rewrite destroys whatever it covers, so it is
        // worth refusing rather than silently mangling a body.
        if !replaced.starts_with(IMPORT_META_HEAD) || !replaced.ends_with(IMPORT_META_TAIL) {
            return Err(ImportMetaRewriteError::new(format!(
                "`import.meta` span {}..{} covers `{replaced}`",
                site.start, site.end
            )));
        }
        rewritten.push_str(&source[cursor..site.start]);
        rewritten.push_str(name);
        rewritten.push_str(&padding_for(replaced, name.len()).ok_or_else(|| {
            ImportMetaRewriteError::new(format!(
                "`import.meta` binding `{name}` does not fit the {} bytes it replaces",
                replaced.len()
            ))
        })?);
        cursor = site.end;
    }
    rewritten.push_str(&source[cursor..]);
    Ok(rewritten)
}

/// Filler that pads `name_len` bytes back out to `replaced.len()` while keeping
/// every line terminator `replaced` contained.
///
/// The terminators come first and the spaces after, so the token following the
/// site keeps its line *and* the line count of the unit is unchanged. Its
/// column shifts, which is the one thing this cannot preserve: an identifier
/// cannot be split across the lines the meta-property was written on.
///
/// Emitting the terminators after the identifier rather than before it is
/// deliberate. A line terminator before it could turn the preceding token into
/// the end of a statement under ASI; after it, the only terminators that can
/// appear between this site and the next token are ones the source already had
/// inside the span, so no ASI decision changes.
fn padding_for(replaced: &str, name_len: usize) -> Option<String> {
    let terminators: String = replaced
        .chars()
        .filter(|ch| matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        .collect();
    let spaces = replaced
        .len()
        .checked_sub(name_len)?
        .checked_sub(terminators.len())?;
    let mut padding = terminators;
    padding.push_str(&" ".repeat(spaces));
    Some(padding)
}

/// `value` as a double-quoted JavaScript string literal.
///
/// U+2028 and U+2029 are escaped even though they are legal in a string literal
/// since ES2019: the merged source is assembled by concatenation, and an
/// unescaped one is a line terminator to the line index the record's spans are
/// built from.
fn js_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for character in value.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\u{2028}' => literal.push_str("\\u2028"),
            '\u{2029}' => literal.push_str("\\u2029"),
            character if (character as u32) < 0x20 => {
                literal.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => literal.push(character),
        }
    }
    literal.push('"');
    literal
}

/// Appends `name` to `names` unless it is already present.
///
/// D2 only. Every one of its call sites — here and in `modules::graph`'s
/// `GetExportedNames` walk — is building an `ExportedNames` list, so a
/// `[[LocalName]]` reaching one is `E0308` rather than a namespace object with
/// a wrong key.
pub(crate) fn push_unique_name(names: &mut Vec<ExportName>, name: &ExportName) {
    if !names.iter().any(|existing| existing == name) {
        names.push(name.clone());
    }
}

fn push_unique_request(requests: &mut Vec<ModuleRequestIr>, request: &ModuleRequestIr) {
    if !requests.iter().any(|existing| existing == request) {
        requests.push(request.clone());
    }
}

fn push_unique_request_key(requests: &mut Vec<ModuleRequestKeyIr>, request: &ModuleRequestKeyIr) {
    if !requests.iter().any(|existing| existing == request) {
        requests.push(request.clone());
    }
}

fn resolved_name(interner: &Interner, name: Sym) -> String {
    interner.resolve_expect(name).to_string()
}

/// `[[LocalName]]` of a `BindingIdentifier` boa resolved out of the interner.
///
/// Every caller is a BoundNames position, so the classification
/// [`LocalName::from_bound_name`] performs is the right one and is total: only
/// `*default*` is not a source-spelled name, and only boa's own
/// anonymous-default marker can produce it.
fn resolved_local_name(interner: &Interner, name: Sym) -> LocalName {
    LocalName::from_bound_name(resolved_name(interner, name))
}

/// `[[ExportName]]` / `[[ImportName]]` boa resolved out of the interner.
///
/// A separate function from [`resolved_local_name`] on purpose: the two
/// accessors boa offers for a `ModuleExportName` pair are called
/// `private_name()` and `alias()` on both `ReExportKind::Named` and
/// `ExportDeclaration::List`, yet `private_name()` means `[[ImportName]]` in the
/// first and `[[LocalName]]` in the second. The conversion names the domain
/// where boa's accessor does not, and swapping the two conversions is `E0308`
/// one line later at the struct literal.
fn resolved_export_name(interner: &Interner, name: Sym) -> ExportName {
    ExportName::new(resolved_name(interner, name))
}

/// Byte offset of the start of every line, so boa's `(line, column)` positions
/// can be turned back into the byte offsets [`SourceSpan`] is expressed in.
///
/// Mirrors the lexer exactly (`boa_parser-0.21.1/src/lexer/cursor.rs`,
/// `next_char`): a line ends at `\r\n`, a bare `\r`, `\n`, U+2028 or U+2029,
/// and a column counts *code points*, not bytes and not UTF-16 code units.
#[derive(Debug)]
struct LineIndex {
    /// Byte offset of the first character of each line, in line order.
    line_starts: Vec<usize>,
    /// Byte length of the source, used as the clamp for anything out of range.
    len: usize,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        let mut characters = source.char_indices().peekable();
        while let Some((offset, character)) = characters.next() {
            let break_len = match character {
                '\r' => {
                    if characters.peek().is_some_and(|(_, next)| *next == '\n') {
                        characters.next();
                        '\r'.len_utf8() + '\n'.len_utf8()
                    } else {
                        '\r'.len_utf8()
                    }
                }
                '\n' | '\u{2028}' | '\u{2029}' => character.len_utf8(),
                _ => continue,
            };
            line_starts.push(offset + break_len);
        }
        Self {
            line_starts,
            len: source.len(),
        }
    }

    /// Byte offset of `position`, clamped to the end of its line.
    ///
    /// A `Span`'s end position is one past its last character, so a column of
    /// `line length + 1` is normal and must land on the line's last byte rather
    /// than swallowing the line break.
    fn offset(&self, source: &str, position: Position) -> usize {
        let line = position.line_number() as usize;
        let Some(&start) = self.line_starts.get(line - 1) else {
            return self.len;
        };
        let end = self.line_starts.get(line).copied().unwrap_or(self.len);
        let content = source[start..end].trim_end_matches(['\r', '\n', '\u{2028}', '\u{2029}']);
        content
            .char_indices()
            .nth(position.column_number() as usize - 1)
            .map_or(start + content.len(), |(offset, _)| start + offset)
    }

    fn source_span(&self, source: &str, span: Span) -> SourceSpan {
        let start = self.offset(source, span.start());
        SourceSpan {
            start,
            end: self.offset(source, span.end()).max(start),
        }
    }
}

fn module_request(
    interner: &Interner,
    request: &boa_ast::declaration::ModuleRequest,
) -> ModuleRequestIr {
    let attributes: Vec<ImportAttributeIr> = request
        .attributes()
        .iter()
        .map(|attribute| ImportAttributeIr {
            key: resolved_name(interner, attribute.key()),
            value: resolved_name(interner, attribute.value()),
        })
        .collect();
    let attributes = ModuleRequestAttributesIr::try_new(attributes)
        .expect("the parser rejects duplicate static import attribute keys");
    ModuleRequestIr::new(
        resolved_name(interner, request.specifier().sym()),
        ImportPhaseIr::from_ast(request.phase()),
        attributes,
    )
}

fn static_catalog_eligibility(
    interner: &Interner,
    request: &boa_ast::declaration::ModuleRequest,
) -> CatalogEligibility {
    request.attributes().iter().fold(
        CatalogEligibility::from_symbol(interner, request.specifier().sym()),
        |eligibility, attribute| {
            eligibility
                .and(CatalogEligibility::from_symbol(interner, attribute.key()))
                .and(CatalogEligibility::from_symbol(interner, attribute.value()))
        },
    )
}

fn record_request(
    interner: &Interner,
    request: &boa_ast::declaration::ModuleRequest,
    admission: super::admission::GraphAdmission,
) -> ModuleRequestIr {
    match admission {
        super::admission::GraphAdmission::LoadedClosure => module_request(interner, request),
        super::admission::GraphAdmission::CompleteCatalog => {
            match static_catalog_eligibility(interner, request) {
                CatalogEligibility::Eligible => module_request(interner, request),
                CatalogEligibility::Unrepresentable => ModuleRequestIr::new(
                    resolved_name(interner, request.specifier().sym()),
                    ImportPhaseIr::from_ast(request.phase()),
                    ModuleRequestAttributesIr::empty(),
                ),
            }
        }
    }
}

fn static_module_request(item: &ModuleItem) -> Option<&boa_ast::declaration::ModuleRequest> {
    match item {
        ModuleItem::ImportDeclaration(import) => Some(import.request()),
        ModuleItem::ExportDeclaration(export) => match export.as_ref() {
            ExportDeclaration::ReExport { request, .. } => Some(request.module_request()),
            _ => None,
        },
        ModuleItem::StatementListItem(_) => None,
    }
}

/// `ParseModule` (16.2.1.6.1): builds a module record from the front end's
/// retained parse product.
///
/// The `ParsedModule` type proves syntax parsing and Boa static semantics
/// already succeeded. This operation can therefore fail only for an early
/// error of `Module : ModuleBody` (16.2.3.1) found by validating the Scriptw-owned
/// entry tables it constructs.
///
/// Top-level `await` is *not* one of them. It sets `[[HasTLA]]` on the record;
/// [`ModuleGraphIr::async_evaluation`] propagates it across the graph and the
/// link stage gives the merged program an asynchronous body.
///
/// Everything else — unresolved specifiers, missing or ambiguous exports — is
/// the graph's job, not this function's, because none of it is decidable from a
/// single module.
pub fn parse_module_record(
    source: &ParsedModule,
    id: ModuleUnitId,
    key: ModuleKey,
) -> Result<ModuleRecordIr, Vec<IrDiagnostic>> {
    parse_record_with_admission(
        source,
        id,
        key,
        super::admission::GraphAdmission::LoadedClosure,
    )
}

pub(super) fn parse_module_record_for_catalog(
    source: &ParsedModule,
    id: ModuleUnitId,
    key: ModuleKey,
) -> Result<ModuleRecordIr, Vec<IrDiagnostic>> {
    parse_record_with_admission(
        source,
        id,
        key,
        super::admission::GraphAdmission::CompleteCatalog,
    )
}

fn parse_record_with_admission(
    source: &ParsedModule,
    id: ModuleUnitId,
    key: ModuleKey,
    admission: super::admission::GraphAdmission,
) -> Result<ModuleRecordIr, Vec<IrDiagnostic>> {
    let record = source.with_compiler_session(|module, interner| {
        build_module_record(module, interner, source.source(), id, key, admission)
    });

    // Early errors first: a `SyntaxError` outranks an unsupported feature, and
    // a module that is not well-formed has no meaningful `[[HasTLA]]`.
    let early_errors = module_early_errors(&record);
    if !early_errors.is_empty() {
        return Err(early_errors);
    }
    let mut record = record;
    let mut reserved = source.with_compiler_session(|module, interner| {
        super::dynamic::validate_linker_identifiers(module.into(), interner, &record.key)
    })?;
    // The scan already sees every BindingIdentifier; the bound and import
    // names are added from the record too, so the reservation never depends on
    // which AST positions the visitor happens to reach.
    reserved.extend(
        record
            .environment
            .iter()
            .map(|binding| binding.name.spec_name())
            .chain(
                record
                    .import_entries
                    .iter()
                    .map(|entry| entry.local_name.spec_name()),
            )
            .filter(|name| crate::UnitLinkerSpelling::could_collide(name))
            .map(str::to_owned),
    );
    record.linker_reserved_names = reserved;

    // `[[HasTLA]]` is a property of the record, never a failure of ParseModule:
    // 16.2.1.6.1 step 12 simply stores it. What it costs is decided later, by
    // the graph (which modules become `[[AsyncEvaluation]]`) and by the link
    // stage (which gives the merged program an asynchronous body).
    Ok(record)
}

fn build_module_record(
    module: &Module,
    interner: &Interner,
    source: &SourceUnit,
    id: ModuleUnitId,
    key: ModuleKey,
    admission: super::admission::GraphAdmission,
) -> ModuleRecordIr {
    let item_list = module.items();
    let items = item_list.items();
    let text = source.source_text.as_str();
    let lines = LineIndex::new(text);

    let mut record = ModuleRecordIr {
        data: ModuleRecordDataIr::SourceText(super::source::ModuleSyntax::from_parsed(
            text, item_list,
        )),
        id,
        key,
        source_len: source.source_text.len(),
        has_top_level_await: false,
        // Modules are strict by 16.2.1.6.1; the linker never reads this field
        // for them, so the value is documentation, not behavior.
        script_entry_strict: true,
        requested_modules: Vec::new(),
        module_resolution_requests: Vec::new(),
        import_entries: Vec::new(),
        local_export_entries: Vec::new(),
        indirect_export_entries: Vec::new(),
        star_export_entries: Vec::new(),
        environment: Vec::new(),
        import_meta_sites: Vec::new(),
        dynamic_import_sites: Vec::new(),
        dynamic_import_heads: Vec::new(),
        catalog_unrepresentable_requests: BTreeSet::new(),
        linker_reserved_names: BTreeSet::new(),
    };

    // 16.2.2.2 ModuleRequests, over *all* module items in source order. The
    // phaseful list deduplicates equal `(ModuleRequestKeyIr, ImportPhaseIr)`
    // occurrences. The parallel phase-free projection coalesces those phase
    // variants for host discovery and resolution only.
    //
    // This is its own pass because neither entry-table pass below can produce
    // source order: ImportEntries must run before ExportEntriesForModule, so
    // collecting requests as a side effect of those passes would put every
    // import ahead of every re-export regardless of how the module was written,
    // and `[[RequestedModules]]` order is what fixes dependency evaluation
    // order for otherwise independent dependencies.
    for item in items {
        let Some(source_request) = static_module_request(item) else {
            continue;
        };
        let request = record_request(interner, source_request, admission);
        if static_catalog_eligibility(interner, source_request)
            == CatalogEligibility::Unrepresentable
        {
            record
                .catalog_unrepresentable_requests
                .insert(request.key().clone());
        }
        push_unique_request_key(&mut record.module_resolution_requests, request.key());
        push_unique_request(&mut record.requested_modules, &request);
    }

    // 16.2.2.3 ImportEntries. Imports are collected before exports because
    // ExportEntriesForModule consults them for `export { imported }`.
    for item in items {
        let ModuleItem::ImportDeclaration(import) = item else {
            continue;
        };
        let request = record_request(interner, import.request(), admission);
        // The span of the binding the entry creates. `ImportDeclaration` itself
        // carries no span in boa 0.21.1, and the binding is the useful thing to
        // point a diagnostic at anyway.
        if let Some(default) = import.default() {
            record.import_entries.push(ImportEntryIr {
                request: request.clone(),
                import_name: ImportNameIr::Name(ExportName::default_export()),
                local_name: resolved_local_name(interner, default.sym()),
                span: Some(lines.source_span(text, default.span())),
            });
        }
        match import.kind() {
            ImportKind::DefaultOrUnnamed => {}
            ImportKind::Namespaced { binding } => {
                record.import_entries.push(ImportEntryIr {
                    request: request.clone(),
                    import_name: ImportNameIr::Namespace,
                    local_name: resolved_local_name(interner, binding.sym()),
                    span: Some(lines.source_span(text, binding.span())),
                });
            }
            ImportKind::Named { names } => {
                for name in names.iter() {
                    // `import { a as b }`: `export_name()` is the exporter's D2
                    // and `binding()` is this module's D1. Two domains, two
                    // conversions.
                    record.import_entries.push(ImportEntryIr {
                        request: request.clone(),
                        import_name: ImportNameIr::Name(resolved_export_name(
                            interner,
                            name.export_name(),
                        )),
                        local_name: resolved_local_name(interner, name.binding().sym()),
                        span: Some(lines.source_span(text, name.binding().span())),
                    });
                }
            }
        }
    }

    // 16.2.3.2 ExportEntries.
    let mut default_local: Option<LocalName> = None;
    for item in items {
        let ModuleItem::ExportDeclaration(export) = item else {
            continue;
        };
        match export.as_ref() {
            ExportDeclaration::ReExport { kind, request } => {
                let request = record_request(interner, request.module_request(), admission);
                match kind {
                    ReExportKind::Namespaced { name: Some(name) } => {
                        record.indirect_export_entries.push(IndirectExportEntryIr {
                            request,
                            import_name: ImportNameIr::Namespace,
                            export_name: resolved_export_name(interner, *name),
                        });
                    }
                    ReExportKind::Namespaced { name: None } => {
                        record
                            .star_export_entries
                            .push(StarExportEntryIr { request });
                    }
                    ReExportKind::Named { names } => {
                        for entry in names.iter() {
                            // Both sides are D2 here. boa calls the first
                            // accessor `private_name()`, but for a re-export it
                            // is the *requested* module's `[[ExportName]]` —
                            // the entry binds nothing locally at all. Contrast
                            // `ExportDeclaration::List` below, where the same
                            // accessor is a `[[LocalName]]`.
                            record.indirect_export_entries.push(IndirectExportEntryIr {
                                request: request.clone(),
                                import_name: ImportNameIr::Name(resolved_export_name(
                                    interner,
                                    entry.private_name(),
                                )),
                                export_name: resolved_export_name(interner, entry.alias()),
                            });
                        }
                    }
                }
            }
            ExportDeclaration::List(names) => {
                for entry in names.iter() {
                    // Here `private_name()` really is a `[[LocalName]]`: the
                    // clause names a binding of *this* module. The two
                    // conversions differ, which is what makes swapping them a
                    // type error at the struct literals below.
                    let local = resolved_local_name(interner, entry.private_name());
                    let export_name = resolved_export_name(interner, entry.alias());
                    // Imported bindings re-export indirectly. Namespace imports
                    // retain the request phase so repeated paths resolve to the
                    // same eager or deferred namespace identity.
                    let imported = record
                        .import_entries
                        .iter()
                        .find(|import| import.local_name == local)
                        .map(|import| (import.request.clone(), import.import_name.clone()));
                    match imported {
                        Some((request, import_name)) => {
                            record.indirect_export_entries.push(IndirectExportEntryIr {
                                request,
                                import_name,
                                export_name,
                            });
                        }
                        None => {
                            record.local_export_entries.push(LocalExportEntryIr {
                                local_name: local,
                                export_name,
                            });
                        }
                    }
                }
            }
            ExportDeclaration::VarStatement(var) => {
                for name in bound_names(var) {
                    push_local_export(&mut record, &resolved_name(interner, name));
                }
            }
            ExportDeclaration::Declaration(declaration) => {
                for name in bound_names(declaration) {
                    push_local_export(&mut record, &resolved_name(interner, name));
                }
            }
            // The five default *declaration* forms all route through
            // `push_default_export`, which is the one place boa's `default`
            // marker is turned into `LocalName::AnonymousDefault`.
            ExportDeclaration::DefaultFunctionDeclaration(function) => {
                push_default_export(
                    &resolved_name(interner, function.name().sym()),
                    &mut record,
                    &mut default_local,
                );
            }
            ExportDeclaration::DefaultGeneratorDeclaration(generator) => {
                push_default_export(
                    &resolved_name(interner, generator.name().sym()),
                    &mut record,
                    &mut default_local,
                );
            }
            ExportDeclaration::DefaultAsyncFunctionDeclaration(function) => {
                push_default_export(
                    &resolved_name(interner, function.name().sym()),
                    &mut record,
                    &mut default_local,
                );
            }
            ExportDeclaration::DefaultAsyncGeneratorDeclaration(generator) => {
                push_default_export(
                    &resolved_name(interner, generator.name().sym()),
                    &mut record,
                    &mut default_local,
                );
            }
            ExportDeclaration::DefaultClassDeclaration(class) => {
                push_default_export(
                    &resolved_name(interner, class.name().sym()),
                    &mut record,
                    &mut default_local,
                );
            }
            ExportDeclaration::DefaultAssignmentExpression(_) => {
                record.local_export_entries.push(LocalExportEntryIr {
                    local_name: LocalName::AnonymousDefault,
                    export_name: ExportName::default_export(),
                });
                default_local = Some(LocalName::AnonymousDefault);
            }
        }
    }

    record.environment = module_environment(item_list, interner, &record, default_local.as_ref());

    let mut scan = ModuleBodyScan {
        interner: Some(interner),
        admission,
        text,
        lines: Some(&lines),
        ..ModuleBodyScan::default()
    };
    item_list.visit_with(&mut scan);
    record.has_top_level_await = scan.top_level_await;
    record.dynamic_import_heads = scan.source_heads();
    record.import_meta_sites = scan.import_meta_sites;
    // The visitor walks an expression's operands before its neighbours rather
    // than strictly left to right, so source order is restored here instead of
    // assumed. `rewrite_import_meta` needs a non-overlapping ascending list.
    record.import_meta_sites.sort_by_key(|span| span.start);
    record.dynamic_import_sites = scan.dynamic_import_sites;

    record
}

/// `export let x` / `export var x` / `export function x`: the coincident case,
/// where the `[[LocalName]]` and the `[[ExportName]]` happen to be the same
/// text.
///
/// The one `&str` is therefore converted **twice**, through the two different
/// constructors. That is not redundancy — it is the whole point. A single
/// shared conversion no longer type-checks, so the case that used to hide a
/// `local_name`/`export_name` swap now cannot.
fn push_local_export(record: &mut ModuleRecordIr, name: &str) {
    record.local_export_entries.push(LocalExportEntryIr {
        local_name: LocalName::from_bound_name(name),
        export_name: ExportName::new(name),
    });
}

/// Records the `*default*` export of a default *declaration*.
///
/// boa names an anonymous default declaration `default`, which is a reserved
/// word and therefore unambiguous: any other name is a real
/// `BindingIdentifier`. This is the one place that marker is read, and the one
/// place a `[[LocalName]]` is decided from anything other than BoundNames.
fn push_default_export(
    declared_name: &str,
    record: &mut ModuleRecordIr,
    default_local: &mut Option<LocalName>,
) {
    let local_name = if declared_name == ExportName::DEFAULT {
        LocalName::AnonymousDefault
    } else {
        LocalName::from_bound_name(declared_name)
    };
    record.local_export_entries.push(LocalExportEntryIr {
        local_name: local_name.clone(),
        export_name: ExportName::default_export(),
    });
    *default_local = Some(local_name);
}

/// 16.2.1.6.4 `InitializeEnvironment`, as a static binding table.
fn module_environment(
    item_list: &boa_ast::ModuleItemList,
    interner: &Interner,
    record: &ModuleRecordIr,
    default_local: Option<&LocalName>,
) -> Vec<ModuleEnvBindingIr> {
    let items = item_list.items();
    let mut environment = Vec::new();
    let mut declared = BTreeSet::new();

    // Import bindings: immutable, indirect, initialized before evaluation.
    for entry in &record.import_entries {
        if declared.insert(entry.local_name.clone()) {
            environment.push(ModuleEnvBindingIr {
                name: entry.local_name.clone(),
                kind: ModuleBindingKindIr::Import,
                mutable: false,
                initialized_before_evaluation: true,
                in_tdz_until_evaluated: false,
                indirect: Some((entry.request.clone(), entry.import_name.clone())),
            });
        }
    }

    for item in items {
        match item {
            ModuleItem::ImportDeclaration(_) => {}
            ModuleItem::StatementListItem(StatementListItem::Declaration(declaration)) => {
                push_declaration_bindings(
                    declaration.as_ref(),
                    interner,
                    &mut environment,
                    &mut declared,
                );
            }
            ModuleItem::StatementListItem(StatementListItem::Statement(statement)) => {
                if let Statement::Var(var) = statement.as_ref() {
                    push_var_bindings(var, interner, &mut environment, &mut declared);
                }
            }
            ModuleItem::ExportDeclaration(export) => match export.as_ref() {
                ExportDeclaration::VarStatement(var) => {
                    push_var_bindings(var, interner, &mut environment, &mut declared);
                }
                ExportDeclaration::Declaration(declaration) => {
                    push_declaration_bindings(
                        declaration,
                        interner,
                        &mut environment,
                        &mut declared,
                    );
                }
                ExportDeclaration::DefaultFunctionDeclaration(_)
                | ExportDeclaration::DefaultGeneratorDeclaration(_)
                | ExportDeclaration::DefaultAsyncFunctionDeclaration(_)
                | ExportDeclaration::DefaultAsyncGeneratorDeclaration(_) => {
                    push_default_binding(
                        default_local,
                        ModuleBindingKindIr::Function,
                        &mut environment,
                        &mut declared,
                    );
                }
                ExportDeclaration::DefaultClassDeclaration(_) => {
                    push_default_binding(
                        default_local,
                        ModuleBindingKindIr::Class,
                        &mut environment,
                        &mut declared,
                    );
                }
                ExportDeclaration::DefaultAssignmentExpression(_) => {
                    push_default_binding(
                        default_local,
                        ModuleBindingKindIr::Let,
                        &mut environment,
                        &mut declared,
                    );
                }
                ExportDeclaration::List(_) | ExportDeclaration::ReExport { .. } => {}
            },
        }
    }

    // VarDeclaredNames also reaches `var`s nested in blocks and loops of the
    // module body; those are module-environment bindings too.
    let mut nested = BTreeSet::new();
    for name in var_declared_names(item_list) {
        nested.insert(resolved_local_name(interner, name));
    }
    for name in nested {
        if declared.insert(name.clone()) {
            environment.push(ModuleEnvBindingIr {
                name,
                kind: ModuleBindingKindIr::Var,
                mutable: true,
                initialized_before_evaluation: true,
                in_tdz_until_evaluated: false,
                indirect: None,
            });
        }
    }

    environment
}

fn push_declaration_bindings(
    declaration: &Declaration,
    interner: &Interner,
    environment: &mut Vec<ModuleEnvBindingIr>,
    declared: &mut BTreeSet<LocalName>,
) {
    let kind = match declaration {
        Declaration::FunctionDeclaration(_)
        | Declaration::GeneratorDeclaration(_)
        | Declaration::AsyncFunctionDeclaration(_)
        | Declaration::AsyncGeneratorDeclaration(_) => ModuleBindingKindIr::Function,
        Declaration::ClassDeclaration(_) => ModuleBindingKindIr::Class,
        Declaration::Lexical(LexicalDeclaration::Const(_)) => ModuleBindingKindIr::Const,
        Declaration::Lexical(_) => ModuleBindingKindIr::Let,
    };
    for name in bound_names(declaration) {
        let name = resolved_local_name(interner, name);
        if declared.insert(name.clone()) {
            environment.push(ModuleEnvBindingIr {
                name,
                kind,
                mutable: kind != ModuleBindingKindIr::Const,
                initialized_before_evaluation: kind == ModuleBindingKindIr::Function,
                in_tdz_until_evaluated: kind != ModuleBindingKindIr::Function,
                indirect: None,
            });
        }
    }
}

fn push_var_bindings(
    var: &VarDeclaration,
    interner: &Interner,
    environment: &mut Vec<ModuleEnvBindingIr>,
    declared: &mut BTreeSet<LocalName>,
) {
    for name in bound_names(var) {
        let name = resolved_local_name(interner, name);
        if declared.insert(name.clone()) {
            environment.push(ModuleEnvBindingIr {
                name,
                kind: ModuleBindingKindIr::Var,
                mutable: true,
                initialized_before_evaluation: true,
                in_tdz_until_evaluated: false,
                indirect: None,
            });
        }
    }
}

fn push_default_binding(
    default_local: Option<&LocalName>,
    kind: ModuleBindingKindIr,
    environment: &mut Vec<ModuleEnvBindingIr>,
    declared: &mut BTreeSet<LocalName>,
) {
    let Some(name) = default_local else {
        return;
    };
    if declared.insert(name.clone()) {
        environment.push(ModuleEnvBindingIr {
            name: name.clone(),
            kind,
            mutable: true,
            initialized_before_evaluation: kind == ModuleBindingKindIr::Function,
            in_tdz_until_evaluated: kind != ModuleBindingKindIr::Function,
            indirect: None,
        });
    }
}

/// Collects the module-body facts that need a full expression walk.
#[derive(Debug, Default)]
struct ModuleBodyScan<'a> {
    interner: Option<&'a Interner>,
    admission: super::admission::GraphAdmission,
    /// The module source text, needed to turn boa positions into byte offsets.
    text: &'a str,
    lines: Option<&'a LineIndex>,
    top_level_await: bool,
    import_meta_sites: Vec<SourceSpan>,
    dynamic_import_sites: Vec<DynamicImportSiteIr>,
    dynamic_import_heads: Vec<(boa_ast::LinearSpan, ImportPhaseIr)>,
    function_depth: usize,
}

/// The exact prefix of an original parsed dynamic import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DynamicImportHead {
    pub(super) span: SourceSpan,
    pub(super) phase: ImportPhaseIr,
}

impl ModuleBodyScan<'_> {
    fn source_heads(&mut self) -> Vec<DynamicImportHead> {
        self.dynamic_import_heads
            .sort_by_key(|(span, _)| span.start().pos());
        let mut chars = self.text.char_indices();
        let mut utf16 = 0;
        let mut byte = 0;
        let mut byte_at = |position: boa_ast::LinearPosition| {
            while utf16 < position.pos() {
                let (offset, ch) = chars.next().expect("parsed import head is inside source");
                utf16 += ch.len_utf16();
                byte = offset + ch.len_utf8();
            }
            assert_eq!(utf16, position.pos(), "ordered parser boundaries");
            byte
        };
        self.dynamic_import_heads
            .iter()
            .map(|(span, phase)| DynamicImportHead {
                span: SourceSpan {
                    start: byte_at(span.start()),
                    end: byte_at(span.end()),
                },
                phase: *phase,
            })
            .collect()
    }
}

/// Statically recovers the request attributes of the literal subset whose
/// final own properties are fixed by syntax alone.
///
/// This is only a graph-discovery optimization. The generated dispatcher
/// always performs the real `Get(options, "with")`, enumerable-own-property
/// walk, and value reads at runtime, so getters, Proxies, computed names and
/// spreads retain their observable behaviour.
fn dynamic_import_attributes(
    node: &ImportCall,
    interner: Option<&Interner>,
) -> DynamicImportAttributesIr {
    let Some(options) = node.options() else {
        return DynamicImportAttributesIr::Known(ModuleRequestAttributesIr::empty());
    };
    let (Expression::ObjectLiteral(options), Some(interner)) = (options, interner) else {
        return DynamicImportAttributesIr::Runtime;
    };

    let mut with_value = None;
    for property in options.properties() {
        let PropertyDefinition::Property(PropertyName::Literal(name), value) = property else {
            return DynamicImportAttributesIr::Runtime;
        };
        let key = resolved_name(interner, name.sym());
        if key == "__proto__" {
            // This changes the options object's prototype. An inherited
            // `with` property is observable through Get(options, "with"), so
            // even a fully literal prototype belongs to the runtime case.
            return DynamicImportAttributesIr::Runtime;
        }
        if key == "with" {
            // Object-literal data properties overwrite an earlier property of
            // the same name, so the last one is what `Get(options, "with")`
            // observes.
            with_value = Some(value);
        }
    }

    let Some(with_value) = with_value else {
        return DynamicImportAttributesIr::Known(ModuleRequestAttributesIr::empty());
    };
    let Expression::ObjectLiteral(with_object) = with_value else {
        return DynamicImportAttributesIr::Runtime;
    };

    let mut attributes: Vec<ImportAttributeIr> = Vec::new();
    for property in with_object.properties() {
        let PropertyDefinition::Property(PropertyName::Literal(name), value) = property else {
            return DynamicImportAttributesIr::Runtime;
        };
        let key = resolved_name(interner, name.sym());
        if key == "__proto__" {
            // `__proto__: value` is the object-literal prototype setter, not
            // an own data property, so EnumerableOwnProperties never reports
            // it. A computed `["__proto__"]` is an ordinary property, but the
            // conservative computed-name arm above classifies that as Runtime.
            continue;
        }
        let Expression::Literal(value) = value else {
            return DynamicImportAttributesIr::Runtime;
        };
        let LiteralKind::String(value) = value.kind() else {
            return DynamicImportAttributesIr::Runtime;
        };
        let value = resolved_name(interner, *value);
        if let Some(attribute) = attributes.iter_mut().find(|attribute| attribute.key == key) {
            // As above, a later object-literal data property overwrites the
            // earlier value before EnumerableOwnProperties observes it.
            attribute.value = value;
        } else {
            attributes.push(ImportAttributeIr { key, value });
        }
    }
    let attributes = ModuleRequestAttributesIr::try_new(attributes)
        .expect("object-literal property replacement leaves unique attribute keys");
    DynamicImportAttributesIr::Known(attributes)
}

/// For Known attributes, retain only the final literal data property per
/// actual UTF-16 key. Runtime options remain possible declared variants.
fn dynamic_catalog_eligibility(
    node: &ImportCall,
    interner: Option<&Interner>,
) -> CatalogEligibility {
    let Some(interner) = interner else {
        return CatalogEligibility::Unrepresentable;
    };
    let specifier = match node.argument() {
        Expression::Literal(literal) => match literal.kind() {
            LiteralKind::String(symbol) => CatalogEligibility::from_symbol(interner, *symbol),
            _ => CatalogEligibility::Eligible,
        },
        _ => CatalogEligibility::Eligible,
    };
    let Some(Expression::ObjectLiteral(options)) = node.options() else {
        return specifier;
    };
    let mut with_value = None;
    for property in options.properties() {
        let PropertyDefinition::Property(PropertyName::Literal(name), value) = property else {
            return specifier;
        };
        let key = interner.resolve_expect(name.sym());
        if key.utf16().iter().copied().eq("__proto__".encode_utf16()) {
            return specifier;
        }
        if key.utf16().iter().copied().eq("with".encode_utf16()) {
            with_value = Some(value);
        }
    }
    let Some(Expression::ObjectLiteral(attributes)) = with_value else {
        return specifier;
    };
    let mut final_properties: Vec<(Vec<u16>, CatalogEligibility)> = Vec::new();
    for property in attributes.properties() {
        let PropertyDefinition::Property(PropertyName::Literal(name), value) = property else {
            return specifier;
        };
        let key = interner.resolve_expect(name.sym());
        if key.utf16().iter().copied().eq("__proto__".encode_utf16()) {
            continue;
        }
        let Expression::Literal(value) = value else {
            return specifier;
        };
        let LiteralKind::String(value) = value.kind() else {
            return specifier;
        };
        let eligibility = CatalogEligibility::from_symbol(interner, name.sym())
            .and(CatalogEligibility::from_symbol(interner, *value));
        if let Some((_, previous)) = final_properties
            .iter_mut()
            .find(|(previous, _)| previous.as_slice() == key.utf16())
        {
            *previous = eligibility;
        } else {
            final_properties.push((key.utf16().to_vec(), eligibility));
        }
    }
    final_properties
        .into_iter()
        .fold(specifier, |eligibility, (_, property)| {
            eligibility.and(property)
        })
}

impl<'ast> Visitor<'ast> for ModuleBodyScan<'_> {
    type BreakTy = core::convert::Infallible;

    fn visit_import_meta(&mut self, node: &'ast ImportMeta) -> ControlFlow<Self::BreakTy> {
        // boa spans the whole `import.meta` meta-property, from the `import`
        // keyword to the end of `meta`
        // (`vendor/boa_parser-0.21.1/src/parser/expression/left_hand_side/member.rs:112`),
        // so the span is exactly the text the linker has to replace — including
        // any whitespace or comment written between the two tokens.
        if let Some(lines) = self.lines {
            self.import_meta_sites
                .push(lines.source_span(self.text, node.span()));
        }
        ControlFlow::Continue(())
    }

    fn visit_import_call(&mut self, node: &'ast ImportCall) -> ControlFlow<Self::BreakTy> {
        let static_specifier = literal_import_specifier(node)
            .zip(self.interner)
            .map(|(symbol, interner)| resolved_name(interner, symbol));
        let catalog_eligibility = dynamic_catalog_eligibility(node, self.interner);
        let attributes = match (self.admission, catalog_eligibility) {
            (
                super::admission::GraphAdmission::CompleteCatalog,
                CatalogEligibility::Unrepresentable,
            ) => DynamicImportAttributesIr::Known(ModuleRequestAttributesIr::empty()),
            (super::admission::GraphAdmission::LoadedClosure, CatalogEligibility::Eligible)
            | (
                super::admission::GraphAdmission::LoadedClosure,
                CatalogEligibility::Unrepresentable,
            )
            | (super::admission::GraphAdmission::CompleteCatalog, CatalogEligibility::Eligible) => {
                dynamic_import_attributes(node, self.interner)
            }
        };
        if let Some(span) = node.head_span() {
            self.dynamic_import_heads
                .push((span, ImportPhaseIr::from_ast(node.phase())));
        }
        self.dynamic_import_sites.push(DynamicImportSiteIr {
            static_specifier,
            phase: ImportPhaseIr::from_ast(node.phase()),
            attributes,
            catalog_eligibility,
            origin: DynamicImportSiteOrigin::RetainedSource,
        });
        node.visit_with(self)
    }

    fn visit_await(
        &mut self,
        node: &'ast boa_ast::expression::Await,
    ) -> ControlFlow<Self::BreakTy> {
        if self.function_depth == 0 {
            self.top_level_await = true;
        }
        node.visit_with(self)
    }

    /// Resource disposal is an implicit Await even when the initializer is
    /// nullish or the declaration is never reached. Classic-for resource heads
    /// also visit this lexical declaration; nested function bodies retain their
    /// own depth and cannot make the enclosing module asynchronous.
    fn visit_lexical_declaration(
        &mut self,
        node: &'ast LexicalDeclaration,
    ) -> ControlFlow<Self::BreakTy> {
        if self.function_depth == 0 && node.is_await_using() {
            self.top_level_await = true;
        }
        node.visit_with(self)
    }

    /// Iterator Await and asynchronous resource heads both contribute to HasTLA.
    ///
    /// Neither is an Await expression or a lexical declaration in the AST:
    /// Boa stores iteration Await on the loop and resource disposal on its head.
    fn visit_for_of_loop(&mut self, node: &'ast ForOfLoop) -> ControlFlow<Self::BreakTy> {
        if self.function_depth == 0
            && (node.r#await()
                || matches!(node.initializer(), IterableLoopInitializer::AwaitUsing(_)))
        {
            self.top_level_await = true;
        }
        node.visit_with(self)
    }

    fn visit_function_body(&mut self, node: &'ast FunctionBody) -> ControlFlow<Self::BreakTy> {
        self.function_depth += 1;
        let result = node.visit_with(self);
        self.function_depth -= 1;
        result
    }
}

fn literal_import_specifier(node: &ImportCall) -> Option<Sym> {
    if let Expression::Literal(literal) = node.argument() {
        if let LiteralKind::String(symbol) = literal.kind() {
            return Some(*symbol);
        }
    }
    None
}

/// Projects `[[RequestedModules]]` to phase-free host-resolution keys and adds
/// string-literal `import(...)` keys, without lowering anything.
///
/// This is what the host loader driver calls to discover a module's
/// dependencies before the graph is assembled.
pub fn scan_module_requests(source: &ParsedModule) -> Vec<ModuleRequestKeyIr> {
    let record = source.with_compiler_session(|module, interner| {
        build_module_record(
            module,
            interner,
            source.source(),
            0,
            ModuleKey::from_host(ANONYMOUS_MODULE_KEY),
            super::admission::GraphAdmission::LoadedClosure,
        )
    });
    requests_with_dynamic_imports(
        record.module_resolution_requests,
        &record.dynamic_import_sites,
    )
}

/// Actual host-loading occurrences retain phase until the loader decides
/// whether to open a loaded target's dependencies. Resolution remains keyed by
/// the phase-free key; a later non-source occurrence promotes the same row.
pub(super) fn scan_module_loading_requests(source: &ParsedModule) -> Vec<ModuleRequestIr> {
    source.with_compiler_session(|module, interner| scan_graph_requests(module, interner, None).0)
}

pub(super) fn loading_requests_with_dynamic_imports(
    requests: Vec<ModuleRequestIr>,
    sites: &[DynamicImportSiteIr],
) -> Vec<ModuleRequestIr> {
    coalesce_loading_requests(
        requests.into_iter().chain(
            sites
                .iter()
                .filter_map(DynamicImportSiteIr::discovery_request),
        ),
    )
}

fn coalesce_loading_requests(
    requests: impl IntoIterator<Item = ModuleRequestIr>,
) -> Vec<ModuleRequestIr> {
    let mut loading: Vec<ModuleRequestIr> = Vec::new();
    for request in requests {
        if let Some(previous) = loading
            .iter_mut()
            .find(|previous| previous.key() == request.key())
        {
            if previous.phase() == ImportPhaseIr::Source && request.phase().loads_dependencies() {
                *previous = request;
            }
        } else {
            loading.push(request);
        }
    }
    loading
}

/// Host discovery needs the request keys and phases, without constructing the
/// lowering record or decoding source ranges. Static rows precede dynamic
/// rows, and the Realm rows retain their separate origin and sorted order.
pub(super) fn scan_graph_requests<'ast>(
    ast: &'ast impl VisitWith,
    interner: &'ast Interner,
    realm: Option<super::realm_request::RealmRequestScan<'ast>>,
) -> (Vec<ModuleRequestIr>, Vec<ModuleRequestKeyIr>) {
    struct Scan<'a> {
        interner: &'a Interner,
        static_requests: Vec<ModuleRequestIr>,
        dynamic_requests: Vec<ModuleRequestIr>,
        realm: Option<super::realm_request::RealmRequestScan<'a>>,
    }
    impl<'ast> Visitor<'ast> for Scan<'_> {
        type BreakTy = core::convert::Infallible;

        fn visit_module_item(&mut self, item: &'ast ModuleItem) -> ControlFlow<Self::BreakTy> {
            if let Some(request) = static_module_request(item) {
                self.static_requests
                    .push(module_request(self.interner, request));
            }
            item.visit_with(self)
        }

        fn visit_import_call(&mut self, node: &'ast ImportCall) -> ControlFlow<Self::BreakTy> {
            if let Some(specifier) = literal_import_specifier(node) {
                let attributes = match dynamic_import_attributes(node, Some(self.interner)) {
                    DynamicImportAttributesIr::Known(attributes) => attributes,
                    DynamicImportAttributesIr::Runtime => ModuleRequestAttributesIr::empty(),
                };
                self.dynamic_requests.push(ModuleRequestIr::new(
                    resolved_name(self.interner, specifier),
                    ImportPhaseIr::from_ast(node.phase()),
                    attributes,
                ));
            }
            node.visit_with(self)
        }

        fn visit_expression(&mut self, expression: &'ast Expression) -> ControlFlow<Self::BreakTy> {
            if let Some(realm) = &mut self.realm {
                realm.observe(expression);
            }
            expression.visit_with(self)
        }
    }
    let mut scan = Scan {
        interner,
        static_requests: Vec::new(),
        dynamic_requests: Vec::new(),
        realm,
    };
    let _ = ast.visit_with(&mut scan);
    (
        coalesce_loading_requests(
            scan.static_requests
                .into_iter()
                .chain(scan.dynamic_requests),
        ),
        scan.realm.map_or_else(
            Vec::new,
            super::realm_request::RealmRequestScan::into_requests,
        ),
    )
}

/// Reads the statically resolvable `import(...)` requests in a Script without
/// changing its parse goal.
///
/// A Script has no `[[RequestedModules]]` table, so only dynamic imports can
/// contribute. This exists specifically so graph discovery never reparses a
/// Script as Module code (which would both violate parse-once and reject legal
/// sloppy Script forms).
pub fn scan_script_module_requests(source: &ParsedScript) -> Vec<ModuleRequestKeyIr> {
    let sites = script_dynamic_import_sites(source);
    requests_with_dynamic_imports(Vec::new(), &sites)
}

pub(crate) fn script_entry_record(
    source: &ParsedScript,
    id: ModuleUnitId,
    key: ModuleKey,
) -> Result<ModuleRecordIr, Vec<IrDiagnostic>> {
    script_record_with_admission(
        source,
        id,
        key,
        super::admission::GraphAdmission::LoadedClosure,
    )
}

pub(super) fn script_entry_record_for_catalog(
    source: &ParsedScript,
    id: ModuleUnitId,
    key: ModuleKey,
) -> Result<ModuleRecordIr, Vec<IrDiagnostic>> {
    script_record_with_admission(
        source,
        id,
        key,
        super::admission::GraphAdmission::CompleteCatalog,
    )
}

fn script_record_with_admission(
    source: &ParsedScript,
    id: ModuleUnitId,
    key: ModuleKey,
    admission: super::admission::GraphAdmission,
) -> Result<ModuleRecordIr, Vec<IrDiagnostic>> {
    let (script_entry_strict, linker_reserved_names) =
        source.with_compiler_session(|script, interner| -> Result<_, Vec<IrDiagnostic>> {
            let reserved =
                super::dynamic::validate_linker_identifiers(script.into(), interner, &key)?;
            Ok((script.strict(), reserved))
        })?;
    let (dynamic_import_sites, dynamic_import_heads) =
        script_sites_with_admission(source, admission);
    Ok(ModuleRecordIr {
        data: ModuleRecordDataIr::Script,
        id,
        key,
        source_len: source.source_text.len(),
        has_top_level_await: false,
        script_entry_strict,
        requested_modules: Vec::new(),
        module_resolution_requests: Vec::new(),
        import_entries: Vec::new(),
        local_export_entries: Vec::new(),
        indirect_export_entries: Vec::new(),
        star_export_entries: Vec::new(),
        environment: Vec::new(),
        import_meta_sites: Vec::new(),
        dynamic_import_sites,
        dynamic_import_heads,
        catalog_unrepresentable_requests: BTreeSet::new(),
        linker_reserved_names,
    })
}

pub(super) fn module_dynamic_import_sites(source: &ParsedModule) -> Vec<DynamicImportSiteIr> {
    source.with_compiler_session(|module, interner| {
        let mut scan = ModuleBodyScan {
            interner: Some(interner),
            text: &source.source_text,
            ..ModuleBodyScan::default()
        };
        let _ = module.visit_with(&mut scan);
        scan.dynamic_import_sites
    })
}

pub(crate) fn script_dynamic_import_sites(source: &ParsedScript) -> Vec<DynamicImportSiteIr> {
    script_sites_with_admission(source, super::admission::GraphAdmission::LoadedClosure).0
}

/// Whether a retained Script AST contains a dynamic import, including a computed one.
pub fn script_has_dynamic_import(source: &ParsedScript) -> bool {
    source.has_dynamic_import()
}

fn script_sites_with_admission(
    source: &ParsedScript,
    admission: super::admission::GraphAdmission,
) -> (Vec<DynamicImportSiteIr>, Vec<DynamicImportHead>) {
    source.with_compiler_session(|script, interner| {
        let mut scan = ModuleBodyScan {
            interner: Some(interner),
            admission,
            text: &source.source_text,
            ..ModuleBodyScan::default()
        };
        let _ = script.visit_with(&mut scan);
        let heads = scan.source_heads();
        (scan.dynamic_import_sites, heads)
    })
}

fn requests_with_dynamic_imports(
    mut requests: Vec<ModuleRequestKeyIr>,
    sites: &[DynamicImportSiteIr],
) -> Vec<ModuleRequestKeyIr> {
    for site in sites {
        let Some(request) = site.discovery_request() else {
            continue;
        };
        push_unique_request_key(&mut requests, request.key());
    }
    requests
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_unit(source: &str) -> ParsedModule {
        let parsed = scriptw_front::parse(source, scriptw_front::ParseOptions::module())
            .expect("module fixture should parse");
        let ParsedSource::Module(module) = parsed else {
            unreachable!("module options produce a module")
        };
        module
    }

    fn record_of(source: &str) -> ModuleRecordIr {
        parse_module_record(&source_unit(source), 0, ModuleKey::from_host("main.mjs"))
            .expect("module record should build")
    }

    /// The entry tables are compared without spans everywhere except the one
    /// test that is *about* spans, so a change in boa's position reporting
    /// cannot fail nine unrelated assertions.
    fn without_spans(entries: &[ImportEntryIr]) -> Vec<ImportEntryIr> {
        entries
            .iter()
            .map(|entry| ImportEntryIr {
                span: None,
                ..entry.clone()
            })
            .collect()
    }

    /// A `[[LocalName]]`, spelled the way the module's own text spells it.
    fn local(name: &str) -> LocalName {
        LocalName::from_bound_name(name)
    }

    fn import(specifier: &str, import_name: ImportNameIr, local_name: &str) -> ImportEntryIr {
        ImportEntryIr {
            request: ModuleRequestIr::plain(specifier),
            import_name,
            local_name: local(local_name),
            span: None,
        }
    }

    fn named(name: &str) -> ImportNameIr {
        ImportNameIr::Name(ExportName::new(name))
    }

    /// The compile-time witness for the coincident case.
    ///
    /// It used to take two `&str`s, which made the *field initialisers* inside
    /// it unswappable while leaving every **call site** free to swap:
    /// `local_export("x", "y")` and `local_export("y", "x")` both compiled. The
    /// parameters are typed now, so the swap is `E0308` where it would actually
    /// be written.
    fn local_export(local_name: LocalName, export_name: ExportName) -> LocalExportEntryIr {
        LocalExportEntryIr {
            local_name,
            export_name,
        }
    }

    fn indirect_export(
        specifier: &str,
        import_name: ImportNameIr,
        export_name: &str,
    ) -> IndirectExportEntryIr {
        IndirectExportEntryIr {
            request: ModuleRequestIr::plain(specifier),
            import_name,
            export_name: ExportName::new(export_name),
        }
    }

    fn attribute(key: &str, value: &str) -> ImportAttributeIr {
        ImportAttributeIr {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    fn attributes(attributes: Vec<ImportAttributeIr>) -> ModuleRequestAttributesIr {
        ModuleRequestAttributesIr::try_new(attributes).expect("test attributes are unique")
    }

    fn request_key(specifier: &str) -> ModuleRequestKeyIr {
        ModuleRequestKeyIr::plain(specifier)
    }

    fn attributed_key(
        specifier: &str,
        request_attributes: Vec<ImportAttributeIr>,
    ) -> ModuleRequestKeyIr {
        ModuleRequestKeyIr::try_new(specifier, request_attributes)
            .expect("test attributes are unique")
    }

    fn attributed_request(
        specifier: &str,
        phase: ImportPhaseIr,
        request_attributes: Vec<ImportAttributeIr>,
    ) -> ModuleRequestIr {
        ModuleRequestIr::try_new(specifier, phase, request_attributes)
            .expect("test attributes are unique")
    }

    fn known_attributes(attributes: Vec<ImportAttributeIr>) -> DynamicImportAttributesIr {
        DynamicImportAttributesIr::Known(self::attributes(attributes))
    }

    // -- 16.2.2.3 ImportEntries -------------------------------------------

    #[test]
    fn import_entries_cover_default_named_and_namespace_clauses() {
        let record = record_of(
            "import alpha, { beta, gamma as delta } from \"./m.mjs\";\n\
             import * as epsilon from \"./n.mjs\";\n\
             import \"./side.mjs\";\n",
        );

        assert_eq!(
            without_spans(&record.import_entries),
            vec![
                // ImportEntriesForModule emits the default binding before the
                // named ones of the same clause.
                import("./m.mjs", named(MODULE_DEFAULT_EXPORT_NAME), "alpha"),
                import("./m.mjs", named("beta"), "beta"),
                import("./m.mjs", named("gamma"), "delta"),
                import("./n.mjs", ImportNameIr::Namespace, "epsilon"),
            ]
        );
        // `import "./side.mjs"` binds nothing but is still a requested module.
        assert_eq!(
            record.requested_modules,
            vec![
                ModuleRequestIr::plain("./m.mjs"),
                ModuleRequestIr::plain("./n.mjs"),
                ModuleRequestIr::plain("./side.mjs"),
            ]
        );
    }

    #[test]
    fn import_entry_spans_point_at_the_local_binding() {
        let source = "import alpha from \"./m.mjs\";\nimport { beta as gamma } from \"./n.mjs\";\n";
        let record = record_of(source);

        let expected: Vec<Option<SourceSpan>> = ["alpha", "gamma"]
            .iter()
            .map(|name| {
                let start = source.find(name).expect("binding appears in the source");
                Some(SourceSpan {
                    start,
                    end: start + name.len(),
                })
            })
            .collect();
        let actual: Vec<Option<SourceSpan>> = record
            .import_entries
            .iter()
            .map(|entry| entry.span)
            .collect();
        assert_eq!(actual, expected);
    }

    // -- 16.2.2.2 ModuleRequests ------------------------------------------

    #[test]
    fn requested_modules_dedup_by_key_within_one_phase() {
        let record = record_of(
            "import { alpha } from \"./m.mjs\" with { type: \"json\" };\n\
             import { beta } from \"./m.mjs\" with { type: \"json\" };\n\
             import { gamma } from \"./m.mjs\";\n",
        );

        // ModuleRequestsEqual compares specifier and attributes. Within this
        // shared evaluation phase the two attributed requests collapse and
        // the bare key does not.
        assert_eq!(
            record.requested_modules,
            vec![
                attributed_request(
                    "./m.mjs",
                    ImportPhaseIr::Evaluation,
                    vec![attribute("type", "json")],
                ),
                ModuleRequestIr::plain("./m.mjs"),
            ]
        );
    }

    #[test]
    fn request_attributes_are_sorted_so_keys_implement_module_requests_equal() {
        let first = record_of("import \"./m.mjs\" with { type: \"json\", charset: \"utf8\" };\n");
        let second = record_of("import \"./m.mjs\" with { charset: \"utf8\", type: \"json\" };\n");

        assert_eq!(
            first.requested_modules[0].attributes(),
            attributes(vec![
                attribute("charset", "utf8"),
                attribute("type", "json")
            ])
            .as_slice()
        );
        assert_eq!(
            first.requested_modules[0].key(),
            second.requested_modules[0].key()
        );

        // U+10000 starts with UTF-16 code unit D800, which sorts before E000
        // even though Rust's scalar-value String ordering puts it after.
        let utf16 = record_of(
            "import \"./m.mjs\" with { \"\u{10000}\": \"astral\", \"\u{e000}\": \"bmp\" };\n",
        );
        assert_eq!(
            utf16.requested_modules[0].attributes(),
            attributes(vec![
                attribute("\u{10000}", "astral"),
                attribute("\u{e000}", "bmp")
            ])
            .as_slice()
        );
    }

    #[test]
    fn request_key_constructor_canonicalizes_reverse_utf16_attribute_order() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Rust String ordering puts E000 before U+10000, while ECMAScript
        // UTF-16 ordering sees D800 first and therefore puts U+10000 first.
        let scalar_order = vec![
            attribute("\u{e000}", "bmp"),
            attribute("\u{10000}", "astral"),
        ];
        let utf16_order = vec![
            attribute("\u{10000}", "astral"),
            attribute("\u{e000}", "bmp"),
        ];
        let first = attributed_key("./m.mjs", scalar_order);
        let second = attributed_key("./m.mjs", utf16_order.clone());

        assert_eq!(first, second);
        assert_eq!(first.cmp(&second), core::cmp::Ordering::Equal);
        assert_eq!(first.attributes(), utf16_order.as_slice());
        let mut first_hash = DefaultHasher::new();
        first.hash(&mut first_hash);
        let mut second_hash = DefaultHasher::new();
        second.hash(&mut second_hash);
        assert_eq!(first_hash.finish(), second_hash.finish());

        let evaluation = ModuleRequestIr::from_key(first.clone(), ImportPhaseIr::Evaluation);
        let source = ModuleRequestIr::from_key(first, ImportPhaseIr::Source);
        assert_ne!(evaluation, source, "occurrences retain their phase");
        assert_eq!(
            evaluation.key(),
            source.key(),
            "host identity ignores phase"
        );
    }

    #[test]
    fn request_attributes_reject_duplicate_keys() {
        let error = ModuleRequestAttributesIr::try_new(vec![
            attribute("type", "json"),
            attribute("type", "css"),
        ])
        .expect_err("duplicate keys cannot inhabit the canonical list");

        assert_eq!(error.key(), "type");
        assert_eq!(error.to_string(), "duplicate import attribute key: type");
    }

    #[test]
    fn requested_modules_follow_source_order_across_item_kinds() {
        // A re-export written before an import must load and evaluate first,
        // which only holds if ModuleRequests is one source-order pass.
        let record = record_of("export * from \"./first.mjs\";\nimport \"./second.mjs\";\n");
        assert_eq!(
            record.requested_modules,
            vec![
                ModuleRequestIr::plain("./first.mjs"),
                ModuleRequestIr::plain("./second.mjs"),
            ]
        );
    }

    #[test]
    fn attributed_reexports_retain_each_request_and_entry_shape() {
        let record = record_of(
            "export * from './star.mjs' with { type: 'json', charset: 'utf8' };\n\
             export * as namespace from './namespace.mjs' with { mode: 'strict' };\n\
             export { value as renamed } from './named.mjs' with { type: 'text' };\n",
        );
        let star_request = attributed_request(
            "./star.mjs",
            ImportPhaseIr::Evaluation,
            vec![attribute("charset", "utf8"), attribute("type", "json")],
        );
        let namespace_request = attributed_request(
            "./namespace.mjs",
            ImportPhaseIr::Evaluation,
            vec![attribute("mode", "strict")],
        );
        let named_request = attributed_request(
            "./named.mjs",
            ImportPhaseIr::Evaluation,
            vec![attribute("type", "text")],
        );

        assert_eq!(
            record.requested_modules,
            vec![
                star_request.clone(),
                namespace_request.clone(),
                named_request.clone(),
            ]
        );
        assert_eq!(
            record.module_resolution_requests,
            vec![
                star_request.key().clone(),
                namespace_request.key().clone(),
                named_request.key().clone(),
            ]
        );
        assert_eq!(
            record.star_export_entries,
            vec![StarExportEntryIr {
                request: star_request,
            }]
        );
        assert_eq!(
            record.indirect_export_entries,
            vec![
                IndirectExportEntryIr {
                    request: namespace_request,
                    import_name: ImportNameIr::Namespace,
                    export_name: ExportName::new("namespace"),
                },
                IndirectExportEntryIr {
                    request: named_request,
                    import_name: named("value"),
                    export_name: ExportName::new("renamed"),
                },
            ]
        );
    }

    #[test]
    fn attributed_import_and_reexport_share_one_canonical_request() {
        let record = record_of(
            "import { imported } from './shared.mjs' with { type: 'json', charset: 'utf8' };\n\
             export { exported as renamed } from './shared.mjs' with { charset: 'utf8', type: 'json' };\n",
        );
        let request = attributed_request(
            "./shared.mjs",
            ImportPhaseIr::Evaluation,
            vec![attribute("charset", "utf8"), attribute("type", "json")],
        );

        assert_eq!(record.requested_modules, vec![request.clone()]);
        assert_eq!(
            record.module_resolution_requests,
            vec![request.key().clone()]
        );
        assert_eq!(record.import_entries.len(), 1);
        assert_eq!(&record.import_entries[0].request, &request);
        assert_eq!(
            record.indirect_export_entries,
            vec![IndirectExportEntryIr {
                request,
                import_name: named("exported"),
                export_name: ExportName::new("renamed"),
            }]
        );
    }

    #[test]
    fn requested_modules_retain_phases_while_resolution_requests_coalesce_them() {
        let record = record_of(
            "import './m.mjs';\n\
             import './m.mjs';\n\
             import defer * as deferred from './m.mjs';\n\
             import source artifact from './m.mjs';\n",
        );

        assert_eq!(
            record
                .requested_modules
                .iter()
                .map(ModuleRequestIr::phase)
                .collect::<Vec<_>>(),
            vec![
                ImportPhaseIr::Evaluation,
                ImportPhaseIr::Defer,
                ImportPhaseIr::Source,
            ]
        );
        assert!(record
            .requested_modules
            .iter()
            .all(|request| request.key() == &record.module_resolution_requests[0]));
        assert_eq!(
            record.module_resolution_requests,
            vec![request_key("./m.mjs")]
        );
    }

    #[test]
    fn requested_modules_keep_phaseful_order_after_an_earlier_key_projection() {
        let record = record_of(
            "import source artifact from './m.mjs';\n\
             import './n.mjs';\n\
             import './m.mjs';\n\
             artifact;",
        );

        assert_eq!(
            record.requested_modules,
            vec![
                ModuleRequestIr::from_key(request_key("./m.mjs"), ImportPhaseIr::Source,),
                ModuleRequestIr::plain("./n.mjs"),
                ModuleRequestIr::plain("./m.mjs"),
            ]
        );
        assert_eq!(
            record.module_resolution_requests,
            vec![request_key("./m.mjs"), request_key("./n.mjs")]
        );
    }

    // -- 16.2.3.2 ExportEntries / 16.2.3.3 ExportEntriesForModule ---------

    #[test]
    fn export_entries_split_into_local_indirect_and_star_tables() {
        let record = record_of(
            "import { imported } from \"./imp.mjs\";\n\
             import * as namespaced from \"./nsimp.mjs\";\n\
             const thing = 1;\n\
             export { thing };\n\
             export { thing as renamed };\n\
             export var counted = 2;\n\
             export function fn() {}\n\
             export * from \"./star.mjs\";\n\
             export * as ns from \"./nsmod.mjs\";\n\
             export { other as aliased } from \"./re.mjs\";\n\
             export { imported };\n\
             export { namespaced };\n",
        );

        assert_eq!(
            record.local_export_entries,
            vec![
                local_export(local("thing"), ExportName::new("thing")),
                local_export(local("thing"), ExportName::new("renamed")),
                local_export(local("counted"), ExportName::new("counted")),
                local_export(local("fn"), ExportName::new("fn")),
            ]
        );
        assert_eq!(
            record.indirect_export_entries,
            vec![
                indirect_export("./nsmod.mjs", ImportNameIr::Namespace, "ns"),
                indirect_export("./re.mjs", named("other"), "aliased"),
                // ParseModule 12.a.iv: `export { imported }` where `imported` is
                // a *named* import is an indirect entry, not a local one.
                indirect_export("./imp.mjs", named("imported"), "imported"),
                indirect_export("./nsimp.mjs", ImportNameIr::Namespace, "namespaced"),
            ]
        );
        assert_eq!(
            record.star_export_entries,
            vec![StarExportEntryIr {
                request: ModuleRequestIr::plain("./star.mjs"),
            }]
        );
    }

    #[test]
    fn reexported_deferred_namespace_retains_its_request_phase() {
        let record = record_of(
            "import defer * as deferred from './value.js'; export { deferred as forwarded };",
        );
        assert!(record.local_export_entries.is_empty());
        assert_eq!(record.indirect_export_entries.len(), 1);
        let export = &record.indirect_export_entries[0];
        assert_eq!(export.import_name, ImportNameIr::Namespace);
        assert_eq!(export.export_name, ExportName::new("forwarded"));
        assert_eq!(export.request.phase(), ImportPhaseIr::Defer);
    }

    #[test]
    fn exporting_a_default_import_is_indirect_on_the_name_default() {
        let record = record_of("import alpha from \"./m.mjs\";\nexport { alpha as beta };\n");

        assert_eq!(record.local_export_entries, Vec::new());
        assert_eq!(
            record.indirect_export_entries,
            vec![indirect_export(
                "./m.mjs",
                named(MODULE_DEFAULT_EXPORT_NAME),
                "beta"
            )]
        );
    }

    // -- 16.2.3.7 `export default` ----------------------------------------

    #[test]
    fn named_default_declarations_keep_their_own_local_name() {
        for source in [
            "export default function alpha() {}",
            "export default function* alpha() {}",
            "export default async function alpha() {}",
            "export default async function* alpha() {}",
            "export default class alpha {}",
        ] {
            let record = record_of(source);
            assert_eq!(
                record.local_export_entries,
                vec![local_export(
                    local("alpha"),
                    ExportName::new(MODULE_DEFAULT_EXPORT_NAME)
                )],
                "{source}"
            );
        }
    }

    #[test]
    fn anonymous_default_forms_bind_the_spec_name_and_rewrite_nothing() {
        for (source, kind) in [
            (
                "export default function () {}",
                ModuleBindingKindIr::Function,
            ),
            (
                "export default function* () {}",
                ModuleBindingKindIr::Function,
            ),
            (
                "export default async function () {}",
                ModuleBindingKindIr::Function,
            ),
            (
                "export default async function* () {}",
                ModuleBindingKindIr::Function,
            ),
            ("export default class {}", ModuleBindingKindIr::Class),
            ("export default 1 + 1;", ModuleBindingKindIr::Let),
        ] {
            let record = record_of(source);
            assert_eq!(
                record.local_export_entries,
                vec![local_export(
                    local(MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME),
                    ExportName::new(MODULE_DEFAULT_EXPORT_NAME)
                )],
                "{source}"
            );
            assert_eq!(
                record.environment,
                vec![ModuleEnvBindingIr {
                    name: LocalName::AnonymousDefault,
                    kind,
                    mutable: true,
                    initialized_before_evaluation: kind == ModuleBindingKindIr::Function,
                    in_tdz_until_evaluated: kind != ModuleBindingKindIr::Function,
                    indirect: None,
                }],
                "{source}"
            );
            // The record never rewrites source text; the body keeps its exact
            // byte length so span-derived `FunctionId`s stay valid.
            assert_eq!(record.source_len, source.len(), "{source}");
        }
    }

    // -- 16.2.1.6.4 InitializeEnvironment ---------------------------------

    #[test]
    fn module_environment_records_kinds_mutability_and_tdz() {
        let record = record_of(
            "import { imported } from \"./m.mjs\";\n\
             var v = 1;\n\
             let l = 2;\n\
             const c = 3;\n\
             function f() {}\n\
             class K {}\n\
             { var nested = 4; }\n",
        );

        assert_eq!(
            record.environment,
            vec![
                ModuleEnvBindingIr {
                    name: local("imported"),
                    kind: ModuleBindingKindIr::Import,
                    mutable: false,
                    initialized_before_evaluation: true,
                    in_tdz_until_evaluated: false,
                    indirect: Some((ModuleRequestIr::plain("./m.mjs"), named("imported"))),
                },
                ModuleEnvBindingIr {
                    name: local("v"),
                    kind: ModuleBindingKindIr::Var,
                    mutable: true,
                    initialized_before_evaluation: true,
                    in_tdz_until_evaluated: false,
                    indirect: None,
                },
                ModuleEnvBindingIr {
                    name: local("l"),
                    kind: ModuleBindingKindIr::Let,
                    mutable: true,
                    initialized_before_evaluation: false,
                    in_tdz_until_evaluated: true,
                    indirect: None,
                },
                ModuleEnvBindingIr {
                    name: local("c"),
                    kind: ModuleBindingKindIr::Const,
                    mutable: false,
                    initialized_before_evaluation: false,
                    in_tdz_until_evaluated: true,
                    indirect: None,
                },
                ModuleEnvBindingIr {
                    name: local("f"),
                    kind: ModuleBindingKindIr::Function,
                    mutable: true,
                    initialized_before_evaluation: true,
                    in_tdz_until_evaluated: false,
                    indirect: None,
                },
                ModuleEnvBindingIr {
                    name: local("K"),
                    kind: ModuleBindingKindIr::Class,
                    mutable: true,
                    initialized_before_evaluation: false,
                    in_tdz_until_evaluated: true,
                    indirect: None,
                },
                // VarDeclaredNames reaches `var`s nested in blocks; they are
                // module-environment bindings too, and they are appended after
                // the source-order walk.
                ModuleEnvBindingIr {
                    name: local("nested"),
                    kind: ModuleBindingKindIr::Var,
                    mutable: true,
                    initialized_before_evaluation: true,
                    in_tdz_until_evaluated: false,
                    indirect: None,
                },
            ]
        );
    }

    // -- `[[HasTLA]]` ------------------------------------------------------

    #[test]
    fn top_level_await_sets_has_tla_rather_than_failing_parse_module() {
        for source in [
            "await Promise.resolve(1);\n",
            "for await (const value of []) { value; }\n",
            "await using resource = null;\n",
            "{ await using resource = undefined; }\n",
            "if (false) { await using resource = null; }\n",
            "for (await using resource = null; false;) {}\n",
            "for (await using resource of []) {}\n",
            "for await (await using resource of []) {}\n",
            "class Example { [await 0]() {} }\n",
        ] {
            let record =
                parse_module_record(&source_unit(source), 0, ModuleKey::from_host("main.mjs"))
                    .unwrap_or_else(|diagnostics| {
                        panic!("top-level await must parse, got {diagnostics:?} for {source}")
                    });
            assert!(record.has_top_level_await, "{source}");
        }
    }

    #[test]
    fn await_inside_a_function_is_not_top_level_await() {
        let record = record_of(
            "async function alpha() { await 1; }\n\
             const beta = async () => { for await (const v of []) { v; } };\n",
        );
        assert!(!record.has_top_level_await);
    }

    #[test]
    fn only_module_owned_async_disposal_contributes_to_has_tla() {
        for source in [
            "using resource = null; for (using next of []) {}",
            "async function f() { await using resource = null; }",
            "const f = async () => { for (await using resource of []) {} };",
            "async function* g() { for (await using resource = null; false;) {} }",
            "const object = { async method() { await using resource = null; } };",
            "class Example { async method() { await using resource = null; } }",
            "class Example { field = async () => { await using resource = null; }; }",
        ] {
            assert!(!record_of(source).has_top_level_await, "{source}");
        }
    }

    // -- `import.meta` and `import()` --------------------------------------

    #[test]
    fn import_meta_and_dynamic_import_sites_are_collected() {
        let record = record_of(
            "import.meta.url;\n\
             const alpha = import(\"./static.mjs\");\n\
             const beta = import(computed);\n\
             function inner() { return import(\"./nested.mjs\"); }\n\
             import.meta.resolve;\n",
        );

        assert_eq!(record.import_meta_uses(), 2);
        assert_eq!(
            record.dynamic_import_sites,
            vec![
                DynamicImportSiteIr {
                    static_specifier: Some("./static.mjs".to_string()),
                    phase: ImportPhaseIr::Evaluation,
                    attributes: known_attributes(Vec::new()),
                    catalog_eligibility: CatalogEligibility::Eligible,
                    origin: DynamicImportSiteOrigin::RetainedSource,
                },
                // A computed specifier is not statically discoverable, so the
                // target cannot be compiled into the artifact.
                DynamicImportSiteIr {
                    static_specifier: None,
                    phase: ImportPhaseIr::Evaluation,
                    attributes: known_attributes(Vec::new()),
                    catalog_eligibility: CatalogEligibility::Eligible,
                    origin: DynamicImportSiteOrigin::RetainedSource,
                },
                // Nesting inside a function does not hide the site: the whole
                // point is that the target is reachable at run time.
                DynamicImportSiteIr {
                    static_specifier: Some("./nested.mjs".to_string()),
                    phase: ImportPhaseIr::Evaluation,
                    attributes: known_attributes(Vec::new()),
                    catalog_eligibility: CatalogEligibility::Eligible,
                    origin: DynamicImportSiteOrigin::RetainedSource,
                },
            ]
        );
    }

    fn record_with_id(source: &str, id: ModuleUnitId) -> ModuleRecordIr {
        parse_module_record(&source_unit(source), id, ModuleKey::from_host("main.mjs"))
            .expect("module record should build")
    }

    /// The spelling `build_graph` chooses for `record`.
    fn spelling_of(record: &ModuleRecordIr) -> UnitLinkerSpelling {
        UnitLinkerSpelling::choose(record.id, record.linker_reserved_names())
            .expect("a free spelling exists")
    }

    /// Every way module source can spell an identifier reserves it, so the
    /// unit's activation cells are never spelled like one: top-level and nested
    /// declarations of every form, parameters, catch and loop bindings,
    /// destructuring, import locals, plain/`typeof`/assignment references and
    /// shorthand properties.
    #[test]
    fn every_identifier_form_reserves_its_spelling() {
        for (template, name) in [
            ("var NAME = 1;", "$d0$"),
            ("let NAME = 1;", "$d0$"),
            ("const NAME = 1;", "$m0$meta"),
            ("function NAME() {}", "$d0$"),
            ("function* NAME() {}", "$d0$"),
            ("async function NAME() {}", "$m0$meta"),
            ("class NAME {}", "$d0$"),
            ("export let NAME = 1;", "$d0$"),
            ("export function NAME() {}", "$m0$meta"),
            ("export default function NAME() {}", "$d0$"),
            ("export default class NAME {}", "$d0$"),
            ("import NAME from './a.mjs';", "$d0$"),
            ("import { x as NAME } from './a.mjs';", "$m0$meta"),
            ("import * as NAME from './a.mjs';", "$d0$"),
            ("function f(NAME) {}", "$d0$"),
            ("function f(a = NAME) {}", "$d0$"),
            ("(NAME) => 0;", "$m0$meta"),
            ("try {} catch (NAME) {}", "$d0$"),
            ("for (const NAME of []) {}", "$d0$"),
            ("const { a: [NAME] } = {};", "$d0$"),
            ("const { NAME } = {};", "$m0$meta"),
            ("({ NAME } = {});", "$d0$"),
            ("(class { m() { return NAME; } });", "$d0$"),
            ("{ let NAME = 1; }", "$d0$"),
            ("export function f() { return NAME; }", "$d0$"),
            ("typeof NAME;", "$m0$meta"),
            ("NAME = 1;", "$d0$"),
            ("({ NAME });", "$d0$"),
        ] {
            let declaration = template.replace("NAME", name);
            let default = if template.contains("export default") {
                ""
            } else {
                "export default 0;"
            };
            let source = format!("{declaration}\n{default}\nimport.meta;");
            let record = record_with_id(&source, 0);
            assert!(
                record.linker_reserved_names().contains(name),
                "{source}: {:?}",
                record.linker_reserved_names()
            );
            let spelling = spelling_of(&record);
            assert_ne!(spelling.anonymous_default().as_str(), name, "{source}");
            assert_ne!(spelling.import_meta().as_str(), name, "{source}");
            assert_eq!(spelling.anonymous_default().as_str(), "$d1$", "{source}");
        }
    }

    /// Property names, strings and other prefixes are user data, not
    /// identifiers, and leave the decimal spelling in place.
    #[test]
    fn non_identifier_spellings_reserve_nothing() {
        let record = record_with_id(
            "const o = { $d0$: 1 }; o.$d0$; '$m0$meta'; $d0$: ;\nexport default 0;",
            0,
        );
        assert!(
            record.linker_reserved_names().is_empty(),
            "{:?}",
            record.linker_reserved_names()
        );
        assert_eq!(spelling_of(&record).anonymous_default().as_str(), "$d0$");
    }

    /// Every slice a site addresses, so a test can say what was found without
    /// hard-coding byte offsets.
    fn site_slices<'a>(source: &'a str, record: &ModuleRecordIr) -> Vec<&'a str> {
        record
            .import_meta_sites
            .iter()
            .map(|site| &source[site.start..site.end])
            .collect()
    }

    #[test]
    fn import_meta_sites_span_the_whole_meta_property() {
        let source = "print(import.meta.url);\nconst m = import.meta;\n";
        let record = record_of(source);
        assert_eq!(
            site_slices(source, &record),
            vec!["import.meta", "import.meta"]
        );
    }

    #[test]
    fn an_import_meta_written_across_lines_or_around_a_comment_is_one_site() {
        // The meta-property is two tokens, so anything that can sit between two
        // tokens can sit inside it — and all of it has to be replaced.
        let source = "const a = import\n  . /* here */ meta;\n";
        let record = record_of(source);
        assert_eq!(
            site_slices(source, &record),
            vec!["import\n  . /* here */ meta"]
        );
    }

    #[test]
    fn a_module_without_import_meta_declares_no_object() {
        let source = "const value = 1;\n";
        let record = record_of(source);
        assert_eq!(record.import_meta_uses(), 0);
        // Nothing to rewrite, and the text comes back untouched rather than
        // merely equivalent.
        assert_eq!(
            rewrite_import_meta(source, &record, &spelling_of(&record)).as_deref(),
            Ok(source)
        );
    }

    #[test]
    fn rewriting_replaces_every_site_with_this_units_binding() {
        let source = "print(import.meta.url);\nconst m = import.meta;\n";
        let record = record_with_id(source, 7);
        let spelling = spelling_of(&record);
        let rewritten =
            rewrite_import_meta(source, &record, &spelling).expect("rewrite should succeed");

        let name = spelling.import_meta();
        assert_eq!(name.as_str(), "$m7$meta");
        assert!(
            !rewritten.contains("import"),
            "no `import.meta` may survive into Script text, got: {rewritten}"
        );
        assert_eq!(rewritten.matches(name.as_str()).count(), 2, "{rewritten}");
        // The rest of the body is untouched.
        assert!(rewritten.contains(".url);"), "{rewritten}");
    }

    #[test]
    fn two_units_get_two_distinct_import_meta_objects() {
        let zero = UnitLinkerSpelling::unreserved(0);
        let one = UnitLinkerSpelling::unreserved(1);
        assert_ne!(zero.import_meta(), one.import_meta());
        assert_ne!(
            import_meta_binding(&zero, "file:///a.mjs").declaration,
            import_meta_binding(&one, "file:///a.mjs").declaration
        );
    }

    #[test]
    fn rewriting_preserves_byte_length_and_line_structure() {
        for source in [
            "print(import.meta.url);\n",
            "const a = import\n  . /* here */ meta;\n",
            "function f() { return import.meta; }\nf();\n",
        ] {
            let record = record_of(source);
            let rewritten = rewrite_import_meta(source, &record, &spelling_of(&record))
                .expect("rewrite should succeed");
            assert_eq!(rewritten.len(), source.len(), "{source}");
            assert_eq!(
                rewritten.matches('\n').count(),
                source.matches('\n').count(),
                "{source}"
            );
        }
    }

    #[test]
    fn the_declaration_is_a_null_prototype_object_carrying_url() {
        let binding = import_meta_binding(&UnitLinkerSpelling::unreserved(0), "file:///root/a.mjs");
        assert_eq!(
            binding.declaration,
            "const $m0$meta = { __proto__: null, url: \"file:///root/a.mjs\" };"
        );
        assert_eq!(binding.name.as_str(), "$m0$meta");
    }

    #[test]
    fn a_url_that_needs_escaping_stays_one_string_literal() {
        let binding = import_meta_binding(
            &UnitLinkerSpelling::unreserved(0),
            "file:///a\"b\\c\u{2028}d\n",
        );
        assert_eq!(
            binding.declaration,
            "const $m0$meta = { __proto__: null, url: \"file:///a\\\"b\\\\c\\u2028d\\n\" };"
        );
        // Whatever the host handed over, the declaration is still one line.
        assert!(!binding.declaration.contains('\n'));
    }

    #[test]
    fn a_site_that_does_not_address_this_source_is_refused_not_applied() {
        let source = "print(import.meta.url);\n";
        let mut record = record_of(source);
        record.import_meta_sites = vec![SourceSpan { start: 0, end: 11 }];
        let error = rewrite_import_meta(source, &record, &spelling_of(&record))
            .expect_err("a bogus span must be reported");
        assert!(error.reason.contains("covers"), "{}", error.reason);
    }

    #[test]
    fn scan_module_requests_reports_static_import_call_targets() {
        let requests = scan_module_requests(&source_unit(
            "import \"./static.mjs\";\nconst alpha = import(\"./dynamic.mjs\");\n",
        ));
        assert_eq!(
            requests,
            vec![request_key("./static.mjs"), request_key("./dynamic.mjs"),]
        );
    }

    #[test]
    fn dynamic_import_literal_attributes_reach_module_requests() {
        let source = source_unit(
            "import('./known.mjs', { with: { type: 'json', charset: 'utf8' } });\n\
             import('./runtime.mjs', options);\n\
             import('./proto.mjs', { with: { __proto__: 'not-an-attribute' } });\n\
             import('./inherited.mjs', { __proto__: { with: { type: 'json' } } });\n",
        );
        let record = parse_module_record(&source, 0, ModuleKey::from_host("main.mjs"))
            .expect("module record should build");

        assert_eq!(
            record.dynamic_import_sites[0].attributes,
            known_attributes(vec![
                attribute("charset", "utf8"),
                attribute("type", "json"),
            ])
        );
        assert_eq!(
            record.dynamic_import_sites[1].attributes,
            DynamicImportAttributesIr::Runtime
        );
        assert_eq!(
            record.dynamic_import_sites[2].attributes,
            known_attributes(Vec::new())
        );
        assert_eq!(
            record.dynamic_import_sites[3].attributes,
            DynamicImportAttributesIr::Runtime
        );
        assert_eq!(
            scan_module_requests(&source),
            vec![
                attributed_key(
                    "./known.mjs",
                    vec![attribute("charset", "utf8"), attribute("type", "json")],
                ),
                // A runtime options shape discovers the attribute-free request
                // as the AOT registry's safe baseline. Runtime matching still
                // requires the eventual attribute list to be exactly empty.
                request_key("./runtime.mjs"),
                request_key("./proto.mjs"),
                request_key("./inherited.mjs"),
            ]
        );
    }

    // -- position mapping ---------------------------------------------------

    #[test]
    fn line_index_maps_positions_to_byte_offsets() {
        // A CRLF break, a multi-byte character, then an LF break: columns count
        // code points, offsets count bytes.
        let source = "let a = 1;\r\nlet \u{e9} = 2;\nlet c = 3;";
        let lines = LineIndex::new(source);

        assert_eq!(lines.offset(source, Position::new(1, 1)), 0);
        // "\r\n" is one line break, so line 2 starts after both bytes.
        assert_eq!(lines.offset(source, Position::new(2, 1)), 12);
        assert_eq!(lines.offset(source, Position::new(2, 5)), 16);
        // The character before this one is two bytes wide.
        assert_eq!(lines.offset(source, Position::new(2, 6)), 18);
        assert_eq!(lines.offset(source, Position::new(3, 1)), 24);
        // A `Span` end sits one past its last character; it must clamp to the
        // end of the line's content rather than swallow the line break.
        assert_eq!(lines.offset(source, Position::new(3, 11)), source.len());
    }
}
