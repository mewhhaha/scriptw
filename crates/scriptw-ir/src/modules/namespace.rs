//! Module namespace exotic objects (10.4.6, `ModuleNamespaceCreate` 16.2.1.10).
//!
//! A namespace object is a static table: every export name the module provides,
//! paired with the binding that backs it. Reads are live because the binding is
//! the exporter's own; writes are rejected; the key order is fixed at compile
//! time.
//!
//! # What 10.4.6 asks for
//!
//! 10.4.6 makes a namespace object unlike any ordinary object, and every one of
//! its invariants is a compile-time constant here rather than runtime state:
//!
//! * `[[GetPrototypeOf]]` is `null` and `[[SetPrototypeOf]]` only succeeds for
//!   `null` ([`ModuleNamespaceIr::PROTOTYPE_IS_NULL`]);
//! * `[[IsExtensible]]` is `false` and `[[PreventExtensions]]` is a no-op that
//!   succeeds ([`ModuleNamespaceIr::EXTENSIBLE`]);
//! * `@@toStringTag` is the non-writable, non-enumerable, non-configurable
//!   string `"Module"` ([`ModuleNamespaceIr::TO_STRING_TAG`]);
//! * every export is a writable, enumerable, non-configurable *data* property
//!   whose value is read through the exporter's binding at access time, so the
//!   binding is live. Writes and deletion fail; compatible property definitions
//!   succeed without changing the binding;
//! * `[[OwnPropertyKeys]]` is [`ModuleNamespaceIr::exports`] in UTF-16
//!   code-unit order, then `@@toStringTag`.
//!
//! Identity is cached separately for eager and deferred requests. Repeated
//! requests in one phase share a cell; the two phases remain distinct even when
//! the reflected module is evaluated eagerly.
//!
//! # The one name domain that is *not* here
//!
//! A backend that emitted a real per-unit Environment Record would address an
//! export through a `$m{unit}$`-prefixed spelling of the exporter's
//! `[[LocalName]]`. That is a distinct name domain, and this file used to carry
//! one such value on every namespace export, written by `ModuleGraphIr::cell_name`
//! and read only by a test. Both are deleted: the source-text linker names an
//! exporter's binding exactly as the exporter spells it, so a prefixed name in
//! generated Script text would bind nothing.
//!
//! If that backend is built, the name it needs must be a **different type** from
//! [`MergedName`] — it is a name in a different scope — and must not be spelled
//! as one. Reintroducing it as a `String` field beside a `MergedName` is the
//! mistake this arrangement exists to prevent.
//!
//! The linker emits private export-reader closures ahead of module evaluation.
//! Exact trusted initializer spans carry their meaning through the merged Script
//! into `ExprIr::ModuleNamespace`; user source cannot request this constructor.
//! The backend stores those closures in a private export table on the canonical
//! object representation. They are internal live-binding readers, not property
//! accessors. Eager and deferred namespaces share this representation; deferred
//! internal methods first invoke the module's existing evaluation thunk.
//!
//! Loaded ECMAScript Source Text Modules have no module source object. Static
//! source bindings fail during linking and dynamic source jobs reject before
//! instantiation; namespace construction never manufactures a replacement.

use crate::*;

use super::evaluation_mode::ModuleMaterializationModeIr;

/// One entry of a module namespace object's export table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNamespaceExportIr {
    /// The exported name, as seen through the namespace object.
    pub export_name: ExportName,
    /// What `ResolveExport` produced for it.
    ///
    /// The single source of the cell a read of this key reaches:
    /// [`namespace_target_reference`] maps it to the merged name, and there is
    /// no second, precomputed spelling of the same thing to drift from it.
    pub target: ResolvedBindingIr,
}

/// A module namespace exotic object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleNamespaceIr {
    /// Module this namespace reflects.
    pub module: ModuleUnitId,
    /// Exports, sorted by UTF-16 code unit as `[[OwnPropertyKeys]]` requires,
    /// with ambiguous and unresolvable names excluded (16.2.1.10).
    pub exports: Vec<ModuleNamespaceExportIr>,
    /// Storage name of the identity-cached namespace object cell.
    ///
    /// Under the source-text linker this doubles as the merged script's binding
    /// name for the object, which is safe because
    /// [`MergedName::minted`] mints it from the unit id and a
    /// [`UnitCellRole`] rather than from source.
    pub cell: MergedName,
    /// Identity and internal-method behavior selected by the import request.
    mode: ModuleNamespaceModeIr,
    /// How the reflected module's body is emitted, independently of identity.
    materialization: ModuleMaterializationModeIr,
    /// Merged-script source that materializes this object, or the reason it
    /// cannot be expressed as Script text.
    ///
    /// Filled by `ensure_namespace`, so it is a pure function of the linked
    /// graph and costs nothing when no importer observes the namespace.
    pub source: Result<String, String>,
}

impl ModuleNamespaceIr {
    pub(crate) const fn mode(&self) -> ModuleNamespaceModeIr {
        self.mode
    }

    /// `[[GetPrototypeOf]]` of a namespace object is always `null` (10.4.6.1).
    pub const PROTOTYPE_IS_NULL: bool = true;
    /// A namespace object is never extensible (10.4.6.3).
    pub const EXTENSIBLE: bool = false;
    /// `@@toStringTag`, non-writable / non-enumerable / non-configurable
    /// (10.4.6).
    pub const TO_STRING_TAG: &'static str = "Module";

    /// `[[OwnPropertyKeys]]`, string keys only, already in UTF-16 code-unit
    /// order. `@@toStringTag` follows them.
    #[must_use]
    pub fn own_property_keys(&self) -> Vec<&ExportName> {
        self.exports
            .iter()
            .map(|export| &export.export_name)
            .collect()
    }
}

/// Sort key giving UTF-16 code-unit order, which is what `[[OwnPropertyKeys]]`
/// requires and what `String` `Ord` (UTF-8 byte order) does *not* give for
/// astral-plane and some BMP names.
fn utf16_sort_key(name: &ExportName) -> Vec<u16> {
    name.as_str().encode_utf16().collect()
}

/// Appends `value` as a double-quoted JavaScript string literal.
///
/// Export names come from `ModuleExportName`, which is an arbitrary string
/// literal (`export { a as "any \u{10000} text" }`), so nothing about a name may
/// be assumed. Escaping runs over UTF-16 code units and emits `\uXXXX` for
/// everything outside printable ASCII, which keeps the generated source ASCII
/// and is the only encoding that survives an unpaired surrogate.
pub(super) fn push_js_string_literal(out: &mut String, value: &str) {
    out.push('"');
    for unit in value.encode_utf16() {
        match unit {
            0x22 => out.push_str("\\\""),
            0x5C => out.push_str("\\\\"),
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0A => out.push_str("\\n"),
            0x0B => out.push_str("\\v"),
            0x0C => out.push_str("\\f"),
            0x0D => out.push_str("\\r"),
            0x20..=0x7E => out.push(char::from(unit as u8)),
            _ => out.push_str(&format!("\\u{unit:04X}")),
        }
    }
    out.push('"');
}

/// Checks the decoded character spelling of a merged binding name.
///
/// The lexer and this emitter consume the same ECMAScript `ID_Start` /
/// `ID_Continue` authority, including its Unicode 17 additions. Escape decoding
/// belongs to the interner; this guard must neither normalize names nor reject
/// combining marks and connector punctuation the parser already accepted.
///
/// This remains a spelling predicate, not another name domain or a reserved-word
/// validator. Source bindings have already passed their grammar-context checks;
/// anonymous defaults reach it only after `LocalName::merged_in` mints `$d…$`.
fn is_binding_identifier(name: &MergedName) -> bool {
    let mut chars = name.as_str().chars();
    let Some(first) = chars.next() else {
        return false;
    };
    scriptw_front::is_identifier_start(u32::from(first))
        && chars.all(|ch| scriptw_front::is_identifier_part(u32::from(ch)))
}

/// Every global the merged script's own preludes spell.
///
/// One list, because the four guards that need it used to be three different
/// literals: `link.rs`'s renamed-import check tested
/// `OBJECT_NAME | SYMBOL_NAME | GLOBAL_THIS_NAME`, while this module's two alias
/// checks and its shadowed-globals check tested only `OBJECT_NAME | SYMBOL_NAME`.
/// So `import * as globalThis from './m.js'` emitted
/// `const globalThis = $m0$namespace;` into the merged scope ahead of
/// the former alias prelude's `Object.defineProperty(globalThis, …)`, which then
/// defined every renamed-import alias on the namespace object — a silent wrong
/// answer where the other two names give a diagnostic. Three literals could
/// disagree; one cannot.
pub(crate) const PRELUDE_GLOBALS: [&str; 3] = [OBJECT_NAME, SYMBOL_NAME, GLOBAL_THIS_NAME];

/// Whether `name` is one of [`PRELUDE_GLOBALS`].
pub(crate) fn shadows_prelude_global(name: &str) -> bool {
    PRELUDE_GLOBALS.contains(&name)
}

/// Expression the merged script evaluates to read a resolved binding.
///
/// The single authority for what a `ResolvedBinding` Record reads as in the
/// merged scope, and the only D1 -> D3 crossing on the emitter's path.
///
/// `None` means the binding cannot be named as Script text: an ambiguous or
/// missing export (which 16.2.1.10 already excludes from the namespace), or a
/// local name no `IdentifierReference` can spell.
#[must_use]
pub fn namespace_target_reference(target: &ResolvedBindingIr) -> Option<MergedName> {
    match target {
        ResolvedBindingIr::Resolved {
            module,
            binding: ModuleBindingNameIr::Namespace(mode),
        } => Some(MergedName::minted(*module, mode.cell_role())),
        ResolvedBindingIr::Resolved {
            module,
            binding: ModuleBindingNameIr::ModuleSource,
        } => Some(MergedName::minted(*module, UnitCellRole::ModuleSource)),
        ResolvedBindingIr::Resolved {
            module,
            binding: ModuleBindingNameIr::Name(name),
        } => {
            // `*default*` is the one `[[LocalName]]` no source text can spell,
            // and `merged_in` is where the merged script's minted name for it
            // comes from. Applied exactly once, here.
            //
            // The one site that keeps `merged_in` rather than
            // `ModuleRecordIr::merged`: `module` and `name` are
            // destructured from the *same* `ResolvedBindingIr::Resolved`, so the
            // "which unit owns this name" pairing is already structural and
            // there is no id to supply independently.
            let reference = name.merged_in(*module);
            is_binding_identifier(&reference).then_some(reference)
        }
        ResolvedBindingIr::Ambiguous | ResolvedBindingIr::NotFound => None,
    }
}

/// Private closure table recognized only through linker-owned source spans.
fn namespace_object_source(namespace: &ModuleNamespaceIr) -> Result<String, String> {
    let defer_evaluate = MergedName::minted(namespace.module, UnitCellRole::DeferEvaluate);
    let mut text = format!("const {} = [", namespace.cell.as_str());
    match namespace.mode {
        ModuleNamespaceModeIr::Eager => text.push_str("void 0"),
        ModuleNamespaceModeIr::Deferred => {
            text.push_str("() => ");
            match namespace.materialization {
                ModuleMaterializationModeIr::Eager => text.push_str("void 0"),
                ModuleMaterializationModeIr::Deferred => {
                    text.push_str(defer_evaluate.as_str());
                    text.push_str("()");
                }
            }
        }
    }
    for export in &namespace.exports {
        let reference = namespace_target_reference(&export.target).ok_or_else(|| {
            format!(
                "export `{}` resolves to a binding the merged script cannot name",
                export.export_name.as_str()
            )
        })?;
        text.push_str(", ");
        push_js_string_literal(&mut text, export.export_name.as_str());
        text.push_str(", () => ");
        match namespace.materialization {
            ModuleMaterializationModeIr::Eager => text.push_str(reference.as_str()),
            ModuleMaterializationModeIr::Deferred => {
                text.push_str(defer_evaluate.as_str());
                text.push_str("()[");
                push_js_string_literal(&mut text, export.export_name.as_str());
                text.push_str("]()");
            }
        }
    }
    text.push_str("];\n");
    Ok(text)
}

/// Merged-script prelude declaring every observed namespace object and binding
/// every `import * as ns` local name to one.
///
/// Emitted ahead of every unit body. That is safe in both directions: the
/// getters are deferred, so no export needs to be initialized yet, and the
/// aliases are plain object references, so copying one is not a lost live
/// binding the way copying an exported `let` would be.
///
/// # Errors
/// Returns one diagnostic per reason the prelude cannot be expressed as Script
/// text, so a graph that cannot be linked says exactly what stopped it instead
/// of emitting source that binds the wrong thing.
pub fn namespace_prelude_source(graph: &ModuleGraphIr) -> Result<String, Vec<IrDiagnostic>> {
    if !graph.link_errors.is_empty() {
        return Err(graph
            .link_errors
            .iter()
            .map(ModuleLinkErrorIr::to_diagnostic)
            .collect());
    }
    if graph
        .materialized_units()
        .all(|(_, _, unit)| unit.namespaces.is_empty())
    {
        return Ok(String::new());
    }
    let mut diagnostics = Vec::new();
    report_shadowed_namespace_globals(graph, &mut diagnostics);
    let aliases = collect_namespace_aliases(graph, &mut diagnostics);

    // Unit order is unit-id order, which is stable across runs and independent
    // of evaluation order — the getters are deferred, so no namespace has to be
    // declared before another.
    let mut text = String::new();
    // The deferred export tables first: a deferred namespace's getter calls a
    // thunk that assigns to one, and the thunk is a hoisted `function` that any
    // unit body can reach before its own declaration is stepped over.
    for (module, mode, _) in graph.materialized_units() {
        match mode {
            ModuleMaterializationModeIr::Eager => {}
            ModuleMaterializationModeIr::Deferred => {
                text.push_str(&deferred_cells_declaration(module));
            }
        }
    }
    for (_, _, unit) in graph.materialized_units() {
        for namespace in unit.namespaces.values() {
            match &namespace.source {
                Ok(source) => text.push_str(source),
                Err(reason) => {
                    diagnostics.push(namespace_unsupported(unit.record.key.as_str(), reason))
                }
            }
        }
    }
    for (local, object) in &aliases {
        text.push_str("const ");
        text.push_str(local.as_str());
        text.push_str(" = ");
        text.push_str(object.as_str());
        text.push_str(";\n");
    }

    if diagnostics.is_empty() {
        Ok(text)
    } else {
        Err(diagnostics)
    }
}

/// Renamed-binding inspection aliases use generated global operations.
/// Canonical namespace construction has no observable global dependency.
fn namespace_prelude_uses_global(graph: &ModuleGraphIr, name: &str) -> bool {
    let renamed_bindings = graph.materialized_units().any(|(_, _, unit)| {
        unit.record
            .import_entries
            .iter()
            .zip(&unit.resolved_imports)
            .any(|(entry, resolved)| {
                entry.request.phase() != ImportPhaseIr::Source
                    && matches!(
                        resolved,
                        ResolvedBindingIr::Resolved {
                            binding: ModuleBindingNameIr::Name(_),
                            ..
                        }
                    )
                    && namespace_target_reference(resolved)
                        .is_some_and(|reference| reference != unit.record.merged(&entry.local_name))
            })
    });
    match name {
        OBJECT_NAME => renamed_bindings,
        SYMBOL_NAME => false,
        GLOBAL_THIS_NAME => renamed_bindings,
        _ => false,
    }
}

fn report_shadowed_namespace_globals(graph: &ModuleGraphIr, diagnostics: &mut Vec<IrDiagnostic>) {
    for (_, mode, unit) in graph.materialized_units() {
        match mode {
            ModuleMaterializationModeIr::Eager => {}
            // A deferred body is a function body, so its declarations cannot
            // shadow the globals used by the outer prelude.
            ModuleMaterializationModeIr::Deferred => continue,
        }
        for shadowed in unit
            .record
            .environment
            .iter()
            .filter(|binding| binding.kind != ModuleBindingKindIr::Import)
        {
            // The merged spelling, because that is the name the prelude's own
            // `Object.` / `Symbol.` reads would resolve against.
            let merged = unit.record.merged(&shadowed.name);
            if !namespace_prelude_uses_global(graph, merged.as_str()) {
                continue;
            }
            diagnostics.push(namespace_unsupported(
                unit.record.key.as_str(),
                &format!(
                    "module prelude uses `{}`, which this module shadows at top level",
                    merged.as_str()
                ),
            ));
        }
    }
}

/// `(local name, namespace object binding)` for every `import * as ns`.
///
/// Unlike an imported *value*, a namespace alias is a fresh `const` in the
/// merged scope rather than a share of the exporter's cell, so it has to be
/// checked for collisions the way an ordinary top-level declaration is —
/// `check_linkable`'s collision map deliberately skips import bindings, so
/// nothing else catches one.
fn collect_namespace_aliases(
    graph: &ModuleGraphIr,
    diagnostics: &mut Vec<IrDiagnostic>,
) -> Vec<(MergedName, MergedName)> {
    // Every top-level name a unit owns outright, in the *merged* spelling —
    // this map is compared against alias names, which are declared into the
    // merged scope, so both sides have to be D3. Import bindings are excluded
    // for the same reason `check_linkable` excludes them: they are deliberately
    // the exporting unit's cell.
    let mut declared: BTreeMap<MergedName, &str> = BTreeMap::new();
    for (_, mode, unit) in graph.materialized_units() {
        match mode {
            ModuleMaterializationModeIr::Eager => {}
            // Same reason as `report_shadowed_namespace_globals`: a deferred
            // unit's declarations live in its thunk scope.
            ModuleMaterializationModeIr::Deferred => continue,
        }
        for binding in &unit.record.environment {
            if binding.kind != ModuleBindingKindIr::Import {
                declared.insert(unit.record.merged(&binding.name), unit.record.key.as_str());
            }
        }
    }

    let mut aliases = Vec::new();
    let mut owners: BTreeMap<MergedName, (&str, MergedName)> = BTreeMap::new();
    for (_, _, unit) in graph.materialized_units() {
        let key = unit.record.key.as_str();
        for (index, entry) in unit.record.import_entries.iter().enumerate() {
            // The alias is emitted as a real `const` in the merged scope, so
            // the name that matters here is the merged one — the same domain
            // as the `declared` map it is checked against below.
            let merged = unit.record.merged(&entry.local_name);
            let local = merged.as_str();
            let Some(ResolvedBindingIr::Resolved {
                module,
                binding: ModuleBindingNameIr::Namespace(mode),
            }) = unit.resolved_imports.get(index)
            else {
                continue;
            };
            let namespace_cell = MergedName::minted(*module, mode.cell_role());
            if !is_binding_identifier(&merged) {
                diagnostics.push(namespace_unsupported(
                    key,
                    &format!("namespace binding `{local}` is not spellable"),
                ));
            } else if namespace_prelude_uses_global(graph, local) {
                diagnostics.push(namespace_unsupported(
                    key,
                    &format!(
                        "module prelude uses `{local}`, which this module binds as a namespace alias"
                    ),
                ));
            } else if let Some((previous, previous_cell)) = owners.get(&merged) {
                if previous_cell != &namespace_cell {
                    diagnostics.push(namespace_unsupported(
                        key,
                        &format!(
                            "namespace binding `{local}` is already bound by module {previous}"
                        ),
                    ));
                }
            } else if let Some(owner) = declared.get(&merged) {
                diagnostics.push(namespace_unsupported(
                    key,
                    &format!(
                        "namespace binding `{local}` collides with a top-level declaration in module {owner}"
                    ),
                ));
            } else {
                owners.insert(merged.clone(), (key, namespace_cell.clone()));
                aliases.push((merged, namespace_cell));
            }
        }
    }
    aliases
}

/// Merged-script text for a deferred module's body and evaluation completion.
///
/// The body keeps its own function declaration scope, including hoisted default
/// exports. Its private readers capture that same scope before execution starts.
/// A separate evaluator surrounds the invocation with an exception boundary, so
/// retaining a thrown value does not move the module's lexical declarations into
/// a catch or try block. Reentrant evaluation returns the published readers;
/// subsequent failed evaluations rethrow the original value, including undefined.
#[cfg(test)]
fn deferred_body_source(
    graph: &ModuleGraphIr,
    module: ModuleUnitId,
    body: &str,
) -> Result<String, String> {
    deferred_body_original_source(
        graph,
        module,
        super::callable_source::OriginalUnitSource::new(body),
    )
    .map(super::callable_source::OriginalUnitSource::into_text)
}

#[cfg(test)]
fn deferred_body_original_source(
    graph: &ModuleGraphIr,
    module: ModuleUnitId,
    body: super::callable_source::OriginalUnitSource,
) -> Result<super::callable_source::OriginalUnitSource, String> {
    let cells = MergedName::minted(module, UnitCellRole::DeferCells);
    let cells = cells.as_str();
    let evaluate = MergedName::minted(module, UnitCellRole::DeferEvaluate);
    let execute = MergedName::minted(module, UnitCellRole::DeferExecute);
    let state = MergedName::minted(module, UnitCellRole::DeferState);
    let error = MergedName::minted(module, UnitCellRole::DeferError);
    let caught = MergedName::minted(module, UnitCellRole::DeferCaughtError);
    // Never `unwrap_or_default`: an empty table would compile to a namespace
    // whose every export reads `undefined` instead of saying what went wrong.
    // `collect_observed_namespaces` always builds one for a deferred module,
    // because being deferred means an `import defer * as ns` resolved to it.
    let exports = graph
        .units
        .get(module as usize)
        .and_then(|unit| unit.namespaces.values().next())
        .map(|namespace| namespace.exports.as_slice())
        .ok_or_else(|| {
            "deferred module has no namespace object to publish its exports through".to_string()
        })?;

    let mut text = format!(
        "function {evaluate}() {{\n\
         if ({state} === {errored}) throw {error};\n\
         if ({state} !== {unstarted}) return {cells};\n\
         {state} = {evaluating};\n\
         try {{\n\
         {execute}();\n\
         {state} = {evaluated};\n\
         return {cells};\n\
         }} catch ({caught}) {{\n\
         {error} = {caught};\n\
         {state} = {errored};\n\
         throw {caught};\n\
         }}\n}}\n\
         function {execute}() {{\n\
         {cells} = {{ __proto__: null",
        evaluate = evaluate.as_str(),
        execute = execute.as_str(),
        state = state.as_str(),
        error = error.as_str(),
        caught = caught.as_str(),
        unstarted = DeferredEvaluationState::Unstarted.word(),
        evaluating = DeferredEvaluationState::Evaluating.word(),
        evaluated = DeferredEvaluationState::Evaluated.word(),
        errored = DeferredEvaluationState::Errored.word(),
    );
    for export in exports {
        let reference = namespace_target_reference(&export.target).ok_or_else(|| {
            format!(
                "deferred export `{}` resolves to a binding the merged script cannot name",
                export.export_name.as_str()
            )
        })?;
        text.push_str(", [");
        push_js_string_literal(&mut text, export.export_name.as_str());
        text.push_str("]: () => ");
        text.push_str(reference.as_str());
    }
    text.push_str(" };\n");
    Ok(body.wrap(&text, "\n;\n}\n"))
}

/// Merged-script declaration of the cell a deferred module's export table lives
/// in.
///
/// Separate from [`deferred_body_source`] because it has to run before any
/// getter can call the thunk, while the thunk itself is a hoisted `function`
/// declaration that can sit wherever the unit's body would have gone.
#[must_use]
pub(crate) fn deferred_cells_declaration(module: ModuleUnitId) -> String {
    format!(
        "let {};\nlet {} = {};\nlet {};\n",
        MergedName::minted(module, UnitCellRole::DeferCells).as_str(),
        MergedName::minted(module, UnitCellRole::DeferState).as_str(),
        DeferredEvaluationState::Unstarted.word(),
        MergedName::minted(module, UnitCellRole::DeferError).as_str(),
    )
}

#[derive(Clone, Copy)]
enum DeferredEvaluationState {
    Unstarted,
    Evaluating,
    Evaluated,
    Errored,
}

impl DeferredEvaluationState {
    const fn word(self) -> u8 {
        match self {
            Self::Unstarted => 0,
            Self::Evaluating => 1,
            Self::Evaluated => 2,
            Self::Errored => 3,
        }
    }
}

fn namespace_unsupported(key: &str, reason: &str) -> IrDiagnostic {
    IrDiagnostic::unsupported(format!(
        "unsupported in scriptw wasm-aot: module {key}: {reason}"
    ))
}

/// `ModuleNamespaceCreate` (16.2.1.10). Idempotent per module.
///
/// Returns the storage name of the cell holding the identity-cached namespace
/// object, so repeated `import * as ns` and `import()` of the same module
/// observe the same object. Returns `None` for an invalid or source-only unit;
/// such a unit has no environment whose exports a namespace could expose.
pub(crate) fn ensure_namespace(
    graph: &mut ModuleGraphIr,
    module: ModuleUnitId,
    mode: ModuleNamespaceModeIr,
) -> Option<MergedName> {
    let cell = MergedName::minted(module, mode.cell_role());
    let materialization = graph.materialization_mode(module)?;
    let Some(index) = usize::try_from(module)
        .ok()
        .filter(|index| *index < graph.units.len())
    else {
        return None;
    };
    if graph.units[index].namespaces.contains_key(&mode) {
        return Some(cell);
    }

    let mut exports: Vec<ModuleNamespaceExportIr> = graph
        .exported_names(module)
        .into_iter()
        .filter_map(|export_name| {
            let target = graph.resolve_export(module, &export_name);
            // 16.2.1.10 step 2.a: ambiguous and unresolvable names are excluded
            // from the namespace, so they are not own properties at all. The
            // match is exhaustive over `ResolvedBindingIr` with no catch-all, so
            // a new resolution shape has to answer this question rather than
            // inherit an answer.
            match target {
                ResolvedBindingIr::Resolved { .. } => Some(ModuleNamespaceExportIr {
                    export_name,
                    target,
                }),
                ResolvedBindingIr::Ambiguous | ResolvedBindingIr::NotFound => None,
            }
        })
        .collect();
    exports.sort_by(|left, right| {
        utf16_sort_key(&left.export_name).cmp(&utf16_sort_key(&right.export_name))
    });

    let mut namespace = ModuleNamespaceIr {
        module,
        exports,
        cell: cell.clone(),
        mode,
        materialization,
        source: Ok(String::new()),
    };
    namespace.source = namespace_object_source(&namespace);
    graph.units[index].namespaces.insert(mode, namespace);
    Some(cell)
}

/// Materializes a namespace object for every module an importer or an
/// `import()` component observes.
///
/// Runs to a fixed point: `export * as ns from "m"` makes one namespace's
/// export resolve to *another* module's namespace, and that module needs an
/// object too even though nobody imports it directly.
pub(crate) fn collect_observed_namespaces(graph: &mut ModuleGraphIr) {
    let mut observed = BTreeSet::new();
    // The host may retain the entry namespace after its one evaluation. Use
    // the same canonical readers and cells as import * and import(), so reads
    // remain live without replaying initialization or copying export values.
    if !graph.entry_is_script && graph.link_errors.is_empty() {
        observed.insert((graph.entry, ModuleNamespaceModeIr::Eager));
    }
    for (_, _, unit) in graph.materialized_units() {
        // `import * as ns from "m"`, and `export * as ns from "m"` re-exported
        // onward. Both hand `m`'s `UnitCellRole::Namespace` cell to a reader, so
        // both make `m`'s namespace object observable.
        for binding in unit
            .resolved_imports
            .iter()
            .chain(unit.resolved_indirect_exports.iter())
        {
            if let ResolvedBindingIr::Resolved {
                module,
                binding: ModuleBindingNameIr::Namespace(mode),
            } = binding
            {
                observed.insert((*module, *mode));
            }
        }
    }
    for component in graph.dynamic_components() {
        // Source phase rejects without a namespace or source object. Only
        // evaluation/defer requests observe a namespace cell.
        if let Some(mode) = component.request().phase().namespace_mode() {
            observed.insert((component.target(), mode));
        }
    }
    for resolution in graph.realm_requests.values() {
        if let super::RealmModuleResolutionIr::Loaded(module) = resolution {
            observed.insert((*module, ModuleNamespaceModeIr::Eager));
        }
    }

    let mut pending: Vec<(ModuleUnitId, ModuleNamespaceModeIr)> =
        observed.iter().copied().collect();
    while let Some((module, mode)) = pending.pop() {
        let Some(_) = ensure_namespace(graph, module, mode) else {
            continue;
        };
        let Some(namespace) = usize::try_from(module)
            .ok()
            .and_then(|index| graph.units.get(index))
            .and_then(|unit| unit.namespaces.get(&mode))
        else {
            continue;
        };
        let nested: Vec<(ModuleUnitId, ModuleNamespaceModeIr)> = namespace
            .exports
            .iter()
            .filter_map(|export| match &export.target {
                ResolvedBindingIr::Resolved {
                    module,
                    binding: ModuleBindingNameIr::Namespace(mode),
                } => Some((*module, *mode)),
                ResolvedBindingIr::Resolved { .. }
                | ResolvedBindingIr::Ambiguous
                | ResolvedBindingIr::NotFound => None,
            })
            .collect();
        for module in nested {
            if observed.insert(module) {
                pending.push(module);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_of(modules: &[(&str, &str)]) -> ModuleGraphIr {
        let sources = ModuleGraphSources {
            realm_requests: Default::default(),
            modules: modules
                .iter()
                .map(|(key, text)| {
                    ModuleSourceIr::new(
                        ModuleKey::from_host(*key),
                        (*text).to_string(),
                        (*key).to_string(),
                    )
                })
                .collect(),
            entry: 0,
            resolutions: Vec::new(),
        };
        let mut graph = super::super::build_graph(&sources).expect("graph should build");
        super::super::link(&mut graph);
        graph
    }

    #[test]
    fn host_observes_only_the_actual_module_entry_namespace() {
        let mut graph = graph_of(&[
            ("entry", "export let live = 1; export default () => ++live;"),
            ("unobserved", "export const value = 2;"),
        ]);
        collect_observed_namespaces(&mut graph);
        let namespace = graph.units[0]
            .namespaces
            .get(&ModuleNamespaceModeIr::Eager)
            .expect("Module entry has its canonical eager namespace");
        assert!(namespace
            .own_property_keys()
            .contains(&&ExportName::new("live")));
        assert!(namespace
            .own_property_keys()
            .contains(&&ExportName::default_export()));
        assert!(graph.units[1].namespaces.is_empty());
        let mut script = graph_of(&[("entry", "export const value = 1;")]);
        script.entry_is_script = true;
        collect_observed_namespaces(&mut script);
        assert!(
            script.units[0].namespaces.is_empty(),
            "Script roots have no host module namespace"
        );
    }

    /// `[[OwnPropertyKeys]]` sorts by UTF-16 code unit, which differs from the
    /// UTF-8 byte order Rust's `str: Ord` gives once a name leaves the BMP —
    /// and a string-literal export name can be anything at all.
    #[test]
    fn namespace_keys_are_in_utf16_code_unit_order() {
        let graph_source = concat!(
            "const a = 1;\n",
            "export { a as '\\u{10000}' };\n",
            "export { a as '\\uFF3A' };\n",
            "export { a as 'b' };\n",
            "export { a as 'a b' };\n",
        );
        let mut graph = graph_of(&[("m", graph_source)]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let namespace = graph.units[0]
            .namespaces
            .get(&ModuleNamespaceModeIr::Eager)
            .expect("namespace should exist");

        let keys = namespace.own_property_keys();
        // U+10000 is a surrogate pair (0xD800, 0xDC00), so in UTF-16 order it
        // sorts *before* U+FF3A even though its UTF-8 bytes sort after.
        let surrogate = keys
            .iter()
            .position(|key| key.as_str().chars().next() == Some('\u{10000}'));
        let fullwidth = keys
            .iter()
            .position(|key| key.as_str().chars().next() == Some('\u{FF3A}'));
        assert!(
            surrogate < fullwidth,
            "UTF-16 order puts a surrogate pair before U+FF3A: {keys:?}"
        );
        // A space sorts before an ASCII letter.
        assert!(
            keys.iter().position(|key| key.as_str() == "a b")
                < keys.iter().position(|key| key.as_str() == "b")
        );
    }

    /// Every export of the namespace names the *exporter's own* binding, so a
    /// read is live rather than a snapshot taken when the object was created —
    /// and it is named with no `$m{unit}$` prefix, because the merged scope
    /// shares the exporter's cell with its importers by name.
    #[test]
    fn namespace_entries_point_at_the_exporter_cell() {
        let mut graph = graph_of(&[("m", "export let value = 1;")]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let namespace = graph.units[0]
            .namespaces
            .get(&ModuleNamespaceModeIr::Eager)
            .expect("namespace should exist");
        let export = namespace
            .exports
            .iter()
            .find(|export| export.export_name.as_str() == "value")
            .expect("`value` is an own key of the namespace");
        assert_eq!(
            namespace_target_reference(&export.target),
            Some(LocalName::from_bound_name("value").merged_in(0))
        );
        assert_eq!(
            namespace_target_reference(&export.target).map(|name| name.as_str().to_string()),
            Some("value".to_string())
        );
    }

    /// Identity is one cell per module: `import * as a` and `import * as b` of
    /// the same module must be the same object.
    #[test]
    fn namespace_identity_is_cached_in_one_cell() {
        let mut graph = graph_of(&[("m", "export let value = 1;")]);
        let first = ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager)
            .expect("entry materializes");
        let second = ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager)
            .expect("entry materializes");
        assert_eq!(first, second);
        assert_eq!(first, MergedName::minted(0, UnitCellRole::Namespace));
        assert_eq!(
            graph.units[0]
                .namespaces
                .get(&ModuleNamespaceModeIr::Eager)
                .map(|ns| ns.cell.clone()),
            Some(first)
        );
    }

    /// `default` is never provided by `export *`, so it is absent from a
    /// namespace assembled only through star exports.
    #[test]
    fn star_exports_do_not_contribute_default() {
        let mut graph = graph_of(&[("m", "export * from 'other';")]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let namespace = graph.units[0]
            .namespaces
            .get(&ModuleNamespaceModeIr::Eager)
            .expect("namespace should exist");
        assert!(!namespace
            .own_property_keys()
            .contains(&&ExportName::default_export()));
    }

    fn request_key(specifier: &str) -> ModuleRequestKeyIr {
        ModuleRequestKeyIr::plain(specifier)
    }

    /// A graph whose entry is the *last* module listed, which is the shape the
    /// linker produces: dependencies first, importer last.
    fn linked_graph(
        modules: &[(&str, &str)],
        resolutions: Vec<(ModuleUnitId, ModuleRequestKeyIr, ModuleUnitId)>,
    ) -> ModuleGraphIr {
        let entry = ModuleUnitId::try_from(modules.len() - 1).expect("entry index fits");
        let sources = ModuleGraphSources {
            realm_requests: Default::default(),
            modules: modules
                .iter()
                .map(|(key, text)| {
                    ModuleSourceIr::new(
                        ModuleKey::from_host(*key),
                        (*text).to_string(),
                        format!("file:///{key}"),
                    )
                })
                .collect(),
            entry,
            resolutions,
        };
        let mut graph = super::super::build_graph(&sources).expect("graph should build");
        super::super::link(&mut graph);
        collect_observed_namespaces(&mut graph);
        graph
    }

    fn source_of(graph: &ModuleGraphIr, module: ModuleUnitId) -> String {
        graph
            .unit(module)
            .namespaces
            .values()
            .next()
            .expect("namespace should exist")
            .source
            .as_ref()
            .expect("namespace should be expressible")
            .clone()
    }

    #[test]
    fn namespace_source_carries_private_live_readers_for_the_linker() {
        let mut graph = graph_of(&[("m", "export const value = 41;")]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let binding = MergedName::minted(0, UnitCellRole::Namespace);
        assert_eq!(
            source_of(&graph, 0),
            format!(
                "const {} = [void 0, \"value\", () => value];\n",
                binding.as_str()
            )
        );
    }

    /// The reader names the *exporter's own* binding, which is what makes the
    /// read live. The per-unit-environment name (`$m0$value`) belongs to a
    /// different, not-yet-built backend and must not leak into the merged
    /// source — and no longer can, since nothing mints one.
    #[test]
    fn namespace_source_reads_the_exporter_binding_rather_than_a_snapshot() {
        let mut graph = graph_of(&[("m", "export let value = 41;")]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let source = source_of(&graph, 0);

        assert!(source.contains("() => value"), "got {source}");
        // The per-unit-environment cell name (`$m0$value`) must not appear. It
        // no longer *can*: nothing in the crate produces such a name since
        // `ModuleGraphIr::cell_name` was deleted. This assertion is the
        // regression guard for that, and it is now a statement about what is
        // constructible rather than about what happened to be chosen. The
        // namespace object's own binding (`$m0$namespace`) shares the same
        // prefix by construction, so asserting the prefix is absent would be
        // unsatisfiable rather than strict.
        assert!(
            !source.contains("$m0$value"),
            "the per-unit-environment cell naming scheme must not reach the merged source: {source}"
        );
    }

    /// `export { a as b }` is the case the direct-import path cannot link, and
    /// the namespace path gets it right for free: the *key* is the export name
    /// and the *reader* names the local one.
    #[test]
    fn namespace_source_separates_the_export_name_from_the_local_name() {
        let mut graph = graph_of(&[("m", "const a = 1;\nexport { a as b };")]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let source = source_of(&graph, 0);
        assert!(source.contains("\"b\", () => a"), "got {source}");
    }

    #[test]
    fn an_empty_namespace_still_has_a_constructor_table() {
        let mut graph = graph_of(&[("m", "export {};")]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let binding = MergedName::minted(0, UnitCellRole::Namespace);
        assert_eq!(
            source_of(&graph, 0),
            format!("const {} = [void 0];\n", binding.as_str())
        );
    }

    /// Properties are defined in `exports` order, which `ensure_namespace`
    /// already sorted, so the emitted source needs no sorting of its own for
    /// `Object.keys` to come out in `[[OwnPropertyKeys]]` order.
    #[test]
    fn namespace_source_defines_keys_in_own_property_keys_order() {
        let graph_source = concat!(
            "const a = 1;\n",
            "export { a as 'b' };\n",
            "export { a as 'a b' };\n",
        );
        let mut graph = graph_of(&[("m", graph_source)]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let source = source_of(&graph, 0);
        assert!(
            source.find("\"a b\"") < source.find("\"b\""),
            "got {source}"
        );
    }

    /// An export name is an arbitrary string literal, so the generated key has
    /// to survive quotes, backslashes, newlines and astral-plane text.
    #[test]
    fn namespace_source_escapes_arbitrary_export_names() {
        let graph_source = concat!(
            "const a = 1;\n",
            "export { a as 'quote\\\" and \\\\ and \\n' };\n",
            "export { a as '\\u{10000}' };\n",
        );
        let mut graph = graph_of(&[("m", graph_source)]);
        ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager).expect("entry materializes");
        let source = source_of(&graph, 0);

        assert!(
            source.contains(r#""quote\" and \\ and \n""#),
            "got {source}"
        );
        // U+10000 is the surrogate pair D800 DC00 in UTF-16, and escaping runs
        // over code units so each half survives on its own.
        assert!(source.contains(r"\uD800\uDC00"), "got {source}");
        assert!(source.is_ascii(), "generated source stays ASCII: {source}");
    }

    fn namespace_of_one_local(module: ModuleUnitId, local: &str) -> ModuleNamespaceIr {
        ModuleNamespaceIr {
            module,
            exports: vec![ModuleNamespaceExportIr {
                export_name: ExportName::default_export(),
                target: ResolvedBindingIr::Resolved {
                    module,
                    binding: ModuleBindingNameIr::Name(LocalName::from_bound_name(local)),
                },
            }],
            cell: MergedName::minted(module, UnitCellRole::Namespace),
            mode: ModuleNamespaceModeIr::Eager,
            materialization: ModuleMaterializationModeIr::Eager,
            source: Ok(String::new()),
        }
    }

    /// `*default*` cannot be written as an `IdentifierReference`, which is why
    /// 8.2.2 chose it — so the getter reads the minted name the merged script
    /// declares in its place instead.
    #[test]
    fn the_anonymous_default_local_is_read_through_its_minted_name() {
        let namespace = namespace_of_one_local(2, MODULE_ANONYMOUS_DEFAULT_LOCAL_NAME);
        let source = namespace_object_source(&namespace).expect("`*default*` has a merged name");
        assert!(
            source.contains(&format!(
                "() => {}",
                LocalName::AnonymousDefault.merged_in(2).as_str()
            )),
            "got {source}"
        );
    }

    /// The spellability guard itself: a local name the merged script cannot
    /// write is reported rather than emitted as broken source.
    #[test]
    fn an_unspellable_local_name_is_reported_rather_than_emitted() {
        let namespace = namespace_of_one_local(0, "*not a binding*");
        let error = namespace_object_source(&namespace).expect_err("the name is unspellable");
        assert!(error.contains("default"), "got {error}");
    }

    #[test]
    fn unicode_export_readers_preserve_decoded_names_without_normalization() {
        for (spelling, decoded) in [
            ("a\u{0301}", "a\u{0301}"),
            (r"a\u0301", "a\u{0301}"),
            (r"a\u093E", "a\u{093e}"),
            ("a\u{203f}", "a\u{203f}"),
            ("\u{309b}", "\u{309b}"),
            (r"\u{088F}", "\u{088f}"),
            ("\u{11db0}", "\u{11db0}"),
            (r"a\u{1ACF}", "a\u{1acf}"),
        ] {
            let module = format!("export let {spelling} = 1;");
            let mut graph = graph_of(&[("m", &module)]);
            ensure_namespace(&mut graph, 0, ModuleNamespaceModeIr::Eager)
                .expect("entry materializes");
            let namespace = graph.units[0]
                .namespaces
                .get(&ModuleNamespaceModeIr::Eager)
                .expect("namespace exists");
            assert_eq!(namespace.exports[0].export_name.as_str(), decoded);
            let reference = namespace_target_reference(&namespace.exports[0].target)
                .expect("the lexer and namespace accept the same spelling");
            assert_eq!(reference.as_str(), decoded);
            let source = source_of(&graph, 0);
            assert!(source.contains(&format!("() => {decoded}")), "{source}");
            scriptw_front::parse(&source, scriptw_front::ParseOptions::script())
                .expect("generated reader spelling parses");
        }
    }

    #[test]
    fn unicode_namespace_aliases_use_the_lexers_spelling() {
        for local in ["n\u{0301}", "n\u{093e}", "n\u{203f}", "\u{088f}"] {
            for (import, role) in [(format!("import * as {local}"), UnitCellRole::Namespace)] {
                let entry = format!("{import} from './a.mjs'; void {local};");
                let graph = linked_graph(
                    &[("a", "export const value = 41;"), ("c", &entry)],
                    vec![(1, request_key("./a.mjs"), 0)],
                );
                let source =
                    namespace_prelude_source(&graph).expect("Unicode alias can be emitted");
                assert!(
                    source.contains(&format!(
                        "const {local} = {};",
                        MergedName::minted(0, role).as_str()
                    )),
                    "{source}"
                );
                scriptw_front::parse(&source, scriptw_front::ParseOptions::script())
                    .expect("generated alias spelling parses");
            }
        }
    }

    #[test]
    fn invalid_synthetic_names_are_not_admitted_by_alphabetic_approximations() {
        for local in ["", "\u{0345}a", "a\u{00b2}", "\u{200c}a", "a😀"] {
            let namespace = namespace_of_one_local(0, local);
            assert!(namespace_object_source(&namespace).is_err(), "{local:?}");
        }
    }

    /// The target behaviour of this lane: `import * as ns` binds a local name to
    /// the exporter's namespace object.
    #[test]
    fn the_prelude_binds_an_import_star_local_to_the_namespace_object() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                ("c", "import * as ns from \"./a.mjs\";\nprint(ns.value);"),
            ],
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let prelude = namespace_prelude_source(&graph).expect("prelude should build");

        let binding = MergedName::minted(0, UnitCellRole::Namespace);
        let binding = binding.as_str();
        assert!(
            prelude.contains(&format!("const {binding} = [void 0,")),
            "got {prelude}"
        );
        assert!(prelude.contains("() => value"), "got {prelude}");
        // The alias comes after the object it names.
        let object = prelude.find(&format!("const {binding} =")).expect("object");
        let alias = prelude
            .find(&format!("const ns = {binding};"))
            .expect("alias");
        assert!(object < alias, "got {prelude}");
    }

    /// An unobserved Script entry needs no host module namespace.
    #[test]
    fn the_prelude_is_empty_when_no_namespace_is_observed() {
        let mut graph = graph_of(&[("entry", "print(41);")]);
        graph.entry_is_script = true;
        assert_eq!(
            namespace_prelude_source(&graph).expect("prelude should build"),
            ""
        );
    }

    #[test]
    fn namespace_construction_does_not_depend_on_shadowed_globals() {
        for name in ["Object", "Symbol", "globalThis"] {
            let source =
                format!("import * as ns from './a.mjs'; const {name} = 1; print(ns.value);");
            let graph = linked_graph(
                &[("a", "export const value = 41;"), ("c", &source)],
                vec![(1, request_key("./a.mjs"), 0)],
            );
            namespace_prelude_source(&graph).expect("namespace uses trusted construction");
        }
    }

    #[test]
    fn namespace_aliases_may_use_global_constructor_names() {
        for name in ["Object", "Symbol", "globalThis"] {
            let source = format!("import * as {name} from './a.mjs'; print({name}.value);");
            let graph = linked_graph(
                &[("a", "export const value = 41;"), ("c", &source)],
                vec![(1, request_key("./a.mjs"), 0)],
            );
            let prelude = namespace_prelude_source(&graph)
                .expect("namespace alias is ordinary lexical binding");
            let binding = MergedName::minted(0, UnitCellRole::Namespace);
            assert!(prelude.contains(&format!("const {name} = {};", binding.as_str())));
        }
    }

    #[test]
    fn source_phase_rejection_precedes_any_generated_global_operation() {
        for declaration in ["const Object = 1;", "const Symbol = 1;"] {
            let source =
                format!("import source src from './a.mjs'; {declaration} print(typeof src);");
            let graph = linked_graph(
                &[("a", "export const value = 41;"), ("c", &source)],
                vec![(1, request_key("./a.mjs"), 0)],
            );
            let diagnostics =
                namespace_prelude_source(&graph).expect_err("no source representation");
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(
                diagnostics[0].code(),
                Some(EarlyErrorCode::ModuleSourceUnavailable)
            );
        }
    }

    #[test]
    fn reachable_namespace_aliases_for_the_same_cell_are_coalesced() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                ("b", "import * as ns from './a.mjs'; print(ns.value);"),
                (
                    "c",
                    "import './b.mjs'; import * as ns from './a.mjs'; print(ns.value);",
                ),
            ],
            vec![
                (1, request_key("./a.mjs"), 0),
                (2, request_key("./a.mjs"), 0),
                (2, request_key("./b.mjs"), 1),
            ],
        );
        assert_eq!(
            graph.materialization_mode(1),
            Some(ModuleMaterializationModeIr::Eager),
            "both importers must contribute aliases"
        );
        let prelude = namespace_prelude_source(&graph).expect("same namespace cell is shared");
        let namespace = MergedName::minted(0, UnitCellRole::Namespace);
        assert_eq!(
            prelude
                .matches(&format!("const ns = {};", namespace.as_str()))
                .count(),
            1,
            "the shared alias must be emitted once: {prelude}"
        );
    }

    // This graph is valid ECMAScript. The current source-text linker cannot
    // represent its distinct bindings in one merged scope, so this tests an
    // explicit implementation limitation rather than a language rejection.
    #[test]
    fn merged_source_guard_reports_distinct_namespace_cells_sharing_a_local_name() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                ("other", "export const different = 42;"),
                ("b", "import * as ns from './a.mjs'; print(ns.value);"),
                (
                    "c",
                    "import './b.mjs'; import * as ns from './other.mjs'; print(ns.different);",
                ),
            ],
            vec![
                (2, request_key("./a.mjs"), 0),
                (3, request_key("./other.mjs"), 1),
                (3, request_key("./b.mjs"), 2),
            ],
        );
        assert_eq!(
            graph.materialization_mode(2),
            Some(ModuleMaterializationModeIr::Eager),
            "the conflicting importer must be reachable"
        );
        let diagnostics = namespace_prelude_source(&graph)
            .expect_err("merged scope cannot separate distinct cells");
        assert!(diagnostics
            .iter()
            .all(|diagnostic| diagnostic.kind() == IrDiagnosticKind::Unsupported));
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("already bound by module")),
            "got {diagnostics:?}"
        );
    }

    #[test]
    fn unreachable_namespace_importers_do_not_contribute_aliases() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                (
                    "b",
                    "import './b.mjs'; import * as unreachable from './a.mjs'; print(unreachable.value);",
                ),
                ("c", "import * as ns from './a.mjs'; print(ns.value);"),
            ],
            vec![
                (1, request_key("./a.mjs"), 0),
                (1, request_key("./b.mjs"), 1),
                (2, request_key("./a.mjs"), 0),
            ],
        );
        // Units with no incoming request are embedder roots. A self-contained
        // cycle gives this importer an incoming edge without making it reachable
        // from the entry, so it must contribute no merged-scope alias.
        assert!(graph.materialization_mode(1).is_none());
        for reachable in [0, 2] {
            assert_eq!(
                graph.materialization_mode(reachable),
                Some(ModuleMaterializationModeIr::Eager),
            );
        }
        let prelude = namespace_prelude_source(&graph).expect("only reachable aliases materialize");
        let namespace = MergedName::minted(0, UnitCellRole::Namespace);
        assert!(prelude.contains(&format!("const ns = {};", namespace.as_str())));
        assert!(!prelude.contains("const unreachable ="), "got {prelude}");
    }

    /// A namespace alias also cannot collide with a name another unit declares
    /// outright — `check_linkable`'s collision map skips import bindings, so
    /// nothing else catches this.
    #[test]
    fn a_namespace_local_colliding_with_another_units_declaration_is_reported() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;\nconst ns = 1;\nprint(ns);"),
                ("c", "import * as ns from \"./a.mjs\";\nprint(ns.value);"),
            ],
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let diagnostics = namespace_prelude_source(&graph).expect_err("collision must be reported");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("collides with a top-level")),
            "got {diagnostics:?}"
        );
    }

    /// A deferred namespace's reader cannot name the exporter's binding — it is
    /// in the thunk's scope, not the merged one — so it reads through the export
    /// table the thunk publishes, and calling the thunk is what evaluates the
    /// module.
    #[test]
    fn a_deferred_namespace_reader_goes_through_the_thunk() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                (
                    "c",
                    "import defer * as ns from \"./a.mjs\";\nprint(ns.value);",
                ),
            ],
            vec![(1, request_key("./a.mjs"), 0)],
        );
        assert_eq!(graph.evaluation_mode(0), ModuleEvaluationModeIr::Deferred);
        let prelude = namespace_prelude_source(&graph).expect("prelude should build");

        assert!(
            prelude.contains(&format!(
                "let {};",
                MergedName::minted(0, UnitCellRole::DeferCells).as_str()
            )),
            "got {prelude}"
        );
        assert!(
            prelude.contains(&format!(
                "() => {}()[\"value\"]()",
                MergedName::minted(0, UnitCellRole::DeferEvaluate).as_str()
            )),
            "got {prelude}"
        );
        // The eager form would have named the exporter's binding directly.
        assert!(!prelude.contains("() => value"), "got {prelude}");
    }

    /// The thunk publishes its export table *before* running the body, so a
    /// binding still in TDZ is captured rather than read, and the key is
    /// computed so that an export named `__proto__` defines a property instead
    /// of setting the prototype.
    #[test]
    fn a_deferred_body_publishes_capturing_readers_before_it_runs() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                (
                    "c",
                    "import defer * as ns from \"./a.mjs\";\nprint(ns.value);",
                ),
            ],
            vec![(1, request_key("./a.mjs"), 0)],
        );
        let thunk = deferred_body_source(&graph, 0, "const value = 41;")
            .expect("the deferred body should be expressible");

        let cells = MergedName::minted(0, UnitCellRole::DeferCells);
        let cells = cells.as_str();
        let state = MergedName::minted(0, UnitCellRole::DeferState);
        let error = MergedName::minted(0, UnitCellRole::DeferError);
        let failure = thunk
            .find(&format!(
                "if ({} === {}) throw {};",
                state.as_str(),
                DeferredEvaluationState::Errored.word(),
                error.as_str(),
            ))
            .expect("cached failure is rethrown before returning readers");
        let readers = thunk
            .find(&format!(
                "if ({} !== {}) return {cells};",
                state.as_str(),
                DeferredEvaluationState::Unstarted.word(),
            ))
            .expect("evaluating and evaluated modules reuse their readers");
        assert!(failure < readers, "cached errors take precedence: {thunk}");
        let table = thunk
            .find("[\"value\"]: () => value")
            .expect("export table");
        let body = thunk.find("const value = 41;").expect("body");
        assert!(table < body, "the table must be published first: {thunk}");
    }

    #[test]
    fn a_source_phase_binding_has_no_namespace_or_source_cell() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                ("c", "import source src from './a.mjs'; print(typeof src);"),
            ],
            vec![(1, request_key("./a.mjs"), 0)],
        );
        assert_eq!(
            graph.evaluation_mode(0),
            ModuleEvaluationModeIr::NotEvaluated
        );
        let diagnostics = namespace_prelude_source(&graph).expect_err("source unavailable");
        assert_eq!(
            diagnostics[0].code(),
            Some(EarlyErrorCode::ModuleSourceUnavailable)
        );
        // Keep the historical selector: this SourceText record has no
        // materializable source object or namespace. A typed source binding
        // still has a reserved source-cell name; that name does not bypass the
        // ModuleSourceUnavailable rejection checked above.
        assert!(graph.units.iter().all(|unit| unit.namespaces.is_empty()));
        assert_eq!(
            namespace_target_reference(&graph.units[1].resolved_imports[0]),
            Some(MergedName::minted(0, UnitCellRole::ModuleSource))
        );
    }

    /// `export * as inner from "m"` makes one namespace's export resolve to
    /// another namespace object. The getter names that object's binding, and
    /// because getters are deferred the two declarations need no ordering.
    #[test]
    fn a_nested_namespace_export_names_the_other_namespace_binding() {
        let graph = linked_graph(
            &[
                ("a", "export const value = 41;"),
                ("b", "export * as inner from \"./a.mjs\";"),
                (
                    "c",
                    "import * as ns from \"./b.mjs\";\nprint(ns.inner.value);",
                ),
            ],
            vec![
                (1, request_key("./a.mjs"), 0),
                (2, request_key("./b.mjs"), 1),
            ],
        );
        let prelude = namespace_prelude_source(&graph).expect("prelude should build");
        assert!(
            prelude.contains(&format!(
                "\"inner\", () => {}",
                MergedName::minted(0, UnitCellRole::Namespace).as_str()
            )),
            "got {prelude}"
        );
    }
}
