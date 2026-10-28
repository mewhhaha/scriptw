//! Per-function attribution for the emitted code section.
//!
//! A Test262 failure that reads
//!
//! ```text
//! [origin:unknown] wasmtime module validation failed: Compilation error: Code for function is too large
//! ```
//!
//! names neither the function nor its size, and the compiler could not answer
//! either question: bodies were pushed straight into a bare
//! [`CodeSection`](wasm_encoder::CodeSection) as anonymous `Function` values.
//!
//! (The `[origin:unknown]` prefix is a separate thing — a `scriptw-test262`
//! `FailureOrigin`, classified from the detail text — and nothing here changes
//! it. Naming the function is the whole claim.)
//!
//! This module closes that gap by construction rather than by convention:
//!
//! * [`EmittedFunction`] is the only thing [`ModuleCode::push`] accepts, and it
//!   cannot be built without a [`FunctionIdentity`]. Its byte length is
//!   measured inside [`EmittedFunction::new`], the one place that owns the
//!   finished body, so no caller can compute or forget it.
//! * [`ModuleCode`] is the only path to a `CodeSection` in this crate.
//!   `emit.rs` no longer imports `CodeSection` at all, so emitting a body
//!   without recording who it belongs to is not expressible.
//! * [`FunctionIdentity::wasm_name`] is an exhaustive match with no `_` arm, so
//!   a new class of emitted function fails `cargo check` until it is named.
//!
//! The identity table is then spent twice. It becomes a Wasm custom `name`
//! section — wasmtime builds its per-function symbol as
//! `wasm[0]::function[N]::<name>` from exactly that section, so the same table
//! that measures a body also names it in wasmtime's own diagnostics — and it
//! feeds the `debug_dump` size report that `tests/emission/emit_golden.rs` records for
//! all 527 CLI fixtures.

use std::num::NonZeroU32;

use scriptw_ir::{FunctionId, HostBuiltinId, StandardBuiltinId};
use wasm_encoder::{CodeSection, FunctionSection, NameMap, NameSection};

// The body handed to `EmittedFunction::new` is the label-counting sink, not the
// raw encoder type: `Function::into_body` is what asserts the body closed every
// frame it opened. See `code_sink.rs`.
use crate::code_sink::Function;
use crate::module::{EmitError, StaticSignature};
use crate::runtime_helpers::RuntimeHelperId;

/// Environment variable that turns on the full per-function size report in
/// `WasmArtifact::debug_dump`. Off by default: the report is one line per
/// emitted function and there are thousands of them in a bootstrap-heavy
/// module.
pub(crate) const EMIT_SIZE_REPORT_ENV: &str = "SCRIPTW_EMIT_SIZE_REPORT";

/// Environment variable naming a file the *full* per-function size report is
/// written to, one `emitted function: key=value ...` line per emitted body.
///
/// [`EMIT_SIZE_REPORT_ENV`] only appends the report to
/// `WasmArtifact::debug_dump`, and that string has exactly one printer in the
/// workspace (`scriptw-engine`, gated on `SCRIPTW_WASM_TRACE_DUMP`) which for
/// most of this compiler's life was unreachable from `scriptw build wasm` and from
/// the Test262 wasm-aot backend. A caller could therefore set the report
/// variable, see nothing, and read that as "no large functions" rather than as
/// "the dump was dropped". This variable is honoured **inside `emit()`**, so no
/// engine or CLI caller can drop it.
///
/// One thing it cannot see: the engine's program-Wasm cache. On a cache hit
/// `emit()` is never called, so the file still holds the report of the last
/// *emission*, not of the last run. Delete the file first if that distinction
/// matters.
pub(crate) const EMIT_SIZE_REPORT_PATH_ENV: &str = "SCRIPTW_EMIT_SIZE_REPORT_PATH";

/// Environment variable holding a per-function body-size budget in bytes.
///
/// Deliberately opt-in. A budget set below the largest body Cranelift accepts
/// today turns green tests red, and the maximum across the 527 CLI fixtures
/// plus a real-suite shard has not been measured yet. Once it has, this becomes
/// a calibrated constant and the check becomes unconditional.
pub(crate) const FUNCTION_BODY_BUDGET_ENV: &str = "SCRIPTW_EMIT_FUNCTION_BODY_BUDGET_BYTES";

/// Which source-level thing a code-section entry is the body of.
///
/// Closed set on purpose: every arm of [`Self::wasm_name`] and
/// [`Self::category`] is spelled out, so a new kind of emitted function is a
/// compile error rather than an unnamed function in the `name` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FunctionIdentity {
    /// The exported `main` body: script top level plus the runtime bootstrap.
    Main,
    /// The separate linked-P owner bootstrap; never a JavaScript main.
    ProgramInitialize,
    /// A function declared by the user program.
    Script { id: FunctionId, name: String },
    /// An auxiliary primitive-only attempt, retaining its real source callable.
    BinaryNumericSource { id: FunctionId, name: String },
    /// An independently prepared Script body with its distinct typed entry.
    PreparedScript { id: FunctionId, name: String },
    /// A standard builtin compiled with its real body.
    StandardBuiltin(StandardBuiltinId),
    /// A host builtin (`print`, the `$262.agent` surface, ...).
    HostBuiltin(HostBuiltinId),
    /// One of the shared runtime helpers.
    RuntimeHelper(RuntimeHelperId),
}

impl FunctionIdentity {
    /// The symbol written into the Wasm `name` section.
    ///
    /// Prefixed by category so a failure reading
    /// `wasm[0]::function[812]::builtin::Intl.DateTimeFormat.prototype.format`
    /// says both what kind of thing blew up and which one.
    pub(crate) fn wasm_name(&self) -> String {
        match self {
            Self::Main => "scriptw::main".to_string(),
            Self::ProgramInitialize => "scriptw::program_init".to_string(),
            Self::BinaryNumericSource { id, name } => format!("js-numeric::{name}#{id}"),
            Self::Script { id, name } | Self::PreparedScript { id, name } if name.is_empty() => {
                format!("js::{id}")
            }
            Self::Script { id, name } | Self::PreparedScript { id, name } => {
                format!("js::{name}#{id}")
            }
            Self::StandardBuiltin(builtin) => format!("builtin::{}", builtin.debug_name()),
            Self::HostBuiltin(builtin) => format!("host::{}", builtin.as_str()),
            Self::RuntimeHelper(helper) => format!("helper::{}", helper.debug_name()),
        }
    }

    /// Coarse bucket used by the size report so a reader can tell at a glance
    /// whether the biggest body is user code, a builtin, or a helper.
    pub(crate) fn category(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::ProgramInitialize => "program-initialize",
            Self::Script { .. } | Self::PreparedScript { .. } => "script",
            Self::BinaryNumericSource { .. } => "numeric-callback",
            Self::StandardBuiltin(_) => "builtin",
            Self::HostBuiltin(_) => "host-builtin",
            Self::RuntimeHelper(_) => "runtime-helper",
        }
    }

    pub(crate) fn is_runtime_helper(&self) -> bool {
        matches!(self, Self::RuntimeHelper(_))
    }

    /// The declaration is derived from the same identity that owns the body.
    /// Prepared Scripts have a distinct entry shape; runtime helpers retain
    /// their existing exhaustive signature authority.
    fn type_index(&self) -> u32 {
        match self {
            Self::Main => StaticSignature::Main.type_index(),
            Self::ProgramInitialize => StaticSignature::ProgramInitialize.type_index(),
            Self::Script { .. } | Self::StandardBuiltin(_) | Self::HostBuiltin(_) => {
                StaticSignature::JavaScriptFunction.type_index()
            }
            Self::PreparedScript { .. } => StaticSignature::PreparedScript.type_index(),
            Self::BinaryNumericSource { .. } => StaticSignature::BinaryNumericCallback.type_index(),
            Self::RuntimeHelper(helper) => helper.type_index(),
        }
    }
}

/// The encoded length of one function body, in bytes, excluding the
/// variable-width size prefix the code section writes in front of it.
///
/// A newtype rather than a bare `u32` so a size cannot be swapped with a
/// function index, a budget, or a local count at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionBodySize(u32);

impl FunctionBodySize {
    /// Measures a finished body at initial emission or final import reencoding,
    /// so a recorded size always belongs to the exact encoded body beside it.
    fn of(raw_body: &[u8]) -> Self {
        Self(u32::try_from(raw_body.len()).unwrap_or(u32::MAX))
    }

    pub const fn bytes(self) -> u32 {
        self.0
    }
}

/// The number of locals a body declares.
///
/// Recorded next to the byte size because it, not the byte size, is what the
/// failure this module exists for is actually about. Cranelift raises
/// `CodeTooLarge` when `VRegAllocator::alloc` exhausts the 2,097,151 virtual
/// registers `VReg::MAX_BITS = 21` permits, and the Wasm frontend materialises
/// a value per live local at every control-flow join — so virtual-register
/// pressure grows with `locals x blocks`, which a byte count alone can hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionLocalCount(u32);

impl FunctionLocalCount {
    /// Measured from the function-owned complete mixed declaration before
    /// encoding; concrete GC reference types need no separate byte decoder.
    fn of(body: &Function) -> Self {
        Self(body.allocated_local_count())
    }

    pub const fn count(self) -> u32 {
        self.0
    }
}

/// One emitted function, as the size report sees it.
///
/// This is the typed form of a `report_lines` row, and it is the *only* form:
/// `WasmArtifact::function_sizes`, the `largest emitted function:` line, the
/// `most locals in an emitted function:` line and the full opt-in report are
/// all rendered from one [`ModuleFunctionTable::summaries`] call, so they
/// cannot disagree with each other. The precedent for insisting on that is the
/// `runtime helper functions: 27` literal in `debug_dump`, which had drifted to
/// a counted truth of 32 + 1 precisely because it was a second copy.
///
/// [`Self::category`] is `&'static str` sourced from
/// [`FunctionIdentity::category`], an exhaustive match with no `_` arm, so a new
/// class of emitted function fails to build until it is named — the report
/// cannot silently acquire an `other` bucket.
///
/// The two size figures keep the newtypes [`EmittedFunctionRecord`] stores them
/// in rather than being flattened to `u32` on the way out. This row exists to
/// carry *two* numbers that are both "a count about one body" and that must
/// never be confused — the whole point of reporting `most_locals` separately
/// from `largest` is that they answer different questions — so
/// `EmittedFunctionSummary { body_bytes: locals, declared_locals: bytes }` has
/// to be a type error rather than a plausible transcription slip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedFunctionSummary {
    pub wasm_index: u32,
    pub name: String,
    pub category: &'static str,
    pub body_bytes: FunctionBodySize,
    /// Exact local count from the same owned body that produced its bytes.
    pub declared_locals: FunctionLocalCount,
}

impl EmittedFunctionSummary {
    /// The `key=value` field list shared by every line of the size report.
    ///
    /// `name=` is **last** on purpose: emitted names contain spaces
    /// (`get Object.prototype.__proto__`, `Array Iterator.prototype.next`), so
    /// a positional layout could not be parsed back. `tests/emission/emit_golden.rs`
    /// parses exactly these keys out of `debug_dump`.
    fn fields(&self) -> String {
        format!(
            "index={} bytes={} locals={} kind={} name={}",
            self.wasm_index,
            self.body_bytes.bytes(),
            self.declared_locals.count(),
            self.category,
            self.name
        )
    }

    /// The largest body among `summaries`.
    ///
    /// Iterates in code-section order and keeps the *last* maximum, which is
    /// what `Iterator::max_by_key` does; the ordering is load-bearing only in
    /// that it must not change, because `debug_dump` is a golden artifact
    /// (rung G) and a re-ordered tie would show up as a spurious diff.
    pub(crate) fn largest(summaries: &[Self]) -> Option<&Self> {
        summaries.iter().max_by_key(|summary| summary.body_bytes)
    }

    /// The body declaring the most locals.
    ///
    /// Reported separately from [`Self::largest`] because they are often
    /// different functions and it is this one that predicts Cranelift's
    /// `CodeTooLarge`: the Wasm frontend materialises a value per live local at
    /// every control-flow join, so virtual-register pressure tracks
    /// `locals x blocks` rather than encoded size.
    ///
    /// Every body carries its measured mixed declaration count.
    pub(crate) fn most_locals(summaries: &[Self]) -> Option<&Self> {
        summaries
            .iter()
            .max_by_key(|summary| summary.declared_locals)
    }

    /// One line per function, largest first, for the opt-in size report.
    pub(crate) fn report_lines(summaries: &[Self], limit: usize) -> Vec<String> {
        let mut ranked = summaries.iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .body_bytes
                .cmp(&left.body_bytes)
                .then(left.wasm_index.cmp(&right.wasm_index))
        });
        ranked
            .into_iter()
            .take(limit)
            .map(|summary| format!("emitted function: {}", summary.fields()))
            .collect()
    }

    /// The `largest emitted function:` / `most locals in an emitted function:`
    /// line body, or the `none` fallback when nothing was emitted.
    pub(crate) fn attribution_line(prefix: &str, summary: Option<&Self>) -> String {
        match summary {
            Some(summary) => format!("{prefix}: {}", summary.fields()),
            None => format!("{prefix}: none"),
        }
    }
}

impl core::fmt::Display for FunctionBodySize {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} bytes", self.0)
    }
}

/// A finished function body together with the identity it was compiled for and
/// its measured size.
///
/// There is no way to build one without an identity, and no way to get a body
/// into the code section without building one.
#[derive(Debug, Clone)]
enum FunctionDeclaration {
    NonCallable,
    ProgramInitialize(crate::function_layout::ProgramInitializeIndex),
    Planned(crate::function_entry::PlannedFunctionEntry),
    BinaryNumeric(crate::function_entry::PlannedBinaryNumericEntry),
}

#[derive(Debug, Clone)]
pub(crate) struct EmittedFunction {
    declaration: FunctionDeclaration,
    identity: FunctionIdentity,
    /// The body in the form `code_sink::Function::into_body().into_raw_body()`
    /// produces: the local
    /// declaration followed by the instruction bytes, with no length prefix.
    /// `CodeSection::raw` prepends the same length prefix `CodeSection::function`
    /// would, so this is byte-identical to pushing the `Function` itself, and it
    /// keeps attribution bound to the exact encoded body.
    raw_body: Vec<u8>,
    literal_operands: Vec<crate::program_literal_operands::FunctionSite>,
    native_program_context: Option<crate::native_program_contexts::FunctionContext>,
    body_bytes: FunctionBodySize,
    declared_locals: FunctionLocalCount,
}

impl EmittedFunction {
    pub(crate) fn binary_numeric(
        entry: crate::function_entry::PlannedBinaryNumericEntry,
        schema: &crate::gc_types::RuntimeSchema,
    ) -> Self {
        let body = entry.compile_body(schema);
        let mut function = Self::new(entry.identity(), body);
        function.declaration = FunctionDeclaration::BinaryNumeric(entry);
        function
    }
    pub(crate) fn main(body: Function) -> Self {
        Self::new(FunctionIdentity::Main, body)
    }
    pub(crate) fn program_initialize(body: crate::emit::CompletedProgramInitializeBody) -> Self {
        let (index, body) = body.into_parts();
        let mut function = Self::new(FunctionIdentity::ProgramInitialize, body);
        function.declaration = FunctionDeclaration::ProgramInitialize(index);
        function
    }
    pub(crate) fn runtime_helper(helper: RuntimeHelperId, body: Function) -> Self {
        Self::new(FunctionIdentity::RuntimeHelper(helper), body)
    }
    pub(crate) fn from_completed_callable(body: crate::emit::CompletedCallableBody) -> Self {
        let (entry, body) = body.into_parts();
        let mut function = Self::new(entry.identity(), body);
        function.declaration = FunctionDeclaration::Planned(entry);
        function
    }

    fn new(identity: FunctionIdentity, body: Function) -> Self {
        // `into_body_named` asserts the label stack is empty, so an emitter
        // that opened a frame and never closed it fails here — with the
        // identity in the panic message, not merely in scope — instead of as
        // an anonymous wasmtime validation error inside whichever Test262 case
        // happened to compile it.
        let declared_locals = FunctionLocalCount::of(&body);
        let literal_operands = body.literal_operands().to_vec();
        let native_program_context = body.native_program_context();
        let raw_body = body.into_body_named(&identity.wasm_name()).into_raw_body();
        let body_bytes = FunctionBodySize::of(&raw_body);
        Self {
            declaration: FunctionDeclaration::NonCallable,
            identity,
            raw_body,
            literal_operands,
            native_program_context,
            body_bytes,
            declared_locals,
        }
    }
}

/// One row of the emitted-function table.
#[derive(Debug, Clone)]
pub(crate) struct EmittedFunctionRecord {
    pub(crate) wasm_index: u32,
    pub(crate) identity: FunctionIdentity,
    pub(crate) body_bytes: FunctionBodySize,
    /// Exact local count from the same owned body that produced its bytes.
    pub(crate) declared_locals: FunctionLocalCount,
}

/// Function declarations and bodies under construction, plus attribution.
///
/// Both encoder sections are private. Each push derives the declaration from
/// the body identity and appends both exactly once, so a body cannot lose its
/// declaration or be paired with a separately assembled signature list.
pub(crate) struct ModuleCode {
    functions: FunctionSection,
    section: CodeSection,
    records: Vec<EmittedFunctionRecord>,
    literal_operands: Vec<crate::program_literal_operands::ModuleSite>,
    native_program_contexts: Vec<crate::native_program_contexts::ModuleContext>,
    type_indices: Vec<u32>,
    next_wasm_index: u32,
}

impl ModuleCode {
    /// `first_wasm_index` is the Wasm function index the first pushed body will
    /// occupy, i.e. the imported function count (imports come first in the
    /// index space).
    pub(crate) fn new(first_wasm_index: u32) -> Self {
        Self {
            functions: FunctionSection::new(),
            section: CodeSection::new(),
            records: Vec::new(),
            literal_operands: Vec::new(),
            native_program_contexts: Vec::new(),
            type_indices: Vec::new(),
            next_wasm_index: first_wasm_index,
        }
    }

    pub(crate) fn push(&mut self, function: EmittedFunction) {
        let wasm_index = self.next_wasm_index;
        self.next_wasm_index += 1;
        let type_index = match &function.declaration {
            FunctionDeclaration::NonCallable => function.identity.type_index(),
            FunctionDeclaration::ProgramInitialize(index) => {
                index.assert_body_index(wasm_index);
                StaticSignature::ProgramInitialize.type_index()
            }
            FunctionDeclaration::Planned(entry) => {
                entry.assert_body_index(wasm_index);
                entry.signature().type_index()
            }
            FunctionDeclaration::BinaryNumeric(entry) => {
                entry.assert_body_index(wasm_index);
                StaticSignature::BinaryNumericCallback.type_index()
            }
        };
        self.functions.function(type_index);
        self.type_indices.push(type_index);
        self.section.raw(&function.raw_body);
        let body = u32::try_from(self.records.len()).expect("code body count overflow");
        if let Some(context) = function.native_program_context {
            self.native_program_contexts
                .push(crate::native_program_contexts::ModuleContext { body, context });
        }
        self.literal_operands.extend(
            function
                .literal_operands
                .into_iter()
                .map(|site| crate::program_literal_operands::ModuleSite::new(body, site)),
        );
        self.records.push(EmittedFunctionRecord {
            wasm_index,
            identity: function.identity,
            body_bytes: function.body_bytes,
            declared_locals: function.declared_locals,
        });
    }

    pub(crate) fn finish(self) -> (FunctionSection, CodeSection, ModuleFunctionTable) {
        (
            self.functions,
            self.section,
            ModuleFunctionTable {
                records: self.records,
                literal_operands: self.literal_operands,
                native_program_contexts: self.native_program_contexts,
                type_indices: self.type_indices,
            },
        )
    }
}

/// Every emitted function, in code-section order, with its identity and size.
#[derive(Debug, Clone)]
pub(crate) struct ModuleFunctionTable {
    records: Vec<EmittedFunctionRecord>,
    literal_operands: Vec<crate::program_literal_operands::ModuleSite>,
    native_program_contexts: Vec<crate::native_program_contexts::ModuleContext>,
    type_indices: Vec<u32>,
}

impl ModuleFunctionTable {
    /// Import compaction preserves every body/local but may shorten structural
    /// LEB operands. Bind attribution to the actual final body bytes and indices.
    pub(crate) fn remeasure_reencoded(
        &mut self,
        bytes: &[u8],
        mut remap: impl FnMut(u32) -> Result<u32, EmitError>,
    ) -> Result<(), EmitError> {
        let fail = |message: String| {
            EmitError::unsupported(format!(
                "compiler function attribution reencoding failed: {message}"
            ))
        };
        let mut records = self.records.iter_mut();
        for payload in wasmparser::Parser::new(0).parse_all(bytes) {
            if let wasmparser::Payload::CodeSectionEntry(body) =
                payload.map_err(|e| fail(e.to_string()))?
            {
                let record = records
                    .next()
                    .ok_or_else(|| fail("extra final body".into()))?;
                let mut locals = 0u32;
                for local in body.get_locals_reader().map_err(|e| fail(e.to_string()))? {
                    let (count, _) = local.map_err(|e| fail(e.to_string()))?;
                    locals = locals
                        .checked_add(count)
                        .ok_or_else(|| fail("local count overflow".into()))?;
                }
                if locals != record.declared_locals.count() {
                    return Err(fail("final body changed declared local count".into()));
                }
                record.wasm_index = remap(record.wasm_index)?;
                record.body_bytes = FunctionBodySize::of(&bytes[body.range()]);
            }
        }
        if records.next().is_some() {
            return Err(fail("missing final body".into()));
        }
        Ok(())
    }

    pub(crate) fn native_program_context_section(
        &self,
    ) -> Option<wasm_encoder::CustomSection<'static>> {
        crate::native_program_contexts::section(&self.native_program_contexts)
    }
    pub(crate) fn literal_operand_section(&self) -> Option<wasm_encoder::CustomSection<'static>> {
        crate::program_literal_operands::section(&self.literal_operands)
    }

    pub(crate) fn records(&self) -> &[EmittedFunctionRecord] {
        &self.records
    }

    /// The type index of each body, in code-section order.
    pub(crate) fn type_indices(&self) -> &[u32] {
        &self.type_indices
    }

    /// The Wasm custom `name` section for this module.
    ///
    /// Indices are appended in ascending order, which the encoding requires;
    /// `ModuleCode::push` hands out indices monotonically, so iteration order
    /// is already ascending.
    pub(crate) fn name_section(&self) -> NameSection {
        let mut names = NameSection::new();
        names.module("scriptw");
        let mut functions = NameMap::new();
        for record in &self.records {
            functions.append(record.wasm_index, &record.identity.wasm_name());
        }
        names.functions(&functions);
        names
    }

    /// The one traversal.
    ///
    /// Every size figure the compiler reports about itself — the typed
    /// `WasmArtifact::function_sizes`, the two `debug_dump` attribution lines
    /// and the opt-in full report — is derived from the slice this returns.
    /// There is deliberately no second accessor that walks `records` for the
    /// same question: two traversals is how the `runtime helper functions: 27`
    /// literal came to disagree with the code section by five.
    ///
    /// Order is code-section order, not size order. `report_lines` sorts a
    /// borrowed view when it needs a ranking, so the stored order stays the one
    /// that makes `wasm_index` monotonic.
    pub(crate) fn summaries(&self) -> Vec<EmittedFunctionSummary> {
        self.records
            .iter()
            .map(|record| EmittedFunctionSummary {
                wasm_index: record.wasm_index,
                name: record.identity.wasm_name(),
                category: record.identity.category(),
                body_bytes: record.body_bytes,
                declared_locals: record.declared_locals,
            })
            .collect()
    }

    pub(crate) fn total_body_bytes(&self) -> u64 {
        self.records
            .iter()
            .map(|record| u64::from(record.body_bytes.bytes()))
            .sum()
    }

    /// Number of runtime-helper bodies actually written, derived from the same
    /// table the bodies were pushed into. The `debug_dump` line this replaces
    /// was a hand-maintained literal `27` against a counted truth of 32 + 1.
    pub(crate) fn runtime_helper_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| record.identity.is_runtime_helper())
            .count()
    }

    /// Fails the emission when any body exceeds `budget`.
    ///
    /// Reports the *largest* offender rather than the first, so a single run
    /// names the function worth attacking.
    pub(crate) fn check_budget(&self, budget: FunctionBodyBudget) -> Result<(), EmitError> {
        let Some(worst) = self
            .records
            .iter()
            .filter(|record| budget.is_exceeded_by(record.body_bytes))
            .max_by_key(|record| record.body_bytes)
        else {
            return Ok(());
        };
        Err(EmitError::function_too_large(
            &worst.identity,
            worst.body_bytes,
            budget,
        ))
    }
}

/// A per-function body-size ceiling, in bytes.
///
/// Built once from a validated value and passed as this type thereafter, so
/// there is no bare `u32` threshold to thread through call sites and no way to
/// express a zero budget that would fail every module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FunctionBodyBudget(NonZeroU32);

impl FunctionBodyBudget {
    pub(crate) const fn from_bytes(bytes: NonZeroU32) -> Self {
        Self(bytes)
    }

    /// Reads the budget from [`FUNCTION_BODY_BUDGET_ENV`]. A missing, empty,
    /// unparseable or zero value means "no budget", which is the default: this
    /// lane lands the check in report-only form until the maximum body size
    /// across the fixture corpus has been measured.
    pub(crate) fn from_env() -> Option<Self> {
        let raw = std::env::var(FUNCTION_BODY_BUDGET_ENV).ok()?;
        let bytes = raw.trim().parse::<u32>().ok()?;
        NonZeroU32::new(bytes).map(Self::from_bytes)
    }

    pub(crate) const fn bytes(self) -> u32 {
        self.0.get()
    }

    const fn is_exceeded_by(self, size: FunctionBodySize) -> bool {
        size.bytes() > self.0.get()
    }
}

impl core::fmt::Display for FunctionBodyBudget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} bytes", self.bytes())
    }
}

/// Whether the opt-in full size report is requested.
pub(crate) fn emit_size_report_requested() -> bool {
    std::env::var_os(EMIT_SIZE_REPORT_ENV).is_some_and(|value| !value.is_empty())
}

/// Writes the full per-function size report to the file named by
/// [`EMIT_SIZE_REPORT_PATH_ENV`], if that variable is set to a non-empty value.
///
/// Called from `emit()` itself rather than from any caller, which is the whole
/// point: the `debug_dump` route has one printer several layers up and has been
/// silently dropped on the two paths that matter most (`scriptw build wasm` and the
/// Test262 wasm-aot backend). A failing write panics rather than being ignored —
/// an empty or absent report file is exactly the ambiguity this exists to
/// remove.
///
/// The env read is the *only* thing this wrapper does; the report text and the
/// write are [`write_size_report_file`], which is what
/// `the_size_report_file_is_the_same_traversal_as_the_typed_report` exercises.
/// The split exists because a test that sets `EMIT_SIZE_REPORT_PATH_ENV` would
/// be visible to every other test in the process, and the sink is the one part
/// of this module that was previously protected by review alone.
///
/// One hazard this deliberately does not hide: the variable names a single
/// path, so N concurrent compilations (the Test262 wasm-aot runner's workers,
/// for instance) all write the same file and the survivor is whichever finished
/// last. Give each worker its own path, or read the file as "some module's
/// report", never "this module's report".
pub(crate) fn write_size_report_file_if_requested(summaries: &[EmittedFunctionSummary]) {
    let Some(path) = std::env::var_os(EMIT_SIZE_REPORT_PATH_ENV) else {
        return;
    };
    if path.is_empty() {
        return;
    }
    write_size_report_file(path.as_ref(), summaries);
}

/// Renders and writes the report. Separated from the env read so it is directly
/// testable; see [`write_size_report_file_if_requested`].
pub(crate) fn write_size_report_file(path: &std::path::Path, summaries: &[EmittedFunctionSummary]) {
    let mut report = EmittedFunctionSummary::report_lines(summaries, usize::MAX).join("\n");
    report.push('\n');
    std::fs::write(path, report).unwrap_or_else(|err| {
        panic!("failed to write the {EMIT_SIZE_REPORT_PATH_ENV} report to {path:?}: {err}")
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalDeclarations;
    use wasm_encoder::{Instruction, ValType};

    fn body(instructions: usize) -> Function {
        let mut function = Function::new(LocalDeclarations::from_runs([(0, ValType::I64)]));
        for _ in 0..instructions {
            function.instruction(&Instruction::I64Const(0));
            function.instruction(&Instruction::Drop);
        }
        function.instruction(&Instruction::End);
        function
    }

    #[test]
    fn recorded_size_is_the_size_of_the_recorded_body() {
        let small = body(1);
        let expected = small.byte_len() as u32;
        let emitted = EmittedFunction::new(FunctionIdentity::Main, small);
        assert_eq!(emitted.body_bytes.bytes(), expected);
        assert_eq!(emitted.identity, FunctionIdentity::Main);
    }

    #[test]
    fn indices_follow_the_imported_function_count() {
        let mut code = ModuleCode::new(3);
        code.push(EmittedFunction::new(FunctionIdentity::Main, body(1)));
        code.push(EmittedFunction::new(
            FunctionIdentity::RuntimeHelper(RuntimeHelperId::TransientByteAlloc),
            body(2),
        ));
        let (_, _, table) = code.finish();
        let indices = table
            .records()
            .iter()
            .map(|record| record.wasm_index)
            .collect::<Vec<_>>();
        assert_eq!(indices, vec![3, 4]);
        assert_eq!(table.runtime_helper_count(), 1);
    }

    #[test]
    fn largest_is_the_largest_body() {
        let mut code = ModuleCode::new(0);
        code.push(EmittedFunction::new(FunctionIdentity::Main, body(1)));
        code.push(EmittedFunction::new(
            FunctionIdentity::RuntimeHelper(RuntimeHelperId::ObjectRead),
            body(64),
        ));
        let (_, _, table) = code.finish();
        let summaries = table.summaries();
        let largest =
            EmittedFunctionSummary::largest(&summaries).expect("table should not be empty");
        assert_eq!(largest.name, "helper::object_read");
        assert_eq!(largest.category, "runtime-helper");
        assert!(table.total_body_bytes() >= u64::from(largest.body_bytes.bytes()));
    }

    /// The typed report and the rendered report are the same traversal, so a
    /// row can never appear in one and not the other, and the `largest`
    /// attribution line can never name a function the table does not hold.
    #[test]
    fn the_report_and_the_typed_summaries_are_one_traversal() {
        let mut code = ModuleCode::new(0);
        code.push(EmittedFunction::new(FunctionIdentity::Main, body(1)));
        code.push(EmittedFunction::new(
            FunctionIdentity::RuntimeHelper(RuntimeHelperId::ObjectRead),
            body(64),
        ));
        code.push(EmittedFunction::new(
            FunctionIdentity::HostBuiltin(HostBuiltinId::Print),
            body(8),
        ));
        let (_, _, table) = code.finish();
        let summaries = table.summaries();
        assert_eq!(summaries.len(), table.records().len());
        assert_eq!(
            summaries
                .iter()
                .map(|summary| summary.wasm_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2],
            "summaries stay in code-section order"
        );

        let lines = EmittedFunctionSummary::report_lines(&summaries, usize::MAX);
        assert_eq!(lines.len(), summaries.len());
        let largest =
            EmittedFunctionSummary::largest(&summaries).expect("table should not be empty");
        assert!(
            lines[0].contains(&format!("name={}", largest.name)),
            "the first report row and `largest` must be the same function: {lines:?}"
        );
        assert_eq!(
            EmittedFunctionSummary::attribution_line("largest emitted function", Some(largest)),
            format!(
                "largest emitted function: {}",
                &lines[0]["emitted function: ".len()..]
            ),
        );
        assert_eq!(
            EmittedFunctionSummary::attribution_line("largest emitted function", None),
            "largest emitted function: none"
        );
    }

    #[test]
    fn budget_failure_names_the_function() {
        let mut code = ModuleCode::new(0);
        code.push(EmittedFunction::new(FunctionIdentity::Main, body(256)));
        let (_, _, table) = code.finish();
        let budget = FunctionBodyBudget::from_bytes(NonZeroU32::new(8).expect("nonzero"));
        let error = table
            .check_budget(budget)
            .expect_err("a 256-instruction body should exceed an 8 byte budget");
        let message = error.to_string();
        assert!(message.contains("scriptw::main"), "{message}");
        assert!(message.contains('8'), "{message}");
    }

    #[test]
    fn a_body_at_the_budget_is_accepted() {
        let mut code = ModuleCode::new(0);
        let single = body(1);
        let size = single.byte_len() as u32;
        code.push(EmittedFunction::new(FunctionIdentity::Main, single));
        let (_, _, table) = code.finish();
        let budget = FunctionBodyBudget::from_bytes(NonZeroU32::new(size).expect("nonzero"));
        assert!(table.check_budget(budget).is_ok());
    }

    #[test]
    fn declared_locals_follow_the_complete_mixed_body_owner() {
        let mut function = Function::new(LocalDeclarations::from_runs([
            (7, ValType::I64),
            (3, ValType::I32),
            (2, ValType::Ref(wasm_encoder::RefType::EQREF)),
        ]));
        function.reserve_typed_local(ValType::F64);
        function.instruction(&Instruction::End);
        let emitted = EmittedFunction::new(FunctionIdentity::Main, function);
        assert_eq!(emitted.declared_locals.count(), 13);
    }

    #[test]
    fn a_body_with_no_locals_declares_none() {
        let mut function = Function::new(LocalDeclarations::from_runs([]));
        function.instruction(&Instruction::End);
        let emitted = EmittedFunction::new(FunctionIdentity::Main, function);
        assert_eq!(emitted.declared_locals.count(), 0);
    }

    #[test]
    fn every_identity_kind_produces_a_distinct_name() {
        let names = [
            FunctionIdentity::Main,
            FunctionIdentity::Script {
                id: "fn0".to_string(),
                name: "outer".to_string(),
            },
            FunctionIdentity::PreparedScript {
                id: "prepared0".to_string(),
                name: "prepared".to_string(),
            },
            FunctionIdentity::BinaryNumericSource {
                id: "fn0".to_string(),
                name: "outer".to_string(),
            },
            FunctionIdentity::StandardBuiltin(StandardBuiltinId::MathMax),
            FunctionIdentity::HostBuiltin(HostBuiltinId::Print),
            FunctionIdentity::RuntimeHelper(RuntimeHelperId::TransientByteAlloc),
        ]
        .iter()
        .map(FunctionIdentity::wasm_name)
        .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn an_anonymous_script_function_still_gets_a_name() {
        let identity = FunctionIdentity::Script {
            id: "fn7".to_string(),
            name: String::new(),
        };
        assert_eq!(identity.wasm_name(), "js::fn7");
    }
}

#[cfg(test)]
mod program_initialize_tests;
