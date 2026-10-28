use super::*;
mod global_var_declarations;
mod program_metadata;
pub(crate) use global_var_declarations::{GlobalVarDeclarationNames, GlobalVarDeclarationTables};
mod shared_pool_storage;
pub(crate) use shared_pool_storage::PoolBytes;
use shared_pool_storage::PoolStringRefs;
mod compiler_owned_pool;
mod runtime_string_descriptors;
pub(crate) use program_metadata::{
    PooledLiteralOperand, ProgramMetadataSpan, POOLED_LITERAL_PROGRAM_DOMAIN,
    POOLED_LITERAL_RUNTIME_DOMAIN,
};
pub(crate) use runtime_string_descriptors::RuntimeStringDescriptors;
mod pooled_literal_domain;
pub(crate) use pooled_literal_domain::PooledLiteralDomain;
mod program_string_descriptors;
pub(crate) use program_string_descriptors::{
    ProgramStringDescriptorImage, ProgramStringDescriptorWord,
};
use scriptw_ir::{ArrayAccumulationElementIr, ValidatedRegExpProgram};
use scriptw_ir::{
    ObjectDestructuringPatternIr, OptionalChainOperationIr, RegExpCaseFolding,
    RegExpCompileErrorKind, RegExpProgram, ResumableLoopIterationEnvironmentIr,
    StaticRegExpCompilation, BUILTIN_REGEXP_FUNCTION_ID,
    BUILTIN_REGEXP_PROTOTYPE_COMPILE_FUNCTION_ID, REALM_EVAL_SCRIPT_METHOD_NAME,
    REGEXP_BACKREFERENCE_IGNORE_CASE, REGEXP_OPCODE_NAMED_BACKREFERENCE,
    REGEXP_OPCODE_NUMBERED_BACKREFERENCE,
};

mod temporal_east_asian_years;
pub(crate) use temporal_east_asian_years::{
    TemporalEastAsianCalendar, TemporalEastAsianYearImage, TemporalEastAsianYearRegion,
    TemporalEastAsianYearRowSlot, EAST_ASIAN_LEAP_ORDINAL_SHIFT,
    EAST_ASIAN_MEAN_LUNAR_MONTH_MILLIS, EAST_ASIAN_MEAN_SOLAR_TERM_MILLIS,
    EAST_ASIAN_MEAN_YEAR_MILLIS, EAST_ASIAN_MILLIS_PER_DAY, EAST_ASIAN_MONTH_MASK,
    EAST_ASIAN_NEW_YEAR_OFFSET_SHIFT, EAST_ASIAN_YEAR_ROW_BYTES, TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE,
};

mod regexp_unicode_properties;
pub(crate) use regexp_unicode_properties::{
    RegExpUnicodePropertyImage, RegExpUnicodePropertyImageKind,
    RegExpUnicodePropertyImageStringWord, RegExpUnicodePropertyImageWord,
    RegExpUnicodePropertyRegion,
};

mod intl_supported_values;
use intl_supported_values::{CompiledSupportedValuesTables, SupportedValuesPoolError};

mod compact_array_recipes;
pub(crate) use compact_array_recipes::{
    CompactArrayNode, CompactArrayOpcode, CompactArrayPlan, CompactArrayRecipe,
    COMPACT_ARRAY_RECIPE_ROW_BYTES, COMPACT_ARRAY_RECIPE_SEGMENT,
};

mod normalization;
mod unicode_case_tables;
use normalization::NormalizationMapping;

pub(crate) const UNHANDLED_REJECTION_TOSTRING_THROWN_MESSAGE: &str =
    "unhandled rejection diagnostic ToString threw";

mod runtime_error_message;
pub(crate) use runtime_error_message::{
    IntlErrorOption, RuntimeErrorMessage, SourceRuntimeErrorMessage,
};

/// Passive segment roles are distinct even though P and R both own UTF-16
/// segment zero. The other cold images exist only in R's sealed static image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PassiveDataSegment {
    PooledCodeUnits,
    Normalization,
    RegExpUnicodeProperties,
    TemporalEastAsianYears,
}
impl PassiveDataSegment {
    pub(crate) const fn index(self) -> u32 {
        match self {
            Self::PooledCodeUnits => 0,
            Self::Normalization => 1,
            Self::RegExpUnicodeProperties => 2,
            Self::TemporalEastAsianYears => 3,
        }
    }
}

pub(crate) const RUNTIME_DATA_SEGMENT_COUNT: usize = 8;
/// R's static arena in virtual-address order, excluding its UTF-16 segment.
pub(crate) const RUNTIME_STATIC_SEGMENT_ORDER: [usize; 7] = [4, 2, 5, 1, 6, 3, 7];

/// The exact virtual address range reserved by the sealed normalization group.
/// Bytes remain in the canonical arena for address/identity admission but R
/// installs them only on the first valid normalization operation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NormalizationRegion {
    start: u32,
    end: u32,
}
impl NormalizationRegion {
    pub(crate) fn start(self) -> u32 {
        self.start
    }
    pub(crate) fn end(self) -> u32 {
        self.end
    }
    pub(crate) fn length(self) -> u32 {
        self.end - self.start
    }
    pub(crate) const fn segment(self) -> PassiveDataSegment {
        PassiveDataSegment::Normalization
    }
    fn offsets(self) -> std::ops::Range<usize> {
        (self.start - STATIC_DATA_OFFSET) as usize..(self.end - STATIC_DATA_OFFSET) as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PooledStringIndex(u32);
impl PooledStringIndex {
    fn for_insertion(existing: usize) -> Self {
        let next_length = existing
            .checked_add(1)
            .and_then(|length| u32::try_from(length).ok())
            .expect("pooled strings fit a GC array");
        Self(next_length - 1)
    }

    pub(crate) const fn ordinal(self) -> u32 {
        self.0
    }
}

#[derive(Debug)]
struct StringRef {
    // Interning assigns this once. Source literals cannot renumber an already
    // collected builtin literal merely by sorting before it.
    index: PooledStringIndex,
    offset: u32,
    len: u32,
    code_units: PooledCodeUnits,
}

/// A slice of the module's compiler-owned passive UTF-16 image. Only string
/// interning creates these bounds; GC construction cannot accept raw offsets.
#[derive(Debug)]
pub(crate) struct PooledCodeUnits {
    byte_offset: u32,
    length: u32,
}

/// Exact raw module data in the compiler-owned static image. Only collection
/// creates these bounds; emission cannot substitute a text encoding.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PooledModuleBytes {
    address: u32,
    length: u32,
}
impl PooledModuleBytes {
    pub(crate) fn metadata_span(
        self,
        strings: &StringPool,
    ) -> Result<ProgramMetadataSpan, EmitError> {
        ProgramMetadataSpan::for_pool(strings, self.address, self.length)
    }
    pub(crate) const fn length(self) -> u32 {
        self.length
    }
}

impl PooledCodeUnits {
    pub(crate) const DATA_SEGMENT: PassiveDataSegment = PassiveDataSegment::PooledCodeUnits;
}

/// Compile-time storage locator for a validated descriptor. Executable P rows
/// translate this into an explicitly Program-relative metadata span.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RegExpProgramRef {
    payload: u64,
}

impl RegExpProgramRef {
    fn new(pointer: u32, program: &ValidatedRegExpProgram) -> Self {
        let length = u32::try_from(program.bytes().len()).expect("validated program length");
        pointer
            .checked_add(length)
            .expect("RegExp descriptor must fit linear memory");
        Self {
            payload: ((pointer as u64) << 32) | length as u64,
        }
    }
    pub(crate) const fn payload(self) -> u64 {
        self.payload
    }
    pub(crate) fn metadata_span(
        self,
        strings: &StringPool,
    ) -> Result<ProgramMetadataSpan, EmitError> {
        ProgramMetadataSpan::for_pool(strings, (self.payload >> 32) as u32, self.payload as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RegExpProgramStaticKey(ValidatedRegExpProgram);

impl RegExpProgramStaticKey {
    pub(crate) fn from_program(program: &RegExpProgram) -> Self {
        let descriptor = ValidatedRegExpProgram::from_program(program)
            .expect("compiler-created RegExp program must validate");
        Self(
            ValidatedRegExpProgram::from_bytes(descriptor.bytes().to_vec())
                .expect("serialized RegExp descriptor must validate"),
        )
    }
}

/// Word index of a source's tagged R/P-relative literal operand in a candidate record.
/// Writer and reader share word indices, byte offsets and derived row stride.
pub(crate) const RUNTIME_REGEXP_RECORD_SOURCE_WORD: usize = 0;
/// See [`RUNTIME_REGEXP_RECORD_SOURCE_WORD`]. `flags`' tagged literal operand.
pub(crate) const RUNTIME_REGEXP_RECORD_FLAGS_WORD: usize = 1;
/// The immutable descriptor's packed P-relative offset/byte length.
pub(crate) const RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD: usize = 2;
/// The closed runtime-entry discriminant.
pub(crate) const RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD: usize = 3;

/// The packed descriptor is always relative to this P's immutable metadata.
pub(crate) const RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD: usize = 4;
pub(crate) const RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN: u64 = 1;

/// Number of `u64` words in one runtime RegExp program table record.
///
/// Keep this the last word's index plus one: it is the length of the writer's
/// array, so a word added without extending it fails to compile at the
/// assignment rather than corrupting the next row.
pub(crate) const RUNTIME_REGEXP_RECORD_WORDS: usize = RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD + 1;

/// Byte stride between runtime RegExp program table records. **Derived**, never
/// typed — see [`RUNTIME_REGEXP_RECORD_SOURCE_WORD`].
pub(crate) const RUNTIME_REGEXP_RECORD_SIZE: u64 = (RUNTIME_REGEXP_RECORD_WORDS * 8) as u64;

/// Byte offset of a record word in the bounded GC metadata view.
pub(crate) const fn runtime_regexp_record_offset(word: usize) -> u64 {
    (word * 8) as u64
}

/// Only the complete static-table writer creates these checked bounds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeRegExpProgramTable {
    address: u32,
    count: u32,
}

impl RuntimeRegExpProgramTable {
    pub(crate) const fn count(self) -> u32 {
        self.count
    }
    pub(crate) fn metadata_span(
        self,
        strings: &StringPool,
    ) -> Result<ProgramMetadataSpan, EmitError> {
        let length = self
            .count
            .checked_mul(RUNTIME_REGEXP_RECORD_SIZE as u32)
            .ok_or_else(|| EmitError::unsupported("candidate row extent exceeds P metadata"))?;
        ProgramMetadataSpan::for_pool(strings, self.address, length)
    }
}

/// The closed outcome word in a static candidate-cache record.
/// Program handles carry validated immutable descriptors; a separate outcome
/// distinguishes syntax rejection from a cache miss or unsupported capability.
pub(crate) const RUNTIME_REGEXP_ENTRY_KIND_PROGRAM: u64 = 0;
/// See [`RUNTIME_REGEXP_ENTRY_KIND_PROGRAM`]. A row with this kind means the
/// compile-time `RegExpProgram::compile` answered
/// [`RegExpCompileErrorKind::InvalidSyntax`] — the pattern is not a legal
/// ECMAScript Pattern, so constructing a RegExp from it at run time is a spec
/// SyntaxError.
///
/// # The risk this row carries, stated in the other direction
///
/// The doc on [`RuntimeRegExpEntry`] argues one direction at length: a *missing*
/// row is a wrong answer, so seen-and-rejected must be recorded. The mirror
/// image is real and is not argued anywhere else, so it is stated here.
///
/// This row makes the compile-time compiler's `InvalidSyntax` verdict
/// **load-bearing at run time**. Before it, a pattern this compiler
/// mis-classified as `InvalidSyntax` merely fell through to the runtime fallback
/// matcher, which frequently answered it correctly. Now it throws a spurious
/// SyntaxError at all seven `emit_runtime_regexp_program_slots` call sites.
/// `scriptw-ir/src/regexp.rs` has ~20 `invalid_syntax(` construction sites
/// against ~7 `unsupported(` ones and none of them has been audited against the
/// grammar, so the premise "`InvalidSyntax` means the spec says invalid" is
/// assumed, not established.
///
/// Two properties widen the blast radius, and both are deliberate elsewhere:
/// the table is looked up by string **value** (`emit_string_payload_equality_i32`
/// is a real byte compare), so a runtime-concatenated string that happens to
/// equal a mis-rejected script literal also throws; and in fallback mode the
/// candidate set is every script string literal, so every mis-rejected literal
/// in a harness file becomes reachable.
///
/// Neither named gate detects this. `annexB/built-ins/RegExp/prototype/compile`
/// is 23 cases and `built-ins/RegExp/named-groups` is 36, and named groups
/// exercise the `Program` path, which is unchanged. **Measure
/// `built-ins/RegExp/prototype` (487 cases) as a delta before treating this as
/// landed, and read any new failure whose detail names SyntaxError as a
/// false-`InvalidSyntax` candidate rather than as unrelated noise.**
pub(crate) const RUNTIME_REGEXP_ENTRY_KIND_REJECTED: u64 = 1;
/// A static compiler capability gap. Like a cache miss, this enters the
/// emitted compiler. Its explicit Unsupported status exits through the typed
/// T19 semantic rejection; a capability gap never invents a JavaScript error.
pub(crate) const RUNTIME_REGEXP_ENTRY_KIND_UNSUPPORTED: u64 = 2;

mod runtime_regexp_entry_kind;
pub(crate) use runtime_regexp_entry_kind::RuntimeRegExpEntryKind;

#[cfg(test)]
mod runtime_regexp_table_tests;

/// What the AOT-built runtime RegExp program table says about one
/// `(source, flags)` pair.
///
/// The table is looked up **by string value** at run time, so an absent row and
/// an illegal pattern used to be the same observable state. `queue_runtime_regexp_programs`
/// wrote rows with
///
/// ```text
/// let Ok(program) = RegExpProgram::compile(compilation_source, flags) else {
///     continue;
/// };
/// ```
///
/// so a pattern the compiler had *seen and rejected* left no trace at all, the
/// emitted lookup fell out of its loop with no else arm, and
/// `new RegExp("(?<x>a)(?<x>b)")` returned a live RegExp carrying
/// `instruction_count == 0` instead of throwing SyntaxError. That is a
/// wrong-answer class, not a missing feature.
///
/// Making the table's value a closed type is what stops it recurring: the
/// writer below matches exhaustively, so a third outcome added later is
/// `error[E0004]` at the table writer rather than one more silently skipped
/// row. `Option<RegExpProgramRef>` would not do it — `unwrap_or`, `if let` and
/// `continue` are all one keystroke away, and `continue` is exactly what was
/// written here.
#[derive(Debug, Clone, Copy)]
enum RuntimeRegExpEntry {
    /// `RegExpProgram::compile` accepted the pair; this is its static data.
    Program(RegExpProgramRef),
    /// `RegExpProgram::compile` answered `InvalidSyntax`: the pattern is not a
    /// legal ECMAScript Pattern. The emitted lookup turns a hit on this row
    /// into a SyntaxError.
    Rejected,
    /// `RegExpProgram::compile` answered `UnsupportedFeature`: the pattern is
    /// outside the static compiler's capability. A hit enters runtime
    /// compilation — see [`RUNTIME_REGEXP_ENTRY_KIND_UNSUPPORTED`].
    Unsupported,
}

/// [`RuntimeRegExpEntry`] before the static program data exists.
///
/// `queue_regexp_program` only queues; the `RegExpProgramRef` a `Program` row
/// needs is not known until `append_regexp_programs` has run. This carries the
/// same three answers across that gap by key instead of by ref, so the
/// intermediate never has to be an `Option` — the shape whose `None` arm is
/// what the original `continue` collapsed into.
enum CandidateOutcome {
    Program(RegExpProgramStaticKey),
    Rejected,
    Unsupported,
}

/// Eight-byte rows contain a source code point and its canonical code point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RegExpCaseFoldingTable {
    pub(crate) ptr: u32,
    pub(crate) count: u32,
}

/// Where the compiler-owned phase of a pool ends. Everything before it is the
/// runtime module's data and is identical for every program; everything after
/// it belongs to the program module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PoolBoundary {
    static_bytes: usize,
    code_unit_bytes: usize,
    strings: u32,
}

impl PoolBoundary {
    pub(crate) const fn code_unit_len(self) -> usize {
        self.code_unit_bytes
    }

    /// Reconstruct only bounded pool dimensions; the runtime cache also checks
    /// the actual data segments, and program collection checks the full boundary.
    pub(crate) fn from_runtime_cache(
        static_bytes: usize,
        code_unit_bytes: usize,
        strings: u32,
    ) -> Option<Self> {
        STATIC_DATA_OFFSET.checked_add(u32::try_from(static_bytes).ok()?)?;
        u32::try_from(code_unit_bytes).ok()?;
        if code_unit_bytes % 2 != 0
            || strings == 0
            || u64::from(strings) > code_unit_bytes as u64 / 2 + 1
        {
            return None;
        }
        Some(Self {
            static_bytes,
            code_unit_bytes,
            strings,
        })
    }

    /// Pooled strings the runtime owns; the program's slots start here.
    pub(crate) const fn strings(self) -> u32 {
        self.strings
    }

    pub(crate) const fn static_len(self) -> usize {
        self.static_bytes
    }

    /// Linear-memory address of the first byte the program owns.
    pub(crate) fn program_static_address(self) -> u32 {
        STATIC_DATA_OFFSET + self.static_bytes as u32
    }
}

#[derive(Debug, Default)]
pub(crate) struct StringPool {
    compiler_owned: PoolBoundary,
    shared_prefix: Option<std::sync::Arc<CompilerOwnedPool>>,
    runtime_string_descriptors: Option<RuntimeStringDescriptors>,
    intl_supported_values: Option<Result<CompiledSupportedValuesTables, SupportedValuesPoolError>>,
    intl_default_locale: Option<scriptw_intl::CanonicalLocaleId>,
    pub(crate) bytes: PoolBytes,
    module_bytes: BTreeMap<scriptw_ir::ModuleUnitId, PooledModuleBytes>,
    compact_array_recipes: compact_array_recipes::CompactArrayRecipes,
    numeric_call_tables: crate::call_rows::NumericCallTables,
    global_var_declarations: BTreeMap<scriptw_ir::StaticScriptId, GlobalVarDeclarationTables>,
    pooled_code_unit_bytes: PoolBytes,
    refs: PoolStringRefs,
    source_runtime_error_messages: BTreeMap<String, i64>,
    script_string_literals: BTreeSet<String>,
    runtime_regexp_candidate_literals: BTreeSet<String>,
    /// Pattern strings the script names **directly at a RegExp construction
    /// site** (`new RegExp("…")`, `RegExp("…")`, `r.compile("…")`).
    ///
    /// Separate from `runtime_regexp_candidate_literals` because that set has a
    /// second job: when it is empty the candidate set falls back to *every*
    /// script string literal. Folding construction-site arguments into it would
    /// silently flip that fallback off for any script that has one, narrowing
    /// the table instead of widening it. This set is always unioned in, so it
    /// can only add rows.
    ///
    /// Measured motivation: `runtime_regexp_candidate_literals` is populated
    /// from declaration initialisers, assignments and array literals — never
    /// from call arguments. In
    /// `annexB/built-ins/RegExp/prototype/compile/duplicate-named-capturing-groups-syntax.js`
    /// the valid pattern reaches it through `let source = "(?<x>a)|(?<x>b)"`,
    /// while the invalid `"(?<x>a)(?<x>b)"` appears only as a call argument, so
    /// the set was non-empty, the fallback did not fire, and the pattern the
    /// test is *about* was never offered to the compiler at all.
    runtime_regexp_argument_literals: BTreeSet<String>,
    regexp_programs: BTreeMap<RegExpProgramStaticKey, RegExpProgramRef>,
    pending_regexp_programs: Vec<RegExpProgramStaticKey>,
    needed_regexp_case_folding: BTreeSet<RegExpCaseFolding>,
    regexp_case_folding_tables: BTreeMap<RegExpCaseFolding, RegExpCaseFoldingTable>,
    regexp_unicode_property_image: Option<RegExpUnicodePropertyImage>,
    temporal_east_asian_year_image: Option<TemporalEastAsianYearImage>,
    runtime_regexp_programs: Vec<(String, String, RuntimeRegExpEntry)>,
    needs_runtime_regexp_programs: bool,
    runtime_regexp_program_table: Option<RuntimeRegExpProgramTable>,
    pub(crate) uses_heap: bool,
    pub(crate) lowercase_mapping_table_ptr: u32,
    pub(crate) lowercase_mapping_count: u32,
    pub(crate) uppercase_mapping_table_ptr: u32,
    pub(crate) uppercase_mapping_count: u32,
    pub(crate) cased_range_table_ptr: u32,
    pub(crate) cased_range_count: u32,
    pub(crate) case_ignorable_range_table_ptr: u32,
    pub(crate) case_ignorable_range_count: u32,
    pub(crate) normalization_region: Option<NormalizationRegion>,
    pub(crate) canonical_decomposition_table_ptr: u32,
    pub(crate) canonical_decomposition_count: u32,
    pub(crate) compatibility_decomposition_table_ptr: u32,
    pub(crate) compatibility_decomposition_count: u32,
    pub(crate) combining_class_table_ptr: u32,
    pub(crate) combining_class_count: u32,
    pub(crate) composition_table_ptr: u32,
    pub(crate) composition_count: u32,
}

impl StringPool {
    pub(crate) fn intl_default_locale(
        &self,
    ) -> Result<&scriptw_intl::CanonicalLocaleId, EmitError> {
        self.intl_default_locale
            .as_ref()
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.intl_default_locale.as_ref())
            })
            .ok_or_else(|| EmitError::unsupported("missing selected Unicode case DefaultLocale"))
    }

    /// Compiler-owned strings and tables first, script-derived data after.
    ///
    /// Builtin and helper bodies embed pool slots and linear-memory addresses.
    /// [`CompilerOwnedPool`] lays out everything they can reference and is the
    /// only way to start script collection, so a program's literals cannot
    /// shift a compiler-owned constant.
    pub(crate) fn collect(
        script: &ScriptIr,
        function_metas: &BTreeMap<FunctionId, WasmFunctionMeta>,
        compiled_standard_builtins: &[StandardBuiltinId],
        uses_temporal_calendar: bool,
        intl_selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<Self, EmitError> {
        CompilerOwnedPool::collect(
            function_metas,
            compiled_standard_builtins,
            uses_temporal_calendar,
            intl_selection,
        )?
        .collect_script(script, function_metas)
    }
}

/// Builtins and the compiler-generated empty dynamic-function bodies: every
/// function whose body the runtime half of the module owns.
fn is_runtime_owned_function(id: &str, meta: &WasmFunctionMeta) -> bool {
    meta.standard_builtin.is_some()
        || meta.host_builtin.is_some()
        || crate::builtins::is_empty_dynamic_function_id(id)
}

/// A [`StringPool`] whose compiler-owned strings and tables are complete.
///
/// Only this type can start script collection, so script-derived strings and
/// bytes always land after every constant builtin bodies can reference.
#[derive(Debug)]
pub(crate) struct CompilerOwnedPool(StringPool);

impl CompilerOwnedPool {
    fn collect(
        function_metas: &BTreeMap<FunctionId, WasmFunctionMeta>,
        compiled_standard_builtins: &[StandardBuiltinId],
        uses_temporal_calendar: bool,
        intl_selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<Self, EmitError> {
        let mut pool = StringPool::default();
        if compiled_standard_builtins.iter().any(|builtin| {
            matches!(
                builtin,
                StandardBuiltinId::StringPrototypeToLocaleLowerCase
                    | StandardBuiltinId::StringPrototypeToLocaleUpperCase
            )
        }) {
            pool.intl_default_locale = Some(
                intl_selection
                    .compilation_data()
                    .map_err(|error| {
                        EmitError::unsupported(format!(
                            "failed to select Unicode case DefaultLocale: {error}"
                        ))
                    })?
                    .identity()
                    .default_locale()
                    .clone(),
            );
        }
        for value in [
            "",
            " ",
            "          ",
            "\n",
            ": ",
            ",",
            "undefined",
            "null",
            "true",
            "false",
            "0",
            "-0",
            "\"",
            "{",
            "}",
            "{}",
            "[",
            "]",
            "[]",
            ":",
            "0.0",
            "1",
            "1.0",
            "NaN",
            "Infinity",
            "-Infinity",
            "1e-7",
            "-1e-7",
            "100000000000000000000",
            "-100000000000000000000",
            "10203040506070809000",
            "-10203040506070809000",
            "1e+22",
            "-1e+22",
            "Symbol()",
            "-1e+21",
            "\\d",
            "\\d{1}",
            "\\d{2}",
            "\\D{2}",
            "0.",
            ".",
            "\u{20BB7}",
            "\\p{Script=Han}",
            "b.",
            "c.",
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
            "[\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}]",
            "\u{1F468}",
            "\u{1F469}",
            "\u{1F467}",
            "\u{1F466}",
            "\u{200D}",
            "\u{F0000}D842",
            "\u{F0000}DFB7",
            "\u{F0000}D83D",
            "\u{F0000}DC68",
            "\u{F0000}DC69",
            "\u{F0000}DC67",
            "\u{F0000}DC66",
            "([\\d]{5})([-\\ ]?[\\d]{4})?$",
            "(?:(?<x>a)|(?<y>a)(?<x>b))(?:(?<z>c)|(?<z>d))",
            "(?:(?:(?<x>a)|(?<x>b)|c)\\k<x>){2}",
            "groups",
            "indices",
            "x",
            "y",
            "z",
            "a",
            "b",
            "c",
            "d",
            "abc",
            "ad",
            "aac",
            "\\ud834",
            "\\udf06",
            "[object Object]",
            "[object Arguments]",
            "[object ",
            "/",
            "prototype",
            "lastIndex",
            "index",
            "input",
            "$_",
            "lastMatch",
            "$&",
            "lastParen",
            "$+",
            "leftContext",
            "$`",
            "rightContext",
            "$",
            "$'",
            "$1",
            "$2",
            "$3",
            "$4",
            "$5",
            "$6",
            "$7",
            "$8",
            "$9",
            "g",
            "l",
            "77",
            "\\u0037\\u0037",
            "\\s",
            "\\w",
            "\\d+",
            "[a-z]",
            "constructor",
            "withResolvers",
            "try",
            "promise",
            "resolve",
            "reject",
            "callee",
            "arguments",
            "caller",
            "valueOf",
            "toString",
            "toUpperCase",
            "toLowerCase",
            "toLocaleLowerCase",
            "toLocaleUpperCase",
            "fromCodePoint",
            "padStart",
            "padEnd",
            "repeat",
            "exec",
            "object",
            "boolean",
            "number",
            "string",
            "default",
            "function",
            "function(handle@",
            ")",
            "length",
            "name",
            "message",
            "stack",
            "error",
            "suppressed",
            "cause",
            "code",
            "errors",
            "global",
            FUNCTION_NAME,
            OBJECT_NAME,
            ARRAY_NAME,
            ARRAY_BUFFER_NAME,
            DATA_VIEW_NAME,
            DATE_NAME,
            REGEXP_NAME,
            MATH_NAME,
            JSON_NAME,
            FLOAT64_ARRAY_NAME,
            FLOAT32_ARRAY_NAME,
            FLOAT16_ARRAY_NAME,
            INT32_ARRAY_NAME,
            INT16_ARRAY_NAME,
            INT8_ARRAY_NAME,
            UINT32_ARRAY_NAME,
            UINT16_ARRAY_NAME,
            UINT8_ARRAY_NAME,
            UINT8_CLAMPED_ARRAY_NAME,
            "BigInt64Array",
            "BigUint64Array",
            "$Realm.Float64Array.prototype",
            "$Realm.Float32Array.prototype",
            "$Realm.Float16Array.prototype",
            "$Realm.Int32Array.prototype",
            "$Realm.Int16Array.prototype",
            "$Realm.Int8Array.prototype",
            "$Realm.Uint32Array.prototype",
            "$Realm.Uint16Array.prototype",
            "$Realm.Uint8Array.prototype",
            "$Realm.Uint8ClampedArray.prototype",
            "$Realm.BigInt64Array.prototype",
            "$Realm.BigUint64Array.prototype",
            "BYTES_PER_ELEMENT",
            REFLECT_NAME,
            NUMBER_NAME,
            STRING_NAME,
            BOOLEAN_NAME,
            ERROR_NAME,
            EVAL_ERROR_NAME,
            AGGREGATE_ERROR_NAME,
            SUPPRESSED_ERROR_NAME,
            RANGE_ERROR_NAME,
            SYNTAX_ERROR_NAME,
            TYPE_ERROR_NAME,
            URI_ERROR_NAME,
            REFERENCE_ERROR_NAME,
            "call",
            "apply",
            "bind",
            "anchor",
            "big",
            "blink",
            "bold",
            "fixed",
            "fontcolor",
            "fontsize",
            "italics",
            "link",
            "small",
            "strike",
            "sub",
            "substr",
            "substring",
            "sup",
            "trim",
            "trimStart",
            "trimLeft",
            "trimEnd",
            "trimRight",
            "match",
            "matchAll",
            "replace",
            "replaceAll",
            "search",
            "split",
            "concat",
            // Object.prototype.toString builds its tag with rooted GC strings.
            "[object ",
            "]",
            "Object",
            "Array",
            "Arguments",
            "Function",
            "Error",
            "Boolean",
            "Number",
            "String",
            "Date",
            "RegExp",
            "[object Undefined]",
            "[object Null]",
            "[object Boolean]",
            "[object Number]",
            "[object String]",
            "[object Symbol]",
            "[object Object]",
            "[object Array]",
            "[object Function]",
            "[object Arguments]",
            "[object BigInt]",
            "[object Error]",
            "[object Date]",
            "[object RegExp]",
            "from",
            "find",
            "findIndex",
            "findLast",
            "findLastIndex",
            "flat",
            "includes",
            "pop",
            "push",
            "shift",
            "unshift",
            "splice",
            "sort",
            "subarray",
            "entries",
            "values",
            "create",
            "getPrototypeOf",
            "setPrototypeOf",
            "defineProperty",
            "defineProperties",
            "getOwnPropertyDescriptor",
            "getOwnPropertyNames",
            "getOwnPropertySymbols",
            "keys",
            "hasOwn",
            "is",
            "isSealed",
            "isFrozen",
            "freeze",
            "isExtensible",
            "hasOwnProperty",
            "__proto__",
            "propertyIsEnumerable",
            "toString",
            "$IsHTMLDDA",
            "Symbol.iterator",
            "iterator next result must be object",
            "return",
            "$ArrayIterator.array",
            "$ArrayIterator.index",
            "$ArrayIterator.done",
            "$ArrayIterator.kind",
            "$StringIterator.string",
            "$StringIterator.index",
            "$RegExpStringIterator.regexp",
            "$RegExpStringIterator.string",
            "$RegExpStringIterator.global",
            "$RegExpStringIterator.unicode",
            "$RegExpStringIterator.done",
            "Array Iterator",
            "String Iterator",
            "Map Iterator",
            "Set Iterator",
            "Generator",
            "allKeyed",
            "allSettledKeyed",
            "status",
            "fulfilled",
            "rejected",
            "value",
            "reason",
            "Array Iterator next called on out-of-bounds TypedArray",
            "WeakMap",
            "WeakSet",
            "WeakRef",
            "FinalizationRegistry",
            "register",
            "unregister",
            // `AsyncDisposableStack`: the property keys its intrinsic installer
            // defines, plus every message its emitters throw. A key or message
            // spelled at an emitter and missing here is a compile-time panic in
            // every full bootstrap (`string ... must exist in pool`), not a
            // runtime miss.
            "AsyncDisposableStack",
            "use",
            "adopt",
            "defer",
            "move",
            "disposed",
            "disposeAsync",
            // `%DisposableStack%`: the method keys are already interned by the
            // async stack and Iterator surfaces above. Keep every synchronous
            // receiver/registration error here beside the constructor error so
            // a new emitter spelling that bypasses the pool fails during
            // bootstrap instead of becoming a latent runtime-only path.
            "DisposableStack",
            "Iterator",
            "toArray",
            "forEach",
            "groupBy",
            "every",
            "some",
            "find",
            "reduce",
            "map",
            "filter",
            "take",
            "drop",
            "Iterator Helper",
            "Iterator.from next method must be callable",
            "Iterator map helper return method must be callable",
            "Iterator map helper return result must be object",
            "$IteratorMapIterator",
            "$IteratorMapNext",
            "$IteratorMapMapper",
            "$IteratorMapIndex",
            "$IteratorMapDone",
            "$IteratorMapExecuting",
            "$IteratorZipIterators",
            "$IteratorZipNextMethods",
            "$IteratorZipOpen",
            "$IteratorZipMode",
            "$IteratorZipPadding",
            "$IteratorZipKeys",
            "$IteratorZipDone",
            "$IteratorZipExecuting",
            "$IteratorZipStarted",
            "$IteratorConcatIterables",
            "$IteratorConcatMethods",
            "$IteratorConcatCurrentIterator",
            "$IteratorConcatCurrentNext",
            "$IteratorConcatIndex",
            "$IteratorConcatActive",
            "$IteratorConcatDone",
            "$IteratorConcatExecuting",
            "mode",
            "padding",
            "shortest",
            "longest",
            "strict",
            "only-full",
            "allow-partial",
            "zipKeyed",
            "Iterator filter helper return method must be callable",
            "Iterator filter helper return result must be object",
            "$IteratorFilterIterator",
            "$IteratorFilterNext",
            "$IteratorFilterPredicate",
            "$IteratorFilterIndex",
            "$IteratorFilterDone",
            "$IteratorFilterExecuting",
            "flatMap",
            "Iterator flatMap helper return method must be callable",
            "Iterator flatMap helper return result must be object",
            "$IteratorFlatMapIterator",
            "$IteratorFlatMapNext",
            "$IteratorFlatMapMapper",
            "$IteratorFlatMapIndex",
            "$IteratorFlatMapDone",
            "$IteratorFlatMapExecuting",
            "$IteratorFlatMapInnerIterator",
            "$IteratorFlatMapInnerNext",
            "$IteratorFlatMapInnerActive",
            "$IteratorTakeIterator",
            "$IteratorTakeNext",
            "$IteratorTakeRemaining",
            "$IteratorTakeDone",
            "$IteratorTakeExecuting",
            "Iterator take helper return method must be callable",
            "Iterator take helper return result must be object",
            "$IteratorDropIterator",
            "$IteratorDropNext",
            "$IteratorDropRemaining",
            "$IteratorDropDone",
            "$IteratorDropExecuting",
            "Iterator drop helper return method must be callable",
            "Iterator drop helper return result must be object",
            "$ScriptwIteratorFromWrapper",
            "$IteratorFromIterator",
            "$IteratorFromNext",
            "Array.prototype.values called on null or undefined",
            "TypedArray.from constructed target is not a typed array",
            "TypedArray.from constructed target is too small",
            "TypedArray.prototype.toString requires TypedArray",
            "construct",
            "ownKeys",
            "has",
            "isArray",
            "isView",
            "isInteger",
            "isSafeInteger",
            "isFinite",
            "isNaN",
            "isError",
            "escape",
            "unescape",
            "asIntN",
            "asUintN",
            "E",
            "LN10",
            "LN2",
            "LOG10E",
            "LOG2E",
            "PI",
            "SQRT1_2",
            "SQRT2",
            "abs",
            "acos",
            "acosh",
            "asin",
            "asinh",
            "atan",
            "atan2",
            "atanh",
            "cbrt",
            "ceil",
            "clz32",
            "cos",
            "cosh",
            "exp",
            "expm1",
            "f16round",
            "floor",
            "fround",
            "hypot",
            "imul",
            "log",
            "log10",
            "log1p",
            "log2",
            "max",
            "min",
            "pow",
            "random",
            "round",
            "sign",
            "sin",
            "sinh",
            "sqrt",
            "sumPrecise",
            "tan",
            "tanh",
            "trunc",
            "toExponential",
            "toFixed",
            "toPrecision",
            "MAX_VALUE",
            "MIN_VALUE",
            "EPSILON",
            "MAX_SAFE_INTEGER",
            "MIN_SAFE_INTEGER",
            "POSITIVE_INFINITY",
            "NEGATIVE_INFINITY",
            "slice",
            "detached",
            "resize",
            "transfer",
            "transferToFixedLength",
            "transferToImmutable",
            "sliceToImmutable",
            "getUint8",
            "setUint8",
            "getInt8",
            "setInt8",
            "getUint16",
            "setUint16",
            "getInt16",
            "setInt16",
            "getUint32",
            "setUint32",
            "getInt32",
            "setInt32",
            "getFloat16",
            "setFloat16",
            "getFloat32",
            "setFloat32",
            "getFloat64",
            "setFloat64",
            "now",
            "UTC",
            "getTime",
            "setTime",
            "getFullYear",
            "getUTCFullYear",
            "getMonth",
            "getUTCMonth",
            "getDate",
            "getUTCDate",
            "getDay",
            "getUTCDay",
            "getHours",
            "getUTCHours",
            "getMinutes",
            "getUTCMinutes",
            "getSeconds",
            "getUTCSeconds",
            "getMilliseconds",
            "getUTCMilliseconds",
            "getTimezoneOffset",
            "getYear",
            "setYear",
            "toUTCString",
            "toGMTString",
            "buffer",
            "byteOffset",
            "byteLength",
            "maxByteLength",
            "resizable",
            "immutable",
            "growable",
            "grow",
            "Symbol.dispose",
            "Symbol.species",
            "Symbol.isConcatSpreadable",
            "Symbol.match",
            "Symbol.matchAll",
            "Symbol.replace",
            "Symbol.search",
            "Symbol.split",
            "Symbol.toStringTag",
            "Module",
            "Deferred Module",
            "then",
            "anonymous",
            "function anonymous(\n) {\n\n}",
            "GeneratorFunction",
            "function GeneratorFunction() { [native code] }",
            "AsyncFunction",
            "function AsyncFunction() { [native code] }",
            "AsyncGenerator",
            "AsyncGeneratorFunction",
            "function AsyncGeneratorFunction() { [native code] }",
            "[Symbol.asyncIterator]",
            "function [Symbol.asyncIterator]() { [native code] }",
            "function next() { [native code] }",
            "function return() { [native code] }",
            "function throw() { [native code] }",
            "Symbol.toPrimitive",
            "Symbol.asyncIterator",
            "Symbol.hasInstance",
            "Symbol.unscopables",
            "Symbol.asyncDispose",
            "Symbol",
            "Symbol(",
            "Cannot create property on symbol",
            "iterator",
            "asyncIterator",
            "hasInstance",
            "isConcatSpreadable",
            "species",
            "toStringTag",
            "toPrimitive",
            "unscopables",
            "dispose",
            "asyncDispose",
            "for",
            "keyFor",
            "description",
            "(?:)",
            "source",
            "flags",
            "gc requires a real collector in wasm-aot",
            "parse",
            "stringify",
            "rawJSON",
            "isRawJSON",
            "add",
            "and",
            "compareExchange",
            "exchange",
            "load",
            "notify",
            "or",
            "pause",
            "store",
            "sub",
            "wait",
            "waitAsync",
            "xor",
            "Atomics.wait blocking wait queues unsupported in wasm-aot",
            "Atomics.waitAsync blocking wait queues unsupported in wasm-aot",
            "not-equal",
            "timed-out",
            "ok",
            "toJSON",
            "hasIndices",
            "unicodeSets",
            "ignoreCase",
            "multiline",
            "dotAll",
            "unicode",
            "sticky",
            "i",
            "m",
            "s",
            "u",
            "RegExp constructor is unsupported in wasm-aot",
            "RegExp.prototype.exec source is not string",
            "RegExp.prototype.exec unsupported pattern",
            "RegExp.prototype[Symbol.match] flags is not string",
            "RegExp.prototype[Symbol.match] is unsupported in wasm-aot",
            ".(.).",
            "^|\\udf06",
            "RegExp.prototype[Symbol.matchAll] receiver is not RegExp",
            "RegExp.prototype[Symbol.matchAll] source is not string",
            "RegExp.prototype[Symbol.matchAll] flags is not string",
            "RegExp.prototype[Symbol.search] source is not string",
            "RegExp.prototype[Symbol.search] flags is not string",
            "\u{20BB7}",
            "\u{10FFFF}",
            "\u{20BB7}a\u{20BB7}b\u{20BB7}",
            "a\u{20BB7}b\u{10FFFF}c",
            "\\p{Script=Han}",
            "\\P{ASCII}",
            "String.prototype.matchAll RegExp flags must contain g",
            "standard builtin body is not emitted unless referenced directly",
            "Cannot redefine JSON reviver property",
            "Cannot add JSON reviver property",
            "ArrayBuffer",
            SHARED_ARRAY_BUFFER_NAME,
            "DataView",
            "get",
            "set",
            "clear",
            "delete",
            "size",
            "async",
            "value",
            "next",
            "done",
            "proxy",
            "revoke",
            "writable",
            "enumerable",
            "configurable",
            "$Proxy.target",
            "$Proxy.handler",
            "Proxy defineProperty trap cannot define non-writable target property",
            "TypedArray.prototype.set backing buffer is detached",
            "TypedArray.prototype.set source buffer is detached",
            "deleteProperty",
            "2",
            "3",
            "4",
            "5",
            "10000",
            "prop",
            "f",
            "v",
            SCRIPTW_GENERATOR_THROW_SLOT,
            "EvalError",
            "AggregateError",
            "RangeError",
            "SyntaxError",
            "URIError",
            "ReferenceError",
            "Function.prototype.call receiver is not callable",
            "Function.prototype.call primitive thisArg boxing unsupported",
            "Function.prototype.apply primitive thisArg boxing unsupported",
            "Function.prototype.call/apply thisArg adaptation failed",
            "Function.prototype.apply argument list must be array or arguments",
            "Array.prototype.concat receiver is not array",
            "Array.prototype.flatMap receiver is not array",
            "Array.prototype.flatMap called on null or undefined",
            "Array.prototype.flatMap constructor is not object",
            "Array.prototype.includes receiver is not array",
            "Array.prototype.includes called on null or undefined",
            "Array.prototype.find called on null or undefined",
            "Array.prototype.findIndex called on null or undefined",
            "Array.prototype.findLast called on null or undefined",
            "Array.prototype.findLastIndex called on null or undefined",
            "Array.prototype.map receiver is not array",
            "Array.prototype.map called on null or undefined",
            "Array.prototype.map constructor is not object",
            "Array.prototype.every receiver is not array",
            "Array.prototype.every called on null or undefined",
            "Array.prototype.every constructor is not object",
            "Array.prototype.some receiver is not array",
            "Array.prototype.some called on null or undefined",
            "Array.prototype.some constructor is not object",
            "Array.prototype.filter receiver is not array",
            "Array.prototype.filter called on null or undefined",
            "Array.prototype.filter constructor is not object",
            "Array.prototype.filter cannot add property to non-extensible target",
            "Array.prototype.filter cannot define non-configurable target property",
            "Object.prototype.valueOf called on null or undefined",
            "Object.keys requires object",
            "Object.values requires object",
            "4294967295",
            "Array.prototype.pop receiver is not array",
            "Array.prototype.sort receiver is not array",
            "Invalid Date",
            "Sun",
            "Mon",
            "Tue",
            "Wed",
            "Thu",
            "Fri",
            "Sat",
            "Jan",
            "Feb",
            "Mar",
            "Apr",
            "May",
            "Jun",
            "Jul",
            "Aug",
            "Sep",
            "Oct",
            "Nov",
            "Dec",
            ", ",
            " GMT",
            " (",
            " GMT+0000 (Coordinated Universal Time)",
            "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)",
            "Thu, 01 Jan 1970 00:00:00 GMT",
            "-",
            "+",
            ":",
            ".",
            "T",
            "Z",
            "toISOString",
            "toTemporalInstant",
            "Temporal",
            "Now",
            "Temporal.Now",
            "instant",
            "zonedDateTimeISO",
            "Instant",
            "PlainDate",
            "Temporal.PlainDate",
            "compare",
            "with",
            "withCalendar",
            "era",
            "eraYear",
            // The `era` half of `CalendarResolveFields` is one emitter shared
            // by `PlainDate`, `PlainDateTime`, `PlainYearMonth`,
            // `PlainMonthDay` and `ZonedDateTime`, so its messages are not
            // per-family and cannot sit behind any one family's gate. They
            // join the unconditional block beside the two property names the
            // same emitter reads.
            "dayOfWeek",
            "dayOfYear",
            "weekOfYear",
            "yearOfWeek",
            "daysInWeek",
            "daysInMonth",
            "daysInYear",
            "monthsInYear",
            "inLeapYear",
            "toLocaleString",
            "Temporal.Instant",
            "from",
            "epochMilliseconds",
            "epochNanoseconds",
            "equals",
            "Intl",
            "Intl.Locale",
            "getCanonicalLocales",
            "Locale",
            "language",
            "script",
            "region",
            "baseName",
            "calendar",
            "collation",
            "firstDayOfWeek",
            "firstDay",
            "weekend",
            "getWeekInfo",
            "getNumberingSystems",
            "getHourCycles",
            "getTextInfo",
            "direction",
            "ltr",
            "rtl",
            "hourCycle",
            "caseFirst",
            "numeric",
            "numberingSystem",
            "variants",
            "und-",
            "0",
            "1",
            "2",
            "3",
            "4",
            "5",
            "6",
            "7",
            "-u",
            "ca",
            "co",
            "fw",
            "hc",
            "kf",
            "kn",
            "nu",
            "mon",
            "tue",
            "wed",
            "thu",
            "fri",
            "sat",
            "sun",
            "h11",
            "h12",
            "h23",
            "h24",
            "upper",
            "lower",
            "false",
            "true",
            "Intl.getCanonicalLocales argument must be an object",
            "ZonedDateTime",
            "Temporal.ZonedDateTime",
            "timeZoneId",
            "calendarId",
            "year",
            "month",
            "monthCode",
            "day",
            "hour",
            "minute",
            "second",
            "millisecond",
            "microsecond",
            "nanosecond",
            "M01",
            "M02",
            "M03",
            "M04",
            "M05",
            "M06",
            "M07",
            "M08",
            "M09",
            "M10",
            "M11",
            "M12",
            "M13",
            "toInstant",
            "UTC",
            "Array.prototype.forEach receiver is not array",
            "String.prototype RegExp/string fallback is unsupported in wasm-aot",
            "AggregateError errors input must be array or arguments",
            "ArrayBuffer resize is not supported by this host",
            "ArrayBuffer receiver is SharedArrayBuffer",
            "SharedArrayBuffer allocation exceeds the wasm-aot shared-memory limit",
            "TypedArray allocation exceeds the wasm-aot buffer-memory limit",
            "derived constructor must call super() before returning",
            "super() invalid in class extending null",
            GLOBAL_THIS_NAME,
            PRINT_NAME,
            IS_CONSTRUCTOR_NAME,
            "<a name=\"",
            "\">",
            "</a>",
            "<big>",
            "</big>",
            "<blink>",
            "</blink>",
            "<b>",
            "</b>",
            "<tt>",
            "</tt>",
            "<font color=\"",
            "<font size=\"",
            "</font>",
            "<i>",
            "</i>",
            "<a href=\"",
            "<small>",
            "</small>",
            "<strike>",
            "</strike>",
            "<sub>",
            "</sub>",
            "<sup>",
            "</sup>",
            "&quot;",
            "bound ",
            "RegExp String Iterator",
        ] {
            pool.intern_string(value);
        }
        if compiled_standard_builtins.contains(&StandardBuiltinId::IntlSupportedValuesOf) {
            pool.collect_intl_supported_values(intl_selection);
        }
        // Keep additional literals after the fixed seed: inserting them into
        // its prefix changes every following packed string offset.
        pool.intern_string("get ");
        pool.intern_string("set ");
        pool.intern_string(UNHANDLED_REJECTION_TOSTRING_THROWN_MESSAGE);
        // Unconditional, and it must stay unconditional: these are the messages
        // `emit_runtime_error_object` now reads out of the pool, and the paths
        // that throw them (`null.x`, an unbound identifier, a TDZ read) are
        // reachable from any program at all. Gating them behind a feature
        // predicate would reintroduce the exact `string must exist in pool`
        // panic this table exists to prevent.
        pool.intern_runtime_error_catalog();
        // Every `TemporalCalendarId` spelling and canonical form, plus every
        // `Era` code, derived from the tables the emitters read rather than
        // listed again. Listing them was a standing drift risk in both
        // directions: a calendar spelling added to `TemporalCalendarId` and
        // forgotten here is the `string must exist in pool` compiler panic
        // fixed in e04bdc061, and `"gregorian"` was already interned twice
        // because it is also an `INTL_DTF_ACCEPTED_CALENDARS` row.
        //
        // Unconditional, and it cannot move behind the `Temporal.PlainDate`
        // gate below: the shared calendar helpers
        // (`compile_temporal_calendar_identifier_helper` and
        // `compile_temporal_calendar_iso_date_probe_helper`) are compiled from
        // `uses_temporal_calendar` in `emit.rs`, whose predicate also fires for
        // a program touching only `Temporal.ZonedDateTime` — and that program
        // does not satisfy the gate.
        for calendar in TemporalCalendarId::ALL {
            pool.intern_string(calendar.canonical());
            for spelling in calendar.spellings() {
                pool.intern_string(spelling);
            }
            // Suitability and canonical output share this closed code census.
            for code in calendar.month_code_spellings() {
                pool.intern_string(code);
            }
            // Every era spelling, not just `code()`: `code()` is defined as
            // `spellings()[0]`, and `CalendarResolveFields` matches an incoming
            // `era` against all of them (`ad`/`bc` are the CLDR aliases of
            // `ce`/`bce`). Interning the same table the resolver reads is what
            // makes "add an alias" a one-place change instead of three.
            //
            // The walk is `TemporalCalendarId::ALL -> eras() -> spellings()`,
            // i.e. literally the table
            // `FunctionBuilder::emit_temporal_resolve_era_to_year` reads, and
            // not a second `Era`-side list that could be short of it. That
            // matters because the resolver emits `strings.payload(spelling)`
            // for every spelling of every era of every calendar: an era
            // reachable from `eras()` but missing here is the `string must
            // exist in pool` compiler panic fixed in e04bdc061, and no
            // `const` assertion can see a list that is never consulted.
            for era in calendar.eras() {
                for spelling in era.spellings() {
                    pool.intern_string(spelling);
                }
            }
        }
        for value in crate::builtins::intl_date_time_format_pool_strings()
            .into_iter()
            .chain(crate::builtins::intl_number_format_pool_strings())
            .chain(crate::builtins::intl_plural_rules_pool_strings())
            .chain(crate::builtins::intl_list_format_pool_strings())
            .chain(crate::builtins::intl_collator_pool_strings())
            .chain(crate::builtins::intl_display_names_pool_strings())
            .chain(crate::builtins::intl_relative_time_pool_strings())
            .chain(crate::builtins::intl_segmenter_pool_strings())
            .chain(crate::builtins::intl_durationformat_pool_strings())
        {
            pool.intern_string(&value);
        }
        for index in 0..=31 {
            pool.intern_string(&index.to_string());
        }
        // Native accessor metadata is selected by its closed captured slot.
        for slot in crate::gc_types::RegExpLegacySlot::ALL {
            for name in slot.names() {
                for prefix in ["get", "set"] {
                    if prefix == "set" && slot != crate::gc_types::RegExpLegacySlot::Input {
                        continue;
                    }
                    let name = format!("{prefix} {name}");
                    pool.intern_string(&name);
                    pool.intern_string(
                        &scriptw_ir::CallableToStringRepresentation::NativeNamed(name)
                            .materialize(),
                    );
                }
            }
        }
        // Created-Realm record keys are host-authored, so user-source
        // collection cannot discover them. Every heap-backed module compiles
        // `createRealm`.
        if !compiled_standard_builtins.is_empty() {
            pool.intern_string(REALM_EVAL_SCRIPT_METHOD_NAME);
            pool.intern_string("$262");
            pool.intern_string("detachArrayBuffer");
            pool.intern_string("AbstractModuleSource");
            // Builtin bodies compute `typeof` results whether or not the
            // script spells a `typeof`.
            pool.collect_typeof_result_strings();
        }
        if compiled_standard_builtins.iter().any(|builtin| {
            matches!(
                builtin,
                StandardBuiltinId::StringPrototypeToLowerCase
                    | StandardBuiltinId::StringPrototypeToLocaleLowerCase
            )
        }) {
            pool.intern_string("ς");
        }
        // Computed patterns and scoped i modifiers may require either
        // character domain without any statically known backreference.
        // Compiler sets, boundaries and matcher references share these tables.
        for folding in [RegExpCaseFolding::Legacy, RegExpCaseFolding::Unicode] {
            pool.needed_regexp_case_folding.insert(folding);
        }
        pool.append_regexp_case_folding_tables();
        pool.append_regexp_unicode_property_image();
        if compiled_standard_builtins.iter().any(|builtin| {
            matches!(
                builtin,
                StandardBuiltinId::StringPrototypeToLowerCase
                    | StandardBuiltinId::StringPrototypeToLocaleLowerCase
            )
        }) {
            pool.append_lowercase_tables();
        }
        if compiled_standard_builtins.iter().any(|builtin| {
            matches!(
                builtin,
                StandardBuiltinId::StringPrototypeToUpperCase
                    | StandardBuiltinId::StringPrototypeToLocaleUpperCase
            )
        }) {
            pool.append_uppercase_tables();
        }
        for property in ["offset", "offsetNanoseconds"] {
            pool.intern_string(property);
        }
        // The `Temporal.PlainDate` error messages and parser-internal literals
        // stay behind a gate: the family is 26 builtins wide and a script that
        // never touches a date would otherwise carry all of it in its data
        // segment. The gate is "any member compiled", not "the constructor
        // compiled", because the stub predicate decides per builtin.
        //
        // The property NAMES are deliberately NOT here — every builtin
        // constructor object is initialized regardless of what the script
        // references, so `install_temporal_plain_date_constructor_intrinsics`
        // needs them unconditionally.
        // The calendar-helper gate also emits the shared PlainDate converter,
        // before discovery necessarily roots its public builtin consumers.
        if uses_temporal_calendar
            || compiled_standard_builtins.iter().any(|builtin| {
                matches!(
                builtin,
                StandardBuiltinId::TemporalPlainDateConstructor
                    | StandardBuiltinId::TemporalPlainDateFrom
                    | StandardBuiltinId::TemporalPlainDateCompare
                    // Zoned field replacement shares the PlainDate field
                    // reader and resolver, including their diagnostics.
                    | StandardBuiltinId::TemporalZonedDateTimePrototypeWith
                    // `Duration` round/total/compare resolve `relativeTo`
                    // through `ToTemporalDateTime` and the ISO calendar math.
                    | StandardBuiltinId::TemporalDurationPrototypeRound
                    | StandardBuiltinId::TemporalDurationPrototypeTotal
                    | StandardBuiltinId::TemporalDurationCompare
            ) || builtin
                .debug_name()
                .starts_with("Temporal.PlainDate.prototype.")
                || builtin
                    .debug_name()
                    .starts_with("get Temporal.PlainDate.prototype.")
                // `Temporal.PlainDateTime`, `Temporal.PlainYearMonth` and
                // `Temporal.PlainMonthDay` reuse the `Temporal.PlainDate`
                // emitters wholesale, so they need their literals too.
                || builtin.debug_name().contains("Temporal.PlainDateTime")
                || builtin.debug_name().contains("Temporal.PlainYearMonth")
                || builtin.debug_name().contains("Temporal.PlainMonthDay")
            })
        {
            for value in [
                "calendarId",
                "year",
                "month",
                "monthCode",
                "day",
                "equals",
                "toString",
                "from",
                "calendar",
                "timeZone",
                "overflow",
                "constrain",
                "reject",
                "calendarName",
                "auto",
                "always",
                "never",
                "critical",
                "iso8601",
                "",
                "-",
                "+",
                "[u-ca=",
                "[!u-ca=",
                "]",
                "M01",
                "M02",
                "M03",
                "M04",
                "M05",
                "M06",
                "M07",
                "M08",
                "M09",
                "M10",
                "M11",
                "M12",
                "M13",
                // `until`/`since` reject an out-of-range smallestUnit or
                // largestUnit with this one message for both options.
            ] {
                pool.intern_string(value);
            }
        }
        // The `Temporal.PlainYearMonth` and `Temporal.PlainMonthDay` families
        // each share one prototype, and a realm bootstrap installs the whole
        // family without every member showing up in
        // `compiled_standard_builtins`, so these are interned unconditionally,
        // the same way the `Temporal.PlainTime` set above is.
        {
            for value in [
                "",
                "+",
                "-",
                "-01",
                "01",
                "1972",
                "1972-",
                // The shared `ToTemporalCalendarIdentifier` helper inlines the
                // ISO-date parser and is emitted for every calendar-bearing
                // Temporal family, so its PlainDate diagnostics cannot stay
                // behind the PlainDate-only gate above.
                // ZonedDateTime differences now emit AddISODate directly.
                // Its RejectISODate diagnostics are shared arithmetic inputs,
                // even when no Plain-family builtin body is compiled.
                // The same shared helper's final time-string arm resolves
                // calendar annotations rather than using PlainTime's
                // ignore-calendar policy.
                "M01",
                "M02",
                "M03",
                "M04",
                "M05",
                "M06",
                "M07",
                "M08",
                "M09",
                "M10",
                "M11",
                "M12",
                "M13",
                "PlainMonthDay",
                "PlainYearMonth",
                "Temporal.PlainMonthDay",
                // `ToTemporalMonthDay` step (k): a non-ISO calendar bounds the
                // *parsed* date by `ISODateWithinLimits`, so this is thrown by
                // the shared `emit_temporal_iso_date_within_limits` rather than
                // by a `Temporal.PlainDate` emitter, and it needs its own row.
                // `ToTemporalMonthDay` step (g). `emit_throw_*_range_error`
                // resolves its message through `StringPool::payload`, which
                // panics rather than interning, so an emitter made reachable
                // without a row here is a compiler panic on every program that
                // touches `Temporal.PlainMonthDay.from` or `.prototype.equals`,
                // not a test failure.
                "Temporal.PlainYearMonth",
                "[!u-ca=",
                "[u-ca=",
                "]",
                "add",
                "always",
                "auto",
                "calendar",
                "calendarId",
                "calendarName",
                "compare",
                "constrain",
                "critical",
                "day",
                "daysInMonth",
                "daysInYear",
                "equals",
                "era",
                "eraYear",
                "from",
                "inLeapYear",
                "iso8601",
                "largestUnit",
                "month",
                "monthCode",
                "monthsInYear",
                "never",
                "overflow",
                "reject",
                "roundingIncrement",
                "roundingMode",
                "since",
                "smallestUnit",
                "subtract",
                "timeZone",
                "toJSON",
                "toLocaleString",
                "toPlainDate",
                "toString",
                "until",
                "valueOf",
                "with",
                "year",
            ] {
                pool.intern_string(value);
            }
        }
        // The `Temporal.PlainTime` family shares one prototype, and a realm
        // bootstrap installs the whole family without every member showing up
        // in `compiled_standard_builtins`, so these are interned
        // unconditionally, the same way the Duration set below is.
        {
            for value in [
                "PlainTime",
                "Temporal.PlainTime",
                "until",
                "since",
                "equals",
                "overflow",
                "constrain",
                "reject",
                "calendar",
                "timeZone",
                ":",
                "0000-01-01T",
            ] {
                pool.intern_string(value);
            }
        }
        // The `Temporal.PlainDateTime` family shares one prototype and is
        // installed wholesale by a realm bootstrap, so its literals are interned
        // unconditionally the same way the PlainTime set above is.
        //
        // THIS BLOCK IS ALSO WHAT KEEPS `Temporal.ZonedDateTime.prototype`
        // INSTALLABLE, which its name does not say. `withCalendar`, `add`,
        // `subtract`, `until` and `since` below are the property keys
        // `install_temporal_zoned_date_time_constructor_intrinsics` passes to
        // `emit_object_define_function_data`, which reaches
        // `StringPool::payload` — and that PANICS rather than degrading on a
        // string that was never interned (the failure mode measured this batch
        // as 24 red `--lib` tests on ``string `...` must exist in pool``, from
        // the two ZonedDateTime guard messages). Being unconditional is what
        // makes that safe today. If this block is ever put behind a
        // PlainDateTime predicate, the ZonedDateTime prototype install goes with
        // it and every program touching `Temporal.ZonedDateTime` panics at emit;
        // give those five their own gate keyed on
        // `names::TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS` at the same time.
        {
            for value in [
                "PlainDateTime",
                "iso8601",
                "calendar",
                "timeZone",
                "calendarName",
                "auto",
                "always",
                "never",
                "critical",
                "overflow",
                "constrain",
                "reject",
                "smallestUnit",
                "largestUnit",
                "roundingMode",
                "roundingIncrement",
                "fractionalSecondDigits",
                "year",
                "month",
                "monthCode",
                "day",
                "hour",
                "minute",
                "second",
                "millisecond",
                "microsecond",
                "nanosecond",
                "with",
                "add",
                "subtract",
                "until",
                "since",
                "round",
                "equals",
                "from",
                "compare",
                "toString",
                "toJSON",
                "toLocaleString",
                "valueOf",
                "withCalendar",
                "calendarId",
                "era",
                "eraYear",
                "dayOfWeek",
                "dayOfYear",
                "weekOfYear",
                "yearOfWeek",
                "daysInWeek",
                "daysInMonth",
                "daysInYear",
                "monthsInYear",
                "inLeapYear",
                "direction",
                "plainTime",
                "hoursInDay",
                "previous",
                "Temporal.ZonedDateTime rounded ISO date is outside the supported range",
                "[!",
                "timeZoneName",
                "",
                "-",
                "+",
                ".",
                ":",
                "[u-ca=",
                "[!u-ca=",
                "]",
                "UTC",
                "M01",
                "M02",
                "M03",
                "M04",
                "M05",
                "M06",
                "M07",
                "M08",
                "M09",
                "M10",
                "M11",
                "M12",
                "M13",
                "Temporal.PlainDateTime",
                "withPlainTime",
                "toPlainDate",
                "toPlainTime",
                "toZonedDateTime",
                "T",
                "Temporal.PlainDateTime is not a valid ISO date",
                "Invalid Temporal.PlainDateTime calendar",
                "Temporal.PlainDateTime fields require year",
                "Temporal.PlainDateTime fields require day",
                "Temporal.PlainDateTime fields require month or monthCode",
                "Invalid Temporal.PlainDateTime monthCode",
                "Temporal.PlainDateTime month and monthCode must agree",
            ] {
                pool.intern_string(value);
            }
        }
        // The `Temporal.Duration` family shares one prototype, and a realm
        // bootstrap installs the whole family without every member showing up
        // in `compiled_standard_builtins`, so these are interned
        // unconditionally rather than gated on a member reference.
        {
            for value in [
                "Duration",
                "Temporal.Duration",
                "years",
                "months",
                "weeks",
                "days",
                "hours",
                "minutes",
                "seconds",
                "milliseconds",
                "microseconds",
                "nanoseconds",
                "year",
                "month",
                "week",
                "day",
                "hour",
                "minute",
                "second",
                "millisecond",
                "microsecond",
                "nanosecond",
                "sign",
                "blank",
                "negated",
                "abs",
                "add",
                "subtract",
                "round",
                "total",
                "with",
                "from",
                "compare",
                "toString",
                "toJSON",
                "toLocaleString",
                "valueOf",
                "largestUnit",
                "fractionalSecondDigits",
                "smallestUnit",
                "roundingIncrement",
                "roundingMode",
                "relativeTo",
                "unit",
                "auto",
                "ceil",
                "floor",
                "expand",
                "trunc",
                "halfCeil",
                "halfFloor",
                "halfExpand",
                "halfTrunc",
                "halfEven",
                "P",
                "T",
                "Y",
                "M",
                "W",
                "D",
                "H",
                "S",
                "PT0S",
                "-",
                "+",
                ".",
                "0",
                "",
                "Temporal.Duration relativeTo target is outside the representable range",
            ] {
                pool.intern_string(value);
            }
        }
        // Both complete converters are emitted with the calendar helpers. Their
        // literals must use the same gate during every dependency-discovery pass,
        // including a pass that only sees a PlainDate consumer.
        if uses_temporal_calendar
            || compiled_standard_builtins.contains(&StandardBuiltinId::TemporalZonedDateTimeFrom)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeWith)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime)
            // `Duration` round/total/compare resolve zoned `relativeTo`
            // strings through the same two-pass parse.
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalDurationPrototypeRound)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalDurationPrototypeTotal)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalDurationCompare)
            // These bodies use shared inverse/offset policy, and equals,
            // compare, until and since also convert through the compiled From.
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeAdd)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeUntil)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeSince)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeRound)
            || compiled_standard_builtins
                .contains(&StandardBuiltinId::TemporalZonedDateTimePrototypeEquals)
            || compiled_standard_builtins.contains(&StandardBuiltinId::TemporalZonedDateTimeCompare)
        {
            for value in [
                "Temporal.PlainDate.prototype.toZonedDateTime options must be an object or undefined",
                "calendar",
                "day",
                "hour",
                "microsecond",
                "millisecond",
                "minute",
                "month",
                "monthCode",
                "nanosecond",
                "offset",
                "second",
                "timeZone",
                "year",
                "M01",
                "M02",
                "M03",
                "M04",
                "M05",
                "M06",
                "M07",
                "M08",
                "M09",
                "M10",
                "M11",
                "M12",
                "M13",
                "UTC",
                "iso8601",
                "Invalid Temporal.ZonedDateTime calendar annotation",
                "Temporal.ZonedDateTime offset does not match its fixed time zone",
            ] {
                pool.intern_string(value);
            }
            // The unconditional runtime-error catalog already pools every
            // typed diagnostic. This closed option walk adds the property and
            // accepted spellings for every policy/converter consumer.
            for key in ZonedDateTimeOptionKey::ALL {
                pool.intern_string(key.property());
                for (spelling, _) in key.allowed() {
                    pool.intern_string(spelling);
                }
            }
        }
        if compiled_standard_builtins.contains(&StandardBuiltinId::StringPrototypeNormalize) {
            for form in StringNormalizationForm::ALL {
                pool.intern_string(form.spelling());
            }
            pool.append_normalization_tables();
        }
        for &(name, _) in scriptw_ir::UINT8_ARRAY_CODEC_STATIC_MEMBERS
            .iter()
            .chain(scriptw_ir::UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS.iter())
        {
            pool.intern_string(name);
        }
        for value in [
            "alphabet",
            "base64",
            "base64url",
            "lastChunkHandling",
            "loose",
            "strict",
            "stop-before-partial",
            "omitPadding",
            "read",
            "written",
        ] {
            pool.intern_string(value);
        }
        if uses_temporal_calendar {
            pool.append_temporal_east_asian_year_image()?;
        }
        // The hidden per-Realm Intl fallback Symbol is allocated by native R
        // formatter bodies, so its fixed description belongs to R's prefix.
        pool.intern_string("IntlLegacyConstructedSymbol");
        // Unconditional: gating on script heap use would shift later slots.
        for symbol in scriptw_ir::WellKnownSymbol::ALL {
            pool.intern_string(symbol.description());
        }
        // Every created Realm installs these globals by name.
        for binding in crate::builtins::created_realm_global_bindings() {
            pool.intern_string(&binding.name);
        }
        // Runtime-owned function names and `toString` text are embedded in
        // builtin and bootstrap bodies, so they belong to this phase. The
        // script's own functions are interned by `collect_script`.
        for meta in function_metas
            .iter()
            .filter(|(id, meta)| is_runtime_owned_function(id, meta))
            .map(|(_, meta)| meta)
        {
            pool.intern_string(&meta.name);
            pool.intern_string(meta.runtime_name());
            pool.intern_string(&meta.to_string_value);
        }
        // The compiler-generated empty dynamic-function bodies bind these in
        // every heap-backed program.
        for name in [
            scriptw_ir::LEXICAL_ARGUMENTS_NAME,
            scriptw_ir::LEXICAL_NEW_TARGET_NAME,
            scriptw_ir::LEXICAL_THIS_NAME,
        ] {
            pool.intern_string(name);
        }
        pool.append_runtime_string_descriptors()?;
        pool.compiler_owned = pool.boundary();
        Ok(Self(pool))
    }

    fn collect_script(
        self,
        script: &ScriptIr,
        function_metas: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<StringPool, EmitError> {
        let mut pool = self.0;
        // `RegExpPrototypeCompile` is what makes the `CallMethod` arm below
        // able to serve its stated purpose. That arm collects the pattern
        // argument of `r.compile("…")` but deliberately does not set this flag,
        // and for the `var r = /[ab]/; function go() { r.compile("xy"); }`
        // spelling there is no *other* setter — so before this disjunct the
        // collected literal was written into a set that was never read, the
        // table was never built, and `emit_runtime_regexp_program_slots`
        // early-returned on a zero row count.
        //
        // Keyed off whether the script references those builtins, not off the
        // compiled-builtin set: every heap-backed module compiles all builtins,
        // and the fallback candidate set is every script string literal, so
        // this table is program-owned data. `RegExpConstructor` itself must
        // NOT be added the same way without measuring: any RegExp literal
        // references it.
        pool.needs_runtime_regexp_programs =
            script.functions.iter().any(|function| {
                function.super_constructor_target.as_deref() == Some(BUILTIN_REGEXP_FUNCTION_ID)
            }) || crate::planning::script_references_standard_builtin(
                script,
                StandardBuiltinId::RegExpPrototypeSymbolSplit,
            ) || crate::planning::script_references_standard_builtin(
                script,
                StandardBuiltinId::RegExpPrototypeCompile,
            );
        // Global declaration admission precedes every body effect, including
        // Scripts whose lexical declarations have no heap-using initializer.
        pool.uses_heap |= script.global_bindings.has_restricted_lexical_declarations();
        for binding in script.global_bindings.iter() {
            pool.intern_string(&binding.name);
        }
        for prepared in &script.prepared_dynamic_functions {
            for argument in &prepared.arguments {
                pool.intern_string(argument);
            }
            pool.intern_source_runtime_error_message(SourceRuntimeErrorMessage::PreparedFunction(
                &prepared.outcome,
            ));
        }
        for meta in function_metas
            .iter()
            .filter(|(id, meta)| !is_runtime_owned_function(id, meta))
            .map(|(_, meta)| meta)
        {
            pool.intern_string(&meta.name);
            pool.intern_string(meta.runtime_name());
            pool.intern_string(&meta.to_string_value);
        }
        for function in &script.functions {
            if let Some(plan) = &function.class_instance_element_plan {
                for element in &plan.elements {
                    let key = match element {
                        ClassInstanceElementIr::Field(field) => &field.key,
                        ClassInstanceElementIr::AutoAccessorBacking(_) => continue,
                    };
                    match key {
                        ClassFieldKeyIr::Public(key) => pool.intern_string(key.as_str()),
                        ClassFieldKeyIr::ComputedPublic(_) => {}
                        ClassFieldKeyIr::Private(_) => {}
                    }
                }
            }
            pool.collect_eval_environment(function.eval_environment.as_ref());
            for param in &function.params {
                pool.intern_string(&param.name);
                if let Some(default_init) = &param.default_init {
                    pool.collect_expr(default_init);
                }
            }
            for binding in &function.owned_env_bindings {
                pool.intern_string(&binding.name);
            }
            for binding in &function.captured_bindings {
                pool.intern_string(&binding.name);
            }
            pool.collect_block(&function.body);
        }
        pool.collect_eval_environment(script.eval_environment.as_ref());
        for body in script.executable_script_bodies() {
            pool.collect_block(body);
        }
        for prepared in &script.prepared_scripts {
            pool.intern_string(&prepared.source);
            pool.intern_source_runtime_error_message(SourceRuntimeErrorMessage::PreparedScript(
                &prepared.outcome,
            ));
        }
        for unit in script.prepared_script_units() {
            pool.collect_eval_environment(unit.eval_environment.as_ref());
            for binding in &unit.owned_env_bindings {
                pool.intern_string(&binding.name);
            }
            for binding in &unit.global_bindings {
                pool.intern_string(&binding.name);
            }
            for name in unit.global_bindings.lexical_names() {
                pool.intern_string(name);
            }
            pool.collect_global_var_declarations(unit)?;
        }
        if pool.needs_runtime_regexp_programs {
            pool.queue_runtime_regexp_programs();
        }
        pool.append_regexp_programs();
        pool.numeric_call_tables.validate()?;
        Ok(pool)
    }
}

impl StringPool {
    fn append_normalization_tables(&mut self) {
        let tables = normalization::tables();
        self.align_bytes(8);
        let start = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        let canonical_sequences_ptr = self.append_codepoints(&tables.canonical_sequences);
        self.canonical_decomposition_table_ptr =
            self.append_normalization_mappings(&tables.canonical_mappings, canonical_sequences_ptr);
        self.canonical_decomposition_count = tables.canonical_mappings.len() as u32;
        let compatibility_sequences_ptr = self.append_codepoints(&tables.compatibility_sequences);
        self.compatibility_decomposition_table_ptr = self.append_normalization_mappings(
            &tables.compatibility_mappings,
            compatibility_sequences_ptr,
        );
        self.compatibility_decomposition_count = tables.compatibility_mappings.len() as u32;

        self.align_bytes(8);
        self.combining_class_table_ptr = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        for (codepoint, combining_class) in &tables.combining_classes {
            self.bytes.extend_from_slice(&codepoint.to_le_bytes());
            self.bytes
                .extend_from_slice(&u32::from(*combining_class).to_le_bytes());
        }
        self.combining_class_count = tables.combining_classes.len() as u32;

        self.align_bytes(8);
        self.composition_table_ptr = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        for (first, second, composed) in &tables.compositions {
            self.bytes.extend_from_slice(&first.to_le_bytes());
            self.bytes.extend_from_slice(&second.to_le_bytes());
            self.bytes.extend_from_slice(&composed.to_le_bytes());
            self.bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        self.composition_count = tables.compositions.len() as u32;
        self.align_bytes(8);
        self.normalization_region = Some(NormalizationRegion {
            start,
            end: STATIC_DATA_OFFSET + self.bytes.len() as u32,
        });
    }

    fn append_codepoints(&mut self, codepoints: &[u32]) -> u32 {
        self.align_bytes(8);
        let table_ptr = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        for codepoint in codepoints {
            self.bytes.extend_from_slice(&codepoint.to_le_bytes());
        }
        table_ptr
    }

    fn append_normalization_mappings(
        &mut self,
        mappings: &[NormalizationMapping],
        sequences_ptr: u32,
    ) -> u32 {
        self.align_bytes(8);
        let table_ptr = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        for mapping in mappings {
            self.bytes
                .extend_from_slice(&mapping.codepoint.to_le_bytes());
            self.bytes
                .extend_from_slice(&(sequences_ptr + mapping.sequence_index * 4).to_le_bytes());
            self.bytes
                .extend_from_slice(&mapping.sequence_len.to_le_bytes());
            self.bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        table_ptr
    }

    fn append_codepoint_ranges(&mut self, ranges: &[std::ops::RangeInclusive<u32>]) -> u32 {
        self.align_bytes(8);
        let table_ptr = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        for range in ranges {
            self.bytes.extend_from_slice(&range.start().to_le_bytes());
            self.bytes.extend_from_slice(&range.end().to_le_bytes());
        }
        table_ptr
    }

    fn align_bytes(&mut self, alignment: usize) {
        let padding = (alignment - self.bytes.len() % alignment) % alignment;
        self.bytes.resize(self.bytes.len() + padding, 0);
    }

    fn collect_eval_environment(&mut self, role: Option<&scriptw_ir::EvalEnvironmentRoleIr>) {
        match role {
            Some(scriptw_ir::EvalEnvironmentRoleIr::Declarative { bindings, .. }) => {
                for binding in bindings {
                    self.intern_string(&binding.source_name);
                }
            }
            Some(scriptw_ir::EvalEnvironmentRoleIr::WithObject { .. }) | None => {}
        }
    }

    fn collect_lexical_environment(&mut self, environment: Option<&LexicalEnvironmentIr>) {
        if let Some(environment) = environment {
            self.collect_eval_environment(environment.eval_environment.as_ref());
        }
    }

    fn collect_for_in_of_environment(&mut self, environment: Option<&ForInOfEnvironmentIr>) {
        if let Some(environment) = environment {
            self.collect_lexical_environment(environment.tdz_environment.as_ref());
            self.collect_lexical_environment(environment.iteration_environment.as_ref());
        }
    }

    fn collect_resumable_iteration_environment(
        &mut self,
        environment: &ResumableLoopIterationEnvironmentIr,
    ) {
        match environment {
            ResumableLoopIterationEnvironmentIr::StorageOnly => {}
            ResumableLoopIterationEnvironmentIr::FreshPerIteration(environment) => {
                self.collect_lexical_environment(Some(environment));
            }
        }
    }

    pub(crate) fn numeric_call_table(
        &self,
        run: &crate::call_rows::NumericCallRun<'_>,
    ) -> Result<crate::call_rows::NumericCallTable, EmitError> {
        self.numeric_call_tables.find(run)
    }

    fn collect_block(&mut self, block: &BlockIr) {
        self.collect_lexical_environment(block.lexical_environment.as_ref());
        let mut next = 0;
        while next < block.statements.len() {
            if let Some(run) =
                crate::call_rows::NumericCallRun::of_prefix(&block.statements[next..])
            {
                self.numeric_call_tables.collect(&run, &mut self.bytes);
                next += run.len();
            } else {
                next += 1;
            }
        }
        for statement in &block.statements {
            self.collect_statement(statement);
        }
    }

    fn collect_statement(&mut self, statement: &StatementIr) {
        match statement {
            StatementIr::ResumableClassDefinition(plan) => {
                self.collect_expr(plan.expression());
                for statement in plan.prefixes().flat_map(|prefix| prefix.statements()) {
                    self.collect_statement(statement);
                }
            }
            StatementIr::ModuleImportBinding(_) => self.uses_heap = true,
            StatementIr::ModuleUnitOnce { block, .. } => self.collect_block(block),
            StatementIr::AsyncModuleInstantiation
            | StatementIr::Empty
            | StatementIr::Debugger
            | StatementIr::Break { .. }
            | StatementIr::Continue { .. } => {}
            StatementIr::Lexical { name, init, .. } => {
                self.intern_string(name);
                collect_finite_string_choices(init, &mut self.runtime_regexp_candidate_literals);
                self.collect_expr(init);
            }
            StatementIr::AnnexBFunctionCopy {
                source_name,
                block_storage_name,
                target,
                admission,
            } => {
                if let Some(admission) = admission {
                    self.intern_string(&admission.name);
                }
                self.intern_string(source_name);
                self.intern_string(block_storage_name);
                match target {
                    AnnexBFunctionCopyTargetIr::OwnerBinding { storage_name } => {
                        self.intern_string(storage_name);
                    }
                    AnnexBFunctionCopyTargetIr::ScriptGlobal { name }
                    | AnnexBFunctionCopyTargetIr::DirectEvalVariable { name }
                    | AnnexBFunctionCopyTargetIr::FunctionVariableEnvironment { name } => {
                        self.intern_string(name);
                    }
                }
            }
            StatementIr::DeclarationEvaluation(init) | StatementIr::Expression(init) => {
                self.collect_expr(init)
            }
            StatementIr::GeneratorYield {
                value,
                form,
                resume_mode,
                ..
            } => {
                match form {
                    YieldForm::Plain => {}
                    YieldForm::Delegate(_) => {
                        for key in [
                            "Symbol.iterator",
                            "next",
                            "done",
                            "value",
                            "return",
                            "throw",
                        ] {
                            self.intern_string(key);
                        }
                    }
                }
                if let GeneratorResumeModeIr::AssignGlobal { name, .. } = resume_mode {
                    self.intern_string(name);
                }
                if let GeneratorResumeModeIr::AssignProperty(reference) = resume_mode {
                    match reference.use_view() {
                        SuspendedPropertyReferenceUse::Ordinary {
                            base_and_receiver,
                            key,
                            strictness: _,
                        } => {
                            self.collect_expr(base_and_receiver);
                            self.collect_property_key(key);
                        }
                    }
                }
                self.collect_expr(value);
            }
            StatementIr::AsyncAwait {
                value, resume_mode, ..
            } => {
                if let scriptw_ir::AsyncResumeModeIr::AssignGlobal { name, .. } = resume_mode {
                    self.intern_string(name);
                }
                self.collect_expr(value);
            }
            StatementIr::Return(value) => self.collect_expr(value),
            StatementIr::Throw(value) => self.collect_expr(value),
            StatementIr::Var(declarators) => self.collect_var_declarators(declarators),
            StatementIr::EmptyStatementCompletion(item) => self.collect_statement(item.statement()),
            StatementIr::LexicalBlock(statements)
            | StatementIr::ParameterInitialization { statements, .. } => {
                for statement in statements {
                    self.collect_statement(statement);
                }
            }
            StatementIr::SyncDisposableScope {
                resources, body, ..
            } => {
                for value in ["Symbol.dispose"] {
                    self.intern_string(value);
                }
                for resource in resources.iter() {
                    self.collect_expr(&resource.initializer);
                }
                self.collect_block(body);
            }
            StatementIr::AsyncDisposableScope {
                resources, body, ..
            } => {
                for value in ["Symbol.asyncDispose", "Symbol.dispose"] {
                    self.intern_string(value);
                }
                for resource in resources.iter() {
                    self.collect_expr(resource.initializer());
                }
                self.collect_block(body);
            }
            StatementIr::Block(block) => self.collect_block(block),
            StatementIr::TryCatch {
                try_block,
                catch_block,
                catch_name,
                catch_source_name,
                catch_parameter_environment,
                ..
            } => {
                self.intern_string(catch_name);
                self.intern_string(catch_source_name);
                self.collect_lexical_environment(catch_parameter_environment.as_ref());
                self.collect_block(try_block);
                self.collect_block(catch_block);
            }
            StatementIr::TryFinally {
                try_block,
                finally_block,
                ..
            } => {
                self.collect_block(try_block);
                self.collect_block(finally_block);
            }
            StatementIr::TryCatchFinally {
                try_block,
                catch_block,
                finally_block,
                catch_name,
                catch_source_name,
                catch_parameter_environment,
                ..
            } => {
                self.intern_string(catch_name);
                self.intern_string(catch_source_name);
                self.collect_lexical_environment(catch_parameter_environment.as_ref());
                self.collect_block(try_block);
                self.collect_block(catch_block);
                self.collect_block(finally_block);
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
                self.collect_expr(condition);
                self.collect_statement(then_branch);
                if let Some(else_branch) = else_branch {
                    self.collect_statement(else_branch);
                }
            }
            StatementIr::AsyncFunctionWhile(plan) => {
                for statement in plan.condition_prefix() {
                    self.collect_statement(statement);
                }
                self.collect_expr(plan.condition());
                self.collect_statement(plan.body());
            }
            StatementIr::While { condition, body } => {
                self.collect_expr(condition);
                self.collect_statement(body);
            }
            StatementIr::DoWhile { body, condition } => {
                self.collect_statement(body);
                self.collect_expr(condition);
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                lexical_environment,
            } => {
                if let Some(environment) = lexical_environment {
                    self.collect_eval_environment(environment.eval_environment.as_ref());
                }
                if let Some(init) = init {
                    self.collect_for_init(init);
                }
                if let Some(test) = test {
                    self.collect_expr(test);
                }
                if let Some(update) = update {
                    self.collect_expr(update);
                }
                self.collect_statement(body);
            }
            StatementIr::OrdinaryGeneratorLoop(plan) => {
                if let Some(environment) = plan.lexical_environment() {
                    self.collect_eval_environment(environment.eval_environment.as_ref());
                }
                for region in plan.regions() {
                    self.collect_block(region.block());
                }
                self.collect_expr(plan.test().value());
                if let Some(update) = plan.update() {
                    self.collect_expr(update.value());
                }
            }
            StatementIr::AsyncGeneratorLoop(plan) => {
                if plan.resource().is_some() {
                    self.uses_heap = true;
                    for key in ["Symbol.dispose", "Symbol.asyncDispose"] {
                        self.intern_string(key);
                    }
                }
                if let Some(environment) = plan.lexical_environment() {
                    self.collect_eval_environment(environment.eval_environment.as_ref());
                }
                for region in plan.regions() {
                    self.collect_block(region.block());
                }
                self.collect_expr(plan.test().value());
                if let Some(update) = plan.update() {
                    self.collect_expr(update.value());
                }
            }
            StatementIr::OrdinaryGeneratorIf(plan) => {
                self.collect_expr(plan.condition());
                self.collect_block(plan.then_branch().block());
                self.collect_block(plan.else_branch().block());
            }
            StatementIr::AsyncGeneratorIf(plan) => {
                self.collect_block(plan.condition().region().block());
                self.collect_expr(plan.condition().value());
                self.collect_block(plan.then_branch().block());
                self.collect_block(plan.else_branch().block());
            }
            StatementIr::AsyncGeneratorSwitch(plan) => {
                if plan.resource().is_some() {
                    self.uses_heap = true;
                    for key in ["Symbol.dispose", "Symbol.asyncDispose"] {
                        self.intern_string(key);
                    }
                }
                self.collect_lexical_environment(plan.lexical_environment());
                for declaration in plan.lexical_declarations() {
                    self.collect_statement(declaration);
                }
                for region in plan.regions() {
                    self.collect_block(region.block());
                }
                for expression in plan.expressions() {
                    self.collect_expr(expression);
                }
            }
            StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
                self.uses_heap = true;
                for key in ["Symbol.iterator", "next", "done", "value", "return"] {
                    self.intern_string(key);
                }
                self.collect_expr(plan.raw_source());
                self.collect_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorResourceScope(plan) => {
                self.uses_heap = true;
                for key in ["Symbol.dispose", "Symbol.asyncDispose"] {
                    self.intern_string(key);
                }
                self.collect_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorResourceRegistration(operation) => {
                self.collect_expr(operation.initializer())
            }
            StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
                self.uses_heap = true;
                for key in ["Symbol.iterator", "next", "done", "value", "return"] {
                    self.intern_string(key);
                }
                self.collect_expr(plan.raw_source());
                self.collect_block(plan.body().block());
            }
            StatementIr::AsyncFunctionArrayDestructuring(plan) => {
                self.uses_heap = true;
                for key in ["Symbol.iterator", "next", "done", "value", "return"] {
                    self.intern_string(key);
                }
                self.collect_expr(plan.raw_source());
                self.collect_block(plan.body());
            }
            StatementIr::OrdinaryGeneratorWith(plan) => {
                self.uses_heap = true;
                self.collect_block(plan.head().region().block());
                self.collect_expr(plan.head().value());
                self.collect_lexical_environment(Some(plan.lexical_environment()));
                self.collect_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorWith(plan) => {
                self.uses_heap = true;
                self.collect_block(plan.head().region().block());
                self.collect_expr(plan.head().value());
                self.collect_lexical_environment(Some(plan.lexical_environment()));
                self.collect_block(plan.body().block());
            }
            StatementIr::AsyncFunctionWith(plan) => {
                self.uses_heap = true;
                self.collect_block(plan.head());
                self.collect_expr(plan.head_value());
                self.collect_lexical_environment(Some(plan.lexical_environment()));
                self.collect_block(plan.body());
            }
            StatementIr::AsyncGeneratorForIn(plan) => {
                self.uses_heap = true;
                self.collect_for_in_of_environment(plan.lexical_environment());
                self.collect_block(plan.head().region().block());
                self.collect_expr(plan.head().value());
                self.collect_block(plan.initialization());
                self.collect_block(plan.body().block());
            }
            StatementIr::AsyncGeneratorForOf(plan) => {
                self.uses_heap = true;
                if plan.resource().is_some() {
                    for key in ["Symbol.dispose", "Symbol.asyncDispose"] {
                        self.intern_string(key);
                    }
                }
                for key in [
                    "Symbol.iterator",
                    "Symbol.asyncIterator",
                    "next",
                    "done",
                    "value",
                    "return",
                ] {
                    self.intern_string(key);
                }
                self.collect_for_in_of_environment(plan.lexical_environment());
                self.collect_block(plan.head().region().block());
                self.collect_expr(plan.head().value());
                self.collect_block(plan.initialization().block());
                self.collect_block(plan.body().block());
            }
            StatementIr::ArrayDestructuringOperation(_) => {
                self.uses_heap = true;
                for key in ["Symbol.iterator", "next", "done", "value", "return"] {
                    self.intern_string(key);
                }
            }
            StatementIr::OrdinaryGeneratorSwitch(plan) => {
                self.collect_block(plan.discriminant().region().block());
                self.collect_expr(plan.discriminant().value());
                if let Some(environment) = plan.lexical_environment() {
                    self.collect_eval_environment(environment.eval_environment.as_ref());
                }
                for declaration in plan.lexical_declarations() {
                    self.collect_statement(declaration);
                }
                for case in plan.cases() {
                    if let Some(selector) = case.selector() {
                        self.collect_block(selector.region().block());
                        self.collect_expr(selector.value());
                    }
                    self.collect_block(case.body().block());
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
                self.collect_resumable_iteration_environment(iteration_environment);
                if let Some(init) = init {
                    self.collect_for_init(init);
                }
                if let Some(test) = test {
                    self.collect_expr(test);
                }
                if let Some(update) = update {
                    self.collect_expr(update);
                }
                for statement in before_suspension {
                    self.collect_statement(statement);
                }
                self.collect_statement(suspension_statement);
                for statement in after_suspension {
                    self.collect_statement(statement);
                }
            }
            StatementIr::AsyncFunctionForOfIterator { iterable, plan } => {
                self.collect_for_in_of_environment(plan.head_environment());
                self.collect_resumable_iteration_environment(plan.iteration_environment());
                self.collect_expr(iterable);
                for statement in plan.body().statements() {
                    self.collect_statement(statement);
                }
            }
            StatementIr::GeneratorForOfIterator { iterable, plan } => {
                self.collect_for_in_of_environment(plan.head_environment());
                self.collect_resumable_iteration_environment(plan.iteration_environment());
                self.collect_expr(iterable);
                for statement in plan.body().statements() {
                    self.collect_statement(statement);
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
                self.collect_expr(condition);
                for statement in then_before_yield
                    .iter()
                    .chain(then_yield_statement.as_deref())
                    .chain(then_after_yield)
                    .chain(else_before_yield)
                    .chain(else_yield_statement.as_deref())
                    .chain(else_after_yield)
                {
                    self.collect_statement(statement);
                }
            }
            StatementIr::ForInArray {
                target: iterable,
                body,
                lexical_environment,
                ..
            }
            | StatementIr::ForInString {
                target: iterable,
                body,
                lexical_environment,
                ..
            }
            | StatementIr::ForInObject {
                target: iterable,
                body,
                lexical_environment,
                ..
            } => {
                self.collect_for_in_of_environment(lexical_environment.as_ref());
                self.collect_expr(iterable);
                self.collect_statement(body);
            }
            StatementIr::ForOfIterator {
                head,
                iterable,
                body,
                lexical_environment,
            } => {
                self.collect_for_in_of_environment(lexical_environment.as_ref());
                match head {
                    ForOfIteratorHeadIr::Assignment { .. } => {}
                    ForOfIteratorHeadIr::SyncDisposable(head) => {
                        for value in ["Symbol.dispose"] {
                            self.intern_string(value);
                        }
                        self.intern_string(head.binding_name());
                    }
                    ForOfIteratorHeadIr::AsyncDisposable(head) => {
                        for value in ["Symbol.asyncDispose", "Symbol.dispose"] {
                            self.intern_string(value);
                        }
                        self.intern_string(head.binding_name());
                    }
                }
                self.collect_expr(iterable);
                self.collect_statement(body);
            }
            StatementIr::AsyncFunctionSwitch(plan) => {
                self.collect_lexical_environment(plan.lexical_environment());
                self.collect_expr(plan.discriminant());
                for declaration in plan.lexical_declarations() {
                    self.collect_statement(declaration);
                }
                for case in plan.cases() {
                    for statement in case.condition_prefix() {
                        self.collect_statement(statement);
                    }
                    if let Some(condition) = case.condition() {
                        self.collect_expr(condition);
                    }
                    self.collect_block(case.body());
                }
            }
            StatementIr::Switch {
                discriminant,
                lexical_declarations,
                cases,
                lexical_environment,
            } => {
                self.collect_lexical_environment(lexical_environment.as_ref());
                self.collect_expr(discriminant);
                for declaration in lexical_declarations {
                    self.collect_statement(declaration);
                }
                for case in cases {
                    if let Some(condition) = &case.condition {
                        self.collect_expr(condition);
                    }
                    self.collect_block(&case.body);
                }
            }
            StatementIr::Labelled { statement, .. } => self.collect_statement(statement),
        }
    }

    fn collect_for_init(&mut self, init: &ForInitIr) {
        match init {
            ForInitIr::Lexical { init, .. } => {
                collect_finite_string_choices(init, &mut self.runtime_regexp_candidate_literals);
                self.collect_expr(init);
            }
            ForInitIr::LexicalBlock(bindings) => {
                for binding in bindings {
                    collect_finite_string_choices(
                        &binding.init,
                        &mut self.runtime_regexp_candidate_literals,
                    );
                    self.collect_expr(&binding.init);
                }
            }
            ForInitIr::Var(declarators) => self.collect_var_declarators(declarators),
            ForInitIr::Expression(expr) => self.collect_expr(expr),
            ForInitIr::Statements(statements) => {
                for statement in statements {
                    self.collect_statement(statement);
                }
            }
            ForInitIr::SyncDisposable(resources) => {
                for value in [
                    "Symbol.dispose",
                    "using declaration resource is not an object",
                    "using declaration resource has no [Symbol.dispose] method",
                    "using declaration [Symbol.dispose] method is not callable",
                ] {
                    self.intern_string(value);
                }
                for resource in resources.iter() {
                    self.intern_string(&resource.binding_name);
                    collect_finite_string_choices(
                        &resource.initializer,
                        &mut self.runtime_regexp_candidate_literals,
                    );
                    self.collect_expr(&resource.initializer);
                }
            }
            ForInitIr::AsyncDisposable(init) => {
                for value in [
                    "Symbol.asyncDispose",
                    "Symbol.dispose",
                    "await using declaration resource is not an object",
                    "await using declaration resource has no disposal method",
                    "await using declaration [Symbol.dispose] method is not callable",
                    "await using declaration [Symbol.asyncDispose] method is not callable",
                ] {
                    self.intern_string(value);
                }
                for resource in init.resources().iter() {
                    self.collect_expr(resource.initializer());
                }
            }
        }
    }

    fn collect_var_declarators(&mut self, declarators: &[VarDeclaratorIr]) {
        for declarator in declarators {
            self.intern_string(&declarator.name);
            if let Some(init) = &declarator.init {
                collect_finite_string_choices(init, &mut self.runtime_regexp_candidate_literals);
                self.collect_expr(init);
            }
        }
    }

    fn collect_typeof_result_strings(&mut self) {
        for result in [
            "undefined",
            "object",
            "boolean",
            "number",
            "bigint",
            "symbol",
            "string",
            "function",
        ] {
            self.intern_string(result);
        }
    }

    fn collect_expr(&mut self, expr: &TypedExpr) {
        match &expr.expr {
            ExprIr::ForeignCall(call) => {
                self.uses_heap = true;
                for operand in call.operands() {
                    self.collect_expr(operand);
                }
            }
            ExprIr::BytesModuleValue(plan) => {
                self.uses_heap = true;
                if !self.module_bytes.contains_key(&plan.module()) {
                    let bounds = u32::try_from(self.bytes.len())
                        .ok()
                        .and_then(|offset| STATIC_DATA_OFFSET.checked_add(offset))
                        .zip(u32::try_from(plan.bytes().len()).ok())
                        .filter(|(address, length)| address.checked_add(*length).is_some());
                    if let Some((address, length)) = bounds {
                        self.bytes.extend_from_slice(plan.bytes());
                        self.module_bytes
                            .insert(plan.module(), PooledModuleBytes { address, length });
                    }
                }
            }
            ExprIr::JsonModuleValue(plan) => {
                self.uses_heap = true;
                let mut pending = vec![plan.value()];
                while let Some(value) = pending.pop() {
                    match value {
                        scriptw_ir::JsonValue::String(units) => {
                            self.intern_string(&scriptw_ir::encode_js_string_utf16(units));
                        }
                        scriptw_ir::JsonValue::Array(values) => pending.extend(values.iter()),
                        scriptw_ir::JsonValue::Object(entries) => {
                            for (key, value) in entries {
                                self.intern_string(&scriptw_ir::encode_js_string_utf16(key));
                                pending.push(value);
                            }
                        }
                        scriptw_ir::JsonValue::Null
                        | scriptw_ir::JsonValue::Boolean(_)
                        | scriptw_ir::JsonValue::Number(_) => {}
                    }
                }
            }
            ExprIr::EnvironmentIdentifier(identifier) => {
                self.uses_heap = true;
                self.intern_string(&identifier.name);
                if matches!(
                    identifier.operation,
                    scriptw_ir::EnvironmentIdentifierOperationIr::Typeof
                ) {
                    self.collect_typeof_result_strings();
                }
                for operand in identifier.operation.operands() {
                    self.collect_expr(operand);
                }
            }
            ExprIr::ModuleEntryEvaluation(entry) => {
                self.uses_heap = true;
                self.collect_expr(entry.evaluation());
            }
            ExprIr::ModuleExecutionGraph(graph) => {
                self.uses_heap = true;
                for source in graph.source_records() {
                    self.intern_string(source.source_tag());
                }
            }
            ExprIr::ModuleBindingRead(_)
            | ExprIr::ModuleEvaluate(_)
            | ExprIr::DeferredModuleEvaluate(_)
            | ExprIr::ModuleHasAsyncDependencies(_)
            | ExprIr::ModuleDeferredImportEvaluate(_) => {
                self.uses_heap = true;
            }
            ExprIr::ModuleNamespacePublish { namespace, .. } => self.collect_expr(namespace),
            ExprIr::ImportMeta { .. } => self.uses_heap = true,
            ExprIr::ModuleNamespace { exports, .. } => {
                self.uses_heap = true;
                self.collect_expr(exports);
                self.intern_string("Module");
                self.intern_string("then");
                self.intern_string("Symbol.toStringTag");
            }
            ExprIr::DynamicImport {
                specifier, options, ..
            } => {
                self.uses_heap = true;
                self.collect_expr(specifier);
                if let Some(options) = options {
                    self.collect_expr(options);
                }
            }
            ExprIr::Symbol { description } => {
                self.uses_heap = true;
                if let Some(description) = description {
                    self.collect_expr(description);
                }
            }
            ExprIr::WellKnownSymbol(_) => self.uses_heap = true,
            ExprIr::String(value) => {
                self.uses_heap = true;
                self.intern_string(value);
                self.script_string_literals.insert(value.clone());
            }
            ExprIr::TemplateObject(template) => {
                self.uses_heap = true;
                for cooked in template.cooked.iter().flatten() {
                    self.intern_string(cooked);
                }
                for raw in &template.raw {
                    self.intern_string(raw);
                }
                self.intern_string("raw");
            }
            ExprIr::RegExpLiteral {
                source,
                flags,
                static_compilation,
            } => {
                self.uses_heap = true;
                self.intern_string(source);
                self.intern_string(flags);
                match static_compilation {
                    Some(StaticRegExpCompilation::Program(program)) => {
                        self.queue_regexp_program(program)
                    }
                    Some(compilation @ StaticRegExpCompilation::InvalidSyntax { .. }) => {
                        self.intern_source_runtime_error_message(
                            SourceRuntimeErrorMessage::RegExp(compilation),
                        );
                    }
                    None => {}
                }
            }
            ExprIr::BigInt(_) => self.uses_heap = true,
            ExprIr::ObjectLiteral(properties) => {
                self.uses_heap = true;
                for property in properties {
                    self.collect_object_property(property);
                }
            }
            ExprIr::ObjectPropertyDefinition(definition) => {
                self.uses_heap = true;
                self.collect_expr(definition.target());
                self.collect_object_property(definition.property());
            }
            ExprIr::ArrayLiteral(elements) => {
                self.uses_heap = true;
                if let Some(plan) = CompactArrayPlan::of(elements) {
                    // A complete pure walk preserves every original string
                    // and RegExp candidate while avoiding recursive, repeated
                    // recipe collection for child arrays covered by this one.
                    for node in plan.nodes() {
                        if let CompactArrayNode::String(text) = node {
                            self.script_string_literals.insert((*text).to_owned());
                            self.runtime_regexp_candidate_literals
                                .insert((*text).to_owned());
                            self.intern_string(text);
                        }
                    }
                    let refs = &self.refs;
                    let runtime_strings = self.compiler_owned.strings();
                    let total_strings = self.pooled_string_count();
                    self.compact_array_recipes.collect(&plan, |text| {
                        refs.get(text)
                            .map(|reference| reference.index)
                            .ok_or_else(|| {
                                EmitError::unsupported(
                                    "compiler invariant: compact Array String was not interned",
                                )
                            })
                            .and_then(|index| {
                                PooledLiteralOperand::for_collected(
                                    index,
                                    runtime_strings,
                                    total_strings,
                                )
                            })
                    });
                    return;
                }
                for element in elements {
                    collect_finite_string_choices(
                        element,
                        &mut self.runtime_regexp_candidate_literals,
                    );
                    self.collect_expr(element);
                }
            }
            ExprIr::ArrayAccumulation(accumulation) => {
                self.uses_heap = true;
                for element in accumulation.elements() {
                    let value = match element {
                        ArrayAccumulationElementIr::Elision => continue,
                        ArrayAccumulationElementIr::Value(value) => value,
                        ArrayAccumulationElementIr::Spread(spread) => &spread.value,
                    };
                    collect_finite_string_choices(
                        value,
                        &mut self.runtime_regexp_candidate_literals,
                    );
                    self.collect_expr(value);
                }
            }
            ExprIr::PropertyRead { target, key } => {
                self.uses_heap = true;
                self.collect_expr(target);
                self.collect_property_key(key);
            }
            ExprIr::OptionalPropertyChain { target, chain } => {
                self.uses_heap = true;
                self.collect_expr(target);
                for operation in chain {
                    match operation {
                        OptionalChainOperationIr::Property { key, .. } => {
                            self.collect_property_key(key);
                        }
                        OptionalChainOperationIr::PrivateProperty { .. } => {}
                        OptionalChainOperationIr::Call { args, .. } => {
                            for arg in args {
                                self.collect_expr(arg);
                            }
                        }
                    }
                }
            }
            ExprIr::DeleteOptionalPropertyChain(deletion) => {
                let target = deletion.target();
                let chain = deletion.prefix();
                self.uses_heap = true;
                self.collect_expr(target);
                for operation in chain {
                    match operation {
                        OptionalChainOperationIr::Property { key, .. } => {
                            self.collect_property_key(key);
                        }
                        OptionalChainOperationIr::PrivateProperty { .. } => {}
                        OptionalChainOperationIr::Call { args, .. } => {
                            for arg in args {
                                self.collect_expr(arg);
                            }
                        }
                    }
                }

                self.collect_property_key(deletion.key());
            }
            ExprIr::PropertyWrite {
                target, key, value, ..
            } => {
                self.uses_heap = true;
                self.collect_expr(target);
                self.collect_property_key(key);
                self.collect_expr(value);
            }
            ExprIr::OrdinaryPropertyAssignment(assignment) => {
                self.uses_heap = true;
                self.collect_expr(assignment.base_and_receiver());
                self.collect_property_key(assignment.referenced_name());
                self.collect_expr(assignment.rhs());
            }
            ExprIr::OrdinaryPropertyLogicalAssignment(assignment) => {
                self.uses_heap = true;
                self.collect_expr(assignment.base_and_receiver());
                self.collect_property_key(assignment.referenced_name());
                self.collect_expr(assignment.rhs());
            }
            ExprIr::OrdinaryPropertyGetCapture(capture) => {
                self.uses_heap = true;
                self.collect_expr(capture.base_and_receiver());
                self.collect_property_key(capture.referenced_name());
            }
            ExprIr::CapturedOrdinaryPropertyWrite(write) => {
                self.uses_heap = true;
                self.collect_expr(write.rhs());
            }
            ExprIr::OrdinaryPropertyNumericUpdate(update) => {
                self.uses_heap = true;
                self.collect_expr(update.base_and_receiver());
                self.collect_property_key(update.referenced_name());
            }
            ExprIr::OrdinaryPropertyEagerCompoundAssignment(mutation) => {
                self.uses_heap = true;
                self.collect_expr(mutation.base_and_receiver());
                self.collect_property_key(mutation.referenced_name());
                self.collect_expr(mutation.result());
            }
            ExprIr::AssignIdentifier { name, value } => {
                self.intern_string(name);
                collect_finite_string_choices(value, &mut self.runtime_regexp_candidate_literals);
                self.collect_expr(value);
            }
            ExprIr::CompoundAssignIdentifier { name, value, .. } => {
                self.intern_string(name);
                self.collect_expr(value);
            }
            ExprIr::UnaryPlus { expr: value }
            | ExprIr::UnaryMinusNumeric { expr: value }
            | ExprIr::UnaryBitwiseNumeric { expr: value, .. }
            | ExprIr::LogicalNot { expr: value }
            | ExprIr::Void { expr: value }
            | ExprIr::DeleteValue { expr: value } => self.collect_expr(value),
            ExprIr::SpecOperation {
                operation,
                operands,
            } => {
                if matches!(operation, SpecOperationIr::IsLooselyEqual) {
                    self.uses_heap = true;
                }
                if matches!(operation, SpecOperationIr::ToIndex) {}
                if matches!(operation, SpecOperationIr::CreateDataPropertyOrThrow) {
                    self.uses_heap = true;
                    self.intern_string("value");
                    self.intern_string("writable");
                    self.intern_string("enumerable");
                    self.intern_string("configurable");
                    self.intern_string(
                        "CreateDataPropertyOrThrow symbol property keys are not supported",
                    );
                }
                if matches!(operation, SpecOperationIr::CopyDataProperties) {
                    self.uses_heap = true;
                    self.intern_string("enumerable");
                }
                if matches!(operation, SpecOperationIr::Set) {
                    self.uses_heap = true;
                    self.intern_string("Set symbol property keys are not supported");
                }
                if matches!(operation, SpecOperationIr::HasOwnProperty) {
                    self.uses_heap = true;
                }
                if matches!(operation, SpecOperationIr::GetMethod) {
                    self.uses_heap = true;
                }
                if matches!(operation, SpecOperationIr::Construct) {
                    self.uses_heap = true;

                    self.intern_string("Spread argument is not an array");
                }
                if matches!(operation, SpecOperationIr::DeletePropertyOrThrow) {
                    self.uses_heap = true;
                    self.intern_string(
                        "DeletePropertyOrThrow symbol property keys are not supported",
                    );
                }
                for operand in operands {
                    self.collect_expr(operand);
                }
            }
            ExprIr::DeleteIdentifier { name, .. } | ExprIr::DeleteGlobalProperty { name, .. } => {
                self.uses_heap = true;
                self.intern_string(name);
            }
            ExprIr::DeleteProperty { target, key, .. } => {
                self.uses_heap = true;
                self.collect_expr(target);
                self.collect_property_key(key);
            }
            ExprIr::TypeOf { expr } => {
                self.collect_typeof_result_strings();
                self.collect_expr(expr);
            }
            ExprIr::TypeOfUnresolvedIdentifier { name } => {
                self.uses_heap = true;
                self.intern_string(name);
                self.collect_typeof_result_strings();
            }
            ExprIr::StringFromCharCode { code } => {
                self.uses_heap = true;
                self.collect_expr(code);
            }
            ExprIr::NewTarget => {}
            ExprIr::UpdateIdentifier { name, .. } => self.intern_string(name),
            ExprIr::BinaryNumber { lhs, rhs, .. }
            | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
            | ExprIr::BitwiseNumeric { lhs, rhs, .. }
            | ExprIr::CompareNumber { lhs, rhs, .. }
            | ExprIr::CompareValue { lhs, rhs, .. }
            | ExprIr::StrictEquality { lhs, rhs, .. }
            | ExprIr::LooseEquality { lhs, rhs, .. }
            | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
            | ExprIr::Comma { lhs, rhs } => {
                if matches!(&expr.expr, ExprIr::LooseEquality { .. }) {
                    self.uses_heap = true;
                }
                for value in [
                    "",
                    ",",
                    "[object Object]",
                    "[object Arguments]",
                    "valueOf",
                    "toString",
                ] {
                    self.intern_string(value);
                }
                self.collect_expr(lhs);
                self.collect_expr(rhs);
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                self.intern_string(name);
                self.collect_expr(value);
                self.collect_expr(body);
            }
            ExprIr::ArrayDestructure { value, pattern, .. } => {
                self.uses_heap = true;
                // `enumerable` is needed by a nested object-rest target
                // (`[{ a, ...rest }] = …`) reached through this pattern.
                for key in [
                    "Symbol.iterator",
                    "next",
                    "done",
                    "value",
                    "return",
                    "enumerable",
                ] {
                    self.intern_string(key);
                }
                self.collect_expr(value);
                pattern.visit_expressions(&mut |expr| self.collect_expr(expr));
                self.collect_array_destructuring_pattern_strings(pattern);
            }
            ExprIr::ObjectDestructure { value, pattern } => {
                self.uses_heap = true;
                // The iterator-protocol keys are needed by a nested array target
                // (`{ a: [b] } = …`) reached through this pattern.
                for key in [
                    "enumerable",
                    "Symbol.iterator",
                    "next",
                    "done",
                    "value",
                    "return",
                ] {
                    self.intern_string(key);
                }
                self.collect_expr(value);
                pattern.visit_expressions(&mut |expr| self.collect_expr(expr));
                self.collect_object_destructuring_pattern_strings(pattern);
            }
            ExprIr::ObjectDestructuringOperation(operation) => {
                self.uses_heap = true;
                for key in [
                    "enumerable",
                    "Symbol.iterator",
                    "next",
                    "done",
                    "value",
                    "return",
                ] {
                    self.intern_string(key);
                }
                operation.visit_expressions(&mut |expr| self.collect_expr(expr));
                if let scriptw_ir::ObjectDestructuringOperationView::PutTarget { target, .. } =
                    operation.use_view()
                {
                    self.collect_destructuring_target_strings(target);
                }
            }
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.collect_expr(condition);
                self.collect_expr(then_expr);
                self.collect_expr(else_expr);
            }
            ExprIr::StringConcat { lhs, rhs } => {
                self.uses_heap = true;
                self.intern_string("undefined");
                self.intern_string("null");
                self.intern_string("true");
                self.intern_string("false");
                self.intern_string("NaN");
                self.intern_string("Infinity");
                self.intern_string("-Infinity");
                self.intern_string("[object Object]");
                self.intern_string("[object Arguments]");
                self.intern_string("");
                self.intern_string(",");
                self.collect_expr(lhs);
                self.collect_expr(rhs);
            }
            ExprIr::CoerciveAdd { lhs, rhs } => {
                self.uses_heap = true;
                for value in [
                    "",
                    ",",
                    "undefined",
                    "null",
                    "true",
                    "false",
                    "NaN",
                    "Infinity",
                    "-Infinity",
                    "[object Object]",
                    "[object Arguments]",
                    "valueOf",
                    "toString",
                ] {
                    self.intern_string(value);
                }
                self.collect_expr(lhs);
                self.collect_expr(rhs);
            }
            ExprIr::CallNamed { name, args } => {
                self.uses_heap = true;
                self.intern_string(name);
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            ExprIr::CaptureOptionalCallReference(capture) => {
                for operand in capture.operands() {
                    self.collect_expr(operand);
                }
            }
            ExprIr::CaptureArgumentList(capture) => {
                self.uses_heap = true;
                for argument in capture.arguments() {
                    self.collect_expr(argument);
                }
            }
            ExprIr::CapturedArgumentList(list) => self.collect_expr(list.binding()),
            ExprIr::SpreadArgument(spread) => {
                self.uses_heap = true;
                self.collect_expr(&spread.value);
            }
            ExprIr::AssertSameValue {
                actual, expected, ..
            } => {
                self.uses_heap = true;
                self.intern_string(ERROR_NAME);
                self.intern_source_runtime_error_message(SourceRuntimeErrorMessage::Expression(
                    expr,
                ));
                self.collect_expr(actual);
                self.collect_expr(expected);
            }
            ExprIr::RuntimeThrow { name, .. } => {
                self.uses_heap = true;
                self.intern_string(name.as_str());
                self.intern_source_runtime_error_message(SourceRuntimeErrorMessage::Expression(
                    expr,
                ));
            }
            ExprIr::GlobalPropertyRead { name } | ExprIr::GlobalIdentifierRead { name } => {
                self.uses_heap = true;
                self.intern_string(name);
            }
            ExprIr::GlobalPropertyWrite { name, value, .. } => {
                self.uses_heap = true;
                self.intern_string(name);
                self.collect_expr(value);
            }
            ExprIr::CallIndirect {
                direct_eval: _,
                callee,
                this_arg,
                args,
                static_regexp_compilation,
            } => {
                self.uses_heap = true;
                // Two independent recognisers, and they are deliberately not
                // the same test.
                //
                // `resolved_callee` depends on type inference having resolved
                // the callee to `RegExp` or `RegExp.prototype.compile`. That is
                // the only one allowed to force a table into existence, because
                // forcing one is expensive: with no string-valued declaration
                // initialiser anywhere, `queue_runtime_regexp_programs` falls
                // back to *every* script string literal.
                //
                // `compile_shaped_callee` is structural — the callee is a
                // property read whose key is literally `compile`. It cannot
                // narrow anything: collected literals are only ever *unioned*
                // into the candidate set (`queue_runtime_regexp_programs`), and
                // the set they are unioned into is not the one whose emptiness
                // picks the fallback. So widening collection is free, while
                // widening the flag is not — hence the split.
                //
                // The split matters because the measured gate case
                // (`annexB/built-ins/RegExp/prototype/compile/duplicate-named-capturing-groups-syntax.js`)
                // spells its illegal pattern as `() => r.compile("(?<x>a)(?<x>b)")`
                // over a `let r = /[ab]/`, which lowers to `CallIndirect` with a
                // `PropertyRead` callee, and whether `function_targets` resolves
                // through the arrow is exactly the thing this lane could not
                // measure. `lower_indirect_method_call` (`scriptw-ir`) keeps the
                // method name on the callee in *both* of its shapes, so the
                // structural test answers without needing inference at all.
                let resolved_regexp_callee = matches!(callee.expr, ExprIr::GlobalPropertyRead { ref name } if name == "RegExp")
                    || callee
                        .function_targets
                        .known_targets()
                        .iter()
                        .any(|target| {
                            matches!(
                                target.as_str(),
                                BUILTIN_REGEXP_FUNCTION_ID
                                    | BUILTIN_REGEXP_PROTOTYPE_COMPILE_FUNCTION_ID
                            )
                        });
                if resolved_regexp_callee || static_regexp_compilation.is_some() {
                    self.needs_runtime_regexp_programs = true;
                }
                if resolved_regexp_callee
                    || static_regexp_compilation.is_some()
                    || callee_names_regexp_compile(callee)
                {
                    // The pattern argument is the one string the script is
                    // demonstrably asking the RegExp compiler about. Offer it
                    // to the compile-time compiler even when it never appears
                    // as a declaration initialiser — otherwise an *illegal*
                    // pattern spelled inline is never compiled, never rejected,
                    // and therefore has no row to throw from.
                    if let Some(pattern) = args.first() {
                        collect_finite_string_choices(
                            pattern,
                            &mut self.runtime_regexp_argument_literals,
                        );
                    }
                }
                if let Some(compilation) = static_regexp_compilation {
                    match compilation {
                        StaticRegExpCompilation::Program(program) => {
                            self.queue_regexp_program(program)
                        }
                        StaticRegExpCompilation::InvalidSyntax { .. } => {
                            self.intern_source_runtime_error_message(
                                SourceRuntimeErrorMessage::RegExp(compilation),
                            );
                        }
                    }
                }
                self.collect_expr(callee);
                if let Some(this_arg) = this_arg {
                    self.collect_expr(this_arg);
                }
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            ExprIr::Construct {
                callee,
                args,
                static_regexp_compilation,
            } => {
                self.uses_heap = true;
                if static_regexp_compilation.is_some()
                    || matches!(callee.expr, ExprIr::GlobalPropertyRead { ref name } if name == "RegExp")
                    || callee
                        .function_targets
                        .known_targets()
                        .contains(BUILTIN_REGEXP_FUNCTION_ID)
                {
                    self.needs_runtime_regexp_programs = true;
                    // Same reasoning as the `CallIndirect` arm above: `new
                    // RegExp("(?<x>a)(?<x>b)")` must reach the compile-time
                    // compiler so the rejection has somewhere to live.
                    if let Some(pattern) = args.first() {
                        collect_finite_string_choices(
                            pattern,
                            &mut self.runtime_regexp_argument_literals,
                        );
                    }
                }
                self.intern_string("prototype");
                if let Some(compilation) = static_regexp_compilation {
                    match compilation {
                        StaticRegExpCompilation::Program(program) => {
                            self.queue_regexp_program(program)
                        }
                        StaticRegExpCompilation::InvalidSyntax { .. } => {
                            self.intern_source_runtime_error_message(
                                SourceRuntimeErrorMessage::RegExp(compilation),
                            );
                        }
                    }
                }
                self.collect_expr(callee);
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            ExprIr::CallMethod {
                receiver,
                key,
                args,
            } => {
                self.uses_heap = true;
                self.collect_expr(receiver);
                self.collect_property_key(key);
                if matches!(key, PropertyKeyIr::StaticString(name) if name == "join")
                    || matches!(key, PropertyKeyIr::StaticString(name) if name == "toString")
                        && receiver
                            .possible_kinds
                            .is_subset_of(KindSet::from_kind(ValueKind::Array))
                {
                    self.intern_string("");
                    self.intern_string(",");
                    self.intern_string("Array.prototype.join receiver is not array");
                }
                if matches!(key, PropertyKeyIr::StaticString(name) if name == "reverse") {
                    self.intern_string("Array.prototype.reverse receiver is not array");
                }
                // A literal pattern argument to `.compile(…)`, collected for the
                // same reason as the `CallIndirect` / `Construct` arms: the
                // compile-time RegExp compiler's verdict on a pattern needs a
                // row in the runtime table, and a pattern that only ever appears
                // as a call argument was never offered to it.
                //
                // Both arms are needed because the *same source text* lowers
                // differently depending on where it sits. Measured with
                // `scriptw inspect`: `function go() { r.compile("xy"); }` over a
                // `var r = /[ab]/` reports `method_calls=1`, while the identical
                // call over a `let r = /[ab]/` reports `method_calls=0,
                // indirect_calls=5`. Collecting at one node only would close the
                // hole for one spelling of the same program.
                //
                // Deliberately does NOT set `needs_runtime_regexp_programs`: a
                // `.compile` call on some unrelated object would then force a
                // runtime table — and, for a script with no string-valued
                // declaration initialisers, one built from EVERY script string
                // literal — for nothing. Collecting costs nothing when no table
                // is built, because `queue_runtime_regexp_programs` is then
                // never called.
                //
                // What *does* set the flag for this shape is the
                // `RegExpPrototypeCompile` disjunct in `collect_script`'s initialiser.
                // Without it this arm was strictly inert for the very program
                // its comment above cites, because no other setter fires on a
                // `CallMethod` node: read the two together.
                if matches!(key, PropertyKeyIr::StaticString(name) if name == "compile") {
                    if let Some(pattern) = args.first() {
                        collect_finite_string_choices(
                            pattern,
                            &mut self.runtime_regexp_argument_literals,
                        );
                    }
                }
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            ExprIr::InstanceOf { lhs, rhs } => {
                self.uses_heap = true;
                self.intern_string("prototype");
                self.collect_expr(lhs);
                self.collect_expr(rhs);
            }
            ExprIr::In { lhs, rhs } => {
                self.uses_heap = true;
                self.collect_expr(lhs);
                self.collect_expr(rhs);
            }
            ExprIr::SuperConstruct { args } => {
                self.uses_heap = true;
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            ExprIr::SuperNewTarget | ExprIr::SuperConstructor => {
                self.uses_heap = true;
            }
            ExprIr::PreparedSuperConstruct(prepared) => {
                self.uses_heap = true;
                for operand in prepared.operands() {
                    self.collect_expr(operand);
                }
            }
            ExprIr::SuperPropertyRead { key, receiver } => {
                self.uses_heap = true;
                self.collect_property_key(key);
                self.collect_expr(receiver);
            }
            ExprIr::SuperPropertyWrite {
                key,
                receiver,
                value,
                ..
            } => {
                self.uses_heap = true;
                self.collect_property_key(key);
                self.collect_expr(receiver);
                self.collect_expr(value);
            }
            ExprIr::SuperPropertyMutation(mutation) => {
                self.uses_heap = true;
                self.collect_property_key(mutation.referenced_name());
                self.collect_expr(mutation.receiver());
                match mutation.operation() {
                    SuperPropertyMutationOperationIr::NumericUpdate { .. }
                    | SuperPropertyMutationOperationIr::Capture(_) => {}
                    SuperPropertyMutationOperationIr::EagerCompound { result, .. }
                    | SuperPropertyMutationOperationIr::PutCaptured { value: result, .. } => {
                        self.collect_expr(result);
                    }
                }
            }
            ExprIr::ClassDefinition(class) => {
                match &class.name_inference {
                    ClassNameInferenceIr::FieldInitializer(ClassFieldNameIr::Static(name)) => {
                        self.intern_string(name);
                    }
                    ClassNameInferenceIr::None
                    | ClassNameInferenceIr::PropertyKeyBinding(_)
                    | ClassNameInferenceIr::FieldInitializer(ClassFieldNameIr::Computed(_)) => {}
                }
                if let Some(name_binding) = &class.name_binding {
                    self.collect_lexical_environment(Some(&name_binding.environment));
                }
                self.uses_heap = true;
                for description in class.private_name_ids.keys() {
                    // Source IR keys omit `#`; PrivateName descriptions retain
                    // it even when the field has no initializer or callable.
                    self.intern_string(&format!("#{description}"));
                }
                self.intern_string("prototype");
                self.intern_string("constructor");
                self.intern_string("$IsHTMLDDA");
                for definition in &class.element_plan.definitions {
                    match definition {
                        ClassElementDefinitionIr::PublicMethod(method) => {
                            self.collect_property_key(&method.key);
                        }
                        ClassElementDefinitionIr::PrivateMethod(_) => {}
                        ClassElementDefinitionIr::ComputedFieldKey { key, .. } => {
                            self.collect_property_key(key);
                        }
                        ClassElementDefinitionIr::AutoAccessor(accessor) => {
                            if let Some(key) = &accessor.computed_key {
                                self.collect_property_key(key);
                            }
                            match &accessor.key {
                                ClassFieldKeyIr::Public(key) => self.intern_string(key),
                                ClassFieldKeyIr::ComputedPublic(_) => {}
                                ClassFieldKeyIr::Private(_) => {}
                            }
                        }
                    }
                }
                for static_element in &class.element_plan.static_elements {
                    let key = match static_element {
                        ClassStaticElementIr::Field(field) => &field.key,
                        ClassStaticElementIr::AutoAccessorBacking(_) => continue,
                        ClassStaticElementIr::Block(_) => continue,
                    };
                    match key {
                        ClassFieldKeyIr::Public(key) => self.intern_string(key),
                        ClassFieldKeyIr::ComputedPublic(_) => {}
                        ClassFieldKeyIr::Private(_) => {}
                    }
                }
                if let Some(heritage) = &class.heritage {
                    self.collect_expr(heritage);
                }
            }
            ExprIr::PrivateRead { target, .. } => {
                self.uses_heap = true;
                self.collect_expr(target);
            }
            ExprIr::PrivateWrite { target, value, .. } => {
                self.uses_heap = true;
                self.collect_expr(target);
                self.collect_expr(value);
            }
            ExprIr::PrivateIn { rhs, .. } => {
                self.uses_heap = true;
                self.collect_expr(rhs);
            }
            ExprIr::ClassFieldNamedFunctionValue { name, .. } => {
                if let ClassFieldNameIr::Static(name) = name {
                    self.intern_string(name);
                }
            }
            ExprIr::Undefined
            | ExprIr::ArrayHole
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::FunctionValue(_)
            | ExprIr::This
            | ExprIr::ExecutionGlobalObject
            | ExprIr::Arguments
            | ExprIr::Identifier(_) => {}
        }
    }

    fn collect_property_key(&mut self, key: &PropertyKeyIr) {
        match key {
            PropertyKeyIr::StaticString(value) => self.intern_string(value),
            PropertyKeyIr::ArrayLength => {}
            PropertyKeyIr::StringExpr(expr) | PropertyKeyIr::ArrayIndex(expr) => {
                self.collect_expr(expr)
            }
        }
    }

    /// Interns every string a destructuring target can turn into a runtime
    /// property key.
    ///
    /// Two of those strings are easy to forget because they are not written as
    /// keys in the source: a `Binding` name is mirrored onto the script global
    /// object by `mirror_binding_to_global_object`, and an
    /// `AssignmentIdentifier` name is written through the checked global
    /// Reference writer when the target resolves globally. Both call
    /// `StringPool::payload`, which panics when the name is not pooled. Abrupt
    /// identifier References additionally contribute their typed error
    /// message; the emitter no longer owns a parallel message literal.
    ///
    /// The match is deliberately exhaustive with no wildcard arm: a new
    /// `DestructuringTargetIr` variant must fail to compile here rather than
    /// reach codegen with an un-interned name.
    fn collect_destructuring_target_strings(&mut self, target: &DestructuringTargetIr) {
        match target {
            DestructuringTargetIr::Binding { mode: _, name } => {
                self.intern_string(name);
            }
            DestructuringTargetIr::ResolvedVarBinding { reference, .. }
            | DestructuringTargetIr::AssignmentIdentifier(reference) => {
                self.intern_string(reference.name());
                match reference.write_disposition() {
                    IdentifierWriteDisposition::WithObject {
                        referenced_name,
                        selection,
                        fallback_storage_name,
                        ..
                    } => {
                        self.intern_string(referenced_name);
                        self.intern_string(fallback_storage_name);
                        self.collect_expr(selection);
                    }
                    IdentifierWriteDisposition::Global {
                        referenced_name, ..
                    }
                    | IdentifierWriteDisposition::Environment {
                        referenced_name, ..
                    } => self.intern_string(referenced_name),
                    IdentifierWriteDisposition::Throw { error } => self
                        .intern_source_runtime_error_message(
                            SourceRuntimeErrorMessage::IdentifierWrite(error),
                        ),
                    IdentifierWriteDisposition::MutableBinding { .. }
                    | IdentifierWriteDisposition::IgnoreImmutableBinding => {}
                }
            }
            DestructuringTargetIr::AssignmentProperty { key, .. } => {
                self.collect_destructuring_property_key_strings(key);
            }
            // Private elements are addressed by brand token, not by a pooled
            // string; the class definition interns their keys.
            DestructuringTargetIr::AssignmentPrivate { .. } => {}
            DestructuringTargetIr::AssignmentSuper {
                capture,
                value_binding,
                put,
            } => {
                self.intern_string(value_binding);
                self.collect_expr(capture);
                self.collect_expr(put);
            }
            DestructuringTargetIr::NestedArray(pattern) => {
                self.collect_array_destructuring_pattern_strings(pattern);
            }
            DestructuringTargetIr::NestedObject(pattern) => {
                self.collect_object_destructuring_pattern_strings(pattern);
            }
        }
    }

    fn collect_destructuring_property_key_strings(&mut self, key: &DestructuringPropertyKeyIr) {
        match key {
            DestructuringPropertyKeyIr::Static(key) => self.intern_string(key),
            // Computed keys are stringified at runtime; `visit_expressions`
            // already walked the key expression.
            DestructuringPropertyKeyIr::Computed(_) => {}
        }
    }

    fn collect_array_destructuring_pattern_strings(
        &mut self,
        pattern: &ArrayDestructuringPatternIr,
    ) {
        for element in &pattern.elements {
            match element {
                ArrayDestructuringElementIr::Elision => {}
                ArrayDestructuringElementIr::Target { target, default: _ }
                | ArrayDestructuringElementIr::Rest { target } => {
                    self.collect_destructuring_target_strings(target);
                }
            }
        }
    }

    fn collect_object_property(&mut self, property: &ObjectPropertyIr) {
        match property {
            ObjectPropertyIr::PrototypeSetter { value } => {
                self.collect_expr(value);
            }
            ObjectPropertyIr::Spread { source } => {
                self.collect_expr(source);
            }
            ObjectPropertyIr::Data { key, value, .. }
            | ObjectPropertyIr::NonEnumerableData { key, value } => {
                self.intern_string(key);
                self.collect_expr(value);
            }
            ObjectPropertyIr::ComputedData { key, value, .. } => {
                self.collect_expr(key);
                self.collect_expr(value);
            }
            ObjectPropertyIr::ComputedMethod { key, .. }
            | ObjectPropertyIr::ComputedGetter { key, .. }
            | ObjectPropertyIr::ComputedSetter { key, .. } => {
                self.collect_expr(key);
            }
            ObjectPropertyIr::Method { key, .. }
            | ObjectPropertyIr::Getter { key, .. }
            | ObjectPropertyIr::Setter { key, .. } => {
                self.intern_string(key);
            }
        }
    }

    fn collect_object_destructuring_pattern_strings(
        &mut self,
        pattern: &ObjectDestructuringPatternIr,
    ) {
        for property in &pattern.properties {
            self.collect_destructuring_property_key_strings(&property.key);
            self.collect_destructuring_target_strings(&property.target);
        }
        if let Some(rest) = &pattern.rest {
            self.collect_destructuring_target_strings(rest);
        }
    }

    fn intern_string(&mut self, value: &str) {
        if self.refs.contains_key(value) {
            return;
        }
        // Validate the resulting table length before mutating either image.
        let index = PooledStringIndex::for_insertion(self.refs.len());
        let offset = STATIC_DATA_OFFSET + self.bytes.len() as u32;
        let bytes = Self::runtime_bytes_for_string(value);
        self.bytes.extend_from_slice(&bytes);
        let units = Self::runtime_code_units_for_string(value);
        let code_units = PooledCodeUnits {
            byte_offset: u32::try_from(self.pooled_code_unit_bytes.len())
                .expect("pooled UTF-16 byte offset fits the Wasm data index domain"),
            length: u32::try_from(units.len()).expect("pooled UTF-16 length fits a GC array"),
        };
        code_units
            .length
            .checked_mul(2)
            .and_then(|byte_length| code_units.byte_offset.checked_add(byte_length))
            .expect("pooled UTF-16 slice fits the Wasm data segment byte domain");
        for unit in units {
            self.pooled_code_unit_bytes
                .extend_from_slice(&unit.to_le_bytes());
        }
        self.refs.insert(
            value.to_string(),
            StringRef {
                index,
                offset,
                len: bytes.len() as u32,
                code_units,
            },
        );
    }

    fn queue_regexp_program(&mut self, program: &RegExpProgram) {
        // The program key omits flags: identical instruction streams may be
        // shared by legacy and Unicode regexps, which need different tables.
        if program.instructions.iter().any(|instruction| {
            matches!(
                instruction.opcode,
                REGEXP_OPCODE_NAMED_BACKREFERENCE | REGEXP_OPCODE_NUMBERED_BACKREFERENCE
            ) && instruction.operand1 & REGEXP_BACKREFERENCE_IGNORE_CASE != 0
        }) {
            self.needed_regexp_case_folding.insert(
                if program.flags.unicode_mode.is_unicode_mode() {
                    RegExpCaseFolding::Unicode
                } else {
                    RegExpCaseFolding::Legacy
                },
            );
        }
        let key = RegExpProgramStaticKey::from_program(program);
        // Literal and static-constructor publication read this P's immutable
        // metadata, never R's linear prefix. Keep one P descriptor even when
        // the same validated program already exists in the shared R image.
        if self
            .regexp_programs
            .get(&key)
            .is_some_and(|reference| reference.metadata_span(self).is_ok())
            || self
                .pending_regexp_programs
                .iter()
                .any(|pending| pending == &key)
        {
            return;
        }
        self.pending_regexp_programs.push(key);
    }

    fn queue_runtime_regexp_programs(&mut self) {
        let candidate_literals = if self.runtime_regexp_candidate_literals.is_empty() {
            &self.script_string_literals
        } else {
            &self.runtime_regexp_candidate_literals
        };
        let mut literals = candidate_literals.iter().cloned().collect::<Vec<_>>();
        // Unioned in, never substituted for `candidate_literals`: the
        // empty/non-empty test above is the fallback switch, and this set must
        // not be able to flip it. See the field's doc comment.
        for literal in &self.runtime_regexp_argument_literals {
            if !literals.iter().any(|source| source == literal) {
                literals.push(literal.clone());
            }
        }
        if !literals.iter().any(|source| source == "(?:)") {
            literals.push("(?:)".to_string());
        }
        if !literals.iter().any(|source| source == "[object Object]") {
            literals.push("[object Object]".to_string());
        }
        // The table is `|literals| x |flags|` rows and every row is now written,
        // including the rejected and unsupported ones, so the flags axis is a
        // multiplier on static data size and on the *linear* scan the emitted
        // lookup does at every runtime construction site. Deduplicating it is
        // therefore not tidiness: the sticky expansion below used to be able to
        // produce `"iy"` twice for a script containing both `"i"` and `"iy"`,
        // which doubled a whole column of rows.
        let mut flags = self
            .script_string_literals
            .iter()
            .filter(|value| is_regexp_flags_literal(value))
            .cloned()
            .collect::<BTreeSet<_>>();
        flags.insert(String::new());
        let sticky_flags = flags
            .iter()
            .filter(|flags| !flags.contains('y'))
            .map(|flags| format!("{flags}y"))
            .collect::<Vec<_>>();
        flags.extend(sticky_flags);
        let flags = flags.into_iter().collect::<Vec<_>>();
        for literal in &literals {
            self.intern_string(literal);
        }
        for flags in &flags {
            self.intern_string(flags);
        }
        let mut candidates = Vec::new();

        for source in &literals {
            let normalized_source = if source.is_empty() { "(?:)" } else { source };
            let compilation_source = if normalized_source == "(?:)" {
                ""
            } else {
                normalized_source
            };
            for flags in &flags {
                // Every candidate pair gets a row, including the rejected ones.
                // The `else { continue }` that used to sit here is the whole
                // defect: it made "seen and illegal" indistinguishable from
                // "never seen", and the emitted lookup could then only fall out
                // of its loop leaving a null program behind.
                match RegExpProgram::compile_utf16(
                    &Self::runtime_code_units_for_string(compilation_source),
                    flags,
                ) {
                    Ok(program) => {
                        let key = RegExpProgramStaticKey::from_program(&program);
                        self.queue_regexp_program(&program);
                        candidates.push((
                            normalized_source.to_string(),
                            flags.clone(),
                            CandidateOutcome::Program(key),
                        ));
                    }
                    // Exhaustive on `RegExpCompileErrorKind`. "Illegal pattern"
                    // and "legal pattern Scriptw cannot compile" are different
                    // answers to the program, and only the first is a
                    // SyntaxError.
                    Err(error) => candidates.push((
                        normalized_source.to_string(),
                        flags.clone(),
                        match error.kind {
                            RegExpCompileErrorKind::InvalidSyntax => CandidateOutcome::Rejected,
                            RegExpCompileErrorKind::UnsupportedFeature => {
                                CandidateOutcome::Unsupported
                            }
                        },
                    )),
                }
            }
        }

        self.append_regexp_programs();
        self.runtime_regexp_programs = candidates
            .into_iter()
            .map(|(source, flags, outcome)| {
                let entry = match outcome {
                    CandidateOutcome::Program(key) => RuntimeRegExpEntry::Program(
                        *self
                            .regexp_programs
                            .get(&key)
                            .or_else(|| {
                                self.runtime_prefix()
                                    .and_then(|prefix| prefix.regexp_programs.get(&key))
                            })
                            .expect("queued runtime RegExp program must have static data"),
                    ),
                    CandidateOutcome::Rejected => RuntimeRegExpEntry::Rejected,
                    CandidateOutcome::Unsupported => RuntimeRegExpEntry::Unsupported,
                };
                (source, flags, entry)
            })
            .collect();
        self.append_runtime_regexp_program_table();
    }

    fn append_runtime_regexp_program_table(&mut self) {
        if self.runtime_regexp_programs.is_empty() {
            return;
        }
        // R descriptors remain R-owned. A candidate that deduplicated to one
        // receives a small P copy so no executable row selects a linear address.
        let references = self
            .runtime_regexp_programs
            .iter()
            .filter_map(|(_, _, entry)| match entry {
                RuntimeRegExpEntry::Program(reference) => Some(*reference),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut descriptors = BTreeMap::new();
        for reference in references {
            if descriptors.contains_key(&reference.payload()) {
                continue;
            }
            let address = (reference.payload() >> 32) as u32;
            let length = reference.payload() as u32;
            let source = address
                .checked_sub(STATIC_DATA_OFFSET)
                .expect("collected descriptor address");
            let end = source
                .checked_add(length)
                .expect("collected descriptor extent");
            let boundary =
                u32::try_from(self.compiler_owned.static_len()).expect("runtime static prefix");
            let span = if end <= boundary {
                let bytes = self.bytes[source as usize..end as usize].to_vec();
                self.align_bytes(8);
                let copied_address = STATIC_DATA_OFFSET
                    .checked_add(u32::try_from(self.bytes.len()).expect("P descriptor offset"))
                    .expect("P descriptor address");
                self.bytes.extend_from_slice(&bytes);
                ProgramMetadataSpan::for_pool(self, copied_address, length)
            } else {
                ProgramMetadataSpan::for_pool(self, address, length)
            }
            .expect("every candidate descriptor belongs to its P metadata image");
            descriptors.insert(reference.payload(), span);
        }
        let padding = (8 - self.bytes.len() % 8) % 8;
        self.bytes.resize(self.bytes.len() + padding, 0);
        let address = STATIC_DATA_OFFSET
            .checked_add(
                u32::try_from(self.bytes.len()).expect("candidate table address fits Wasm memory"),
            )
            .expect("candidate table address fits Wasm memory");
        let count =
            u32::try_from(self.runtime_regexp_programs.len()).expect("candidate count fits u32");
        let extent = count
            .checked_mul(RUNTIME_REGEXP_RECORD_SIZE as u32)
            .expect("candidate extent fits u32");
        address
            .checked_add(extent)
            .expect("candidate table fits Wasm memory");
        self.runtime_regexp_program_table = Some(RuntimeRegExpProgramTable { address, count });
        for (source, flags, entry) in &self.runtime_regexp_programs {
            // Assigned through the shared word indices rather than as a
            // positional array literal, so the writer's layout and the
            // emitter's bounded GC offsets are the same facts rather than two
            // agreeing transcriptions. A non-`Program` row leaves the program
            // handle zeroed, as does a total miss in the object's slot.
            let mut record = [0u64; RUNTIME_REGEXP_RECORD_WORDS];
            record[RUNTIME_REGEXP_RECORD_SOURCE_WORD] = PooledLiteralOperand::for_pool(
                self,
                self.pooled_string_index(source)
                    .expect("candidate source is interned"),
            )
            .expect("candidate source has a checked literal domain")
            .word();
            record[RUNTIME_REGEXP_RECORD_FLAGS_WORD] = PooledLiteralOperand::for_pool(
                self,
                self.pooled_string_index(flags)
                    .expect("candidate flags are interned"),
            )
            .expect("candidate flags have a checked literal domain")
            .word();
            // Exhaustive on purpose. This is the site the `continue` used to
            // hide behind: a new entry kind must be given a record encoding
            // here, or this stops compiling.
            record[RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD] = match entry {
                RuntimeRegExpEntry::Program(program) => {
                    record[RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD] =
                        descriptors[&program.payload()].packed();
                    record[RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD] =
                        RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN;
                    RuntimeRegExpEntryKind::Program.word()
                }
                RuntimeRegExpEntry::Rejected => RuntimeRegExpEntryKind::Rejected.word(),
                RuntimeRegExpEntry::Unsupported => RuntimeRegExpEntryKind::Unsupported.word(),
            };
            for value in record {
                self.bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }

    fn append_regexp_programs(&mut self) {
        if self.pending_regexp_programs.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_regexp_programs);
        for key in pending {
            self.align_bytes(8);
            let ptr = STATIC_DATA_OFFSET
                .checked_add(u32::try_from(self.bytes.len()).expect("static data length"))
                .expect("RegExp descriptor address");
            let reference = RegExpProgramRef::new(ptr, &key.0);
            self.bytes.extend_from_slice(key.0.bytes());
            self.regexp_programs.insert(key, reference);
        }
    }

    fn append_regexp_case_folding_tables(&mut self) {
        for folding in self.needed_regexp_case_folding.clone() {
            if self.regexp_case_folding_table(folding).is_some() {
                continue;
            }
            let mappings = folding.mappings();
            self.align_bytes(8);
            let ptr = STATIC_DATA_OFFSET + self.bytes.len() as u32;
            for &(source, canonical) in mappings {
                self.bytes.extend_from_slice(&source.to_le_bytes());
                self.bytes.extend_from_slice(&canonical.to_le_bytes());
            }
            self.regexp_case_folding_tables.insert(
                folding,
                RegExpCaseFoldingTable {
                    ptr,
                    count: mappings.len() as u32,
                },
            );
        }
    }

    pub(crate) fn regexp_case_folding_table(
        &self,
        folding: RegExpCaseFolding,
    ) -> Option<RegExpCaseFoldingTable> {
        self.regexp_case_folding_tables
            .get(&folding)
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.regexp_case_folding_tables.get(&folding))
            })
            .copied()
    }

    pub(crate) fn runtime_bytes_for_string(value: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(value.len());
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != JS_STRING_SURROGATE_SENTINEL {
                Self::push_utf8_char(&mut bytes, ch);
                continue;
            }

            if chars.peek().copied() == Some(JS_STRING_SURROGATE_SENTINEL) {
                chars.next();
                Self::push_utf8_char(&mut bytes, JS_STRING_SURROGATE_SENTINEL);
                continue;
            }

            let mut consumed = String::new();
            let mut code_unit = 0_u16;
            let mut is_marker = true;
            for _ in 0..4 {
                let Some(hex) = chars.next() else {
                    is_marker = false;
                    break;
                };
                consumed.push(hex);
                let Some(value) = hex.to_digit(16) else {
                    is_marker = false;
                    break;
                };
                code_unit = (code_unit << 4) | value as u16;
            }

            if is_marker && (0xD800..=0xDFFF).contains(&code_unit) {
                Self::push_wtf8_code_unit(&mut bytes, code_unit);
            } else {
                Self::push_utf8_char(&mut bytes, JS_STRING_SURROGATE_SENTINEL);
                bytes.extend_from_slice(consumed.as_bytes());
            }
        }
        bytes
    }

    fn push_utf8_char(bytes: &mut Vec<u8>, ch: char) {
        let mut buffer = [0; 4];
        bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    }

    fn push_wtf8_code_unit(bytes: &mut Vec<u8>, code_unit: u16) {
        let code = code_unit as u32;
        if code < 0x80 {
            bytes.push(code as u8);
        } else if code < 0x800 {
            bytes.push((0xC0 | (code >> 6)) as u8);
            bytes.push((0x80 | (code & 0x3F)) as u8);
        } else {
            bytes.push((0xE0 | (code >> 12)) as u8);
            bytes.push((0x80 | ((code >> 6) & 0x3F)) as u8);
            bytes.push((0x80 | (code & 0x3F)) as u8);
        }
    }

    fn boundary(&self) -> PoolBoundary {
        PoolBoundary {
            static_bytes: self.bytes.len(),
            code_unit_bytes: self.pooled_code_unit_bytes.len(),
            strings: self.pooled_string_count(),
        }
    }

    pub(crate) fn compiler_owned_boundary(&self) -> PoolBoundary {
        self.compiler_owned
    }

    /// The pool of the runtime module: nothing past the compiler-owned phase.
    pub(crate) fn require_runtime_only(&self) -> Result<(), EmitError> {
        if self.boundary() == self.compiler_owned && !self.compact_array_recipes.has_recipes() {
            Ok(())
        } else {
            Err(EmitError::unsupported(
                "compiler invariant violated: the runtime module's pool holds program data",
            ))
        }
    }

    /// Partial compiler-only test pools keep the same physical segment roles
    /// with an empty deferred region before the following calendar image.
    pub(super) fn normalization_image_region(&self) -> NormalizationRegion {
        self.normalization_region.unwrap_or_else(|| {
            let end = self.temporal_east_asian_year_region().start();
            NormalizationRegion { start: end, end }
        })
    }

    /// Publish cold images without initializing their reserved linear ranges.
    pub(crate) fn append_runtime_data(&self, section: &mut wasm_encoder::DataSection) {
        assert_eq!(section.len(), PooledCodeUnits::DATA_SEGMENT.index());
        let boundary = self.compiler_owned;
        section.passive(
            self.pooled_code_unit_bytes[..boundary.code_unit_bytes]
                .iter()
                .copied(),
        );
        let normalization = self.normalization_image_region();
        let norm = normalization.offsets();
        let properties = self.regexp_unicode_property_region();
        let property = properties.offsets();
        let years = self.temporal_east_asian_year_region();
        let year = years.offsets();
        assert!(property.end <= norm.start && norm.end <= year.start);
        assert!(year.end <= boundary.static_bytes);
        assert_eq!(section.len(), normalization.segment().index());
        section.passive(self.bytes[norm.clone()].iter().copied());
        assert_eq!(section.len(), properties.segment().index());
        section.passive(self.bytes[property.clone()].iter().copied());
        assert_eq!(section.len(), years.segment().index());
        section.passive(self.bytes[year.clone()].iter().copied());
        for (span, address) in [
            (0..property.start, STATIC_DATA_OFFSET),
            (property.end..norm.start, properties.end()),
            (norm.end..year.start, normalization.end()),
            (year.end..boundary.static_bytes, years.end()),
        ] {
            section.active(
                0,
                &wasm_encoder::ConstExpr::i32_const(address as i32),
                self.bytes[span].iter().copied(),
            );
        }
        assert_eq!(section.len() as usize, RUNTIME_DATA_SEGMENT_COUNT);
    }

    /// The program's passive UTF-16, metadata, descriptor and recipe images.
    /// Literal descriptors use P-local slots. All executable P metadata reads
    /// use the immutable ProgramInstance image; the segment is not installed in
    /// shared linear memory. Compact recipes use their P module's passive image.
    pub(crate) fn append_program_data(&self, section: &mut wasm_encoder::DataSection) {
        assert_eq!(section.len(), PooledCodeUnits::DATA_SEGMENT.index());
        section.passive(self.program_utf16_bytes().iter().copied());
        section.passive(self.program_metadata_bytes().iter().copied());
        self.program_string_descriptor_image()
            .append_passive(section);
        assert_eq!(section.len(), COMPACT_ARRAY_RECIPE_SEGMENT);
        section.passive(self.program_recipe_bytes().iter().copied());
    }

    /// Segment index of the program's passive static bytes.
    pub(crate) const PROGRAM_STATIC_DATA_SEGMENT: u32 = 1;

    pub(crate) fn program_static_len(&self) -> u32 {
        (self.bytes.len() - self.compiler_owned.static_bytes) as u32
    }

    pub(crate) fn module_bytes(
        &self,
        module: scriptw_ir::ModuleUnitId,
    ) -> Result<PooledModuleBytes, EmitError> {
        self.module_bytes.get(&module).copied().ok_or_else(|| {
            EmitError::unsupported("Bytes Module payload exceeds the static image address domain")
        })
    }

    /// Pooled strings the program adds, with bounds into its own passive
    /// UTF-16 segment.
    pub(crate) fn program_pooled_string_initializers(
        &self,
    ) -> impl Iterator<Item = (PooledStringIndex, PooledCodeUnits)> + '_ {
        let boundary = self.compiler_owned;
        self.refs
            .program_values()
            .filter(move |reference| reference.index.ordinal() >= boundary.strings)
            .map(move |reference| {
                (
                    reference.index,
                    PooledCodeUnits {
                        byte_offset: reference.code_units.byte_offset
                            - boundary.code_unit_bytes as u32,
                        length: reference.code_units.length,
                    },
                )
            })
    }

    pub(crate) fn pooled_string_count(&self) -> u32 {
        u32::try_from(self.refs.len()).expect("pooled strings fit a GC array")
    }
    pub(crate) fn pooled_string_initializers(
        &self,
    ) -> impl Iterator<Item = (PooledStringIndex, &PooledCodeUnits)> {
        self.refs
            .values()
            .map(|reference| (reference.index, &reference.code_units))
    }
    pub(crate) fn pooled_string_index(&self, value: &str) -> Result<PooledStringIndex, EmitError> {
        self.refs
            .get(value)
            .map(|reference| reference.index)
            .ok_or_else(|| {
                EmitError::unsupported(format!("string `{value}` was not collected in this module"))
            })
    }
    pub(crate) fn runtime_error_string_index(
        &self,
        message: RuntimeErrorMessage,
    ) -> Result<PooledStringIndex, EmitError> {
        if let Some(text) = message.catalog_text() {
            return self.pooled_string_index(text);
        }
        // A collected source message carries the pool's checked compiler-only
        // handle. Resolve it here; that handle never becomes a JavaScript value.
        let handle = self.runtime_error_payload(message);
        self.refs
            .values()
            .find_map(|reference| {
                let candidate =
                    (((reference.offset as u64) << 32) | u64::from(reference.len)) as i64;
                (candidate == handle).then_some(reference.index)
            })
            .ok_or_else(|| {
                EmitError::unsupported("runtime diagnostic does not belong to this string pool")
            })
    }
    pub(crate) fn runtime_code_units_for_string(value: &str) -> Vec<u16> {
        scriptw_ir::decode_js_string_utf16(value)
    }

    pub(crate) fn payload(&self, value: &str) -> i64 {
        let string = self
            .refs
            .get(value)
            .unwrap_or_else(|| panic!("string `{value}` must exist in pool"));
        assert!(
            string.offset < (1 << 31),
            "string pool offset {} exceeds the PropertyKey marker boundary",
            string.offset
        );
        (((string.offset as u64) << 32) | string.len as u64) as i64
    }

    pub(crate) fn property_key_symbol_payload(&self, value: &str) -> i64 {
        self.payload(value) | PROPERTY_KEY_SYMBOL_MARKER as i64
    }

    pub(crate) fn static_builtin_property_key_payload(&self, value: &str) -> i64 {
        if value.starts_with("Symbol.") {
            return self.property_key_symbol_payload(value);
        }
        self.payload(value)
    }

    pub(crate) const fn runtime_regexp_table(&self) -> Option<RuntimeRegExpProgramTable> {
        self.runtime_regexp_program_table
    }

    pub(crate) fn regexp_program(&self, program: &RegExpProgram) -> RegExpProgramRef {
        *self
            .regexp_programs
            .get(&RegExpProgramStaticKey::from_program(program))
            .or_else(|| {
                self.runtime_prefix().and_then(|prefix| {
                    prefix
                        .regexp_programs
                        .get(&RegExpProgramStaticKey::from_program(program))
                })
            })
            .expect("collected RegExp literal program must have static data")
    }

    #[cfg(test)]
    pub(crate) fn collect_regexp_program_for_test(
        &mut self,
        program: &RegExpProgram,
    ) -> RegExpProgramRef {
        self.queue_regexp_program(program);
        self.append_regexp_programs();
        self.regexp_program(program)
    }
}

/// Does this `CallIndirect` callee *structurally* name `RegExp.prototype.compile`?
///
/// Purely a shape test on the two callee spellings `lower_indirect_method_call`
/// can produce (`scriptw-ir/src/lowering.rs`): an `ExprIr::PropertyRead` with a
/// static key, and a `GetV` spec operation whose second operand is the key
/// string. No type inference is consulted, which is the point — the arm that
/// uses this needs an answer for a call the inference may not have resolved
/// through an enclosing arrow.
///
/// Only ever used to *widen* candidate collection, never to force a table into
/// existence. A `.compile` on some unrelated object therefore costs at most one
/// extra literal in a set that is unioned in, and costs nothing at all when no
/// runtime table is built.
fn callee_names_regexp_compile(callee: &TypedExpr) -> bool {
    match &callee.expr {
        ExprIr::PropertyRead { key, .. } => {
            matches!(key, PropertyKeyIr::StaticString(name) if name == "compile")
        }
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            operands,
        } => matches!(
            operands.get(1).map(|key| &key.expr),
            Some(ExprIr::String(name)) if name == "compile"
        ),
        _ => false,
    }
}

/// Finite string choices recognized directly from IR are an AOT optimization.
/// Unrecognized shapes contribute no table row and use the emitted Pattern
/// compiler at runtime. Recognized invalid patterns retain a rejected row, whose
/// exact byte match throws SyntaxError before publication. Keep this collector
/// conservative; extending it changes which static parser verdicts are cached.
fn collect_finite_string_choices(expr: &TypedExpr, choices: &mut BTreeSet<String>) {
    match &expr.expr {
        ExprIr::String(value) => {
            choices.insert(value.clone());
        }
        ExprIr::ArrayLiteral(elements) => {
            for element in elements {
                collect_finite_string_choices(element, choices);
            }
        }
        ExprIr::ArrayAccumulation(accumulation) => {
            for element in accumulation.elements() {
                match element {
                    ArrayAccumulationElementIr::Elision => {}
                    ArrayAccumulationElementIr::Value(value) => {
                        collect_finite_string_choices(value, choices)
                    }
                    ArrayAccumulationElementIr::Spread(spread) => {
                        collect_finite_string_choices(&spread.value, choices)
                    }
                }
            }
        }
        ExprIr::ObjectLiteral(properties) => {
            for property in properties {
                collect_object_property_finite_string_choices(property, choices);
            }
        }
        ExprIr::ObjectPropertyDefinition(definition) => {
            collect_finite_string_choices(definition.target(), choices);
            collect_object_property_finite_string_choices(definition.property(), choices);
        }
        ExprIr::ObjectDestructuringOperation(operation) => {
            operation.visit_expressions(&mut |expr| collect_finite_string_choices(expr, choices));
        }
        ExprIr::Conditional {
            then_expr,
            else_expr,
            ..
        } => {
            collect_finite_string_choices(then_expr, choices);
            collect_finite_string_choices(else_expr, choices);
        }
        _ => {}
    }
}

fn collect_object_property_finite_string_choices(
    property: &ObjectPropertyIr,
    choices: &mut BTreeSet<String>,
) {
    match property {
        ObjectPropertyIr::PrototypeSetter { value }
        | ObjectPropertyIr::Spread { source: value }
        | ObjectPropertyIr::Data { value, .. }
        | ObjectPropertyIr::NonEnumerableData { value, .. }
        | ObjectPropertyIr::ComputedData { value, .. } => {
            collect_finite_string_choices(value, choices);
        }
        ObjectPropertyIr::ComputedMethod { .. }
        | ObjectPropertyIr::ComputedGetter { .. }
        | ObjectPropertyIr::ComputedSetter { .. }
        | ObjectPropertyIr::Method { .. }
        | ObjectPropertyIr::Getter { .. }
        | ObjectPropertyIr::Setter { .. } => {}
    }
}

fn is_regexp_flags_literal(value: &str) -> bool {
    let mut seen = BTreeSet::new();
    value.chars().all(|flag| {
        matches!(flag, 'd' | 'g' | 'i' | 'm' | 's' | 'u' | 'v' | 'y') && seen.insert(flag)
    }) && !(seen.contains(&'u') && seen.contains(&'v'))
}

#[cfg(test)]
mod host_created_realm_property_name_pool_tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};

    #[test]
    fn create_realm_pools_its_host_published_eval_script_name() {
        const SOURCE: &str = "__scriptwCreateRealm();";
        assert!(!SOURCE.contains(REALM_EVAL_SCRIPT_METHOD_NAME));

        let parsed = parse(SOURCE, ParseOptions::script()).expect("script should parse");
        let script = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262)
            .script
            .expect("script should lower");
        assert!(script.host_builtins.contains(&HostBuiltinId::CreateRealm));

        let pool = StringPool::collect(
            &script,
            &BTreeMap::new(),
            // The R/P compiler pre-seeds host keys in the heap runtime's
            // compiler-owned pool, before it visits this program's source.
            &[StandardBuiltinId::ObjectConstructor],
            false,
            &scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::default()),
        )
        .expect("host-name pool should collect");
        let payload = pool.payload(REALM_EVAL_SCRIPT_METHOD_NAME);
        let length = (payload as u64 & 0xFFFF_FFFF) as usize;
        assert_eq!(
            length,
            StringPool::runtime_bytes_for_string(REALM_EVAL_SCRIPT_METHOD_NAME).len()
        );
    }

    #[test]
    fn create_realm_pools_its_host_published_dollar262_names() {
        const SOURCE: &str = "__scriptwCreateRealm();";
        assert!(!SOURCE.contains("$262"));
        assert!(!SOURCE.contains("detachArrayBuffer"));

        let parsed = parse(SOURCE, ParseOptions::script()).expect("script should parse");
        let script = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262)
            .script
            .expect("script should lower");
        assert!(script.host_builtins.contains(&HostBuiltinId::CreateRealm));

        let pool = StringPool::collect(
            &script,
            &BTreeMap::new(),
            // The R/P compiler pre-seeds host keys in the heap runtime's
            // compiler-owned pool, before it visits this program's source.
            &[StandardBuiltinId::ObjectConstructor],
            false,
            &scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::default()),
        )
        .expect("host-name pool should collect");
        for name in ["$262", "detachArrayBuffer"] {
            let payload = pool.payload(name);
            let length = (payload as u64 & 0xFFFF_FFFF) as usize;
            assert_eq!(
                length,
                StringPool::runtime_bytes_for_string(name).len(),
                "created-realm $262 member `{name}` must be pooled"
            );
        }
    }
}

#[cfg(test)]
mod runtime_error_message_pool_tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::lower;

    #[test]
    #[should_panic(expected = "must exist in pool")]
    fn payload_panics_for_a_string_that_was_never_interned() {
        let pool = StringPool::default();
        let _ = pool.payload("a string that is deliberately absent from the pool");
    }

    /// Exercise production collection, including its unconditional error catalog.
    fn production_pool_for_an_empty_script() -> StringPool {
        let parsed = parse(";", ParseOptions::script()).expect("empty script should parse");
        let script = lower(&parsed).script.expect("empty script should lower");
        StringPool::collect(
            &script,
            &BTreeMap::new(),
            &[],
            false,
            &scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::default()),
        )
        .expect("empty-script pool should collect")
    }

    #[test]
    fn shared_temporal_arithmetic_literals_do_not_require_plain_family_bodies() {
        let parsed = parse(";", ParseOptions::script()).expect("empty script should parse");
        let script = lower(&parsed).script.expect("empty script should lower");
        for builtin in [
            StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
            StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
            StandardBuiltinId::TemporalPlainDateTimePrototypeUntil,
            StandardBuiltinId::TemporalPlainDateFrom,
        ] {
            let pool = StringPool::collect(
                &script,
                &BTreeMap::new(),
                &[builtin],
                false,
                &scriptw_intl::IntlDataSelection::new(
                    scriptw_intl::IntlCompilationProfile::default(),
                ),
            )
            .expect("literal pool should collect");
            for message in [
                "Temporal.PlainDate is not a valid ISO date",
                "Temporal.PlainDate is outside the supported date range",
                "plainTime",
            ] {
                let payload = pool.payload(message) as u64;
                let offset = (payload >> 32) as usize - STATIC_DATA_OFFSET as usize;
                let len = (payload & 0xFFFF_FFFF) as usize;
                assert_eq!(
                    &pool.bytes[offset..offset + len],
                    message.as_bytes(),
                    "missing shared literal for {}",
                    builtin.debug_name(),
                );
            }
        }
    }

    #[test]
    fn zoned_policy_and_converter_literals_follow_arithmetic_and_conversion_consumers() {
        let parsed = parse(";", ParseOptions::script()).expect("empty script should parse");
        let script = lower(&parsed).script.expect("empty script should lower");
        // A source need not spell From/With or supply options to compile these
        // shared branches. No sibling builtin may supply their missing pool.
        for builtin in [
            StandardBuiltinId::TemporalZonedDateTimePrototypeAdd,
            StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract,
            StandardBuiltinId::TemporalZonedDateTimePrototypeUntil,
            StandardBuiltinId::TemporalZonedDateTimePrototypeSince,
            StandardBuiltinId::TemporalZonedDateTimePrototypeRound,
            StandardBuiltinId::TemporalZonedDateTimePrototypeEquals,
            StandardBuiltinId::TemporalZonedDateTimeCompare,
        ] {
            let pool = StringPool::collect(
                &script,
                &BTreeMap::new(),
                &[builtin],
                false,
                &scriptw_intl::IntlDataSelection::new(
                    scriptw_intl::IntlCompilationProfile::default(),
                ),
            )
            .expect("literal pool should collect");
            for message in [
                "Invalid Temporal.ZonedDateTime disambiguation option",
                "Invalid Temporal.ZonedDateTime offset option",
                "Invalid Temporal.ZonedDateTime overflow option",
                "Temporal.ZonedDateTime.prototype.with options must be an object or undefined",
                "Temporal.ZonedDateTime.from requires a string or Temporal.ZonedDateTime",
                "Temporal.ZonedDateTime.from options must be an object or undefined",
                "Temporal.ZonedDateTime property bag requires timeZone",
                "Temporal.ZonedDateTime property bag year is outside the supported instant range",
                "Invalid Temporal.ZonedDateTime string",
                "Temporal.ZonedDateTime string requires one bracketed time zone",
                "Invalid Temporal.ZonedDateTime calendar annotation",
                "Temporal.ZonedDateTime offset does not match its fixed time zone",
                "disambiguation",
                "compatible",
                "earlier",
                "later",
                "reject",
                "offset",
                "use",
                "prefer",
                "ignore",
                "overflow",
                "constrain",
            ] {
                let payload = pool.payload(message) as u64;
                let offset = (payload >> 32) as usize - STATIC_DATA_OFFSET as usize;
                let len = (payload & 0xFFFF_FFFF) as usize;
                assert_eq!(
                    &pool.bytes[offset..offset + len],
                    message.as_bytes(),
                    "missing shared zoned literal for {}",
                    builtin.debug_name(),
                );
            }
        }
    }

    #[test]
    fn zoned_field_replacement_pools_shared_date_field_diagnostics() {
        let parsed = parse(";", ParseOptions::script()).expect("empty script should parse");
        let script = lower(&parsed).script.expect("empty script should lower");
        let pool = StringPool::collect(
            &script,
            &BTreeMap::new(),
            &[StandardBuiltinId::TemporalZonedDateTimePrototypeWith],
            false,
            &scriptw_intl::IntlDataSelection::new(scriptw_intl::IntlCompilationProfile::default()),
        )
        .expect("literal pool should collect");
        for message in [
            "Temporal.PlainDate monthCode must be a string",
            "Invalid Temporal.PlainDate monthCode",
            "Temporal.PlainDate fields require year",
            "Temporal.PlainDate fields require day",
            "Temporal.PlainDate fields require month or monthCode",
            "Temporal.PlainDate month and monthCode must agree",
            "Temporal.PlainDate month and day must be positive",
            "Temporal.PlainDateTime fields must be finite",
            "Temporal.PlainDateTime month and day must be positive",
        ] {
            let payload = pool.payload(message) as u64;
            let offset = (payload >> 32) as usize - STATIC_DATA_OFFSET as usize;
            let len = (payload & 0xFFFF_FFFF) as usize;
            assert_eq!(&pool.bytes[offset..offset + len], message.as_bytes());
        }
    }

    #[test]
    fn rejection_diagnostic_is_interned_without_moving_fixed_literals() {
        let pool = production_pool_for_an_empty_script();
        assert_eq!(
            pool.payload(","),
            ((((STATIC_DATA_OFFSET as u64) + 14) << 32) | 1) as i64,
        );
        let message = UNHANDLED_REJECTION_TOSTRING_THROWN_MESSAGE;
        let payload = pool.payload(message) as u64;
        let offset = (payload >> 32) as usize - STATIC_DATA_OFFSET as usize;
        let len = (payload & 0xFFFF_FFFF) as usize;
        assert_eq!(&pool.bytes[offset..offset + len], message.as_bytes());
    }

    #[test]
    fn every_catalog_error_message_resolves_in_the_production_pool() {
        let pool = production_pool_for_an_empty_script();
        for &message in RuntimeErrorMessage::CATALOG {
            let text = message
                .catalog_text()
                .expect("catalog row carries static text");
            let payload = pool.runtime_error_payload(message) as u64;
            let offset = (payload >> 32) as usize - STATIC_DATA_OFFSET as usize;
            let len = (payload & 0xffff_ffff) as usize;
            assert_eq!(
                &pool.bytes[offset..offset + len],
                StringPool::runtime_bytes_for_string(text)
            );
        }
    }
}

#[cfg(test)]
mod pooled_string_index_tests {
    use super::*;

    #[test]
    fn later_literals_preserve_collected_slots_and_initialization_data() {
        let mut pool = StringPool::default();
        let literal = "middle\0\u{10000}";
        pool.intern_string(literal);
        let original_index = pool.pooled_string_index(literal).unwrap().ordinal();
        let original_data = pool.pooled_code_unit_bytes.clone();
        pool.intern_string(literal);
        assert_eq!(pool.pooled_string_count(), 1);
        assert_eq!(pool.pooled_code_unit_bytes, original_data);

        // One literal sorts before the original and one after it. The GC slot
        // must retain its identity regardless of the map's traversal order.
        pool.intern_string("!earlier");
        pool.intern_string("zzlater");
        assert_eq!(
            pool.pooled_string_index(literal).unwrap().ordinal(),
            original_index
        );
        assert!(pool.pooled_string_index("absent").is_err());
        let mut initialized = BTreeMap::new();
        for (index, units) in pool.pooled_string_initializers() {
            let start = units.byte_offset as usize;
            let end = start + units.length as usize * 2;
            let bytes = pool.pooled_code_unit_bytes[start..end].to_vec();
            assert!(initialized.insert(index.ordinal(), bytes).is_none());
        }
        assert_eq!(
            initialized.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        for text in [literal, "!earlier", "zzlater"] {
            let index = pool.pooled_string_index(text).unwrap().ordinal();
            let expected = StringPool::runtime_code_units_for_string(text)
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            assert_eq!(initialized[&index], expected);
        }
    }

    #[test]
    fn source_diagnostics_resolve_their_original_collected_slot() {
        let mut pool = StringPool::default();
        let source = SourceRuntimeErrorMessage::IdentifierWrite(
            scriptw_ir::IdentifierWriteErrorIr::ImmutableBinding,
        );
        pool.intern_source_runtime_error_message(source);
        let message = pool.source_runtime_error_message(source).unwrap();
        let original_index = pool.runtime_error_string_index(message).unwrap().ordinal();
        pool.intern_string("!earlier source literal");
        pool.intern_runtime_error_catalog();
        assert_eq!(
            pool.runtime_error_string_index(message).unwrap().ordinal(),
            original_index
        );
        for &message in RuntimeErrorMessage::CATALOG {
            let text = message.catalog_text().expect("catalog text");
            assert_eq!(
                pool.runtime_error_string_index(message).unwrap().ordinal(),
                pool.pooled_string_index(text).unwrap().ordinal()
            );
        }
    }

    #[test]
    fn pooled_slot_admission_preserves_a_representable_table_length() {
        let last_existing = u32::MAX as usize - 1;
        assert_eq!(
            PooledStringIndex::for_insertion(last_existing).ordinal(),
            u32::MAX - 1
        );
        assert!(
            std::panic::catch_unwind(|| PooledStringIndex::for_insertion(u32::MAX as usize))
                .is_err()
        );
    }
}

#[cfg(test)]
mod regexp_program_validation_tests {
    use super::*;
    use scriptw_ir::{RegExpProgramWord, REGEXP_NAMED_GROUP_TABLE_MAGIC_VERSION};

    #[test]
    fn static_program_named_group_table_is_serialized_and_no_names_use_zero() {
        let unnamed = RegExpProgram::compile("a", "").expect("program should compile");
        let named = RegExpProgram::compile("(?<x>a)(?<y>b)", "").expect("program should compile");
        let mut pool = StringPool::default();
        let unnamed_ref = pool.collect_regexp_program_for_test(&unnamed);
        let named_ref = pool.collect_regexp_program_for_test(&named);

        let descriptor = |reference: RegExpProgramRef| {
            let offset = ((reference.payload() >> 32) as u32 - STATIC_DATA_OFFSET) as usize;
            let length = reference.payload() as u32 as usize;
            ValidatedRegExpProgram::from_bytes(pool.bytes[offset..offset + length].to_vec())
                .unwrap()
        };
        let unnamed = descriptor(unnamed_ref);
        let named = descriptor(named_ref);
        assert_eq!(unnamed.word(RegExpProgramWord::NamedGroupTableOffset), 0);
        let offset = named.word(RegExpProgramWord::NamedGroupTableOffset) as usize;
        let read = |at: usize| u64::from_le_bytes(named.bytes()[at..at + 8].try_into().unwrap());
        assert_eq!(read(offset), REGEXP_NAMED_GROUP_TABLE_MAGIC_VERSION);
        assert_eq!(read(offset + 8), 2);
        assert_eq!(read(offset + 16), 2);
        assert_eq!(read(offset + 24), 32);
        let records = offset + 32;
        let candidates = offset + read(records + 8) as usize;
        assert_eq!(read(candidates), 1);
        assert_eq!(read(candidates + 8), 2);
        for (record, expected) in [(records, "x"), (records + 24, "y")] {
            let payload = read(record);
            let start = offset + (payload >> 32) as usize;
            let end = start + payload as u32 as usize;
            assert_eq!(&named.bytes()[start..end], expected.as_bytes());
        }
    }

    #[test]
    fn static_program_dedup_key_includes_named_group_mappings() {
        let base = RegExpProgram::compile("(a)(b)", "").expect("program should compile");
        let mut first = base.clone();
        first.named_groups.push(scriptw_ir::RegExpNamedGroup {
            name: "x".into(),
            capture_ids: vec![1],
        });
        let mut second = first.clone();
        second.named_groups[0].capture_ids = vec![2];
        let mut pool = StringPool::default();
        let first_ref = pool.collect_regexp_program_for_test(&first);
        let second_ref = pool.collect_regexp_program_for_test(&second);
        assert_ne!(first_ref.payload(), second_ref.payload());
        for reference in [first_ref, second_ref] {
            assert_eq!((reference.payload() >> 32) % 8, 0);
        }
    }

    #[test]
    fn shared_programs_collect_both_case_folding_modes_before_deduplication() {
        let legacy = RegExpProgram::compile(r"^(.)\1$", "i").unwrap();
        let unicode = RegExpProgram::compile(r"^(.)\1$", "ui").unwrap();
        assert_eq!(
            RegExpProgramStaticKey::from_program(&legacy),
            RegExpProgramStaticKey::from_program(&unicode)
        );
        for append_between in [false, true] {
            for (first, second) in [(&legacy, &unicode), (&unicode, &legacy)] {
                let mut pool = StringPool::default();
                pool.queue_regexp_program(first);
                if append_between {
                    pool.append_regexp_programs();
                }
                pool.queue_regexp_program(second);
                pool.append_regexp_programs();
                pool.append_regexp_case_folding_tables();
                assert_eq!(
                    pool.regexp_program(first).payload(),
                    pool.regexp_program(second).payload()
                );
                assert_eq!(pool.regexp_case_folding_tables.len(), 2);
                for folding in [RegExpCaseFolding::Legacy, RegExpCaseFolding::Unicode] {
                    let table = pool
                        .regexp_case_folding_table(folding)
                        .expect("required mapping table");
                    assert_eq!(table.ptr % 8, 0);
                    assert_eq!(table.count as usize, folding.mappings().len());
                    let start = (table.ptr - STATIC_DATA_OFFSET) as usize;
                    let rows = pool.bytes[start..start + table.count as usize * 8]
                        .chunks_exact(8)
                        .map(|row| {
                            (
                                u32::from_le_bytes(row[..4].try_into().unwrap()),
                                u32::from_le_bytes(row[4..].try_into().unwrap()),
                            )
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(rows, folding.mappings());
                }
                let bytes = pool.bytes.clone();
                pool.append_regexp_case_folding_tables();
                assert_eq!(pool.bytes, bytes);
            }
        }
    }

    #[test]
    fn backreference_tables_follow_scoped_references_instead_of_global_flags() {
        let mut sensitive = StringPool::default();
        sensitive.queue_regexp_program(&RegExpProgram::compile(r"(a)(?-i:\1)", "i").unwrap());
        sensitive.append_regexp_case_folding_tables();
        assert!(sensitive.regexp_case_folding_tables.is_empty());
        let mut scoped = StringPool::default();
        scoped.queue_regexp_program(&RegExpProgram::compile(r"(a)(?i:\1)", "u").unwrap());
        scoped.append_regexp_case_folding_tables();
        assert_eq!(scoped.regexp_case_folding_tables.len(), 1);
        assert!(scoped
            .regexp_case_folding_table(RegExpCaseFolding::Legacy)
            .is_none());
        assert_eq!(
            scoped
                .regexp_case_folding_table(RegExpCaseFolding::Unicode)
                .unwrap()
                .count,
            1512
        );
    }
}

pub(crate) fn align_heap_start(bytes: usize) -> u64 {
    ((STATIC_DATA_OFFSET as u64 + bytes as u64) + 7) & !7
}

pub(crate) fn initial_memory_pages(static_data_bytes: usize, uses_heap: bool) -> u64 {
    let required = if uses_heap {
        // PrivateByteAlloc checks aligned end against memory.size and grows
        // before cursor publication; the spare page is not an allocation cap.
        align_heap_start(static_data_bytes) + WASM_PAGE_SIZE
    } else {
        STATIC_DATA_OFFSET as u64 + static_data_bytes as u64
    };
    required.div_ceil(WASM_PAGE_SIZE).max(1)
}
