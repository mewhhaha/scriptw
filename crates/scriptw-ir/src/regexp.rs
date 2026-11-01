use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

use icu_properties::props::{GeneralCategory, GeneralCategoryGroup, IdContinue, IdStart, Script};
use icu_properties::script::ScriptWithExtensions;
use icu_properties::{CodePointMapData, CodePointSetData, PropertyParser};
use regress::{
    unicode_property_binary_from_str, unicode_simple_case_fold, unicode_simple_case_fold_mappings,
    unicode_string_property_from_str, unicode_string_property_sequences, UnicodePropertyBinary,
    UnicodeStringProperty,
};

mod source_escape;
pub use source_escape::{
    escape_regexp_pattern_utf16, REGEXP_EMPTY_SOURCE, REGEXP_SOURCE_LINE_ESCAPES,
    REGEXP_SOURCE_SLASH_ESCAPE,
};

/// The encoded width of every [`RegExpInstruction`] in bytes.
pub const REGEXP_INSTRUCTION_WIDTH: usize = 24;

/// The opcode for a successful match.
pub const REGEXP_OPCODE_ACCEPT: u64 = 0;
/// The opcode for an exact ASCII code-unit match.
pub const REGEXP_OPCODE_LITERAL_ASCII: u64 = 1;
/// The opcode for membership in an ASCII character class.
pub const REGEXP_OPCODE_POSITIVE_ASCII_CLASS: u64 = 2;
/// Branch to `operand0`, retaining `operand1` as the ordered fallback.
pub const REGEXP_OPCODE_SPLIT: u64 = 3;
/// Unconditionally branch to the absolute instruction index in `operand0`.
pub const REGEXP_OPCODE_JUMP: u64 = 4;
/// Record the current input position as the start of a numbered capture.
pub const REGEXP_OPCODE_CAPTURE_START: u64 = 5;
/// Record the current input position as the end of a numbered capture.
pub const REGEXP_OPCODE_CAPTURE_END: u64 = 6;
/// Clear captures in the one-based half-open range `[operand0, operand1)`.
pub const REGEXP_OPCODE_CLEAR_CAPTURE_RANGE: u64 = 7;
/// Match an ECMAScript WhiteSpace or LineTerminator code point (`\\s`).
pub const REGEXP_OPCODE_WHITESPACE: u64 = 8;
/// Match one UTF-16 code unit other than an ECMAScript line terminator.
pub const REGEXP_OPCODE_DOT: u64 = 9;
/// Match one Unicode scalar value (or a lone UTF-16 surrogate in Unicode mode).
pub const REGEXP_OPCODE_LITERAL_CODE_POINT: u64 = 10;
/// Match membership in a code-point range set stored in the program's range
/// pool. `operand0` is the index of the first range entry and `operand1` packs
/// the entry count in bits 1.. with the complement bit in bit 0.
pub const REGEXP_OPCODE_UNICODE_PROPERTY: u64 = 11;
/// Match the capture selected by a named backreference.
pub const REGEXP_OPCODE_NAMED_BACKREFERENCE: u64 = 12;
/// Match outside an ASCII character class.
pub const REGEXP_OPCODE_NEGATIVE_ASCII_CLASS: u64 = 13;
/// Match the numbered capture stored in `operand0`.
pub const REGEXP_OPCODE_NUMBERED_BACKREFERENCE: u64 = 14;
/// Numbered-backreference operand1 bit proving the capture cannot be empty.
/// Named backreferences reserve this bit.
pub const REGEXP_BACKREFERENCE_NONEMPTY: u64 = 1;
/// Backreference operand1 bit for the resolved ignoreCase modifier at the reference.
pub const REGEXP_BACKREFERENCE_IGNORE_CASE: u64 = 2;
/// Assert that the current position is the start of the input or a line.
pub const REGEXP_OPCODE_ASSERT_START: u64 = 17;
/// Assert that the current position is the end of the input or a line.
pub const REGEXP_OPCODE_ASSERT_END: u64 = 18;
/// Match one code point that is not ECMAScript WhiteSpace or a LineTerminator.
pub const REGEXP_OPCODE_NOT_WHITESPACE: u64 = 19;
/// Enter a lookaround body in the direction encoded by `operand0`.
pub const REGEXP_OPCODE_LOOKAROUND_START: u64 = 20;
/// Complete a lookaround body. `operand0` identifies its failure sentinel.
pub const REGEXP_OPCODE_LOOKAROUND_END: u64 = 21;
/// Handle exhaustion of every path through a lookaround body.
pub const REGEXP_OPCODE_LOOKAROUND_FAILURE: u64 = 22;
/// Enter one optional iteration whose atom may leave the input index unchanged.
/// `operand0` is the attempt target. `operand1` packs the fallback target in
/// bits 1 and above and [`QuantifierPreference::Lazy`] in bit 0.
pub const REGEXP_OPCODE_PROGRESS_SPLIT: u64 = 23;
/// Complete one nullable optional iteration. `operand0` names its paired
/// progress split and `operand1` is the continuation for a changed input index.
pub const REGEXP_OPCODE_PROGRESS_CHECK: u64 = 24;
/// Assert whether the adjacent input characters have different word membership.
/// `operand0` selects the WordCharacters range slice; `operand1` packs its count
/// in bits 1 and above and the assertion polarity in bit 0.
pub const REGEXP_OPCODE_WORD_BOUNDARY: u64 = 25;
/// Activate one counted repetition with the admitted minimum/maximum bounds.
pub const REGEXP_OPCODE_REPEAT_BEGIN: u64 = 26;
/// Choose the body or paired Exit. Operand0 is End; operand1 is slot<<1 | lazy.
pub const REGEXP_OPCODE_REPEAT_GUARD: u64 = 27;
/// Complete an iteration of the Begin in operand0; operand1 is zero.
pub const REGEXP_OPCODE_REPEAT_END: u64 = 28;
/// Deactivate the Begin in operand0; operand1 is zero.
pub const REGEXP_OPCODE_REPEAT_EXIT: u64 = 29;
mod natural;
pub use natural::{
    RegExpNatural, RegExpRepeatBoundWord, RegExpRepeatBounds, RegExpRepeatMaximum,
    RegExpRepeatMaximumKind, RegExpRepeatStateWord, REGEXP_REPEAT_BOUND_RECORD_SIZE,
    REGEXP_REPEAT_COUNTER_DECIMAL_DIGITS, REGEXP_REPEAT_COUNTER_LIMB_WIDTH,
    REGEXP_REPEAT_COUNTER_RADIX, REGEXP_REPEAT_STATE_HEADER_SIZE,
};

/// The encoded width of one code-point range-pool entry in bytes.
pub const REGEXP_RANGE_ENTRY_WIDTH: usize = 8;

/// A deliberately generous ceiling on the number of pooled code-point ranges.
pub const REGEXP_MAX_RANGE_ENTRIES: usize = 1 << 16;

/// A ceiling for source bodies and finite Unicode string-set matcher programs.
///
/// Counted repetitions retain one body independently of their numeric bounds.
/// This ceiling limits the actual instruction stream, without unrolling bounds. The bound must still cover the
/// largest finite Unicode string sets (`\p{RGI_Emoji}` lowers to ~18k
/// instructions), so it is sized at 32k rather than the historical 4k.
pub const REGEXP_MAX_INSTRUCTIONS: usize = 32768;

/// A fixed-width instruction in a backend-neutral regular-expression program.
///
/// `PositiveAsciiClass` stores its 128-bit membership bitmap with bits 0 through
/// 63 in `operand0` and bits 64 through 127 in `operand1`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegExpInstruction {
    pub opcode: u64,
    pub operand0: u64,
    pub operand1: u64,
}

impl RegExpInstruction {
    pub const fn accept() -> Self {
        Self {
            opcode: REGEXP_OPCODE_ACCEPT,
            operand0: 0,
            operand1: 0,
        }
    }

    pub const fn literal_ascii(code_unit: u8) -> Self {
        Self {
            opcode: REGEXP_OPCODE_LITERAL_ASCII,
            operand0: code_unit as u64,
            operand1: 0,
        }
    }

    pub const fn literal_code_point(code_point: u32) -> Self {
        assert!(code_point <= 0x10ffff);
        Self {
            opcode: REGEXP_OPCODE_LITERAL_CODE_POINT,
            operand0: code_point as u64,
            operand1: 0,
        }
    }

    pub const fn code_point_range_set(first_entry: u32, entry_count: u32, negated: bool) -> Self {
        Self {
            opcode: REGEXP_OPCODE_UNICODE_PROPERTY,
            operand0: first_entry as u64,
            operand1: ((entry_count as u64) << 1) | (negated as u64),
        }
    }

    pub const fn positive_ascii_class(bitmap_low: u64, bitmap_high: u64) -> Self {
        Self {
            opcode: REGEXP_OPCODE_POSITIVE_ASCII_CLASS,
            operand0: bitmap_low,
            operand1: bitmap_high,
        }
    }

    pub const fn negative_ascii_class(bitmap_low: u64, bitmap_high: u64) -> Self {
        Self {
            opcode: REGEXP_OPCODE_NEGATIVE_ASCII_CLASS,
            operand0: bitmap_low,
            operand1: bitmap_high,
        }
    }

    pub const fn split(primary_pc: usize, fallback_pc: usize) -> Self {
        Self {
            opcode: REGEXP_OPCODE_SPLIT,
            operand0: primary_pc as u64,
            operand1: fallback_pc as u64,
        }
    }

    fn progress_split(
        attempt_pc: usize,
        fallback_pc: usize,
        preference: QuantifierPreference,
    ) -> Self {
        Self {
            opcode: REGEXP_OPCODE_PROGRESS_SPLIT,
            operand0: attempt_pc as u64,
            operand1: ((fallback_pc as u64) << 1) | preference.word(),
        }
    }

    fn progress_check(progress_split_pc: usize, continuation_pc: usize) -> Self {
        Self {
            opcode: REGEXP_OPCODE_PROGRESS_CHECK,
            operand0: progress_split_pc as u64,
            operand1: continuation_pc as u64,
        }
    }

    pub const fn repeat_begin(slot: u32) -> Self {
        Self {
            opcode: REGEXP_OPCODE_REPEAT_BEGIN,
            operand0: slot as u64,
            operand1: 0,
        }
    }

    fn repeat_guard(end_pc: usize, slot: u32, preference: QuantifierPreference) -> Self {
        Self {
            opcode: REGEXP_OPCODE_REPEAT_GUARD,
            operand0: end_pc as u64,
            operand1: ((slot as u64) << 1) | preference.word(),
        }
    }

    pub const fn repeat_end(begin_pc: usize) -> Self {
        Self {
            opcode: REGEXP_OPCODE_REPEAT_END,
            operand0: begin_pc as u64,
            operand1: 0,
        }
    }

    pub const fn repeat_exit(begin_pc: usize) -> Self {
        Self {
            opcode: REGEXP_OPCODE_REPEAT_EXIT,
            operand0: begin_pc as u64,
            operand1: 0,
        }
    }

    pub const fn jump(target_pc: usize) -> Self {
        Self {
            opcode: REGEXP_OPCODE_JUMP,
            operand0: target_pc as u64,
            operand1: 0,
        }
    }

    pub const fn capture_start(capture_id: u32) -> Self {
        Self {
            opcode: REGEXP_OPCODE_CAPTURE_START,
            operand0: capture_id as u64,
            operand1: 0,
        }
    }

    pub const fn capture_end(capture_id: u32) -> Self {
        Self {
            opcode: REGEXP_OPCODE_CAPTURE_END,
            operand0: capture_id as u64,
            operand1: 0,
        }
    }

    pub const fn clear_capture_range(first_capture_id: u32, end_capture_id: u32) -> Self {
        Self {
            opcode: REGEXP_OPCODE_CLEAR_CAPTURE_RANGE,
            operand0: first_capture_id as u64,
            operand1: end_capture_id as u64,
        }
    }

    pub const fn whitespace() -> Self {
        Self {
            opcode: REGEXP_OPCODE_WHITESPACE,
            operand0: 0,
            operand1: 0,
        }
    }

    pub const fn not_whitespace() -> Self {
        Self {
            opcode: REGEXP_OPCODE_NOT_WHITESPACE,
            operand0: 0,
            operand1: 0,
        }
    }

    pub const fn dot() -> Self {
        Self {
            opcode: REGEXP_OPCODE_DOT,
            operand0: RegExpModifierOverride::Inherit.operand_code(),
            operand1: 0,
        }
    }

    pub const fn named_backreference(name_id: u32, folding: CaseFolding) -> Self {
        Self {
            opcode: REGEXP_OPCODE_NAMED_BACKREFERENCE,
            operand0: name_id as u64,
            operand1: folding.backreference_operand(),
        }
    }

    pub const fn numbered_backreference(capture_id: u32, folding: CaseFolding) -> Self {
        Self {
            opcode: REGEXP_OPCODE_NUMBERED_BACKREFERENCE,
            operand0: capture_id as u64,
            operand1: folding.backreference_operand(),
        }
    }

    pub const fn nonempty_numbered_backreference(capture_id: u32, folding: CaseFolding) -> Self {
        Self {
            opcode: REGEXP_OPCODE_NUMBERED_BACKREFERENCE,
            operand0: capture_id as u64,
            operand1: REGEXP_BACKREFERENCE_NONEMPTY | folding.backreference_operand(),
        }
    }

    pub const fn assert_start() -> Self {
        Self {
            opcode: REGEXP_OPCODE_ASSERT_START,
            operand0: RegExpModifierOverride::Inherit.operand_code(),
            operand1: 0,
        }
    }

    pub const fn assert_end() -> Self {
        Self {
            opcode: REGEXP_OPCODE_ASSERT_END,
            operand0: RegExpModifierOverride::Inherit.operand_code(),
            operand1: 0,
        }
    }

    const fn lookaround_start(direction: RegExpMatchDirection) -> Self {
        Self {
            opcode: REGEXP_OPCODE_LOOKAROUND_START,
            operand0: direction.operand_bit(),
            operand1: 0,
        }
    }

    const fn lookaround_end(
        failure_pc: usize,
        after_pc: usize,
        polarity: &LookaroundPolarity,
        parent_direction: RegExpMatchDirection,
    ) -> Self {
        Self {
            opcode: REGEXP_OPCODE_LOOKAROUND_END,
            operand0: failure_pc as u64,
            operand1: (after_pc as u64)
                | (parent_direction.operand_bit() << 62)
                | (polarity.operand_bit() << 63),
        }
    }

    const fn lookaround_failure(
        after_pc: usize,
        polarity: &LookaroundPolarity,
        parent_direction: RegExpMatchDirection,
    ) -> Self {
        Self {
            opcode: REGEXP_OPCODE_LOOKAROUND_FAILURE,
            operand0: after_pc as u64,
            operand1: polarity.operand_bit() | (parent_direction.operand_bit() << 1),
        }
    }

    const fn word_boundary(
        first_entry: u32,
        entry_count: u32,
        polarity: WordBoundaryPolarity,
    ) -> Self {
        Self {
            opcode: REGEXP_OPCODE_WORD_BOUNDARY,
            operand0: first_entry as u64,
            operand1: ((entry_count as u64) << 1) | polarity.operand_bit(),
        }
    }

    pub const fn positive_ascii_class_contains(self, code_unit: u8) -> bool {
        if self.opcode != REGEXP_OPCODE_POSITIVE_ASCII_CLASS || code_unit >= 128 {
            return false;
        }

        if code_unit < 64 {
            self.operand0 & (1_u64 << code_unit) != 0
        } else {
            self.operand1 & (1_u64 << (code_unit - 64)) != 0
        }
    }
}

const _: () = assert!(std::mem::size_of::<RegExpInstruction>() == REGEXP_INSTRUCTION_WIDTH);

/// Which of the three mutually exclusive RegExp Unicode grammars applies.
///
/// This mode is carried from flag parsing through atom and character-class
/// parsing. A pair of `unicode` / `unicode_sets` booleans could express an
/// impossible fourth state and previously let `v`-mode class atoms silently
/// inherit `u`-mode escape rules. Keeping the three legal states in one closed
/// domain makes both mistakes compile errors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RegExpUnicodeMode {
    /// Neither `u` nor `v`: legacy and Annex B grammar rules apply.
    #[default]
    Legacy,
    /// The `u` flag: Unicode-mode grammar rules apply.
    Unicode,
    /// The `v` flag: UnicodeSets-mode grammar rules apply.
    UnicodeSets,
}

impl RegExpUnicodeMode {
    /// Whether the restrictions shared by the `u` and `v` modes apply.
    pub const fn is_unicode_mode(self) -> bool {
        match self {
            RegExpUnicodeMode::Legacy => false,
            RegExpUnicodeMode::Unicode | RegExpUnicodeMode::UnicodeSets => true,
        }
    }

    /// Applies the mode-specific `ClassEscape` identity-escape grammar.
    ///
    /// This is deliberately exhaustive: `v` accepts the additional
    /// `ClassSetReservedPunctuator` alternatives that `u` does not.
    fn allows_class_identity_escape(self, escaped: u8) -> bool {
        match self {
            RegExpUnicodeMode::Legacy => true,
            RegExpUnicodeMode::Unicode => is_class_identity_escape(escaped),
            RegExpUnicodeMode::UnicodeSets => {
                is_class_identity_escape(escaped) || is_class_set_reserved_punctuator(escaped)
            }
        }
    }
}

/// The two grammars that may reach an ordinary character-class parser.
///
/// `UnicodeSets` has a distinct class parser and is deliberately not
/// representable here. Bitmap versus code-point ranges is only an encoding
/// choice; both ordinary encoders must receive the same grammar mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrdinaryClassMode {
    Legacy { named_captures: bool },
    Unicode,
}

impl OrdinaryClassMode {
    const fn is_unicode(self) -> bool {
        match self {
            Self::Legacy { .. } => false,
            Self::Unicode => true,
        }
    }

    const fn unicode_mode(self) -> RegExpUnicodeMode {
        match self {
            Self::Legacy { .. } => RegExpUnicodeMode::Legacy,
            Self::Unicode => RegExpUnicodeMode::Unicode,
        }
    }

    const fn named_captures(self) -> bool {
        match self {
            Self::Legacy { named_captures } => named_captures,
            Self::Unicode => false,
        }
    }

    fn allows_class_identity_escape(self, escaped: u8) -> bool {
        match self {
            Self::Legacy { named_captures } => escaped != b'k' || !named_captures,
            Self::Unicode => is_class_identity_escape(escaped),
        }
    }
}

/// RegExp flags that affect matching wrappers rather than match instructions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegExpFlags {
    pub has_indices: bool,
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
    pub dot_all: bool,
    pub sticky: bool,
    pub unicode_mode: RegExpUnicodeMode,
}

/// One source-ordered named capture group and all numbered captures sharing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegExpNamedGroup {
    pub name: String,
    pub capture_ids: Vec<u32>,
}

/// A compiled, backend-neutral regular-expression matcher program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegExpProgram {
    pub flags: RegExpFlags,
    /// Number of numbered captures in opening-parenthesis order.
    pub capture_count: u32,
    /// Named groups in first-source-occurrence order.
    pub named_groups: Vec<RegExpNamedGroup>,
    pub instructions: Vec<RegExpInstruction>,
    /// Inclusive code-point ranges referenced by range-set instructions. The
    /// encoded blob stores these immediately after the instruction stream.
    pub ranges: Vec<(u32, u32)>,
    /// Exact bound pairs in dense counted-slot order.
    pub repeat_bounds: Vec<RegExpRepeatBounds>,
}

/// An append-only pool of sorted, disjoint inclusive code-point ranges.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RegExpRangePool {
    entries: Vec<(u32, u32)>,
    interned: BTreeMap<Vec<(u32, u32)>, u32>,
}

impl RegExpRangePool {
    /// Interns `ranges`, returning the first entry index and the entry count.
    fn intern(
        &mut self,
        ranges: &[(u32, u32)],
        offset: usize,
    ) -> Result<(u32, u32), RegExpCompileError> {
        if let Some(&first) = self.interned.get(ranges) {
            return Ok((first, ranges.len() as u32));
        }
        if self.entries.len() + ranges.len() > REGEXP_MAX_RANGE_ENTRIES {
            return Err(RegExpCompileError::unsupported_feature(
                offset,
                "regular-expression code-point range pool is too large",
            ));
        }
        let first = self.entries.len() as u32;
        self.entries.extend_from_slice(ranges);
        self.interned.insert(ranges.to_vec(), first);
        Ok((first, ranges.len() as u32))
    }

    fn into_entries(self) -> Vec<(u32, u32)> {
        self.entries
    }
}

/// Normalizes an arbitrary list of inclusive ranges into a sorted, disjoint set.
fn normalize_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.retain(|(start, end)| start <= end);
    ranges.sort_unstable();
    let mut normalized: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match normalized.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => normalized.push((start, end)),
        }
    }
    normalized
}

fn ranges_contain(ranges: &[(u32, u32)], code_point: u32) -> bool {
    ranges
        .binary_search_by(|(start, end)| {
            if code_point < *start {
                std::cmp::Ordering::Greater
            } else if code_point > *end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn complement_ranges(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut complement = Vec::with_capacity(ranges.len() + 1);
    let mut next = 0_u32;
    for &(start, end) in ranges {
        if start > next {
            complement.push((next, start - 1));
        }
        next = end.saturating_add(1);
        if end == u32::MAX {
            return complement;
        }
    }
    if next <= 0x10ffff {
        complement.push((next, 0x10ffff));
    }
    complement
}

fn intersect_ranges(left: &[(u32, u32)], right: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        let start = left[i].0.max(right[j].0);
        let end = left[i].1.min(right[j].1);
        if start <= end {
            result.push((start, end));
        }
        if left[i].1 < right[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

fn subtract_ranges(left: &[(u32, u32)], right: &[(u32, u32)]) -> Vec<(u32, u32)> {
    intersect_ranges(left, &complement_ranges(right))
}

impl RegExpProgram {
    pub fn compile(pattern: &str, flags: &str) -> Result<Self, RegExpCompileError> {
        Self::compile_source_bytes(pattern.as_bytes(), flags)
    }

    /// Compiles the semantic UTF-16 Pattern without turning raw surrogates into
    /// RegExp escapes. Adjacent raw pairs and fixed escaped pairs have separate
    /// grammar ownership; a mixed raw/escaped pair remains two Unicode atoms.
    pub fn compile_utf16(pattern: &[u16], flags: &str) -> Result<Self, RegExpCompileError> {
        let source = Utf16PatternSource::new(pattern);
        Self::compile_source_bytes(source.bytes(), flags)
    }

    fn compile_source_bytes(pattern: &[u8], flags: &str) -> Result<Self, RegExpCompileError> {
        let flags = parse_flags(flags)?;
        let parsed = parse_pattern(pattern, flags.unicode_mode, flags.ignore_case)?;
        let mut instructions = Vec::with_capacity(pattern.len() + 1);
        let mut repeat_bounds = Vec::new();
        let mut lowerer = ProgramLowerer::new(
            &mut instructions,
            &mut repeat_bounds,
            pattern.len(),
            &parsed.named_groups,
        );
        lowerer.alternatives(&parsed.alternatives)?;
        lowerer.error_offset = pattern.len();
        lowerer.push(RegExpInstruction::accept())?;
        Ok(Self {
            flags,
            capture_count: parsed.capture_count,
            named_groups: parsed.named_groups,
            instructions,
            ranges: parsed.ranges,
            repeat_bounds,
        })
    }

    /// Encodes match instructions followed by the code-point range pool. Flags
    /// remain wrapper behavior.
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(
            self.instructions.len() * REGEXP_INSTRUCTION_WIDTH
                + self.ranges.len() * REGEXP_RANGE_ENTRY_WIDTH,
        );
        for instruction in &self.instructions {
            encoded.extend_from_slice(&instruction.opcode.to_le_bytes());
            encoded.extend_from_slice(&instruction.operand0.to_le_bytes());
            encoded.extend_from_slice(&instruction.operand1.to_le_bytes());
        }
        for (start, end) in &self.ranges {
            encoded.extend_from_slice(&start.to_le_bytes());
            encoded.extend_from_slice(&end.to_le_bytes());
        }
        encoded
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegExpCompileErrorKind {
    InvalidSyntax,
    UnsupportedFeature,
}

/// The ECMA-262 production or early-error clause an [`RegExpCompileErrorKind::InvalidSyntax`]
/// rejection enforces.
///
/// # Why this exists, and why it is mandatory rather than advisory
///
/// The two verdicts are not symmetric. `InvalidSyntax` is a claim about
/// **ECMAScript** — a conforming engine rejects this pattern, so Scriptw must throw
/// a `SyntaxError` for it. `UnsupportedFeature` is a claim about **this
/// compiler** — the pattern is legal and Scriptw cannot build a matcher program for
/// it yet, so the runtime fallback gets its turn. Their costs differ by an order
/// of magnitude: an over-eager `UnsupportedFeature` loses a fast path, while an
/// over-eager `InvalidSyntax` invents a `SyntaxError` for a legal program. Since
/// batch 7 it does so at run time too — `scriptw-aot-wasm`'s runtime RegExp table
/// maps `InvalidSyntax` to `RuntimeRegExpEntry::Rejected`, which throws.
///
/// Batch 8 found two sites that had made exactly that mistake (`/\//u` and the
/// `v`-mode `ClassSetReservedPunctuator` escapes), so the citation is a
/// **parameter** of [`RegExpCompileError::invalid_syntax`], not a comment: a
/// rejection whose author cannot name the production it enforces does not
/// compile, and `unsupported_feature` — which cites nothing, because it claims
/// nothing about the spec — is the correct constructor for it.
///
/// [`SyntaxRule::ALL`] and the exhaustive `match` in [`SyntaxRule::citation`]
/// are what keep the witness table in this module's tests total: a new variant
/// forces an edit to both, and the table test fails until the new rule has a
/// pinned `(pattern, flags)` witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SyntaxRule {
    /// `(` with no matching `)`.
    UnclosedGroup,
    /// `)` with no matching `(`.
    StrayClosingParenthesis,
    /// The modifier-group flag list `(?ims-ims: … )`.
    ModifierFlags,
    /// A quantifier with no atom in front of it.
    QuantifierWithoutAtom,
    /// A quantifier applied to another quantifier.
    QuantifierAfterQuantifier,
    /// `{n,m}` with `m` below `n`.
    QuantifierBounds,
    /// A `SyntaxCharacter` used unescaped where the grammar requires an escape.
    UnescapedSyntaxCharacter,
    /// `\k` not followed by a `GroupName`.
    NamedBackreferenceSyntax,
    /// `\k<name>` naming no group.
    UnknownGroupName,
    /// Two group specifiers that can match at the same position sharing a name.
    DuplicateGroupName,
    /// The identifier grammar shared by `(?<name>…)` and `\k<name>`.
    RegExpIdentifierName,
    /// The flag string: unknown, duplicated, non-ASCII, or `u` with `v`.
    Flags,
    /// A `\` with nothing, or nothing legal, after it.
    CharacterEscape,
    /// `IdentityEscape` in Unicode mode.
    IdentityEscape,
    /// `ClassEscape` / `ClassSetCharacter` inside a character class.
    ClassEscape,
    /// A raw or escaped character admitted by a `v`-mode set operand.
    ClassSetCharacter,
    /// The `\q{…}` delimiter and string-body grammar.
    ClassStringDisjunction,
    /// The mutually exclusive `v`-mode class union/intersection/subtraction grammar.
    ClassSetExpression,
    /// A negated UnicodeSets class whose contents may contain strings.
    NegatedClassMayContainStrings,
    /// `\xHH`.
    HexEscapeSequence,
    /// `\uHHHH`.
    UnicodeEscapeSequence,
    /// `\u{…}`.
    CodePointEscape,
    /// `\u{…}` above `U+10FFFF`.
    CodePointEscapeRange,
    /// The shape of `\p{…}` / `\P{…}`.
    UnicodePropertyEscape,
    /// The property name or value inside `\p{…}`.
    UnicodePropertyName,
    /// `[` with no matching `]`.
    UnclosedCharacterClass,
    /// A class range whose bounds are the wrong way round.
    ClassRangeOrder,
    /// A class range bound that is a character class rather than a character.
    ClassRangeBound,
}

impl SyntaxRule {
    /// Every rule, for the witness table in this module's tests.
    ///
    /// Hand-maintained, and deliberately paired with the exhaustive `match` in
    /// [`SyntaxRule::citation`]: adding a variant without extending that match
    /// is a compile error, and `every_syntax_rule_has_a_pinned_witness` fails
    /// for a variant that reaches this array with no witness. The array cannot
    /// be derived, so `all_syntax_rules_are_listed_once` checks it is sorted
    /// and duplicate-free — a copy-paste omission then shows up as a failing
    /// test rather than as a silently unaudited rule.
    pub(crate) const ALL: [SyntaxRule; 28] = [
        SyntaxRule::UnclosedGroup,
        SyntaxRule::StrayClosingParenthesis,
        SyntaxRule::ModifierFlags,
        SyntaxRule::QuantifierWithoutAtom,
        SyntaxRule::QuantifierAfterQuantifier,
        SyntaxRule::QuantifierBounds,
        SyntaxRule::UnescapedSyntaxCharacter,
        SyntaxRule::NamedBackreferenceSyntax,
        SyntaxRule::UnknownGroupName,
        SyntaxRule::DuplicateGroupName,
        SyntaxRule::RegExpIdentifierName,
        SyntaxRule::Flags,
        SyntaxRule::CharacterEscape,
        SyntaxRule::IdentityEscape,
        SyntaxRule::ClassEscape,
        SyntaxRule::ClassSetCharacter,
        SyntaxRule::ClassStringDisjunction,
        SyntaxRule::ClassSetExpression,
        SyntaxRule::NegatedClassMayContainStrings,
        SyntaxRule::HexEscapeSequence,
        SyntaxRule::UnicodeEscapeSequence,
        SyntaxRule::CodePointEscape,
        SyntaxRule::CodePointEscapeRange,
        SyntaxRule::UnicodePropertyEscape,
        SyntaxRule::UnicodePropertyName,
        SyntaxRule::UnclosedCharacterClass,
        SyntaxRule::ClassRangeOrder,
        SyntaxRule::ClassRangeBound,
    ];

    /// The production or early-error clause this rule enforces.
    ///
    /// Exhaustive, with no catch-all arm, per AGENTS.md.
    pub(crate) const fn citation(self) -> &'static str {
        match self {
            SyntaxRule::UnclosedGroup => {
                "22.2.1 Atom :: `(` GroupSpecifier? Disjunction `)`; the closing parenthesis is not optional"
            }
            SyntaxRule::StrayClosingParenthesis => {
                "22.2.1 Disjunction; `)` is a SyntaxCharacter and is not a PatternCharacter"
            }
            SyntaxRule::ModifierFlags => {
                "22.2.1 Atom :: `(` `?` RegularExpressionFlags `-`? RegularExpressionFlags? `:` Disjunction `)`, and its 22.2.1.1 early errors"
            }
            SyntaxRule::QuantifierWithoutAtom => {
                "22.2.1 Term :: Atom Quantifier; a Quantifier requires a preceding Atom"
            }
            SyntaxRule::QuantifierAfterQuantifier => {
                "22.2.1 Term :: Atom Quantifier; a Quantifier is not itself an Atom"
            }
            SyntaxRule::QuantifierBounds => {
                "22.2.1.1: it is a Syntax Error if the MV of the first DecimalDigits of a QuantifierPrefix is larger than the MV of the second"
            }
            SyntaxRule::UnescapedSyntaxCharacter => {
                "22.2.1 Atom :: PatternCharacter :: SourceCharacter but not SyntaxCharacter; Annex B ExtendedPatternCharacter does not apply in UnicodeMode"
            }
            SyntaxRule::NamedBackreferenceSyntax => {
                "22.2.1 AtomEscape[+NamedCaptureGroups] :: `k` GroupName"
            }
            SyntaxRule::UnknownGroupName => {
                "22.2.1.1: it is a Syntax Error if GroupSpecifiersThatMatch(GroupName) is empty"
            }
            SyntaxRule::DuplicateGroupName => {
                "22.2.1.1 Pattern early error: it is a Syntax Error if MightBothParticipate is true for two GroupSpecifiers with the same name"
            }
            SyntaxRule::RegExpIdentifierName => {
                "22.2.1 RegExpIdentifierName :: RegExpIdentifierStart RegExpIdentifierPart*, and its 22.2.1.1 early errors"
            }
            SyntaxRule::Flags => {
                "22.2.3.1 RegExpInitialize: it is a Syntax Error if F contains a code unit outside `dgimsuvy`, repeats one, or contains both `u` and `v`"
            }
            SyntaxRule::CharacterEscape => {
                "22.2.1 AtomEscape :: CharacterEscape and ClassEscape :: CharacterEscape; `\\` must be followed by an escape"
            }
            SyntaxRule::IdentityEscape => {
                "22.2.1 IdentityEscape[+UnicodeMode] :: SyntaxCharacter | `/`"
            }
            SyntaxRule::ClassEscape => {
                "22.2.1 ClassEscape[+UnicodeMode] :: `b` | `-` | CharacterClassEscape | CharacterEscape, extended in UnicodeSetsMode by ClassSetCharacter :: `\\` ClassSetReservedPunctuator; B.1.2 SourceCharacterIdentityEscape[+NamedCaptureGroups] excludes `k` in legacy classes"
            }
            SyntaxRule::ClassSetCharacter => {
                "22.2.1 ClassSetCharacter :: [lookahead not in ClassSetReservedDoublePunctuator] SourceCharacter but not ClassSetSyntaxCharacter | `\\` CharacterEscape[+UnicodeMode] | `\\` ClassSetReservedPunctuator | `\\b`; CharacterEscape :: `0` [lookahead not in DecimalDigit]"
            }
            SyntaxRule::ClassStringDisjunction => {
                "22.2.1 ClassStringDisjunction :: `\\q{` ClassStringDisjunctionContents `}`, where each non-empty ClassString is a sequence of ClassSetCharacter nodes"
            }
            SyntaxRule::ClassSetExpression => {
                "22.2.1 ClassContents[+UnicodeSetsMode] :: ClassSetExpression, where ClassSetExpression is exactly one ClassUnion, ClassIntersection, or ClassSubtraction"
            }
            SyntaxRule::NegatedClassMayContainStrings => {
                "22.2.1.1 CharacterClass and NestedClass early errors: `[^` ClassContents `]` is a Syntax Error if the 22.2.1.8 MayContainStrings result for ClassContents is true"
            }
            SyntaxRule::HexEscapeSequence => {
                "22.2.1 CharacterEscape :: HexEscapeSequence :: `x` HexDigit HexDigit"
            }
            SyntaxRule::UnicodeEscapeSequence => {
                "22.2.1 RegExpUnicodeEscapeSequence[+UnicodeMode] :: `u` Hex4Digits"
            }
            SyntaxRule::CodePointEscape => {
                "22.2.1 RegExpUnicodeEscapeSequence[+UnicodeMode] :: `u{` CodePoint `}`"
            }
            SyntaxRule::CodePointEscapeRange => {
                "22.2.1 CodePoint early error: it is a Syntax Error if the MV of HexDigits is greater than 0x10FFFF"
            }
            SyntaxRule::UnicodePropertyEscape => {
                "22.2.1 CharacterClassEscape :: `p{` UnicodePropertyValueExpression `}`"
            }
            SyntaxRule::UnicodePropertyName => {
                "22.2.1.1: it is a Syntax Error if UnicodeMatchProperty / UnicodeMatchPropertyValue does not resolve against tables 69-72"
            }
            SyntaxRule::UnclosedCharacterClass => "22.2.1 CharacterClass :: `[` ClassContents `]`",
            SyntaxRule::ClassRangeOrder => {
                "22.2.1.1 range early errors: the CharacterValue of the first ClassAtom or ClassSetCharacter must not be strictly greater than that of the second"
            }
            SyntaxRule::ClassRangeBound => {
                "22.2.1 ClassSetRange :: ClassSetCharacter `-` ClassSetCharacter; 22.2.1.1 ordinary class ranges reject a bound whose IsCharacterClass is true"
            }
        }
    }
}

/// A cursor failure in validated UTF-8 or UTF-16-derived WTF-8 source is a
/// compiler capability defect, never an ECMAScript syntax rejection. Source
/// decoding includes raw lone surrogate units; RegExp escapes are parsed later.
const NON_BOUNDARY_SOURCE: &str =
    "regular-expression source could not be decoded at a character boundary";

/// A compile failure with the byte offset in the pattern or flag string supplied
/// to [`RegExpProgram::compile`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegExpCompileError {
    pub kind: RegExpCompileErrorKind,
    pub offset: usize,
    pub message: String,
    /// The rule an `InvalidSyntax` rejection enforces.
    ///
    /// `None` exactly when `kind` is `UnsupportedFeature`, which is a claim
    /// about this compiler and cites nothing. Carried on the error rather than
    /// only passed to the constructor so that a test can pin *which* site
    /// answered, not merely that some site did.
    pub(crate) rule: Option<SyntaxRule>,
}

impl RegExpCompileError {
    /// A claim that a conforming engine rejects this pattern.
    ///
    /// `rule` is mandatory. See [`SyntaxRule`] for why.
    fn invalid_syntax(rule: SyntaxRule, offset: usize, message: impl Into<String>) -> Self {
        Self {
            kind: RegExpCompileErrorKind::InvalidSyntax,
            offset,
            message: message.into(),
            rule: Some(rule),
        }
    }

    /// A claim about this compiler only: the pattern is legal and Scriptw cannot
    /// build a matcher program for it. Cites nothing, by construction.
    fn unsupported_feature(offset: usize, message: impl Into<String>) -> Self {
        Self {
            kind: RegExpCompileErrorKind::UnsupportedFeature,
            offset,
            message: message.into(),
            rule: None,
        }
    }
}

impl fmt::Display for RegExpCompileError {
    /// An `InvalidSyntax` rejection prints the rule it enforces.
    ///
    /// Not decoration, and not only for the reader: this is what puts
    /// [`SyntaxRule::citation`] on the product path. `lowering.rs` formats this
    /// message into the `SyntaxError` a rejected pattern throws, so the thrown
    /// error now names the production — and a citation that no code path reads
    /// would be exactly the "compiles clean, no call site" shape AGENTS.md
    /// warns about, which is how an unaudited claim survives.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at byte {}", self.message, self.offset)?;
        match self.rule {
            Some(rule) => write!(formatter, " ({})", rule.citation()),
            None => Ok(()),
        }
    }
}

/// [`SyntaxRule::ALL`] must list every variant exactly once. The length is the
/// half of that a `const` can check; `all_syntax_rules_are_listed_once` checks
/// ordering and uniqueness, and `every_syntax_rule_has_a_pinned_witness` checks
/// that each one is demonstrable.
const _: () = assert!(SyntaxRule::ALL.len() == 28);

impl Error for RegExpCompileError {}

struct ParsedPattern {
    alternatives: Vec<Vec<ParsedTerm>>,
    capture_count: u32,
    named_groups: Vec<RegExpNamedGroup>,
    ranges: Vec<(u32, u32)>,
}

enum ParsedTerm {
    Quantified {
        atom: ParsedAtom,
        quantifier: Quantifier,
        quantifier_offset: usize,
    },
    LegacyUtf16Pair {
        pair: LegacyUtf16Pair,
        trail_quantifier: Quantifier,
        quantifier_offset: usize,
    },
}

mod legacy_utf16_pair;
mod lexical;
mod pure_epsilon;
mod source;
pub use lexical::{
    regexp_character_escape, regexp_hex_digit_value, RegExpScopedModifier,
    REGEXP_CHARACTER_ESCAPES, REGEXP_DIGIT_RANGES, REGEXP_HEX_DIGIT_RANGES,
    REGEXP_LEGACY_THREE_DIGIT_OCTAL_LAST, REGEXP_WHITESPACE_RANGES, REGEXP_WORD_RANGES,
};
mod opcode;
mod program;
pub use opcode::{RegExpControlFlow, RegExpInputProgress, RegExpOpcode, RegExpOperandRule};
pub use program::{
    RegExpProgramValidationError, RegExpProgramWord, ValidatedRegExpProgram,
    REGEXP_NAMED_GROUP_TABLE_MAGIC_VERSION, REGEXP_PROGRAM_HEADER_SIZE,
    REGEXP_PROGRAM_MAGIC_VERSION,
};

use legacy_utf16_pair::LegacyUtf16Pair;
use source::{pattern_code_point_at, Utf16PatternSource};

enum ParsedTermAtom {
    Ordinary(ParsedAtom),
    LegacyUtf16Pair(LegacyUtf16Pair),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordBoundaryPolarity {
    Boundary,
    NonBoundary,
}

impl WordBoundaryPolarity {
    const fn operand_bit(self) -> u64 {
        match self {
            Self::Boundary => 0,
            Self::NonBoundary => 1,
        }
    }
}

/// Whether a lookaround succeeds when its body matches or fails.
enum LookaroundPolarity {
    Positive,
    Negative,
}

impl LookaroundPolarity {
    fn from_syntax_marker(marker: u8) -> Option<Self> {
        match marker {
            b'=' => Some(Self::Positive),
            b'!' => Some(Self::Negative),
            _ => None,
        }
    }

    const fn operand_bit(&self) -> u64 {
        match self {
            Self::Positive => 0,
            Self::Negative => 1,
        }
    }
}

enum ParsedAtom {
    Instruction(RegExpInstruction),
    FiniteClassSet(FiniteClassSetAtom),
    Capture {
        id: u32,
        body: Vec<Vec<ParsedTerm>>,
        subtree_end: u32,
    },
    NonCapture {
        body: Vec<Vec<ParsedTerm>>,
        subtree_start: u32,
        subtree_end: u32,
    },
    NamedBackreference {
        name: String,
        offset: usize,
        folding: CaseFolding,
    },
    NumberedBackreference {
        capture_id: u32,
        folding: CaseFolding,
    },
    Lookaround {
        polarity: LookaroundPolarity,
        direction: RegExpMatchDirection,
        body: Vec<Vec<ParsedTerm>>,
        subtree_start: u32,
        subtree_end: u32,
    },
}

struct NamedCapture {
    name: String,
    id: u32,
    offset: usize,
    path: Vec<(u32, usize)>,
}

fn parse_pattern(
    pattern: &[u8],
    unicode_mode: RegExpUnicodeMode,
    ignore_case: bool,
) -> Result<ParsedPattern, RegExpCompileError> {
    if pattern.is_empty() {
        return Ok(ParsedPattern {
            alternatives: vec![Vec::new()],
            capture_count: 0,
            named_groups: Vec::new(),
            ranges: Vec::new(),
        });
    }

    let (total_capture_count, has_named_capture_syntax) = regexp_capture_syntax(pattern);
    let mut parser = PatternParser {
        bytes: pattern,
        offset: 0,
        capture_count: 0,
        unicode_mode,
        modifiers: Modifiers {
            ignore_case,
            multiline: RegExpModifierOverride::Inherit,
            dot_all: RegExpModifierOverride::Inherit,
        },
        ranges: RegExpRangePool::default(),
        choice_count: 0,
        choice_path: Vec::new(),
        named_captures: Vec::new(),
        total_capture_count,
        has_named_capture_syntax,
    };
    let alternatives = parser.alternatives(None)?;
    let named_groups = named_groups(&parser.named_captures)?;
    validate_named_backreferences(&alternatives, &named_groups)?;
    Ok(ParsedPattern {
        alternatives,
        capture_count: parser.capture_count,
        named_groups,
        ranges: parser.ranges.into_entries(),
    })
}

/// Matching state that RegExp modifier groups (`(?i-s:…)`) can override for the
/// enclosed pattern only.
struct Modifiers {
    ignore_case: bool,
    multiline: RegExpModifierOverride,
    dot_all: RegExpModifierOverride,
}

/// The local `m` or `s` behavior selected by a RegExp modifier group.
pub enum RegExpModifierOverride {
    Inherit,
    ForceOn,
    ForceOff,
}

impl RegExpModifierOverride {
    pub const fn operand_code(&self) -> u64 {
        match self {
            Self::Inherit => 0,
            Self::ForceOn => 1,
            Self::ForceOff => 2,
        }
    }
}

struct PatternParser<'a> {
    bytes: &'a [u8],
    offset: usize,
    capture_count: u32,
    unicode_mode: RegExpUnicodeMode,
    modifiers: Modifiers,
    ranges: RegExpRangePool,
    choice_count: u32,
    choice_path: Vec<(u32, usize)>,
    named_captures: Vec<NamedCapture>,
    total_capture_count: u32,
    has_named_capture_syntax: bool,
}

impl PatternParser<'_> {
    fn alternatives(
        &mut self,
        opening: Option<usize>,
    ) -> Result<Vec<Vec<ParsedTerm>>, RegExpCompileError> {
        let choice_id = self.choice_count;
        self.choice_count += 1;
        self.choice_path.push((choice_id, 0));
        let result = self.alternatives_inner(opening, &mut vec![Vec::new()]);
        self.choice_path.pop();
        result
    }

    fn alternatives_inner(
        &mut self,
        opening: Option<usize>,
        alternatives: &mut Vec<Vec<ParsedTerm>>,
    ) -> Result<Vec<Vec<ParsedTerm>>, RegExpCompileError> {
        loop {
            match self.bytes.get(self.offset).copied() {
                None => {
                    if let Some(opening) = opening {
                        return Err(RegExpCompileError::invalid_syntax(
                            SyntaxRule::UnclosedGroup,
                            opening,
                            "regular-expression capturing group is unclosed",
                        ));
                    }
                    break;
                }
                Some(b')') => {
                    if opening.is_none() {
                        return Err(RegExpCompileError::invalid_syntax(
                            SyntaxRule::StrayClosingParenthesis,
                            self.offset,
                            "regular-expression closing parenthesis has no opening parenthesis",
                        ));
                    }
                    self.offset += 1;
                    break;
                }
                Some(b'|') => {
                    self.offset += 1;
                    alternatives.push(Vec::new());
                    self.choice_path.last_mut().unwrap().1 += 1;
                }
                _ => alternatives.last_mut().unwrap().push(self.term()?),
            }
        }
        Ok(std::mem::take(alternatives))
    }

    fn term(&mut self) -> Result<ParsedTerm, RegExpCompileError> {
        let atom_offset = self.offset;
        let atom = if self.bytes[self.offset] == b'(' {
            if self.bytes.get(self.offset + 1) == Some(&b'?') {
                match self.bytes.get(self.offset + 2).copied() {
                    Some(b':') => {
                        self.offset += 3;
                        let subtree_start = self.capture_count + 1;
                        let body = self.alternatives(Some(atom_offset))?;
                        ParsedTermAtom::Ordinary(ParsedAtom::NonCapture {
                            body,
                            subtree_start,
                            subtree_end: self.capture_count + 1,
                        })
                    }
                    Some(b'<') => {
                        let polarity = self
                            .bytes
                            .get(self.offset + 3)
                            .copied()
                            .and_then(LookaroundPolarity::from_syntax_marker);
                        if let Some(polarity) = polarity {
                            self.offset += 4;
                            let subtree_start = self.capture_count + 1;
                            let body = self.alternatives(Some(atom_offset))?;
                            if !lookbehind_body_supported(&body) {
                                return Err(RegExpCompileError::unsupported_feature(
                                    atom_offset,
                                    "lookbehind body uses an unsupported matcher atom",
                                ));
                            }
                            ParsedTermAtom::Ordinary(ParsedAtom::Lookaround {
                                polarity,
                                direction: RegExpMatchDirection::Reverse,
                                body,
                                subtree_start,
                                subtree_end: self.capture_count + 1,
                            })
                        } else {
                            let name = self.parse_group_name()?;
                            self.capture_count =
                                self.capture_count.checked_add(1).ok_or_else(|| {
                                    RegExpCompileError::unsupported_feature(
                                        atom_offset,
                                        "regular-expression has too many numbered captures",
                                    )
                                })?;
                            let id = self.capture_count;
                            self.named_captures.push(NamedCapture {
                                name,
                                id,
                                offset: atom_offset,
                                path: self.choice_path.clone(),
                            });
                            let body = self.alternatives(Some(atom_offset))?;
                            ParsedTermAtom::Ordinary(ParsedAtom::Capture {
                                id,
                                body,
                                subtree_end: self.capture_count + 1,
                            })
                        }
                    }
                    Some(b'i' | b'm' | b's' | b'-') => {
                        let modifiers = self.parse_modifier_group_prefix(atom_offset)?;
                        let outer = std::mem::replace(&mut self.modifiers, modifiers);
                        let subtree_start = self.capture_count + 1;
                        let body = self.alternatives(Some(atom_offset));
                        self.modifiers = outer;
                        ParsedTermAtom::Ordinary(ParsedAtom::NonCapture {
                            body: body?,
                            subtree_start,
                            subtree_end: self.capture_count + 1,
                        })
                    }
                    Some(marker @ (b'=' | b'!')) => {
                        let polarity = LookaroundPolarity::from_syntax_marker(marker)
                            .expect("lookahead syntax admits only polarity markers");
                        self.offset += 3;
                        let subtree_start = self.capture_count + 1;
                        let body = self.alternatives(Some(atom_offset))?;
                        ParsedTermAtom::Ordinary(ParsedAtom::Lookaround {
                            polarity,
                            direction: RegExpMatchDirection::Forward,
                            body,
                            subtree_start,
                            subtree_end: self.capture_count + 1,
                        })
                    }
                    _ => {
                        return Err(RegExpCompileError::invalid_syntax(
                            SyntaxRule::ModifierFlags,
                            self.offset,
                            "invalid regular-expression group prefix",
                        ));
                    }
                }
            } else {
                self.capture_count = self.capture_count.checked_add(1).ok_or_else(|| {
                    RegExpCompileError::unsupported_feature(
                        self.offset,
                        "regular-expression has too many numbered captures",
                    )
                })?;
                let id = self.capture_count;
                self.offset += 1;
                let body = self.alternatives(Some(atom_offset))?;
                ParsedTermAtom::Ordinary(ParsedAtom::Capture {
                    id,
                    body,
                    subtree_end: self.capture_count + 1,
                })
            }
        } else {
            let atom = parse_instruction_atom(
                self.bytes,
                &mut self.offset,
                self.unicode_mode,
                &self.modifiers,
                &mut self.ranges,
                self.total_capture_count,
                self.has_named_capture_syntax,
            )?;
            match atom {
                ParsedTermAtom::Ordinary(ParsedAtom::Instruction(mut instruction)) => {
                    apply_modifiers(
                        &mut instruction,
                        &self.modifiers,
                        self.unicode_mode,
                        &mut self.ranges,
                        atom_offset,
                    )?;
                    ParsedTermAtom::Ordinary(ParsedAtom::Instruction(instruction))
                }
                atom => atom,
            }
        };
        let quantifier_offset = self.offset;
        let mut quantifier = parse_postfix_quantifier(self.bytes, &mut self.offset)?;
        if let ParsedTermAtom::Ordinary(ParsedAtom::Lookaround { direction, .. }) = &atom {
            if self.offset != quantifier_offset {
                if self.unicode_mode.is_unicode_mode()
                    || matches!(direction, RegExpMatchDirection::Reverse)
                {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::QuantifierWithoutAtom,
                        quantifier_offset,
                        "only legacy lookahead assertions may be quantified",
                    ));
                }
                let minimum = u64::from(!quantifier.is_optional());
                quantifier = Quantifier::new(
                    minimum,
                    Some(minimum),
                    matches!(quantifier.preference, QuantifierPreference::Lazy),
                );
            }
        } else if self.offset != quantifier_offset
            && matches!(
                atom,
                ParsedTermAtom::Ordinary(ParsedAtom::Instruction(RegExpInstruction {
                    opcode: REGEXP_OPCODE_ASSERT_START
                        | REGEXP_OPCODE_ASSERT_END
                        | REGEXP_OPCODE_WORD_BOUNDARY,
                    ..
                }))
            )
        {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::QuantifierWithoutAtom,
                quantifier_offset,
                "a boundary assertion cannot be quantified",
            ));
        }
        Ok(match atom {
            ParsedTermAtom::Ordinary(atom) => ParsedTerm::Quantified {
                atom,
                quantifier,
                quantifier_offset,
            },
            ParsedTermAtom::LegacyUtf16Pair(pair) => ParsedTerm::LegacyUtf16Pair {
                pair,
                trail_quantifier: quantifier,
                quantifier_offset,
            },
        })
    }

    /// Parses `(?ims-ims:` and returns the modifier state for the group body.
    ///
    /// `self.offset` is left immediately after the `:`.
    fn parse_modifier_group_prefix(
        &mut self,
        group_offset: usize,
    ) -> Result<Modifiers, RegExpCompileError> {
        let mut cursor = group_offset + 2;
        let mut added = 0_u8;
        let mut removed = 0_u8;
        let mut seen_dash = false;
        loop {
            let Some(&byte) = self.bytes.get(cursor) else {
                // `ModifierFlags`, not `UnclosedGroup`: what ran out here is the
                // modifier prefix of `(?` RegularExpressionFlags `:`, so
                // `UnclosedGroup`'s citation ("Atom :: `(` GroupSpecifier?
                // Disjunction `)`") names a production this pattern does not
                // violate — and since `Display` puts `citation()` on the product
                // path, `/(?i/` would throw a `SyntaxError` naming the wrong
                // rule. The five sibling rejections in this function all cite
                // `ModifierFlags`. `every_syntax_rule_has_a_pinned_witness` is
                // per-RULE, not per-SITE, so it cannot catch a wrong variant at a
                // non-witness site; this is one such site.
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ModifierFlags,
                    group_offset,
                    "regular-expression modifier group is unclosed",
                ));
            };
            if byte == b':' {
                cursor += 1;
                break;
            }
            if byte == b'-' {
                if seen_dash {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::ModifierFlags,
                        cursor,
                        "regular-expression modifier group has a repeated `-`",
                    ));
                }
                seen_dash = true;
                cursor += 1;
                continue;
            }
            let bit = match RegExpScopedModifier::from_marker(byte) {
                Some(modifier) => modifier.bit(),
                None => {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::ModifierFlags,
                        cursor,
                        format!(
                            "invalid regular-expression modifier `{}`",
                            byte.escape_ascii()
                        ),
                    ));
                }
            };
            if added & bit != 0 || removed & bit != 0 {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ModifierFlags,
                    cursor,
                    format!("duplicate regular-expression modifier `{}`", byte as char),
                ));
            }
            if seen_dash {
                removed |= bit;
            } else {
                added |= bit;
            }
            cursor += 1;
        }
        if added == 0 && removed == 0 {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::ModifierFlags,
                group_offset,
                "regular-expression modifier group has no modifiers",
            ));
        }
        self.offset = cursor;
        let mut modifiers = Modifiers {
            ignore_case: self.modifiers.ignore_case,
            multiline: match &self.modifiers.multiline {
                RegExpModifierOverride::Inherit => RegExpModifierOverride::Inherit,
                RegExpModifierOverride::ForceOn => RegExpModifierOverride::ForceOn,
                RegExpModifierOverride::ForceOff => RegExpModifierOverride::ForceOff,
            },
            dot_all: match &self.modifiers.dot_all {
                RegExpModifierOverride::Inherit => RegExpModifierOverride::Inherit,
                RegExpModifierOverride::ForceOn => RegExpModifierOverride::ForceOn,
                RegExpModifierOverride::ForceOff => RegExpModifierOverride::ForceOff,
            },
        };
        if added & RegExpScopedModifier::IgnoreCase.bit() != 0 {
            modifiers.ignore_case = true;
        }
        if removed & RegExpScopedModifier::IgnoreCase.bit() != 0 {
            modifiers.ignore_case = false;
        }
        if added & RegExpScopedModifier::Multiline.bit() != 0 {
            modifiers.multiline = RegExpModifierOverride::ForceOn;
        }
        if removed & RegExpScopedModifier::Multiline.bit() != 0 {
            modifiers.multiline = RegExpModifierOverride::ForceOff;
        }
        if added & RegExpScopedModifier::DotAll.bit() != 0 {
            modifiers.dot_all = RegExpModifierOverride::ForceOn;
        }
        if removed & RegExpScopedModifier::DotAll.bit() != 0 {
            modifiers.dot_all = RegExpModifierOverride::ForceOff;
        }
        Ok(modifiers)
    }

    fn parse_group_name(&mut self) -> Result<String, RegExpCompileError> {
        let start = self.offset + 3;
        let (name, end) = parse_regexp_identifier_name(self.bytes, start, "named capture group")?;
        self.offset = end;
        Ok(name)
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_instruction_atom(
    bytes: &[u8],
    offset: &mut usize,
    unicode_mode: RegExpUnicodeMode,
    modifiers: &Modifiers,
    pool: &mut RegExpRangePool,
    total_capture_count: u32,
    has_named_capture_syntax: bool,
) -> Result<ParsedTermAtom, RegExpCompileError> {
    let atom_offset = *offset;
    let byte = bytes[atom_offset];
    let unicode = unicode_mode.is_unicode_mode();
    if byte == b'\\'
        && bytes
            .get(atom_offset + 1)
            .is_some_and(|byte| !byte.is_ascii())
    {
        if unicode {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::IdentityEscape,
                atom_offset,
                "non-ASCII identity escape is invalid in Unicode mode",
            ));
        }
        return parse_non_ascii_pattern_atom(bytes, offset, atom_offset + 1, unicode_mode);
    }
    if bytes.get(atom_offset..atom_offset + 2) == Some(b"\\k") {
        if !unicode && !has_named_capture_syntax {
            *offset += 2;
            return Ok(ParsedTermAtom::Ordinary(ParsedAtom::Instruction(
                RegExpInstruction::literal_ascii(b'k'),
            )));
        }
        if bytes.get(atom_offset + 2) != Some(&b'<') {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::NamedBackreferenceSyntax,
                atom_offset,
                "malformed named backreference",
            ));
        }
        let (name, end) =
            parse_regexp_identifier_name(bytes, atom_offset + 3, "named backreference")?;
        *offset = end;
        return Ok(ParsedTermAtom::Ordinary(ParsedAtom::NamedBackreference {
            name,
            offset: atom_offset,
            folding: CaseFolding::from_flags(modifiers.ignore_case, unicode_mode),
        }));
    }
    if byte == b'\\' {
        if let Some(marker @ (b'b' | b'B')) = bytes.get(atom_offset + 1) {
            let polarity = match marker {
                b'b' => WordBoundaryPolarity::Boundary,
                b'B' => WordBoundaryPolarity::NonBoundary,
                _ => unreachable!("word-boundary syntax has two polarity markers"),
            };
            let ranges = case_close_ranges(
                REGEXP_WORD_RANGES,
                CaseFolding::from_flags(modifiers.ignore_case, unicode_mode),
            );
            let (first_entry, entry_count) = pool.intern(&ranges, atom_offset)?;
            *offset += 2;
            return Ok(ParsedTermAtom::Ordinary(ParsedAtom::Instruction(
                RegExpInstruction::word_boundary(first_entry, entry_count, polarity),
            )));
        }
        if matches!(bytes.get(atom_offset + 1), Some(b'1'..=b'9')) {
            let mut end = atom_offset + 1;
            let mut capture_id = Some(0_u32);
            while let Some(digit @ b'0'..=b'9') = bytes.get(end) {
                capture_id = capture_id
                    .and_then(|value| value.checked_mul(10)?.checked_add(u32::from(digit - b'0')));
                end += 1;
            }
            if let Some(capture_id) = capture_id.filter(|id| *id <= total_capture_count) {
                *offset = end;
                return Ok(ParsedTermAtom::Ordinary(
                    ParsedAtom::NumberedBackreference {
                        capture_id,
                        folding: CaseFolding::from_flags(modifiers.ignore_case, unicode_mode),
                    },
                ));
            }
        }
        if unicode_mode == RegExpUnicodeMode::UnicodeSets {
            let mut property_end = atom_offset;
            if let Some(value) = parse_unicode_property_of_strings(
                bytes,
                &mut property_end,
                CaseFolding::from_flags(modifiers.ignore_case, unicode_mode),
            )? {
                *offset = property_end;
                return Ok(ParsedTermAtom::Ordinary(ParsedAtom::FiniteClassSet(
                    FiniteClassSetAtom::new(
                        value.finite,
                        false,
                        modifiers.ignore_case,
                        pool,
                        atom_offset,
                    )?,
                )));
            }
        }
    }
    if !byte.is_ascii() {
        return parse_non_ascii_pattern_atom(bytes, offset, atom_offset, unicode_mode);
    }
    let instruction = match byte {
        b'^' => {
            *offset += 1;
            RegExpInstruction::assert_start()
        }
        b'$' => {
            *offset += 1;
            RegExpInstruction::assert_end()
        }
        b'{' => {
            // A complete braced quantifier cannot appear without a preceding
            // atom. Incomplete/non-decimal forms are Annex B literal braces.
            let mut probe = atom_offset;
            if parse_braced_quantifier(bytes, &mut probe)?.is_some() {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::QuantifierWithoutAtom,
                    atom_offset,
                    "regular-expression quantifier has no preceding atom",
                ));
            }
            if unicode {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnescapedSyntaxCharacter,
                    atom_offset,
                    "unescaped regular-expression opening brace is invalid in Unicode mode",
                ));
            }
            *offset += 1;
            RegExpInstruction::literal_ascii(byte)
        }
        b'}' => {
            // A `}` closing a braced quantifier never reaches this arm; a
            // lone one is an Annex B literal outside Unicode mode only.
            if unicode {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnescapedSyntaxCharacter,
                    atom_offset,
                    "unescaped regular-expression closing brace is invalid in Unicode mode",
                ));
            }
            *offset += 1;
            RegExpInstruction::literal_ascii(byte)
        }
        b']' => {
            // Annex B extends PatternCharacter with a lone `]`; Unicode mode
            // does not.
            if unicode {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnescapedSyntaxCharacter,
                    atom_offset,
                    "unescaped regular-expression closing bracket is invalid in Unicode mode",
                ));
            }
            *offset += 1;
            RegExpInstruction::literal_ascii(byte)
        }
        b'[' => match unicode_mode {
            RegExpUnicodeMode::Legacy => parse_class(
                bytes,
                offset,
                OrdinaryClassMode::Legacy {
                    named_captures: has_named_capture_syntax,
                },
                modifiers,
                pool,
            )?,
            RegExpUnicodeMode::Unicode => {
                if let Some(instruction) = parse_single_unicode_class(bytes, offset)? {
                    instruction
                } else {
                    parse_class(bytes, offset, OrdinaryClassMode::Unicode, modifiers, pool)?
                }
            }
            RegExpUnicodeMode::UnicodeSets => {
                match parse_unicode_sets_class(bytes, offset, modifiers, pool)? {
                    UnicodeSetsClassAtom::Instruction(instruction) => instruction,
                    UnicodeSetsClassAtom::FiniteClassSet(atom) => {
                        return Ok(ParsedTermAtom::Ordinary(ParsedAtom::FiniteClassSet(atom)));
                    }
                }
            }
        },
        b'\\' => parse_escaped_atom(bytes, offset, unicode_mode, modifiers, pool)?,
        b'.' => {
            *offset += 1;
            RegExpInstruction::dot()
        }
        b'*' | b'+' | b'?' => {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::QuantifierWithoutAtom,
                atom_offset,
                "regular-expression quantifier has no preceding atom",
            ));
        }
        byte if is_syntax_character(byte) => {
            return Err(RegExpCompileError::unsupported_feature(
                atom_offset,
                format!(
                    "unsupported regular-expression metacharacter `{}`",
                    byte as char
                ),
            ));
        }
        byte => {
            *offset += 1;
            RegExpInstruction::literal_ascii(byte)
        }
    };
    Ok(ParsedTermAtom::Ordinary(ParsedAtom::Instruction(
        instruction,
    )))
}

/// Direct and legacy identity source characters share the actual UTF-16 atom
/// owner. A following quantifier belongs to the trailing unit of a BMP pattern.
fn parse_non_ascii_pattern_atom(
    bytes: &[u8],
    offset: &mut usize,
    scalar_offset: usize,
    unicode_mode: RegExpUnicodeMode,
) -> Result<ParsedTermAtom, RegExpCompileError> {
    let (code_point, end) = pattern_code_point_at(bytes, scalar_offset)?;
    *offset = end;
    if unicode_mode.is_unicode_mode() || code_point <= 0xffff {
        return Ok(ParsedTermAtom::Ordinary(ParsedAtom::Instruction(
            RegExpInstruction::literal_code_point(code_point),
        )));
    }
    let pair = LegacyUtf16Pair::from_scalar(
        char::from_u32(code_point).expect("an astral source character is a Unicode scalar"),
    )
    .expect("an astral Unicode scalar has one UTF-16 surrogate pair");
    Ok(ParsedTermAtom::LegacyUtf16Pair(pair))
}

fn regexp_capture_syntax(bytes: &[u8]) -> (u32, bool) {
    let mut capture_count = 0_u32;
    let mut has_named_capture = false;
    let mut offset = 0;
    let mut in_class = false;
    while let Some(&byte) = bytes.get(offset) {
        if byte == b'\\' {
            offset += 2;
            continue;
        }
        if byte == b'[' {
            in_class = true;
            offset += 1;
            continue;
        }
        if byte == b']' && in_class {
            in_class = false;
            offset += 1;
            continue;
        }
        if byte != b'(' || in_class {
            offset += 1;
            continue;
        }
        if bytes.get(offset + 1) == Some(&b'?') {
            // Every `(?` group is noncapturing except a named capture. The
            // parser validates its prefix; scoped modifiers do not add IDs.
            if bytes.get(offset + 2) == Some(&b'<')
                && !matches!(bytes.get(offset + 3), Some(b'=') | Some(b'!'))
            {
                capture_count += 1;
                has_named_capture = true;
            }
        } else {
            capture_count += 1;
        }
        offset += 1;
    }
    (capture_count, has_named_capture)
}

/// Optional attempts require their paired progress owner exactly when the
/// parsed atom has a successful path that can retain the input position.
#[derive(Clone, Copy)]
enum OptionalAtomProgress {
    MustAdvance,
    MayRemainAtSameIndex,
}

impl OptionalAtomProgress {
    fn for_atom(atom: &ParsedAtom) -> Self {
        if atom_nullable(atom) {
            Self::MayRemainAtSameIndex
        } else {
            Self::MustAdvance
        }
    }
}

fn atom_nullable(atom: &ParsedAtom) -> bool {
    match atom {
        ParsedAtom::Instruction(instruction) => matches!(
            instruction.opcode,
            REGEXP_OPCODE_ASSERT_START | REGEXP_OPCODE_ASSERT_END | REGEXP_OPCODE_WORD_BOUNDARY
        ),
        ParsedAtom::Capture { body, .. } | ParsedAtom::NonCapture { body, .. } => body
            .iter()
            .any(|sequence| sequence.iter().all(|term| term_nullable(term))),
        ParsedAtom::NamedBackreference { .. } => true,
        // An unmatched capture is empty even if its body cannot match empty.
        // Body nullability alone does not prove that a capture participated.
        ParsedAtom::NumberedBackreference { .. } => true,
        ParsedAtom::Lookaround { .. } => true,
        ParsedAtom::FiniteClassSet(atom) => atom.contains_empty,
    }
}

fn lookbehind_body_supported(alternatives: &[Vec<ParsedTerm>]) -> bool {
    alternatives.iter().flatten().all(|term| match term {
        ParsedTerm::Quantified { atom, .. } => match atom {
            ParsedAtom::Instruction(instruction) => matches!(
                instruction.opcode,
                REGEXP_OPCODE_LITERAL_ASCII
                    | REGEXP_OPCODE_LITERAL_CODE_POINT
                    | REGEXP_OPCODE_UNICODE_PROPERTY
                    | REGEXP_OPCODE_POSITIVE_ASCII_CLASS
                    | REGEXP_OPCODE_NEGATIVE_ASCII_CLASS
                    | REGEXP_OPCODE_WHITESPACE
                    | REGEXP_OPCODE_NOT_WHITESPACE
                    | REGEXP_OPCODE_DOT
                    | REGEXP_OPCODE_ASSERT_START
                    | REGEXP_OPCODE_ASSERT_END
                    | REGEXP_OPCODE_WORD_BOUNDARY
            ),
            ParsedAtom::Capture { body, .. } | ParsedAtom::NonCapture { body, .. } => {
                lookbehind_body_supported(body)
            }
            ParsedAtom::FiniteClassSet(_) => true,
            ParsedAtom::Lookaround { .. } => true,
            ParsedAtom::NamedBackreference { .. } | ParsedAtom::NumberedBackreference { .. } => {
                true
            }
        },
        ParsedTerm::LegacyUtf16Pair { .. } => true,
    })
}
fn term_nullable(term: &ParsedTerm) -> bool {
    match term {
        ParsedTerm::Quantified {
            atom, quantifier, ..
        } => quantifier.is_optional() || atom_nullable(atom),
        ParsedTerm::LegacyUtf16Pair { .. } => false,
    }
}

/// Runs the named-backreference early error over the complete Pattern.
/// The lowerer retains the same check as defense in depth.
fn validate_named_backreferences(
    alternatives: &[Vec<ParsedTerm>],
    named_groups: &[RegExpNamedGroup],
) -> Result<(), RegExpCompileError> {
    for term in alternatives.iter().flatten() {
        let ParsedTerm::Quantified { atom, .. } = term else {
            continue;
        };
        match atom {
            ParsedAtom::Capture { body, .. }
            | ParsedAtom::NonCapture { body, .. }
            | ParsedAtom::Lookaround { body, .. } => {
                validate_named_backreferences(body, named_groups)?;
            }
            ParsedAtom::NamedBackreference { name, offset, .. } => {
                if !named_groups.iter().any(|group| group.name == *name) {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::UnknownGroupName,
                        *offset,
                        format!("unknown named backreference `{name}`"),
                    ));
                }
            }
            ParsedAtom::Instruction(_)
            | ParsedAtom::FiniteClassSet(_)
            | ParsedAtom::NumberedBackreference { .. } => {}
        }
    }
    Ok(())
}

fn named_groups(captures: &[NamedCapture]) -> Result<Vec<RegExpNamedGroup>, RegExpCompileError> {
    let mut groups = Vec::<RegExpNamedGroup>::new();
    let mut first = Vec::<&NamedCapture>::new();
    for capture in captures {
        if let Some((index, prior)) = first
            .iter()
            .enumerate()
            .find(|(_, prior)| prior.name == capture.name)
        {
            if !duplicate_names_diverge(prior, capture) {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::DuplicateGroupName,
                    capture.offset,
                    format!("duplicate named capture group `{}`", capture.name),
                ));
            }
            // Every prior occurrence must be on a distinct arm of a shared choice.
            if groups[index].capture_ids.len() > 1
                && captures
                    .iter()
                    .filter(|other| other.name == capture.name)
                    .take_while(|other| other.id != capture.id)
                    .any(|other| !duplicate_names_diverge(other, capture))
            {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::DuplicateGroupName,
                    capture.offset,
                    format!("duplicate named capture group `{}`", capture.name),
                ));
            }
            groups[index].capture_ids.push(capture.id);
        } else {
            first.push(capture);
            groups.push(RegExpNamedGroup {
                name: capture.name.clone(),
                capture_ids: vec![capture.id],
            });
        }
    }
    Ok(groups)
}

fn duplicate_names_diverge(left: &NamedCapture, right: &NamedCapture) -> bool {
    left.path.iter().any(|(choice, arm)| {
        right
            .path
            .iter()
            .any(|(other_choice, other_arm)| choice == other_choice && arm != other_arm)
    })
}

fn ascii_hex_value(byte: u8) -> Option<u32> {
    regexp_hex_digit_value(u32::from(byte))
}

/// Which `RegExpIdentifierName` grammar position is being classified.
///
/// Keeping the two Unicode property domains closed prevents a new call site
/// from selecting `ID_Start` or `ID_Continue` through a stringly regular
/// expression. The pinned ICU property tables are the semantic data source;
/// the vendored `regress` dependency supplies pinned Unicode property, string
/// sequence and simple-fold data. Product matching uses Scriptw's emitted program;
/// the former third-party generator membership fold has been removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegExpIdentifierPosition {
    Start,
    Continue,
}

impl RegExpIdentifierPosition {
    fn after(prefix: &str) -> Self {
        if prefix.is_empty() {
            Self::Start
        } else {
            Self::Continue
        }
    }

    pub fn accepts(self, code_point: char) -> bool {
        match self {
            Self::Start => {
                matches!(code_point, '$' | '_')
                    || CodePointSetData::new::<IdStart>().contains(code_point)
            }
            Self::Continue => {
                matches!(code_point, '$' | '_' | '\u{200C}' | '\u{200D}')
                    || CodePointSetData::new::<IdContinue>().contains(code_point)
            }
        }
    }

    /// Ordered, disjoint scalar ranges for emitted RegExpIdentifierName checks.
    /// This projects the same pinned ICU property authority as `accepts`;
    /// ECMAScript's extra identifier characters are included in that projection.
    pub fn ranges(self) -> Vec<(u32, u32)> {
        let mut ranges: Vec<_> = match self {
            Self::Start => CodePointSetData::new::<IdStart>()
                .iter_ranges()
                .map(|range| (*range.start(), *range.end()))
                .collect(),
            Self::Continue => CodePointSetData::new::<IdContinue>()
                .iter_ranges()
                .map(|range| (*range.start(), *range.end()))
                .collect(),
        };
        ranges.extend([
            (u32::from('$'), u32::from('$')),
            (u32::from('_'), u32::from('_')),
        ]);
        if self == Self::Continue {
            ranges.push((0x200c, 0x200d));
        }
        normalize_ranges(ranges)
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Continue => "continuation",
        }
    }
}

fn parse_regexp_identifier_name(
    bytes: &[u8],
    start: usize,
    description: &'static str,
) -> Result<(String, usize), RegExpCompileError> {
    let mut name = String::new();
    let mut cursor = start;

    loop {
        let Some(&byte) = bytes.get(cursor) else {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::RegExpIdentifierName,
                cursor,
                format!("{description} identifier is unclosed"),
            ));
        };
        if byte == b'>' {
            if name.is_empty() {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::RegExpIdentifierName,
                    cursor,
                    format!("{description} identifier is empty"),
                ));
            }
            return Ok((name, cursor + 1));
        }

        let code_point_offset = cursor;
        let code_point = if byte == b'\\' {
            if bytes.get(cursor + 1) != Some(&b'u') {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::RegExpIdentifierName,
                    cursor,
                    format!("{description} contains a non-Unicode identifier escape"),
                ));
            }

            if bytes.get(cursor + 2) == Some(&b'{') {
                let digits_start = cursor + 3;
                let mut value = 0_u32;
                cursor = digits_start;
                if bytes.get(cursor) == Some(&b'}') {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::RegExpIdentifierName,
                        cursor,
                        format!("{description} contains an empty Unicode identifier escape"),
                    ));
                }
                loop {
                    let Some(&digit) = bytes.get(cursor) else {
                        return Err(RegExpCompileError::invalid_syntax(
                            SyntaxRule::RegExpIdentifierName,
                            cursor,
                            format!("{description} contains an unclosed Unicode identifier escape"),
                        ));
                    };
                    if digit == b'}' {
                        break;
                    }
                    let Some(digit) = ascii_hex_value(digit) else {
                        return Err(RegExpCompileError::invalid_syntax(
                            SyntaxRule::RegExpIdentifierName,
                            cursor,
                            format!("{description} contains a malformed Unicode identifier escape"),
                        ));
                    };
                    value = value
                        .checked_mul(16)
                        .and_then(|value| value.checked_add(digit))
                        .ok_or_else(|| {
                            RegExpCompileError::invalid_syntax(
                                SyntaxRule::RegExpIdentifierName,
                                code_point_offset,
                                format!("{description} Unicode identifier escape is out of range"),
                            )
                        })?;
                    cursor += 1;
                }
                cursor += 1;
                char::from_u32(value).ok_or_else(|| {
                    RegExpCompileError::invalid_syntax(
                        SyntaxRule::RegExpIdentifierName,
                        code_point_offset,
                        format!("{description} Unicode identifier escape is not a scalar value"),
                    )
                })?
            } else {
                let digits = bytes.get(cursor + 2..cursor + 6).ok_or_else(|| {
                    RegExpCompileError::invalid_syntax(
                        SyntaxRule::RegExpIdentifierName,
                        bytes.len(),
                        format!("{description} contains an incomplete Unicode identifier escape"),
                    )
                })?;
                let invalid_digit = digits.iter().position(|digit| !digit.is_ascii_hexdigit());
                if let Some(invalid_digit) = invalid_digit {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::RegExpIdentifierName,
                        cursor + 2 + invalid_digit,
                        format!("{description} contains a malformed Unicode identifier escape"),
                    ));
                }
                let high = digits.iter().fold(0_u16, |value, digit| {
                    (value << 4) | ascii_hex_value(*digit).expect("hex digit") as u16
                });
                cursor += 6;

                if (0xD800..=0xDBFF).contains(&high) {
                    let low_digits = bytes.get(cursor + 2..cursor + 6).filter(|digits| {
                        bytes.get(cursor..cursor + 2) == Some(b"\\u")
                            && digits.iter().all(u8::is_ascii_hexdigit)
                    });
                    if let Some(low_digits) = low_digits {
                        let low = low_digits.iter().fold(0_u16, |value, digit| {
                            (value << 4) | ascii_hex_value(*digit).expect("hex digit") as u16
                        });
                        if (0xDC00..=0xDFFF).contains(&low) {
                            cursor += 6;
                            let scalar = 0x1_0000
                                + ((u32::from(high) - 0xD800) << 10)
                                + (u32::from(low) - 0xDC00);
                            char::from_u32(scalar).expect("paired surrogates form a scalar")
                        } else {
                            return Err(RegExpCompileError::invalid_syntax(
                                SyntaxRule::RegExpIdentifierName,
                                code_point_offset,
                                format!("{description} contains an unpaired lead surrogate escape"),
                            ));
                        }
                    } else {
                        return Err(RegExpCompileError::invalid_syntax(
                            SyntaxRule::RegExpIdentifierName,
                            code_point_offset,
                            format!("{description} contains an unpaired lead surrogate escape"),
                        ));
                    }
                } else if (0xDC00..=0xDFFF).contains(&high) {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::RegExpIdentifierName,
                        code_point_offset,
                        format!("{description} contains an unpaired trail surrogate escape"),
                    ));
                } else {
                    char::from_u32(u32::from(high)).expect("non-surrogate u16 is a scalar")
                }
            }
        } else {
            let (code_point, end) = pattern_code_point_at(bytes, cursor)?;
            cursor = end;
            char::from_u32(code_point).ok_or_else(|| {
                RegExpCompileError::invalid_syntax(
                    SyntaxRule::RegExpIdentifierName,
                    code_point_offset,
                    format!("{description} contains a raw unpaired surrogate"),
                )
            })?
        };

        let position = RegExpIdentifierPosition::after(&name);
        if !position.accepts(code_point) {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::RegExpIdentifierName,
                code_point_offset,
                format!(
                    "{description} code point U+{:04X} is not valid in identifier {position}",
                    u32::from(code_point),
                    position = position.description(),
                ),
            ));
        }
        name.push(code_point);
    }
}

/// One of the eight flag letters 22.2.3.1 accepts. Closed domain.
///
/// Introduced to delete a dead arm rather than to abstract anything.
/// [`parse_flags`] used to narrow `byte` to these eight in one `match` and then
/// re-`match` the same byte, and that second match carried a
/// `byte if first_unsupported.is_none()` arm plus a trailing
/// `if let Some(..) = first_unsupported { unsupported_feature(..) }`. Neither
/// could ever run: every byte outside the eight has already returned
/// `InvalidSyntax` from the first match. It compiled, it formatted cleanly and
/// it produced no dead-code warning — exactly the pattern AGENTS.md calls out
/// ("if something is unreachable from the product path, that should fail to
/// build, not merely fail to run"). Parsing the byte into this enum once makes
/// the second match exhaustive over a closed set with no catch-all, so the
/// unreachable state stops being expressible instead of being commented as
/// impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlagLetter {
    HasIndices,
    Global,
    IgnoreCase,
    Multiline,
    DotAll,
    Unicode,
    UnicodeSets,
    Sticky,
}

impl FlagLetter {
    /// `None` for any code unit outside `dgimsuvy`, which 22.2.3.1 makes a
    /// `SyntaxError`.
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'd' => Some(FlagLetter::HasIndices),
            b'g' => Some(FlagLetter::Global),
            b'i' => Some(FlagLetter::IgnoreCase),
            b'm' => Some(FlagLetter::Multiline),
            b's' => Some(FlagLetter::DotAll),
            b'u' => Some(FlagLetter::Unicode),
            b'v' => Some(FlagLetter::UnicodeSets),
            b'y' => Some(FlagLetter::Sticky),
            _ => None,
        }
    }

    /// The bit this letter occupies in the seen-flags set.
    fn bit(self) -> u8 {
        match self {
            FlagLetter::HasIndices => 1 << 0,
            FlagLetter::Global => 1 << 1,
            FlagLetter::IgnoreCase => 1 << 2,
            FlagLetter::Multiline => 1 << 3,
            FlagLetter::DotAll => 1 << 4,
            FlagLetter::Unicode => 1 << 5,
            FlagLetter::UnicodeSets => 1 << 6,
            FlagLetter::Sticky => 1 << 7,
        }
    }

    /// Records this letter on `flags`. Exhaustive, no catch-all.
    fn apply(self, flags: &mut RegExpFlags) {
        match self {
            FlagLetter::HasIndices => flags.has_indices = true,
            FlagLetter::Global => flags.global = true,
            FlagLetter::IgnoreCase => flags.ignore_case = true,
            FlagLetter::Multiline => flags.multiline = true,
            FlagLetter::DotAll => flags.dot_all = true,
            FlagLetter::Unicode => flags.unicode_mode = RegExpUnicodeMode::Unicode,
            FlagLetter::UnicodeSets => flags.unicode_mode = RegExpUnicodeMode::UnicodeSets,
            FlagLetter::Sticky => flags.sticky = true,
        }
    }
}

fn parse_flags(flags: &str) -> Result<RegExpFlags, RegExpCompileError> {
    let mut parsed = RegExpFlags::default();
    let mut seen_flags = 0_u8;
    for (offset, byte) in flags.bytes().enumerate() {
        if !byte.is_ascii() {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::Flags,
                offset,
                "regular-expression flags must be ASCII",
            ));
        }

        let Some(letter) = FlagLetter::from_byte(byte) else {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::Flags,
                offset,
                format!("unknown regular-expression flag `{}`", byte as char),
            ));
        };
        if seen_flags & letter.bit() != 0 {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::Flags,
                offset,
                format!("duplicate regular-expression flag `{}`", byte as char),
            ));
        }
        if (letter == FlagLetter::Unicode && seen_flags & FlagLetter::UnicodeSets.bit() != 0)
            || (letter == FlagLetter::UnicodeSets && seen_flags & FlagLetter::Unicode.bit() != 0)
        {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::Flags,
                offset,
                "regular-expression flags `u` and `v` are mutually exclusive",
            ));
        }
        seen_flags |= letter.bit();
        letter.apply(&mut parsed);
    }

    Ok(parsed)
}

/// Applies scoped modifiers before an atom enters the matcher program.
fn apply_modifiers(
    instruction: &mut RegExpInstruction,
    modifiers: &Modifiers,
    unicode_mode: RegExpUnicodeMode,
    pool: &mut RegExpRangePool,
    offset: usize,
) -> Result<(), RegExpCompileError> {
    match instruction.opcode {
        REGEXP_OPCODE_DOT => {
            instruction.operand0 = modifiers.dot_all.operand_code();
            return Ok(());
        }
        REGEXP_OPCODE_ASSERT_START | REGEXP_OPCODE_ASSERT_END => {
            instruction.operand0 = modifiers.multiline.operand_code();
            return Ok(());
        }
        _ => {}
    }
    let folding = CaseFolding::from_flags(modifiers.ignore_case, unicode_mode);
    if folding == CaseFolding::Sensitive {
        return Ok(());
    }
    let (ranges, negated) = match instruction.opcode {
        REGEXP_OPCODE_LITERAL_ASCII | REGEXP_OPCODE_LITERAL_CODE_POINT => {
            let code_point = instruction.operand0 as u32;
            (vec![(code_point, code_point)], false)
        }
        REGEXP_OPCODE_POSITIVE_ASCII_CLASS | REGEXP_OPCODE_NEGATIVE_ASCII_CLASS => {
            let ranges = (0..128)
                .filter(|member| {
                    let word = if *member < 64 {
                        instruction.operand0
                    } else {
                        instruction.operand1
                    };
                    word & (1 << (member % 64)) != 0
                })
                .map(|member| (member, member))
                .collect();
            (
                ranges,
                instruction.opcode == REGEXP_OPCODE_NEGATIVE_ASCII_CLASS,
            )
        }
        _ => return Ok(()),
    };
    let ranges = normalize_ranges(ranges);
    let closed = case_close_ranges(&ranges, folding);
    if closed == ranges {
        return Ok(());
    }
    if closed.iter().all(|(_, end)| *end < 128) {
        let mut low = 0;
        let mut high = 0;
        for (start, end) in closed {
            add_ascii_range(&mut low, &mut high, start as u8, end as u8);
        }
        *instruction = if negated {
            RegExpInstruction::negative_ascii_class(low, high)
        } else {
            RegExpInstruction::positive_ascii_class(low, high)
        };
    } else {
        *instruction = finish_range_set(closed, negated, CaseFolding::Sensitive, pool, offset)?;
    }
    Ok(())
}

fn parse_escaped_atom(
    bytes: &[u8],
    offset: &mut usize,
    unicode_mode: RegExpUnicodeMode,
    modifiers: &Modifiers,
    pool: &mut RegExpRangePool,
) -> Result<RegExpInstruction, RegExpCompileError> {
    let escape_offset = *offset;
    let unicode = unicode_mode.is_unicode_mode();
    let Some(&escaped) = bytes.get(escape_offset + 1) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::CharacterEscape,
            escape_offset,
            "regular-expression escape is missing its escaped character",
        ));
    };
    if unicode && matches!(escaped, b'p' | b'P') {
        return parse_unicode_property_escape(bytes, offset, unicode_mode, modifiers, pool);
    }
    if escaped == b'u' {
        // DEFECT 3, found while picking a witness for `SyntaxRule::CodePointEscape`
        // and the same fingerprint as DEFECT 1: `RegExpUnicodeEscapeSequence`
        // has two alternatives in Unicode mode,
        // ``u` Hex4Digits`` and ``u{` CodePoint `}``, and only the class parser
        // implemented the second (`parse_class_atom`'s `b'u'` arm, just below).
        // So `/[\u{41}]/u` compiled while `/\u{41}/u` — and therefore every
        // astral pattern written the ordinary way, `/\u{1F600}/u` — was refused
        // as a SyntaxError. `parse_unicode_escape` sees the four bytes `{41}`,
        // finds them not all hex digits, and in Unicode mode there is no
        // fallback to the Annex B identity escape.
        if unicode && bytes.get(escape_offset + 2) == Some(&b'{') {
            let (code_point, end) = parse_braced_code_point_escape(bytes, escape_offset)?;
            *offset = end;
            return Ok(if code_point <= 0x7f {
                RegExpInstruction::literal_ascii(code_point as u8)
            } else {
                RegExpInstruction::literal_code_point(code_point)
            });
        }
        let (code_unit, consumed) = match parse_unicode_escape(bytes, escape_offset) {
            Ok(parsed) => parsed,
            Err(_) if !unicode => {
                *offset += 2;
                return Ok(RegExpInstruction::literal_ascii(b'u'));
            }
            Err(error) => return Err(error),
        };
        *offset = consumed;
        if unicode && (0xD800..=0xDBFF).contains(&code_unit) {
            if bytes.get(consumed..consumed + 2) == Some(b"\\u") {
                if let Ok((low, end)) = parse_unicode_escape(bytes, consumed) {
                    if (0xDC00..=0xDFFF).contains(&low) {
                        let scalar = 0x1_0000
                            + (((code_unit as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
                        *offset = end;
                        return Ok(RegExpInstruction::literal_code_point(scalar));
                    }
                }
            }
        }
        return Ok(if code_unit <= 0x7f {
            RegExpInstruction::literal_ascii(code_unit as u8)
        } else {
            RegExpInstruction::literal_code_point(code_unit as u32)
        });
    }
    if escaped == b'x' {
        let digits = bytes.get(escape_offset + 2..escape_offset + 4);
        if !digits.is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit)) {
            if !unicode {
                *offset += 2;
                return Ok(RegExpInstruction::literal_ascii(b'x'));
            }
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::HexEscapeSequence,
                escape_offset,
                "malformed hexadecimal escape",
            ));
        }
        let digits = digits.unwrap();
        let value = digits.iter().fold(0_u8, |value, digit| {
            (value << 4) | ascii_hex_value(*digit).unwrap() as u8
        });
        *offset += 4;
        return Ok(if value.is_ascii() {
            RegExpInstruction::literal_ascii(value)
        } else {
            RegExpInstruction::literal_code_point(u32::from(value))
        });
    }
    if escaped == b'c'
        && matches!(
            bytes.get(escape_offset + 2),
            Some(b'a'..=b'z') | Some(b'A'..=b'Z')
        )
    {
        let control = bytes[escape_offset + 2].to_ascii_uppercase() % 32;
        *offset += 3;
        return Ok(RegExpInstruction::literal_ascii(control));
    }
    if escaped == b'c' && !unicode {
        // Annex B's standalone backslash leaves an incomplete control marker
        // to become the next atom, including its own possible quantifier.
        *offset += 1;
        return Ok(RegExpInstruction::literal_ascii(b'\\'));
    }
    if matches!(escaped, b'n' | b'r' | b't' | b'v' | b'f') {
        let value = regexp_character_escape(escaped).expect("fixed character escape") as u8;
        *offset += 2;
        return Ok(RegExpInstruction::literal_ascii(value));
    }
    if !unicode && matches!(escaped, b'0'..=b'7') {
        let (value, end) = parse_legacy_octal_escape(bytes, escape_offset);
        *offset = end;
        return Ok(if value.is_ascii() {
            RegExpInstruction::literal_ascii(value)
        } else {
            RegExpInstruction::literal_code_point(u32::from(value))
        });
    }
    // `CharacterEscape[+UnicodeMode] :: 0 [lookahead ∉ DecimalDigit]` is NUL.
    // A trailing digit falls through to the identity-escape SyntaxError below
    // instead, mirroring the class path's `b'0'` arm next door.
    if unicode && escaped == b'0' && !matches!(bytes.get(escape_offset + 2), Some(b'0'..=b'9')) {
        *offset += 2;
        return Ok(RegExpInstruction::literal_ascii(0));
    }
    if matches!(escaped, b'd' | b'D') {
        let mut bitmap_low = 0;
        let mut bitmap_high = 0;
        add_ascii_range(&mut bitmap_low, &mut bitmap_high, b'0', b'9');
        *offset += 2;
        return Ok(if escaped == b'd' {
            RegExpInstruction::positive_ascii_class(bitmap_low, bitmap_high)
        } else {
            RegExpInstruction::negative_ascii_class(bitmap_low, bitmap_high)
        });
    }
    if matches!(escaped, b'w' | b'W') {
        let mut bitmap_low = 0;
        let mut bitmap_high = 0;
        add_ascii_range(&mut bitmap_low, &mut bitmap_high, b'A', b'Z');
        add_ascii_range(&mut bitmap_low, &mut bitmap_high, b'a', b'z');
        add_ascii_range(&mut bitmap_low, &mut bitmap_high, b'0', b'9');
        add_ascii_member(&mut bitmap_low, &mut bitmap_high, b'_');
        *offset += 2;
        return Ok(if escaped == b'w' {
            RegExpInstruction::positive_ascii_class(bitmap_low, bitmap_high)
        } else {
            RegExpInstruction::negative_ascii_class(bitmap_low, bitmap_high)
        });
    }
    if matches!(escaped, b's' | b'S') {
        *offset += 2;
        return Ok(if escaped == b's' {
            RegExpInstruction::whitespace()
        } else {
            RegExpInstruction::not_whitespace()
        });
    }
    // `IdentityEscape[+UnicodeMode] :: SyntaxCharacter | `/``. BOTH alternatives,
    // which is the whole of batch 8's DEFECT 1: this tested SyntaxCharacter
    // alone (the predicate then called `is_regex_metacharacter`), so `/\//u` and
    // `/https?:\/\//u` — ordinary patterns, not corner cases — were rejected as
    // SyntaxErrors. The class path next door had the rule right
    // (`is_class_identity_escape` has always carried `/`), so `/[\/]/u` compiled
    // while `/\//u` did not. That divergence is the fingerprint of one predicate
    // serving two productions.
    if !is_unicode_identity_escape(escaped) {
        if unicode {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::IdentityEscape,
                escape_offset,
                format!(
                    "invalid regular-expression identity escape `\\{}`",
                    escaped as char
                ),
            ));
        }
        *offset += 2;
        return Ok(RegExpInstruction::literal_ascii(escaped));
    }

    *offset += 2;
    Ok(RegExpInstruction::literal_ascii(escaped))
}

/// Parses `\p{…}` / `\P{…}` and yields the matching code-point ranges.
///
/// UnicodeSets closes the property before complementing it. Unicode mode
/// instead complements the property first and lets CharacterSetMatcher close
/// that result, so `\P` deliberately has different ignore-case behavior.
fn parse_unicode_property_ranges(
    bytes: &[u8],
    offset: &mut usize,
    mode: RegExpUnicodeMode,
    folding: CaseFolding,
) -> Result<Vec<(u32, u32)>, RegExpCompileError> {
    let escape_offset = *offset;
    let complement = bytes[escape_offset + 1] == b'P';
    if bytes.get(escape_offset + 2) != Some(&b'{') {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodePropertyEscape,
            escape_offset,
            "malformed Unicode property escape",
        ));
    }
    let value_start = escape_offset + 3;
    let Some(relative_end) = bytes[value_start..].iter().position(|byte| *byte == b'}') else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodePropertyEscape,
            escape_offset,
            "malformed Unicode property escape",
        ));
    };
    let value_end = value_start + relative_end;
    let value = std::str::from_utf8(&bytes[value_start..value_end])
        .ok()
        .filter(|value| value.is_ascii())
        .ok_or_else(|| {
            RegExpCompileError::invalid_syntax(
                SyntaxRule::UnicodePropertyName,
                escape_offset,
                "Unicode property names contain only ASCII property-name characters",
            )
        })?;
    if value.is_empty() {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodePropertyEscape,
            escape_offset,
            "malformed Unicode property escape",
        ));
    }

    let Some(ranges) = unicode_property_ranges(value) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodePropertyName,
            escape_offset,
            format!("unknown Unicode property escape `{value}`"),
        ));
    };
    *offset = value_end + 1;
    let ranges = normalize_ranges(ranges);
    let ranges = if mode == RegExpUnicodeMode::UnicodeSets {
        case_close_ranges(&ranges, folding)
    } else {
        ranges
    };
    Ok(if complement {
        complement_ranges(&ranges)
    } else {
        ranges
    })
}

fn parse_unicode_property_escape(
    bytes: &[u8],
    offset: &mut usize,
    mode: RegExpUnicodeMode,
    modifiers: &Modifiers,
    pool: &mut RegExpRangePool,
) -> Result<RegExpInstruction, RegExpCompileError> {
    let escape_offset = *offset;
    let folding = CaseFolding::from_flags(modifiers.ignore_case, mode);
    let ranges = parse_unicode_property_ranges(bytes, offset, mode, folding)?;
    finish_range_set(ranges, false, folding, pool, escape_offset)
}

/// One complete, exact `UnicodePropertyValueExpression` whose value contains
/// only code points. ICU supplies ranges only after this owner validates the
/// ECMAScript spelling; its broader binary-name lookup is not a grammar source.
/// Properties of strings remain owned by the separate UnicodeSets consumer.
#[must_use]
pub(crate) struct RegExpCodePointProperty<'a> {
    spelling: &'a str,
    kind: CodePointPropertyKind,
}

enum CodePointPropertyKind {
    Binary(UnicodePropertyBinary),
    GeneralCategory(GeneralCategoryGroup),
    Script {
        family: ScriptPropertyFamily,
        value: ScriptPropertyValue,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ScriptPropertyFamily {
    Script,
    Extensions,
}

enum ScriptPropertyValue {
    PinnedIcu(Script),
    Unicode17(crate::regexp_unicode17::Unicode17Script),
}

impl<'a> RegExpCodePointProperty<'a> {
    fn parse(spelling: &'a str) -> Option<Self> {
        let expression = spelling.split_once('=');
        let word = match expression {
            Some((_, value)) => value,
            None => spelling,
        };
        if word.is_empty()
            || !word
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return None;
        }
        let kind = match expression {
            Some(("General_Category" | "gc", value)) => CodePointPropertyKind::GeneralCategory(
                PropertyParser::<GeneralCategoryGroup>::new().get_strict(value)?,
            ),
            Some(("Script" | "sc", value)) => {
                Self::script_kind(value, ScriptPropertyFamily::Script)?
            }
            Some(("Script_Extensions" | "scx", value)) => {
                Self::script_kind(value, ScriptPropertyFamily::Extensions)?
            }
            Some(_) => return None,
            None => {
                if let Some(property) = unicode_property_binary_from_str(spelling) {
                    CodePointPropertyKind::Binary(property)
                } else {
                    CodePointPropertyKind::GeneralCategory(
                        PropertyParser::<GeneralCategoryGroup>::new().get_strict(spelling)?,
                    )
                }
            }
        };
        Some(Self { spelling, kind })
    }

    fn script_kind(value: &str, family: ScriptPropertyFamily) -> Option<CodePointPropertyKind> {
        let value = if let Some(script) = PropertyParser::<Script>::new().get_strict(value) {
            ScriptPropertyValue::PinnedIcu(script)
        } else {
            ScriptPropertyValue::Unicode17(crate::regexp_unicode17::Unicode17Script::from_alias(
                value,
            )?)
        };
        Some(CodePointPropertyKind::Script { family, value })
    }

    /// The delta's alias key was accepted by the complete grammar constructor.
    pub(crate) fn spelling(&self) -> &str {
        self.spelling
    }

    /// Only a validated new Script/Script_Extensions value can supply a
    /// missing ICU base. No other property may acquire ranges from delta rows.
    pub(crate) fn new_script_ranges(&self) -> Option<&'static [(u32, u32)]> {
        match &self.kind {
            CodePointPropertyKind::Script {
                value: ScriptPropertyValue::Unicode17(script),
                ..
            } => Some(script.ranges()),
            CodePointPropertyKind::Binary(_)
            | CodePointPropertyKind::GeneralCategory(_)
            | CodePointPropertyKind::Script {
                value: ScriptPropertyValue::PinnedIcu(_),
                ..
            } => None,
        }
    }

    fn icu_ranges(&self) -> Option<Vec<(u32, u32)>> {
        match &self.kind {
            CodePointPropertyKind::Binary(property) => Some(binary_property_ranges(*property)),
            CodePointPropertyKind::GeneralCategory(group) => Some(general_category_ranges(*group)),
            CodePointPropertyKind::Script { family, value } => match value {
                ScriptPropertyValue::PinnedIcu(script) => Some(script_ranges(*script, *family)),
                ScriptPropertyValue::Unicode17(_) => None,
            },
        }
    }

    fn into_ranges(self) -> Option<Vec<(u32, u32)>> {
        let base = self.icu_ranges();
        crate::regexp_unicode17::apply_unicode17_delta(self, base)
    }
}

/// Immutable property rows minted only from the same validated native authority
/// that supplies static RegExp ranges. The emitted compiler consumes the entire
/// catalog; a caller cannot construct a named row with unvalidated ranges.
pub struct RegExpUnicodePropertyCatalogEntry {
    name: String,
    value: UnicodePropertyCatalogValue,
}

enum UnicodePropertyCatalogValue {
    CodePoints(Vec<(u32, u32)>),
    Strings(Vec<Vec<u32>>),
}

pub enum RegExpUnicodePropertyCatalogValue<'a> {
    CodePoints(&'a [(u32, u32)]),
    /// Complete nonempty provider sequences, deduplicated and ordered by their
    /// exact code-point keys. Singleton sequences remain present in this view.
    Strings(&'a [Vec<u32>]),
}

impl RegExpUnicodePropertyCatalogEntry {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn value(&self) -> RegExpUnicodePropertyCatalogValue<'_> {
        match &self.value {
            UnicodePropertyCatalogValue::CodePoints(ranges) => {
                RegExpUnicodePropertyCatalogValue::CodePoints(ranges)
            }
            UnicodePropertyCatalogValue::Strings(sequences) => {
                RegExpUnicodePropertyCatalogValue::Strings(sequences)
            }
        }
    }
}

/// Complete exact aliases of every supported code-point family and all seven
/// string-property sequence sets. Native parsing and the immutable emitted
/// image borrow this same once-validated authority; no Pattern selects rows.
pub fn regexp_unicode_property_catalog() -> &'static [RegExpUnicodePropertyCatalogEntry] {
    static CATALOG: OnceLock<Vec<RegExpUnicodePropertyCatalogEntry>> = OnceLock::new();
    CATALOG.get_or_init(|| build_unicode_property_catalog(script_ranges))
}

fn build_unicode_property_catalog(
    mut script_provider: impl FnMut(Script, ScriptPropertyFamily) -> Vec<(u32, u32)>,
) -> Vec<RegExpUnicodePropertyCatalogEntry> {
    use icu_properties::props::ParseableEnumeratedProperty;
    let mut names = BTreeSet::new();
    for property in UnicodePropertyBinary::ALL {
        for alias in property.aliases() {
            names.insert((*alias).to_owned());
        }
    }
    for (alias, _) in GeneralCategoryGroup::SINGLETON.map.iter() {
        names.insert(alias.clone());
        for family in ["General_Category", "gc"] {
            names.insert(format!("{family}={alias}"));
        }
    }
    let mut scripts: BTreeSet<String> = Script::SINGLETON
        .map
        .iter()
        .map(|(alias, _)| alias)
        .collect();
    for script in crate::regexp_unicode17::Unicode17Script::ALL {
        scripts.extend(script.aliases().iter().map(|alias| (*alias).to_owned()));
    }
    for alias in scripts {
        for family in ["Script", "sc", "Script_Extensions", "scx"] {
            names.insert(format!("{family}={alias}"));
        }
    }
    let mut rows: BTreeMap<String, UnicodePropertyCatalogValue> = BTreeMap::new();
    let mut script_bases = BTreeMap::new();
    for name in names {
        let property = RegExpCodePointProperty::parse(&name)
            .expect("enumerated alias must pass the native ECMAScript property constructor");
        // ICU aliases repeat the same provider traversal. Reuse only that
        // base; the Unicode 17 delta still consumes each exact spelling and
        // its own Vec. This temporary map is dropped after construction.
        let base = match &property.kind {
            CodePointPropertyKind::Script {
                family,
                value: ScriptPropertyValue::PinnedIcu(script),
            } => Some(
                script_bases
                    .entry((*family, *script))
                    .or_insert_with(|| script_provider(*script, *family))
                    .clone(),
            ),
            _ => property.icu_ranges(),
        };
        let ranges = crate::regexp_unicode17::apply_unicode17_delta(property, base)
            .expect("validated property has pinned Unicode ranges");
        rows.insert(
            name,
            UnicodePropertyCatalogValue::CodePoints(normalize_ranges(ranges)),
        );
    }
    for &property in UnicodeStringProperty::ALL {
        rows.insert(
            property.name().to_owned(),
            UnicodePropertyCatalogValue::Strings(validated_unicode_string_property_sequences(
                property,
            )),
        );
    }
    rows.into_iter()
        .map(|(name, value)| RegExpUnicodePropertyCatalogEntry { name, value })
        .collect()
}

/// A property set has no empty sequence. Keep its complete singleton and
/// multi-code-point keys together until the actual operand partitions them.
/// Validation and canonicalization happen once, before minting a catalog row.
fn validated_unicode_string_property_sequences(property: UnicodeStringProperty) -> Vec<Vec<u32>> {
    unicode_string_property_sequences(property)
        .iter()
        .map(|sequence| {
            assert!(
                !sequence.is_empty(),
                "a Unicode string property has no empty key"
            );
            assert!(
                sequence.iter().all(|code_point| *code_point <= 0x10ffff),
                "a Unicode string property uses the complete code-point domain"
            );
            sequence.to_vec()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Resolves the validated property using the pinned ICU 2.0 (Unicode 16)
/// ranges and the committed Unicode 17 delta.
fn unicode_property_ranges(value: &str) -> Option<Vec<(u32, u32)>> {
    RegExpCodePointProperty::parse(value)?.into_ranges()
}

fn binary_property_ranges(property: UnicodePropertyBinary) -> Vec<(u32, u32)> {
    macro_rules! ranges {
        ($marker:ident) => {
            CodePointSetData::new::<icu_properties::props::$marker>()
                .iter_ranges()
                .map(|range| (*range.start(), *range.end()))
                .collect()
        };
    }
    use UnicodePropertyBinary::*;
    match property {
        Alphabetic => ranges!(Alphabetic),
        CaseIgnorable => ranges!(CaseIgnorable),
        Cased => ranges!(Cased),
        ChangesWhenCasefolded => ranges!(ChangesWhenCasefolded),
        ChangesWhenCasemapped => ranges!(ChangesWhenCasemapped),
        ChangesWhenLowercased => ranges!(ChangesWhenLowercased),
        ChangesWhenTitlecased => ranges!(ChangesWhenTitlecased),
        ChangesWhenUppercased => ranges!(ChangesWhenUppercased),
        DefaultIgnorableCodePoint => ranges!(DefaultIgnorableCodePoint),
        GraphemeBase => ranges!(GraphemeBase),
        GraphemeExtend => ranges!(GraphemeExtend),
        IDContinue => ranges!(IdContinue),
        IDStart => ranges!(IdStart),
        Math => ranges!(Math),
        XIDContinue => ranges!(XidContinue),
        XIDStart => ranges!(XidStart),
        ASCIIHexDigit => ranges!(AsciiHexDigit),
        BidiControl => ranges!(BidiControl),
        Dash => ranges!(Dash),
        Deprecated => ranges!(Deprecated),
        Diacritic => ranges!(Diacritic),
        Extender => ranges!(Extender),
        HexDigit => ranges!(HexDigit),
        IDSBinaryOperator => ranges!(IdsBinaryOperator),
        IDSTrinaryOperator => ranges!(IdsTrinaryOperator),
        Ideographic => ranges!(Ideographic),
        JoinControl => ranges!(JoinControl),
        LogicalOrderException => ranges!(LogicalOrderException),
        Lowercase => ranges!(Lowercase),
        NoncharacterCodePoint => ranges!(NoncharacterCodePoint),
        PatternSyntax => ranges!(PatternSyntax),
        PatternWhiteSpace => ranges!(PatternWhiteSpace),
        QuotationMark => ranges!(QuotationMark),
        Radical => ranges!(Radical),
        RegionalIndicator => ranges!(RegionalIndicator),
        SentenceTerminal => ranges!(SentenceTerminal),
        SoftDotted => ranges!(SoftDotted),
        TerminalPunctuation => ranges!(TerminalPunctuation),
        UnifiedIdeograph => ranges!(UnifiedIdeograph),
        Uppercase => ranges!(Uppercase),
        VariationSelector => ranges!(VariationSelector),
        WhiteSpace => ranges!(WhiteSpace),
        Emoji => ranges!(Emoji),
        EmojiComponent => ranges!(EmojiComponent),
        EmojiModifier => ranges!(EmojiModifier),
        EmojiModifierBase => ranges!(EmojiModifierBase),
        EmojiPresentation => ranges!(EmojiPresentation),
        ExtendedPictographic => ranges!(ExtendedPictographic),
        ChangesWhenNFKCCasefolded => ranges!(ChangesWhenNfkcCasefolded),
        BidiMirrored => ranges!(BidiMirrored),
        Ascii => vec![(0, 0x7f)],
        Any => vec![(0, 0x10ffff)],
        Assigned => {
            let unassigned = CodePointMapData::<GeneralCategory>::new()
                .iter_ranges_for_value(GeneralCategory::Unassigned)
                .map(|range| (*range.start(), *range.end()))
                .collect::<Vec<_>>();
            complement_ranges(&normalize_ranges(unassigned))
        }
    }
}

fn general_category_ranges(group: GeneralCategoryGroup) -> Vec<(u32, u32)> {
    CodePointMapData::<GeneralCategory>::new()
        .iter_ranges_for_group(group)
        .map(|range| (*range.start(), *range.end()))
        .collect()
}

fn script_ranges(script: Script, family: ScriptPropertyFamily) -> Vec<(u32, u32)> {
    match family {
        ScriptPropertyFamily::Script => CodePointMapData::<Script>::new()
            .iter_ranges_for_value(script)
            .map(|range| (*range.start(), *range.end()))
            .collect(),
        ScriptPropertyFamily::Extensions => ScriptWithExtensions::new()
            .get_script_extensions_ranges(script)
            .map(|range| (*range.start(), *range.end()))
            .collect(),
    }
}

/// ECMAScript Canonicalize domain shared by character sets and backreferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaseFolding {
    Sensitive,
    Legacy,
    Unicode,
}

impl CaseFolding {
    const fn backreference_operand(self) -> u64 {
        match self {
            Self::Sensitive => 0,
            Self::Legacy | Self::Unicode => REGEXP_BACKREFERENCE_IGNORE_CASE,
        }
    }

    /// Sorted nonidentity `(character, canonical_character)` mappings.
    /// Legacy characters are UTF-16 units; Unicode characters are code points.
    pub fn mappings(self) -> &'static [(u32, u32)] {
        static LEGACY: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
        static UNICODE: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
        match self {
            Self::Sensitive => &[],
            // Legacy canonicalization is identity outside the UTF-16 domain.
            Self::Legacy => LEGACY.get_or_init(|| {
                (0..=u16::MAX as u32)
                    .filter_map(|character| {
                        let canonical = self.canonicalize(character);
                        (canonical != character).then_some((character, canonical))
                    })
                    .collect()
            }),
            Self::Unicode => UNICODE.get_or_init(|| unicode_simple_case_fold_mappings().collect()),
        }
    }

    fn from_flags(ignore_case: bool, unicode_mode: RegExpUnicodeMode) -> Self {
        if !ignore_case {
            Self::Sensitive
        } else if unicode_mode.is_unicode_mode() {
            Self::Unicode
        } else {
            Self::Legacy
        }
    }

    fn canonicalize(self, code_point: u32) -> u32 {
        match self {
            Self::Sensitive => code_point,
            Self::Unicode => unicode_simple_case_fold(code_point),
            Self::Legacy => {
                // Legacy Canonicalize uppercases one UTF-16 code unit and
                // rejects expansions and non-ASCII to ASCII mappings.
                let Some(character) = char::from_u32(code_point).filter(|_| code_point <= 0xffff)
                else {
                    return code_point;
                };
                let mut uppercase = character.to_uppercase();
                let mapped = uppercase.next().expect("uppercase mapping is nonempty") as u32;
                if uppercase.next().is_some()
                    || mapped > 0xffff
                    || (code_point >= 128 && mapped < 128)
                {
                    code_point
                } else {
                    mapped
                }
            }
        }
    }
}

// The legacy mapping above uses Rust's generated Unicode data. A toolchain
// upgrade must review this alongside the bundled simple-fold table.
const _: () = assert!(char::UNICODE_VERSION.0 == 17 && char::UNICODE_VERSION.1 == 0);

fn case_fold_classes(folding: CaseFolding) -> &'static [Vec<u32>] {
    static LEGACY: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    static UNICODE: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let classes = match folding {
        CaseFolding::Sensitive => return &[],
        CaseFolding::Legacy => &LEGACY,
        CaseFolding::Unicode => &UNICODE,
    };
    classes.get_or_init(|| {
        let mut groups: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &(code_point, key) in folding.mappings() {
            groups
                .entry(key)
                .or_insert_with(|| vec![key])
                .push(code_point);
        }
        groups.into_values().collect()
    })
}

/// Closes `ranges` under simple case folding, matching the effect of
/// canonicalizing both the input and the set members.
fn case_close_ranges(ranges: &[(u32, u32)], folding: CaseFolding) -> Vec<(u32, u32)> {
    let mut extra = Vec::new();
    for members in case_fold_classes(folding) {
        if members
            .iter()
            .any(|code_point| ranges_contain(ranges, *code_point))
        {
            extra.extend(
                members
                    .iter()
                    .map(|code_point| (*code_point, *code_point))
                    .filter(|(code_point, _)| !ranges_contain(ranges, *code_point)),
            );
        }
    }
    if extra.is_empty() {
        return ranges.to_vec();
    }
    let mut closed = ranges.to_vec();
    closed.append(&mut extra);
    normalize_ranges(closed)
}

fn parse_unicode_escape(bytes: &[u8], start: usize) -> Result<(u16, usize), RegExpCompileError> {
    if bytes.get(start..start + 2) != Some(b"\\u") {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodeEscapeSequence,
            start,
            "malformed Unicode escape",
        ));
    }
    let digits = bytes.get(start + 2..start + 6).ok_or_else(|| {
        RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodeEscapeSequence,
            start,
            "malformed Unicode escape",
        )
    })?;
    if digits.len() != 4 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnicodeEscapeSequence,
            start,
            "malformed Unicode escape",
        ));
    }
    let value = digits.iter().fold(0_u16, |value, digit| {
        let digit = digit.to_ascii_lowercase();
        let digit = match digit {
            b'0'..=b'9' => digit - b'0',
            b'a'..=b'f' => digit - b'a' + 10,
            _ => unreachable!(),
        };
        (value << 4) | digit as u16
    });
    Ok((value, start + 6))
}

/// One member of a character class in the general code-point representation.
enum ClassAtom {
    CodePoint(u32),
    Ranges(Vec<(u32, u32)>),
}

impl ClassAtom {
    fn into_ranges(self) -> Vec<(u32, u32)> {
        match self {
            Self::CodePoint(code_point) => vec![(code_point, code_point)],
            Self::Ranges(ranges) => ranges,
        }
    }
}

/// Returns whether the class starting at `offset` holds anything the ASCII
/// bitmap representation cannot express.
fn class_needs_code_point_ranges(bytes: &[u8], offset: usize) -> bool {
    let mut cursor = offset + 1;
    while let Some(&byte) = bytes.get(cursor) {
        match byte {
            b']' => return false,
            b'\\' => {
                // Whitespace and complemented sets include non-ASCII members.
                // The range parser also owns the full word/property escapes.
                if matches!(
                    bytes.get(cursor + 1),
                    Some(b'p' | b'P' | b'u' | b'x' | b'w' | b'W' | b'D' | b's' | b'S')
                ) {
                    return true;
                }
                // Annex B octal escapes can name every byte, while the bitmap
                // has only 128 bits. Decode before selecting that representation.
                if matches!(bytes.get(cursor + 1), Some(b'0'..=b'7'))
                    && !parse_legacy_octal_escape(bytes, cursor).0.is_ascii()
                {
                    return true;
                }
                cursor += 2;
            }
            byte if !byte.is_ascii() => return true,
            _ => cursor += 1,
        }
    }
    false
}

fn parse_class(
    bytes: &[u8],
    offset: &mut usize,
    mode: OrdinaryClassMode,
    modifiers: &Modifiers,
    pool: &mut RegExpRangePool,
) -> Result<RegExpInstruction, RegExpCompileError> {
    let unicode = mode.is_unicode();
    if !class_needs_code_point_ranges(bytes, *offset) {
        return parse_ascii_class(bytes, offset, mode);
    }
    let class_offset = *offset;
    let mut cursor = class_offset + 1;
    let negated = bytes.get(cursor) == Some(&b'^');
    cursor += usize::from(negated);

    let mut ranges = Vec::new();
    let mut pending_trail = None;
    loop {
        let (range_offset, start) = if let Some((trail, source_offset)) = pending_trail.take() {
            (source_offset, ClassAtom::CodePoint(trail))
        } else {
            let Some(&member) = bytes.get(cursor) else {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnclosedCharacterClass,
                    class_offset,
                    "regular-expression character class is unclosed",
                ));
            };
            if member == b']' {
                break;
            }
            let range_offset = cursor;
            let start = parse_class_atom(
                bytes,
                &mut cursor,
                mode.unicode_mode(),
                mode.named_captures(),
                CaseFolding::from_flags(modifiers.ignore_case, mode.unicode_mode()),
            )?;
            (range_offset, start)
        };
        // In legacy grammar a raw astral character is two ClassAtoms. The
        // leading unit precedes any range starting at the trailing unit.
        let start = match start {
            ClassAtom::CodePoint(code_point) if !unicode => {
                if let Some(pair) =
                    char::from_u32(code_point).and_then(LegacyUtf16Pair::from_scalar)
                {
                    let [lead, trail] = pair.code_units();
                    ranges.push((lead, lead));
                    ClassAtom::CodePoint(trail)
                } else {
                    ClassAtom::CodePoint(code_point)
                }
            }
            atom => atom,
        };
        if bytes.get(cursor) == Some(&b'-') && bytes.get(cursor + 1) != Some(&b']') {
            cursor += 1;
            if bytes.get(cursor).is_none() {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnclosedCharacterClass,
                    class_offset,
                    "regular-expression character class is unclosed",
                ));
            }
            let end_offset = cursor;
            let end = parse_class_atom(
                bytes,
                &mut cursor,
                mode.unicode_mode(),
                mode.named_captures(),
                CaseFolding::from_flags(modifiers.ignore_case, mode.unicode_mode()),
            )?;
            let end = match end {
                ClassAtom::CodePoint(code_point) if !unicode => {
                    if let Some(pair) =
                        char::from_u32(code_point).and_then(LegacyUtf16Pair::from_scalar)
                    {
                        let [lead, trail] = pair.code_units();
                        // Only the leading unit ends this range; the trailing
                        // unit is the next grammar atom, even before `]`.
                        pending_trail = Some((trail, end_offset));
                        ClassAtom::CodePoint(lead)
                    } else {
                        ClassAtom::CodePoint(code_point)
                    }
                }
                atom => atom,
            };
            match (start, end) {
                (ClassAtom::CodePoint(start), ClassAtom::CodePoint(end)) if end < start => {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::ClassRangeOrder,
                        range_offset,
                        "regular-expression character class range is reversed",
                    ));
                }
                (ClassAtom::CodePoint(start), ClassAtom::CodePoint(end)) => {
                    ranges.push((start, end));
                }
                (start, end) if unicode => {
                    let _ = (start, end);
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::ClassRangeBound,
                        range_offset,
                        "regular-expression character class range bound is a class escape",
                    ));
                }
                (start, end) => {
                    ranges.extend(start.into_ranges());
                    ranges.extend(end.into_ranges());
                    ranges.push((u32::from(b'-'), u32::from(b'-')));
                }
            }
        } else {
            ranges.extend(start.into_ranges());
        }
    }

    *offset = cursor + 1;
    finish_range_set(
        ranges,
        negated,
        CaseFolding::from_flags(modifiers.ignore_case, mode.unicode_mode()),
        pool,
        class_offset,
    )
}

/// Normalizes, case-closes and interns `ranges`, returning a range-set atom.
fn finish_range_set(
    ranges: Vec<(u32, u32)>,
    negated: bool,
    folding: CaseFolding,
    pool: &mut RegExpRangePool,
    offset: usize,
) -> Result<RegExpInstruction, RegExpCompileError> {
    let ranges = case_close_ranges(&normalize_ranges(ranges), folding);
    let (first_entry, entry_count) = pool.intern(&ranges, offset)?;
    Ok(RegExpInstruction::code_point_range_set(
        first_entry,
        entry_count,
        negated,
    ))
}

fn parse_class_atom(
    bytes: &[u8],
    cursor: &mut usize,
    mode: RegExpUnicodeMode,
    named_captures: bool,
    folding: CaseFolding,
) -> Result<ClassAtom, RegExpCompileError> {
    let unicode = mode.is_unicode_mode();
    let offset = *cursor;
    let Some(&member) = bytes.get(offset) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnclosedCharacterClass,
            offset,
            "regular-expression character class is unclosed",
        ));
    };
    if member != b'\\' {
        let (code_point, end) = pattern_code_point_at(bytes, offset)?;
        *cursor = end;
        return Ok(ClassAtom::CodePoint(code_point));
    }

    let Some(&escaped) = bytes.get(offset + 1) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::CharacterEscape,
            offset,
            "regular-expression escape is missing its escaped character",
        ));
    };
    // NamedCaptureGroups is a complete-pattern census parameter, including
    // forward group syntax. Annex B excludes k from legacy IdentityEscape
    // inside classes just as it does outside them.
    if !unicode && named_captures && escaped == b'k' {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::ClassEscape,
            offset,
            "named-capture grammar does not admit class identity escape `\\k`",
        ));
    }
    match escaped {
        b'd' => {
            *cursor += 2;
            Ok(ClassAtom::Ranges(REGEXP_DIGIT_RANGES.to_vec()))
        }
        b'D' => {
            *cursor += 2;
            Ok(ClassAtom::Ranges(complement_ranges(REGEXP_DIGIT_RANGES)))
        }
        b's' => {
            *cursor += 2;
            Ok(ClassAtom::Ranges(REGEXP_WHITESPACE_RANGES.to_vec()))
        }
        b'S' => {
            *cursor += 2;
            Ok(ClassAtom::Ranges(complement_ranges(
                REGEXP_WHITESPACE_RANGES,
            )))
        }
        b'w' => {
            *cursor += 2;
            Ok(ClassAtom::Ranges(case_close_ranges(
                REGEXP_WORD_RANGES,
                folding,
            )))
        }
        b'W' => {
            *cursor += 2;
            Ok(ClassAtom::Ranges(complement_ranges(&case_close_ranges(
                REGEXP_WORD_RANGES,
                folding,
            ))))
        }
        b'p' | b'P' if unicode => {
            let ranges = parse_unicode_property_ranges(bytes, cursor, mode, folding)?;
            Ok(ClassAtom::Ranges(ranges))
        }
        b'b' => {
            *cursor += 2;
            Ok(ClassAtom::CodePoint(0x08))
        }
        b'n' | b'r' | b't' | b'v' | b'f' => {
            let value = regexp_character_escape(escaped).expect("fixed character escape") as u32;
            *cursor += 2;
            Ok(ClassAtom::CodePoint(value))
        }
        b'c' if matches!(bytes.get(offset + 2), Some(b'a'..=b'z') | Some(b'A'..=b'Z')) => {
            let control = u32::from(bytes[offset + 2].to_ascii_uppercase() % 32);
            *cursor += 3;
            Ok(ClassAtom::CodePoint(control))
        }
        b'c' if !unicode && matches!(bytes.get(offset + 2), Some(b'0'..=b'9') | Some(b'_')) => {
            let control = u32::from(bytes[offset + 2] % 32);
            *cursor += 3;
            Ok(ClassAtom::CodePoint(control))
        }
        // Annex B's standalone-backslash `ClassAtomNoDash` owns only the
        // backslash when `c` cannot complete either control escape. Leave `c`
        // for the next atom instead of turning the pair into one identity.
        b'c' if !unicode => {
            *cursor += 1;
            Ok(ClassAtom::CodePoint(u32::from(b'\\')))
        }
        b'x' => {
            let digits = bytes.get(offset + 2..offset + 4);
            if let Some(digits) = digits.filter(|digits| digits.iter().all(u8::is_ascii_hexdigit)) {
                let value = digits.iter().fold(0_u32, |value, digit| {
                    (value << 4) | ascii_hex_value(*digit).unwrap_or(0)
                });
                *cursor += 4;
                return Ok(ClassAtom::CodePoint(value));
            }
            if unicode {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::HexEscapeSequence,
                    offset,
                    "malformed hexadecimal escape",
                ));
            }
            *cursor += 2;
            Ok(ClassAtom::CodePoint(u32::from(b'x')))
        }
        b'u' => {
            if unicode && bytes.get(offset + 2) == Some(&b'{') {
                let (value, end) = parse_braced_code_point_escape(bytes, offset)?;
                *cursor = end;
                return Ok(ClassAtom::CodePoint(value));
            }
            match parse_unicode_escape(bytes, offset) {
                Ok((code_unit, end)) => {
                    *cursor = end;
                    if unicode && (0xd800..=0xdbff).contains(&code_unit) {
                        if let Ok((low, low_end)) = parse_unicode_escape(bytes, end) {
                            if (0xdc00..=0xdfff).contains(&low) {
                                *cursor = low_end;
                                return Ok(ClassAtom::CodePoint(
                                    0x1_0000
                                        + (((u32::from(code_unit) - 0xd800) << 10)
                                            | (u32::from(low) - 0xdc00)),
                                ));
                            }
                        }
                    }
                    Ok(ClassAtom::CodePoint(u32::from(code_unit)))
                }
                Err(error) if unicode => Err(error),
                Err(_) => {
                    *cursor += 2;
                    Ok(ClassAtom::CodePoint(u32::from(b'u')))
                }
            }
        }
        b'0'..=b'7' if !unicode => {
            let (value, end) = parse_legacy_octal_escape(bytes, offset);
            *cursor = end;
            Ok(ClassAtom::CodePoint(u32::from(value)))
        }
        b'0' if !matches!(bytes.get(offset + 2), Some(b'0'..=b'9')) => {
            *cursor += 2;
            Ok(ClassAtom::CodePoint(0))
        }
        // DEFECT 2 lived here, in the argument rather than in the predicate:
        // `parse_class_set` passed a literal `true` for what was then a
        // `unicode: bool` parameter, so every `v`-mode class atom was checked
        // against the `u`-mode `ClassEscape` rule and the thirteen additional
        // `ClassSetReservedPunctuator` escapes (`[\&]`, `[\!]`, `[\#]`, `[\%]`,
        // `[\,]`, `[\:]`, `[\;]`, `[\<]`, `[\=]`, `[\>]`, `[\@]`, ``[\`]``,
        // `[\~]`) were rejected as SyntaxErrors. The parameter is a
        // `RegExpUnicodeMode` now, so a literal `true` does not compile and the
        // third mode cannot silently reuse the second's rule again.
        escaped if unicode && !mode.allows_class_identity_escape(escaped) => {
            Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::ClassEscape,
                offset,
                "invalid regular-expression class escape",
            ))
        }
        _ => {
            let (code_point, end) = pattern_code_point_at(bytes, offset + 1)?;
            *cursor = end;
            Ok(ClassAtom::CodePoint(code_point))
        }
    }
}

/// The result of parsing one complete `v`-mode class. A class string remains a
/// typed parser atom until the complete Pattern has passed early errors.
enum UnicodeSetsClassAtom {
    Instruction(RegExpInstruction),
    FiniteClassSet(FiniteClassSetAtom),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FiniteClassSetAtom {
    multi_code_point_strings: Vec<Vec<RegExpInstruction>>,
    singleton: RegExpInstruction,
    contains_empty: bool,
}

impl FiniteClassSetAtom {
    fn new(
        value: FiniteClassSet,
        negated: bool,
        ignore_case: bool,
        pool: &mut RegExpRangePool,
        offset: usize,
    ) -> Result<Self, RegExpCompileError> {
        let FiniteClassSet { ranges, strings } = value;
        let folding = CaseFolding::from_flags(ignore_case, RegExpUnicodeMode::UnicodeSets);
        let ranges = case_close_ranges(&normalize_ranges(ranges), folding);
        let ranges = if negated {
            complement_ranges(&ranges)
        } else {
            ranges
        };
        let singleton = finish_range_set(ranges, false, CaseFolding::Sensitive, pool, offset)?;
        let contains_empty = strings.iter().any(Vec::is_empty);
        let modifiers = Modifiers {
            ignore_case,
            multiline: RegExpModifierOverride::Inherit,
            dot_all: RegExpModifierOverride::Inherit,
        };
        let mut multi_code_point_strings = strings
            .into_iter()
            .filter(|string| !string.is_empty())
            .map(|string| {
                string
                    .into_iter()
                    .map(|code_point| {
                        let mut instruction = RegExpInstruction::literal_code_point(code_point);
                        // Canonical string keys already own set algebra. Match the
                        // input's complete simple-fold equivalence class per position.
                        apply_modifiers(
                            &mut instruction,
                            &modifiers,
                            RegExpUnicodeMode::UnicodeSets,
                            pool,
                            offset,
                        )?;
                        Ok(instruction)
                    })
                    .collect::<Result<Vec<_>, RegExpCompileError>>()
            })
            .collect::<Result<Vec<_>, RegExpCompileError>>()?;
        multi_code_point_strings.sort_by_key(|string| std::cmp::Reverse(string.len()));
        Ok(Self {
            multi_code_point_strings,
            singleton,
            contains_empty,
        })
    }

    fn has_strings(&self) -> bool {
        self.contains_empty || !self.multi_code_point_strings.is_empty()
    }
}

/// Parses a `v`-mode `ClassSetExpression`, including nested classes, `--`
/// difference and `&&` intersection.
fn parse_unicode_sets_class(
    bytes: &[u8],
    offset: &mut usize,
    modifiers: &Modifiers,
    pool: &mut RegExpRangePool,
) -> Result<UnicodeSetsClassAtom, RegExpCompileError> {
    let class_offset = *offset;
    let mut cursor = class_offset;
    let folding = CaseFolding::from_flags(modifiers.ignore_case, RegExpUnicodeMode::UnicodeSets);
    let ParsedClassSet { value, negated } = parse_class_set(bytes, &mut cursor, folding)?;
    *offset = cursor;
    let atom = FiniteClassSetAtom::new(
        value.finite,
        negated,
        modifiers.ignore_case,
        pool,
        class_offset,
    )?;
    if atom.has_strings() {
        Ok(UnicodeSetsClassAtom::FiniteClassSet(atom))
    } else {
        Ok(UnicodeSetsClassAtom::Instruction(atom.singleton))
    }
}

/// The two recursive `ClassSetExpression` operations.
///
/// A string previously carried this state and every value other than `"--"`
/// silently meant intersection. Keeping the domain closed makes a new
/// operation define both its source token and its range semantics before the
/// parser compiles again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassSetOperator {
    Intersection,
    Subtraction,
}

impl ClassSetOperator {
    fn at(bytes: &[u8], cursor: usize) -> Option<Self> {
        match bytes.get(cursor..cursor + 2) {
            Some(b"&&") => Some(Self::Intersection),
            Some(b"--") => Some(Self::Subtraction),
            _ => None,
        }
    }

    const fn token(self) -> &'static str {
        match self {
            Self::Intersection => "&&",
            Self::Subtraction => "--",
        }
    }

    const fn rejects_operand_start(self, byte: u8) -> bool {
        match self {
            // `ClassIntersection :: ClassSetOperand && [lookahead != &]
            // ClassSetOperand`.
            Self::Intersection => byte == b'&',
            Self::Subtraction => false,
        }
    }

    fn apply(self, left: ClassSetValue, right: ClassSetValue) -> ClassSetValue {
        let may_contain_strings = match self {
            Self::Intersection => left.may_contain_strings && right.may_contain_strings,
            Self::Subtraction => left.may_contain_strings,
        };
        let finite = match self {
            Self::Intersection => left.finite.intersection(right.finite),
            Self::Subtraction => left.finite.subtraction(right.finite),
        };
        ClassSetValue {
            finite,
            may_contain_strings,
        }
    }
}

/// One validated `ClassSetCharacter`.
///
/// A bare `u32` cannot record that the UnicodeSets-only raw-character and
/// reserved-double-punctuator restrictions were checked. Keeping that proof in
/// a private type prevents the union and range parsers from accepting a code
/// point obtained through the more permissive ordinary-class parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClassSetCharacter(u32);

/// A fully parsed `ClassStringDisjunction`, including the exact
/// `MayContainStrings` result needed by negated-class early errors.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedClassStringDisjunction {
    alternatives: BTreeSet<Vec<u32>>,
    may_contain_strings: bool,
}

/// The only three cardinality classes relevant to `MayContainStrings` for one
/// `ClassString`: exactly one character is false; empty or multiple is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassStringLength {
    Empty,
    One,
    Multiple,
}

impl ClassStringLength {
    const fn push_character(self) -> Self {
        match self {
            Self::Empty => Self::One,
            Self::One | Self::Multiple => Self::Multiple,
        }
    }

    const fn may_contain_strings(self) -> bool {
        match self {
            Self::Empty | Self::Multiple => true,
            Self::One => false,
        }
    }
}

/// Matcher semantics retained while the complete enclosing class is parsed.
/// A class-string operand never becomes fake code-point ranges merely so the
/// remaining syntax can be checked.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FiniteClassSet {
    ranges: Vec<(u32, u32)>,
    strings: BTreeSet<Vec<u32>>,
}

impl FiniteClassSet {
    fn code_points(ranges: Vec<(u32, u32)>) -> Self {
        Self {
            ranges: normalize_ranges(ranges),
            strings: BTreeSet::new(),
        }
    }

    fn class_strings(alternatives: BTreeSet<Vec<u32>>, folding: CaseFolding) -> Self {
        let mut ranges = Vec::new();
        let mut strings = BTreeSet::new();
        for alternative in alternatives {
            // MaybeSimpleCaseFolding belongs to each operand, before algebra.
            // Simple folding preserves code-point length, including lone surrogates.
            let alternative = alternative
                .into_iter()
                .map(|code_point| folding.canonicalize(code_point))
                .collect::<Vec<_>>();
            match alternative.as_slice() {
                [code_point] => ranges.push((*code_point, *code_point)),
                [] | [_, _, ..] => {
                    strings.insert(alternative);
                }
            }
        }
        Self {
            // Expanded preimages keep singletons in the same fold-closed range
            // representation as ordinary characters and complemented operands.
            ranges: case_close_ranges(&normalize_ranges(ranges), folding),
            strings,
        }
    }

    fn union(self, right: Self) -> Self {
        let mut ranges = self.ranges;
        ranges.extend(right.ranges);
        let mut strings = self.strings;
        strings.extend(right.strings);
        Self {
            ranges: normalize_ranges(ranges),
            strings,
        }
    }

    fn intersection(self, right: Self) -> Self {
        Self {
            ranges: intersect_ranges(&self.ranges, &right.ranges),
            strings: self.strings.intersection(&right.strings).cloned().collect(),
        }
    }

    fn subtraction(self, right: Self) -> Self {
        Self {
            ranges: subtract_ranges(&self.ranges, &right.ranges),
            strings: self.strings.difference(&right.strings).cloned().collect(),
        }
    }

    fn complement(self) -> Self {
        debug_assert!(self.strings.is_empty());
        Self {
            ranges: complement_ranges(&self.ranges),
            strings: BTreeSet::new(),
        }
    }
}

struct ClassSetValue {
    finite: FiniteClassSet,
    may_contain_strings: bool,
}

impl ClassSetValue {
    fn code_points(ranges: Vec<(u32, u32)>, folding: CaseFolding) -> Self {
        Self {
            finite: FiniteClassSet::code_points(case_close_ranges(
                &normalize_ranges(ranges),
                folding,
            )),
            may_contain_strings: false,
        }
    }

    fn class_string(string: ValidatedClassStringDisjunction, folding: CaseFolding) -> Self {
        Self {
            finite: FiniteClassSet::class_strings(string.alternatives, folding),
            may_contain_strings: string.may_contain_strings,
        }
    }

    /// Property strings use the same operand-local folding as direct strings.
    fn finite_property_of_strings(strings: BTreeSet<Vec<u32>>, folding: CaseFolding) -> Self {
        Self {
            finite: FiniteClassSet::class_strings(strings, folding),
            may_contain_strings: true,
        }
    }

    fn union(self, right: Self) -> Self {
        Self {
            finite: self.finite.union(right.finite),
            may_contain_strings: self.may_contain_strings || right.may_contain_strings,
        }
    }
}

/// A syntactically complete bracketed class. Top-level lowering preserves its
/// negation flag; nested operands materialize the complement only when the
/// value is still code-point-only.
struct ParsedClassSet {
    value: ClassSetValue,
    negated: bool,
}

impl ParsedClassSet {
    fn into_nested_value(self) -> ClassSetValue {
        debug_assert!(!self.negated || !self.value.may_contain_strings);
        ClassSetValue {
            finite: if self.negated {
                self.value.finite.complement()
            } else {
                self.value.finite
            },
            may_contain_strings: self.value.may_contain_strings,
        }
    }
}

/// One `ClassSetOperand`, kept distinct from a `ClassSetRange`.
///
/// Only a validated `Character` may become either end of a range. A nested
/// class or a character-class escape is a complete set operand and cannot
/// accidentally acquire range syntax merely because both lower to the same
/// range vector.
enum ClassSetOperand {
    Character(ClassSetCharacter),
    NestedSet(ClassSetValue),
    ClassString(ValidatedClassStringDisjunction),
}

impl ClassSetOperand {
    fn into_value(self, folding: CaseFolding) -> ClassSetValue {
        match self {
            Self::Character(ClassSetCharacter(code_point)) => {
                ClassSetValue::code_points(vec![(code_point, code_point)], folding)
            }
            Self::NestedSet(value) => value,
            Self::ClassString(string) => ClassSetValue::class_string(string, folding),
        }
    }

    fn into_range_bound(self) -> Option<ClassSetCharacter> {
        match self {
            Self::Character(character) => Some(character),
            Self::NestedSet(_) | Self::ClassString(_) => None,
        }
    }
}

/// The two atomic operand forms handled by the shared escape decoder.
enum ClassSetAtomicOperand {
    Character(ClassSetCharacter),
    CharacterClassEscape(Vec<(u32, u32)>),
}

impl ClassSetAtomicOperand {
    fn into_operand(self, folding: CaseFolding) -> ClassSetOperand {
        match self {
            Self::Character(character) => ClassSetOperand::Character(character),
            Self::CharacterClassEscape(ranges) => {
                ClassSetOperand::NestedSet(ClassSetValue::code_points(ranges, folding))
            }
        }
    }
}

fn parse_class_set(
    bytes: &[u8],
    cursor: &mut usize,
    folding: CaseFolding,
) -> Result<ParsedClassSet, RegExpCompileError> {
    let class_offset = *cursor;
    debug_assert_eq!(bytes.get(class_offset), Some(&b'['));
    *cursor += 1;
    let negated = bytes.get(*cursor) == Some(&b'^');
    *cursor += usize::from(negated);

    match bytes.get(*cursor).copied() {
        None => {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::UnclosedCharacterClass,
                class_offset,
                "regular-expression character class is unclosed",
            ));
        }
        Some(b']') => {
            *cursor += 1;
            return Ok(ParsedClassSet {
                value: ClassSetValue::code_points(Vec::new(), folding),
                negated,
            });
        }
        Some(_) if ClassSetOperator::at(bytes, *cursor).is_some() => {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::ClassSetExpression,
                *cursor,
                "regular-expression class-set operation is missing its left operand",
            ));
        }
        Some(_) => {}
    }

    let first_offset = *cursor;
    let first = parse_unicode_sets_operand(bytes, cursor, class_offset, folding)?;
    let value = match ClassSetOperator::at(bytes, *cursor) {
        Some(operator) => {
            parse_class_set_operation_tail(bytes, cursor, class_offset, operator, first, folding)?
        }
        None => {
            parse_class_set_union_tail(bytes, cursor, class_offset, first, first_offset, folding)?
        }
    };
    if negated && value.may_contain_strings {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::NegatedClassMayContainStrings,
            class_offset,
            "negated UnicodeSets character class may not contain strings",
        ));
    }
    Ok(ParsedClassSet { value, negated })
}

/// Parses exactly one `ClassSetOperand` and validates its UnicodeSets-only
/// lexical restrictions before returning the typed operand.
fn parse_unicode_sets_operand(
    bytes: &[u8],
    cursor: &mut usize,
    class_offset: usize,
    folding: CaseFolding,
) -> Result<ClassSetOperand, RegExpCompileError> {
    match bytes.get(*cursor).copied() {
        None => Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnclosedCharacterClass,
            class_offset,
            "regular-expression character class is unclosed",
        )),
        Some(b']') => Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::ClassSetExpression,
            *cursor,
            "regular-expression class-set operation is missing an operand",
        )),
        Some(b'[') => {
            let nested = parse_class_set(bytes, cursor, folding)?;
            Ok(ClassSetOperand::NestedSet(nested.into_nested_value()))
        }
        Some(b'\\') if bytes.get(*cursor + 1) == Some(&b'p') => {
            if let Some(value) = parse_unicode_property_of_strings(bytes, cursor, folding)? {
                Ok(ClassSetOperand::NestedSet(value))
            } else {
                parse_unicode_sets_character_or_class_escape(bytes, cursor, folding)
                    .map(|operand| operand.into_operand(folding))
            }
        }
        Some(b'\\') if bytes.get(*cursor + 1) == Some(&b'q') => {
            validate_class_string_disjunction(bytes, cursor).map(ClassSetOperand::ClassString)
        }
        Some(_) => parse_unicode_sets_character_or_class_escape(bytes, cursor, folding)
            .map(|operand| operand.into_operand(folding)),
    }
}

fn parse_unicode_property_of_strings(
    bytes: &[u8],
    cursor: &mut usize,
    folding: CaseFolding,
) -> Result<Option<ClassSetValue>, RegExpCompileError> {
    let offset = *cursor;
    if bytes.get(offset..offset + 3) != Some(br"\p{") {
        return Ok(None);
    }
    let value_start = offset + 3;
    let Some(relative_end) = bytes[value_start..].iter().position(|byte| *byte == b'}') else {
        return Ok(None);
    };
    let value_end = value_start + relative_end;
    let value = std::str::from_utf8(&bytes[value_start..value_end])
        .ok()
        .filter(|value| value.is_ascii())
        .ok_or_else(|| {
            RegExpCompileError::invalid_syntax(
                SyntaxRule::UnicodePropertyName,
                offset,
                "Unicode property names contain only ASCII property-name characters",
            )
        })?;
    let Some(property) = unicode_string_property_from_str(value) else {
        return Ok(None);
    };
    let entry = regexp_unicode_property_catalog()
        .iter()
        .find(|entry| entry.name() == property.name())
        .expect("the complete catalog contains every closed string property");
    let RegExpUnicodePropertyCatalogValue::Strings(sequences) = entry.value() else {
        unreachable!("a closed string-property name cannot name a code-point row");
    };
    let strings = sequences.iter().cloned().collect();
    let value = ClassSetValue::finite_property_of_strings(strings, folding);
    *cursor = value_end + 1;
    Ok(Some(value))
}

/// Validates one `ClassStringDisjunction` without lowering its string
/// semantics. Only a fully closed, grammar-valid `\q{…}` reaches the explicit
/// class-string value carried through the complete enclosing expression.
fn validate_class_string_disjunction(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<ValidatedClassStringDisjunction, RegExpCompileError> {
    let offset = *cursor;
    if bytes.get(offset..offset + 3) != Some(br"\q{") {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::ClassStringDisjunction,
            offset,
            "regular-expression class-string disjunction must begin with `\\q{`",
        ));
    }
    *cursor += 3;
    let mut alternatives = BTreeSet::new();
    let mut string = Vec::new();
    let mut string_length = ClassStringLength::Empty;
    let mut may_contain_strings = false;

    loop {
        match bytes.get(*cursor).copied() {
            None | Some(b']') => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassStringDisjunction,
                    offset,
                    "regular-expression class-string disjunction is unclosed",
                ));
            }
            Some(b'}') => {
                may_contain_strings |= string_length.may_contain_strings();
                alternatives.insert(std::mem::take(&mut string));
                *cursor += 1;
                return Ok(ValidatedClassStringDisjunction {
                    alternatives,
                    may_contain_strings,
                });
            }
            // `ClassString` may be empty, so leading, trailing and adjacent
            // disjunction delimiters are all grammatical.
            Some(b'|') => {
                may_contain_strings |= string_length.may_contain_strings();
                alternatives.insert(std::mem::take(&mut string));
                string_length = ClassStringLength::Empty;
                *cursor += 1;
            }
            Some(_) => {
                let ClassSetCharacter(code_point) =
                    parse_unicode_sets_class_set_character(bytes, cursor)?;
                string.push(code_point);
                string_length = string_length.push_character();
            }
        }
    }
}

/// Parses one `ClassSetCharacter`, excluding raw `ClassSetSyntaxCharacter`
/// and every `ClassSetReservedDoublePunctuator` before delegating escape and
/// code-point decoding to the shared class-atom parser.
fn parse_unicode_sets_class_set_character(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<ClassSetCharacter, RegExpCompileError> {
    let offset = *cursor;
    match parse_unicode_sets_character_or_class_escape(bytes, cursor, CaseFolding::Sensitive)? {
        ClassSetAtomicOperand::Character(character) => Ok(character),
        ClassSetAtomicOperand::CharacterClassEscape(_) => Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::ClassSetCharacter,
            offset,
            "character-class escape is not a ClassSetCharacter",
        )),
    }
}

/// Parses the atomic alternatives shared by a `ClassSetOperand` and a
/// `ClassSetCharacter`. Raw characters are validated before the shared
/// ordinary-class decoder can turn them into an unqualified code point.
fn parse_unicode_sets_character_or_class_escape(
    bytes: &[u8],
    cursor: &mut usize,
    folding: CaseFolding,
) -> Result<ClassSetAtomicOperand, RegExpCompileError> {
    let offset = *cursor;
    let Some(&member) = bytes.get(offset) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnclosedCharacterClass,
            offset,
            "regular-expression character class is unclosed",
        ));
    };

    if member == b'\\'
        && bytes.get(offset + 1) == Some(&b'0')
        && matches!(bytes.get(offset + 2), Some(b'0'..=b'9'))
    {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::ClassSetCharacter,
            offset,
            "`\\0` character escape cannot be followed by a decimal digit",
        ));
    }

    if member != b'\\' {
        if is_class_set_reserved_double_punctuator(bytes, offset) {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::ClassSetCharacter,
                offset,
                "reserved double punctuator is not a UnicodeSets class character",
            ));
        }
        if is_class_set_syntax_character(member) {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::ClassSetCharacter,
                offset,
                "UnicodeSets syntax character must be escaped in a class operand",
            ));
        }
    }

    match parse_class_atom(
        bytes,
        cursor,
        RegExpUnicodeMode::UnicodeSets,
        false,
        folding,
    )? {
        ClassAtom::CodePoint(code_point) => Ok(ClassSetAtomicOperand::Character(
            ClassSetCharacter(code_point),
        )),
        ClassAtom::Ranges(ranges) => Ok(ClassSetAtomicOperand::CharacterClassEscape(ranges)),
    }
}

/// Parses the remainder of a `ClassUnion` after its first operand.
fn parse_class_set_union_tail(
    bytes: &[u8],
    cursor: &mut usize,
    class_offset: usize,
    mut operand: ClassSetOperand,
    mut operand_offset: usize,
    folding: CaseFolding,
) -> Result<ClassSetValue, RegExpCompileError> {
    let mut union = ClassSetValue::code_points(Vec::new(), folding);
    loop {
        if bytes.get(*cursor) == Some(&b'-')
            && bytes.get(*cursor + 1) != Some(&b']')
            && bytes.get(*cursor + 1) != Some(&b'-')
        {
            *cursor += 1;
            let end = parse_unicode_sets_operand(bytes, cursor, class_offset, folding)?;
            let start = operand.into_range_bound();
            let end = end.into_range_bound();
            let (Some(ClassSetCharacter(start)), Some(ClassSetCharacter(end))) = (start, end)
            else {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassRangeBound,
                    operand_offset,
                    "regular-expression UnicodeSets range bound is not a ClassSetCharacter",
                ));
            };
            if end < start {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassRangeOrder,
                    operand_offset,
                    "regular-expression character class range is reversed",
                ));
            }
            union = union.union(ClassSetValue::code_points(vec![(start, end)], folding));
        } else {
            union = union.union(operand.into_value(folding));
        }

        match bytes.get(*cursor).copied() {
            None => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnclosedCharacterClass,
                    class_offset,
                    "regular-expression character class is unclosed",
                ));
            }
            Some(b']') => {
                *cursor += 1;
                return Ok(union);
            }
            Some(_) if ClassSetOperator::at(bytes, *cursor).is_some() => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassSetExpression,
                    *cursor,
                    "regular-expression class union cannot be an operation operand",
                ));
            }
            Some(_) => {
                operand_offset = *cursor;
                operand = parse_unicode_sets_operand(bytes, cursor, class_offset, folding)?;
            }
        }
    }
}

/// Parses a homogeneous `ClassIntersection` or `ClassSubtraction` tail.
fn parse_class_set_operation_tail(
    bytes: &[u8],
    cursor: &mut usize,
    class_offset: usize,
    operator: ClassSetOperator,
    first: ClassSetOperand,
    folding: CaseFolding,
) -> Result<ClassSetValue, RegExpCompileError> {
    let mut value = first.into_value(folding);
    loop {
        let operator_offset = *cursor;
        debug_assert_eq!(ClassSetOperator::at(bytes, operator_offset), Some(operator));
        *cursor += 2;

        match bytes.get(*cursor).copied() {
            None => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnclosedCharacterClass,
                    class_offset,
                    "regular-expression character class is unclosed",
                ));
            }
            Some(b']') => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassSetExpression,
                    operator_offset,
                    format!(
                        "regular-expression class-set `{}` is missing its right operand",
                        operator.token()
                    ),
                ));
            }
            Some(byte) if operator.rejects_operand_start(byte) => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassSetExpression,
                    *cursor,
                    format!(
                        "regular-expression class-set `{}` cannot be followed by `{}`",
                        operator.token(),
                        char::from(byte)
                    ),
                ));
            }
            Some(_) if ClassSetOperator::at(bytes, *cursor).is_some() => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassSetExpression,
                    operator_offset,
                    format!(
                        "regular-expression class-set `{}` is missing its right operand",
                        operator.token()
                    ),
                ));
            }
            Some(_) => {}
        }

        let right = parse_unicode_sets_operand(bytes, cursor, class_offset, folding)?;
        value = operator.apply(value, right.into_value(folding));

        match bytes.get(*cursor).copied() {
            None => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnclosedCharacterClass,
                    class_offset,
                    "regular-expression character class is unclosed",
                ));
            }
            Some(b']') => {
                *cursor += 1;
                return Ok(value);
            }
            Some(_) if ClassSetOperator::at(bytes, *cursor) == Some(operator) => {}
            Some(_) => {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::ClassSetExpression,
                    *cursor,
                    format!(
                        "regular-expression class-set `{}` operands must be separated by the same operator",
                        operator.token()
                    ),
                ));
            }
        }
    }
}

/// `ClassEscape[+UnicodeMode]`'s identity-escape set: `SyntaxCharacter`, plus
/// `/` from `IdentityEscape`, plus `-` from `ClassEscape` itself.
///
/// This predicate has always been right, which is the useful part of the
/// evidence: `/[\/]/u` compiled while `/\//u` did not, and that divergence is
/// what identified DEFECT 1 as a missing alternative rather than a design
/// choice.
fn is_class_identity_escape(escaped: u8) -> bool {
    // Delegates rather than re-spelling `SyntaxCharacter | `/``. Written out
    // independently, `IdentityEscape[+UnicodeMode]` lived in two predicates and
    // the next correction to that production would have had to be made twice —
    // getting it right in only one of them is exactly the atom/class divergence
    // that made DEFECT 1 detectable. As delegation, this reads as the spec does:
    // the `u`-mode identity escape, plus `-` from `ClassEscape` itself.
    is_unicode_identity_escape(escaped) || escaped == b'-'
}

/// ``ClassSetReservedPunctuator :: one of & - ! # % , : ; < = > @ ` ~`` (22.2.1).
///
/// Fourteen characters. `-` is already a `u`-mode `ClassEscape` alternative, so
/// thirteen of them are new in `v` mode — and HEAD rejected all thirteen.
fn is_class_set_reserved_punctuator(byte: u8) -> bool {
    matches!(
        byte,
        b'&' | b'-'
            | b'!'
            | b'#'
            | b'%'
            | b','
            | b':'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'@'
            | b'`'
            | b'~'
    )
}

/// `ClassSetSyntaxCharacter :: one of ( ) [ ] { } / - \\ |` (22.2.1).
const fn is_class_set_syntax_character(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'/' | b'-' | b'\\' | b'|'
    )
}

/// The nineteen doubled tokens in `ClassSetReservedDoublePunctuator`.
fn is_class_set_reserved_double_punctuator(bytes: &[u8], cursor: usize) -> bool {
    let Some(&punctuator) = bytes.get(cursor) else {
        return false;
    };
    bytes.get(cursor + 1) == Some(&punctuator)
        && matches!(
            punctuator,
            b'&' | b'!'
                | b'#'
                | b'$'
                | b'%'
                | b'*'
                | b'+'
                | b','
                | b'.'
                | b':'
                | b';'
                | b'<'
                | b'='
                | b'>'
                | b'?'
                | b'@'
                | b'^'
                | b'`'
                | b'~'
        )
}

fn parse_braced_code_point_escape(
    bytes: &[u8],
    escape_offset: usize,
) -> Result<(u32, usize), RegExpCompileError> {
    let mut cursor = escape_offset + 3;
    let mut value = 0_u32;
    let mut digits = 0;
    while let Some(&byte) = bytes.get(cursor) {
        if byte == b'}' {
            break;
        }
        let Some(digit) = ascii_hex_value(byte) else {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::CodePointEscape,
                escape_offset,
                "malformed Unicode code-point escape",
            ));
        };
        value = value
            .checked_mul(16)
            .and_then(|value| value.checked_add(digit))
            .filter(|value| *value <= 0x10ffff)
            .ok_or_else(|| {
                RegExpCompileError::invalid_syntax(
                    SyntaxRule::CodePointEscapeRange,
                    escape_offset,
                    "Unicode code-point escape is out of range",
                )
            })?;
        digits += 1;
        cursor += 1;
    }
    if digits == 0 || bytes.get(cursor) != Some(&b'}') {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::CodePointEscape,
            escape_offset,
            "malformed Unicode code-point escape",
        ));
    }
    Ok((value, cursor + 1))
}

#[derive(Clone, Copy)]
struct AsciiClassAtom {
    bitmap_low: u64,
    bitmap_high: u64,
    singleton: Option<u8>,
}

fn parse_ascii_class(
    bytes: &[u8],
    offset: &mut usize,
    mode: OrdinaryClassMode,
) -> Result<RegExpInstruction, RegExpCompileError> {
    let class_offset = *offset;
    let mut cursor = class_offset + 1;
    let Some(&first) = bytes.get(cursor) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnclosedCharacterClass,
            class_offset,
            "regular-expression character class is unclosed",
        ));
    };
    let negated = first == b'^';
    cursor += usize::from(negated);

    let mut bitmap_low = 0;
    let mut bitmap_high = 0;
    loop {
        let Some(&member) = bytes.get(cursor) else {
            return Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::UnclosedCharacterClass,
                class_offset,
                "regular-expression character class is unclosed",
            ));
        };
        if member == b']' {
            break;
        }
        let range_start = parse_ascii_class_atom(bytes, &mut cursor, mode)?;

        if bytes.get(cursor) == Some(&b'-') && bytes.get(cursor + 1) != Some(&b']') {
            let range_offset = cursor;
            cursor += 1;
            if bytes.get(cursor).is_none() {
                return Err(RegExpCompileError::invalid_syntax(
                    SyntaxRule::UnclosedCharacterClass,
                    class_offset,
                    "regular-expression character class is unclosed",
                ));
            }
            let range_end = parse_ascii_class_atom(bytes, &mut cursor, mode)?;
            match (range_start.singleton, range_end.singleton) {
                (Some(start), Some(end)) if end < start => {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::ClassRangeOrder,
                        range_offset,
                        "regular-expression character class range is reversed",
                    ));
                }
                (Some(start), Some(end)) => {
                    add_ascii_range(&mut bitmap_low, &mut bitmap_high, start, end);
                }
                _ if mode.is_unicode() => {
                    return Err(RegExpCompileError::invalid_syntax(
                        SyntaxRule::ClassRangeBound,
                        range_offset,
                        "regular-expression character class range bound is a class escape",
                    ));
                }
                _ => {
                    bitmap_low |= range_start.bitmap_low | range_end.bitmap_low;
                    bitmap_high |= range_start.bitmap_high | range_end.bitmap_high;
                    add_ascii_member(&mut bitmap_low, &mut bitmap_high, b'-');
                }
            }
        } else {
            bitmap_low |= range_start.bitmap_low;
            bitmap_high |= range_start.bitmap_high;
        }
    }

    *offset = cursor + 1;
    Ok(if negated {
        RegExpInstruction::negative_ascii_class(bitmap_low, bitmap_high)
    } else {
        RegExpInstruction::positive_ascii_class(bitmap_low, bitmap_high)
    })
}

fn parse_single_unicode_class(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Option<RegExpInstruction>, RegExpCompileError> {
    let class_offset = *offset;
    let Some(relative_end) = bytes[class_offset + 1..]
        .iter()
        .position(|byte| *byte == b']')
    else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnclosedCharacterClass,
            class_offset,
            "regular-expression character class is unclosed",
        ));
    };
    let end = class_offset + 1 + relative_end;
    if class_offset + 1 == end {
        return Ok(None);
    }
    let (code_point, character_end) = pattern_code_point_at(bytes, class_offset + 1)?;
    if code_point < 128 || character_end != end {
        return Ok(None);
    }
    *offset = end + 1;
    Ok(Some(RegExpInstruction::literal_code_point(code_point)))
}

fn parse_ascii_class_atom(
    bytes: &[u8],
    cursor: &mut usize,
    mode: OrdinaryClassMode,
) -> Result<AsciiClassAtom, RegExpCompileError> {
    let offset = *cursor;
    let Some(&member) = bytes.get(offset) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::UnclosedCharacterClass,
            offset,
            "regular-expression character class is unclosed",
        ));
    };
    if member != b'\\' {
        if !member.is_ascii() {
            return Err(RegExpCompileError::unsupported_feature(
                offset,
                "non-ASCII regular-expression source is unsupported by this matcher-program grammar",
            ));
        }
        *cursor += 1;
        return Ok(singleton_ascii_class_atom(member));
    }

    let Some(&escaped) = bytes.get(offset + 1) else {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::CharacterEscape,
            offset,
            "regular-expression escape is missing its escaped character",
        ));
    };
    match escaped {
        b'd' => {
            *cursor += 2;
            let mut atom = AsciiClassAtom {
                bitmap_low: 0,
                bitmap_high: 0,
                singleton: None,
            };
            add_ascii_range(&mut atom.bitmap_low, &mut atom.bitmap_high, b'0', b'9');
            Ok(atom)
        }
        b's' => {
            *cursor += 2;
            let mut atom = AsciiClassAtom {
                bitmap_low: 0,
                bitmap_high: 0,
                singleton: None,
            };
            add_ascii_range(&mut atom.bitmap_low, &mut atom.bitmap_high, 0x09, 0x0d);
            add_ascii_member(&mut atom.bitmap_low, &mut atom.bitmap_high, 0x20);
            Ok(atom)
        }
        b'c' if matches!(bytes.get(offset + 2), Some(b'a'..=b'z') | Some(b'A'..=b'Z')) => {
            let control = bytes[offset + 2].to_ascii_uppercase() % 32;
            *cursor += 3;
            Ok(singleton_ascii_class_atom(control))
        }
        b'c' if !mode.is_unicode()
            && matches!(bytes.get(offset + 2), Some(b'0'..=b'9') | Some(b'_')) =>
        {
            let control = bytes[offset + 2] % 32;
            *cursor += 3;
            Ok(singleton_ascii_class_atom(control))
        }
        // Annex B's standalone-backslash `ClassAtomNoDash` consumes only the
        // backslash here. The loop parses `c` separately.
        b'c' if !mode.is_unicode() => {
            *cursor += 1;
            Ok(singleton_ascii_class_atom(b'\\'))
        }
        b'0'..=b'7' if !mode.is_unicode() => {
            let (value, end) = parse_legacy_octal_escape(bytes, offset);
            *cursor = end;
            Ok(singleton_ascii_class_atom(value))
        }
        b'0' if !matches!(bytes.get(offset + 2), Some(b'0'..=b'9')) => {
            *cursor += 2;
            Ok(singleton_ascii_class_atom(0))
        }
        b'b' => {
            *cursor += 2;
            Ok(singleton_ascii_class_atom(0x08))
        }
        b'n' | b'r' | b't' | b'v' | b'f' => {
            let value = regexp_character_escape(escaped).expect("fixed character escape") as u8;
            *cursor += 2;
            Ok(singleton_ascii_class_atom(value))
        }
        escaped if !mode.allows_class_identity_escape(escaped) => {
            Err(RegExpCompileError::invalid_syntax(
                SyntaxRule::ClassEscape,
                offset,
                "invalid regular-expression class escape",
            ))
        }
        _ => {
            *cursor += 2;
            Ok(singleton_ascii_class_atom(escaped))
        }
    }
}

fn singleton_ascii_class_atom(member: u8) -> AsciiClassAtom {
    let mut atom = AsciiClassAtom {
        bitmap_low: 0,
        bitmap_high: 0,
        singleton: Some(member),
    };
    add_ascii_member(&mut atom.bitmap_low, &mut atom.bitmap_high, member);
    atom
}

fn parse_legacy_octal_escape(bytes: &[u8], escape_offset: usize) -> (u8, usize) {
    let first = bytes[escape_offset + 1];
    let max_digits = if first <= REGEXP_LEGACY_THREE_DIGIT_OCTAL_LAST {
        3
    } else {
        2
    };
    let mut value = 0_u8;
    let mut cursor = escape_offset + 1;
    for _ in 0..max_digits {
        let Some(digit @ b'0'..=b'7') = bytes.get(cursor).copied() else {
            break;
        };
        value = value * 8 + (digit - b'0');
        cursor += 1;
    }
    (value, cursor)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrivialQuantifierOptional {
    None,
    Once,
    Unbounded,
}

struct TrivialQuantifier {
    required: bool,
    optional: TrivialQuantifierOptional,
}

enum QuantifierBody<'a> {
    Counted(&'a RegExpRepeatBounds),
    Trivial(TrivialQuantifier),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuantifierPreference {
    Greedy,
    Lazy,
}

impl QuantifierPreference {
    const fn word(self) -> u64 {
        match self {
            Self::Greedy => 0,
            Self::Lazy => 1,
        }
    }
}

/// A must-advance atom cannot satisfy this minimum on addressable inputs.
const HUGE_QUANTIFIER_BOUND: u64 = 1 << 32;

#[derive(Clone)]
struct Quantifier {
    bounds: RegExpRepeatBounds,
    preference: QuantifierPreference,
}

impl Quantifier {
    fn new(minimum: u64, maximum: Option<u64>, lazy: bool) -> Self {
        Self {
            bounds: RegExpRepeatBounds::new(
                RegExpNatural::from_u64(minimum),
                maximum.map_or(RegExpRepeatMaximum::Unbounded, |value| {
                    RegExpRepeatMaximum::Finite(RegExpNatural::from_u64(value))
                }),
            )
            .expect("ordered built-in quantifier bounds"),
            preference: if lazy {
                QuantifierPreference::Lazy
            } else {
                QuantifierPreference::Greedy
            },
        }
    }

    fn body(&self, nullable: bool) -> QuantifierBody<'_> {
        let trivial = match (
            self.bounds.minimum().checked_to_u64(),
            self.bounds.maximum(),
        ) {
            (Some(0), RegExpRepeatMaximum::Finite(maximum)) if maximum.is_zero() => {
                TrivialQuantifier {
                    required: false,
                    optional: TrivialQuantifierOptional::None,
                }
            }
            (Some(0), RegExpRepeatMaximum::Finite(maximum)) if maximum.is_one() => {
                TrivialQuantifier {
                    required: false,
                    optional: TrivialQuantifierOptional::Once,
                }
            }
            (Some(0), RegExpRepeatMaximum::Unbounded) => TrivialQuantifier {
                required: false,
                optional: TrivialQuantifierOptional::Unbounded,
            },
            (Some(1), RegExpRepeatMaximum::Finite(maximum)) if maximum.is_one() => {
                TrivialQuantifier {
                    required: true,
                    optional: TrivialQuantifierOptional::None,
                }
            }
            (Some(1), RegExpRepeatMaximum::Unbounded) if !nullable => TrivialQuantifier {
                required: true,
                optional: TrivialQuantifierOptional::Unbounded,
            },
            _ => return QuantifierBody::Counted(&self.bounds),
        };
        QuantifierBody::Trivial(trivial)
    }

    fn is_optional(&self) -> bool {
        self.bounds.minimum().is_zero()
    }
    fn has_impossible_consuming_minimum(&self) -> bool {
        self.bounds.minimum() >= &RegExpNatural::from_u64(HUGE_QUANTIFIER_BOUND)
    }
}

fn parse_postfix_quantifier(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Quantifier, RegExpCompileError> {
    let Some(&byte) = bytes.get(*offset) else {
        return Ok(Quantifier::new(1, Some(1), false));
    };
    let start = *offset;
    let mut quantifier = match byte {
        b'?' => {
            *offset += 1;
            Quantifier::new(0, Some(1), false)
        }
        b'*' => {
            *offset += 1;
            Quantifier::new(0, None, false)
        }
        b'+' => {
            *offset += 1;
            Quantifier::new(1, None, false)
        }
        b'{' => match parse_braced_quantifier(bytes, offset)? {
            Some(quantifier) => quantifier,
            None => {
                return Ok(Quantifier::new(1, Some(1), false));
            }
        },
        _ => {
            return Ok(Quantifier::new(1, Some(1), false));
        }
    };
    quantifier.preference = if bytes.get(*offset) == Some(&b'?') {
        *offset += 1;
        QuantifierPreference::Lazy
    } else {
        QuantifierPreference::Greedy
    };
    let repeated_brace = if bytes.get(*offset) == Some(&b'{') {
        let mut probe = *offset;
        match parse_braced_quantifier(bytes, &mut probe)? {
            Some(_) => true,
            None => false,
        }
    } else {
        false
    };
    if matches!(bytes.get(*offset), Some(b'?' | b'*' | b'+')) || repeated_brace {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::QuantifierAfterQuantifier,
            *offset,
            "regular-expression quantifier follows another quantifier",
        ));
    }
    debug_assert!(start < *offset);
    Ok(quantifier)
}

fn parse_braced_quantifier(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Option<Quantifier>, RegExpCompileError> {
    let start = *offset;
    let mut cursor = start + 1;
    let min_start = cursor;
    let min = parse_decimal_natural(bytes, &mut cursor);
    if cursor == start + 1 {
        return Ok(None);
    }
    let min = min.expect("nonempty decimal span");
    let min_end = cursor;
    let mut max_start = min_start;
    let mut max_end = min_end;
    let max = match bytes.get(cursor) {
        Some(b'}') => {
            cursor += 1;
            Some(min.clone())
        }
        Some(b',') => {
            cursor += 1;
            if bytes.get(cursor) == Some(&b'}') {
                cursor += 1;
                None
            } else {
                max_start = cursor;
                let max = parse_decimal_natural(bytes, &mut cursor);
                if cursor == max_start || bytes.get(cursor) != Some(&b'}') {
                    return Ok(None);
                }
                max_end = cursor;
                cursor += 1;
                Some(max.expect("nonempty upper decimal span"))
            }
        }
        _ => return Ok(None),
    };
    // Original decimal spans are the sole syntax-order authority. Both exact
    // bounds remain finite when written as DecimalDigits; neither is saturated
    // or reclassified as unbounded after this check.
    if max.is_some()
        && cmp_decimal_spans(&bytes[min_start..min_end], &bytes[max_start..max_end])
            == std::cmp::Ordering::Greater
    {
        return Err(RegExpCompileError::invalid_syntax(
            SyntaxRule::QuantifierBounds,
            start,
            "regular-expression quantifier bounds are reversed",
        ));
    }
    *offset = cursor;
    Ok(Some(Quantifier {
        bounds: RegExpRepeatBounds::new(
            min,
            max.map_or(RegExpRepeatMaximum::Unbounded, RegExpRepeatMaximum::Finite),
        )
        .expect("original decimal spans established exact bound order"),
        preference: QuantifierPreference::Greedy,
    }))
}

/// Order two ASCII digit spans by mathematical value.
fn cmp_decimal_spans(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    fn strip(span: &[u8]) -> &[u8] {
        let start = span
            .iter()
            .position(|&digit| digit != b'0')
            .unwrap_or(span.len());
        &span[start..]
    }
    let (left, right) = (strip(left), strip(right));
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn parse_decimal_natural(bytes: &[u8], offset: &mut usize) -> Option<RegExpNatural> {
    let start = *offset;
    while bytes.get(*offset).is_some_and(u8::is_ascii_digit) {
        *offset += 1;
    }
    RegExpNatural::from_decimal_digits(&bytes[start..*offset])
}

enum NullableQuantifierContinuation {
    NextInstruction,
    Repeat,
}

#[must_use = "a nullable optional quantifier attempt must emit its paired progress check"]
struct PendingNullableQuantifierProgress {
    split_pc: usize,
    attempt_pc: usize,
    preference: QuantifierPreference,
}

#[must_use = "a completed nullable quantifier attempt must receive its quantifier fallback"]
struct PendingNullableQuantifierFallback {
    split_pc: usize,
    attempt_pc: usize,
    preference: QuantifierPreference,
}

#[must_use = "a counted repetition must publish its paired Guard, End and Exit"]
struct PendingCountedRepeat {
    begin_pc: usize,
    slot: u32,
    preference: QuantifierPreference,
}

struct ProgramLowerer<'a> {
    instructions: &'a mut Vec<RegExpInstruction>,
    error_offset: usize,
    named_groups: &'a [RegExpNamedGroup],
    repeat_bounds: &'a mut Vec<RegExpRepeatBounds>,
}

#[derive(Clone, Copy)]
enum RegExpMatchDirection {
    Forward,
    Reverse,
}

impl RegExpMatchDirection {
    const fn operand_bit(self) -> u64 {
        match self {
            Self::Forward => 0,
            Self::Reverse => 1,
        }
    }
}

impl<'a> ProgramLowerer<'a> {
    fn new(
        instructions: &'a mut Vec<RegExpInstruction>,
        repeat_bounds: &'a mut Vec<RegExpRepeatBounds>,
        pattern_len: usize,
        named_groups: &'a [RegExpNamedGroup],
    ) -> Self {
        Self {
            instructions,
            error_offset: pattern_len,
            named_groups,
            repeat_bounds,
        }
    }

    fn push(&mut self, instruction: RegExpInstruction) -> Result<(), RegExpCompileError> {
        if self.instructions.len() >= REGEXP_MAX_INSTRUCTIONS {
            return Err(RegExpCompileError::unsupported_feature(
                self.error_offset,
                format!(
                    "regular-expression expands beyond the {REGEXP_MAX_INSTRUCTIONS}-instruction matcher-program limit"
                ),
            ));
        }
        self.instructions.push(instruction);
        Ok(())
    }

    fn alternatives(&mut self, alternatives: &[Vec<ParsedTerm>]) -> Result<(), RegExpCompileError> {
        if pure_epsilon::alternatives_are_pure_epsilon(alternatives) {
            return Ok(());
        }
        let mut exits = Vec::new();
        for (index, sequence) in alternatives.iter().enumerate() {
            if index + 1 == alternatives.len() {
                self.sequence(sequence)?;
                break;
            }
            let split = self.instructions.len();
            self.push(RegExpInstruction::split(0, 0))?;
            let primary = self.instructions.len();
            self.sequence(sequence)?;
            let exit = self.instructions.len();
            self.push(RegExpInstruction::jump(0))?;
            let fallback = self.instructions.len();
            self.instructions[split] = RegExpInstruction::split(primary, fallback);
            exits.push(exit);
        }
        let after = self.instructions.len();
        for exit in exits {
            self.instructions[exit] = RegExpInstruction::jump(after);
        }
        Ok(())
    }

    fn sequence(&mut self, terms: &[ParsedTerm]) -> Result<(), RegExpCompileError> {
        for term in terms {
            match term {
                ParsedTerm::Quantified {
                    atom,
                    quantifier,
                    quantifier_offset,
                } => self.quantified(atom, quantifier.clone(), *quantifier_offset)?,
                ParsedTerm::LegacyUtf16Pair {
                    pair,
                    trail_quantifier,
                    quantifier_offset,
                } => {
                    self.error_offset = *quantifier_offset;
                    self.push(pair.lead_instruction())?;
                    self.quantified(
                        &ParsedAtom::Instruction(pair.trail_instruction()),
                        trail_quantifier.clone(),
                        *quantifier_offset,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn begin_counted_repeat(
        &mut self,
        bounds: &RegExpRepeatBounds,
        preference: QuantifierPreference,
    ) -> Result<PendingCountedRepeat, RegExpCompileError> {
        let begin_pc = self.instructions.len();
        let slot = u32::try_from(self.repeat_bounds.len()).expect("instruction-bounded slot count");
        self.repeat_bounds.push(bounds.clone());
        self.push(RegExpInstruction::repeat_begin(slot))?;
        self.push(RegExpInstruction::repeat_guard(0, slot, preference))?;
        Ok(PendingCountedRepeat {
            begin_pc,
            slot,
            preference,
        })
    }

    fn finish_counted_repeat(
        &mut self,
        pending: PendingCountedRepeat,
    ) -> Result<(), RegExpCompileError> {
        let end_pc = self.instructions.len();
        self.push(RegExpInstruction::repeat_end(pending.begin_pc))?;
        self.push(RegExpInstruction::repeat_exit(pending.begin_pc))?;
        self.instructions[pending.begin_pc + 1] =
            RegExpInstruction::repeat_guard(end_pc, pending.slot, pending.preference);
        Ok(())
    }

    fn quantified(
        &mut self,
        atom: &ParsedAtom,
        quantifier: Quantifier,
        offset: usize,
    ) -> Result<(), RegExpCompileError> {
        self.error_offset = offset;
        if pure_epsilon::atom_is_pure_epsilon(atom) {
            return Ok(());
        }
        let progress = OptionalAtomProgress::for_atom(atom);
        if quantifier.has_impossible_consuming_minimum()
            && matches!(progress, OptionalAtomProgress::MustAdvance)
        {
            return self.never_match(RegExpMatchDirection::Forward);
        }
        let trivial = match quantifier.body(matches!(
            progress,
            OptionalAtomProgress::MayRemainAtSameIndex,
        )) {
            QuantifierBody::Counted(bounds) => {
                let pending = self.begin_counted_repeat(bounds, quantifier.preference)?;
                self.atom(atom)?;
                return self.finish_counted_repeat(pending);
            }
            QuantifierBody::Trivial(trivial) => trivial,
        };
        // `X+` on a must-advance atom is a single body plus a loop-back
        // split. Emitting the required copy and a separate star copy would
        // duplicate the body; for thousand-string finite class sets that
        // doubling alone exceeds the matcher-program cap.
        if trivial.required
            && matches!(trivial.optional, TrivialQuantifierOptional::Unbounded)
            && matches!(progress, OptionalAtomProgress::MustAdvance)
        {
            let body = self.instructions.len();
            self.atom(atom)?;
            let split = self.instructions.len();
            self.push(RegExpInstruction::split(0, 0))?;
            let after = self.instructions.len();
            self.instructions[split] = match quantifier.preference {
                QuantifierPreference::Greedy => RegExpInstruction::split(body, after),
                QuantifierPreference::Lazy => RegExpInstruction::split(after, body),
            };
            return Ok(());
        }
        if trivial.required {
            self.atom(atom)?;
        }
        match trivial.optional {
            TrivialQuantifierOptional::None => {}
            TrivialQuantifierOptional::Once => match progress {
                OptionalAtomProgress::MustAdvance => self.optional(atom, quantifier.preference)?,
                OptionalAtomProgress::MayRemainAtSameIndex => {
                    self.nullable_optional(atom, quantifier.preference)?
                }
            },
            TrivialQuantifierOptional::Unbounded => match progress {
                OptionalAtomProgress::MustAdvance => self.star(atom, quantifier.preference)?,
                OptionalAtomProgress::MayRemainAtSameIndex => {
                    self.nullable_star(atom, quantifier.preference)?;
                }
            },
        }
        Ok(())
    }

    fn optional(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let split = self.instructions.len();
        self.push(RegExpInstruction::split(0, 0))?;
        let attempt = self.instructions.len();
        self.atom(atom)?;
        let after = self.instructions.len();
        self.instructions[split] = match preference {
            QuantifierPreference::Greedy => RegExpInstruction::split(attempt, after),
            QuantifierPreference::Lazy => RegExpInstruction::split(after, attempt),
        };
        Ok(())
    }

    fn star(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let split = self.instructions.len();
        self.push(RegExpInstruction::split(0, 0))?;
        let attempt = self.instructions.len();
        self.atom(atom)?;
        self.push(RegExpInstruction::jump(split))?;
        let after = self.instructions.len();
        self.instructions[split] = match preference {
            QuantifierPreference::Greedy => RegExpInstruction::split(attempt, after),
            QuantifierPreference::Lazy => RegExpInstruction::split(after, attempt),
        };
        Ok(())
    }

    fn nullable_optional(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let pending = self.begin_nullable_optional(preference)?;
        self.atom(atom)?;
        let fallback = self
            .complete_nullable_optional(pending, NullableQuantifierContinuation::NextInstruction)?;
        self.finish_nullable_optional(fallback, self.instructions.len());
        Ok(())
    }

    fn nullable_star(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let pending = self.begin_nullable_optional(preference)?;
        self.atom(atom)?;
        let fallback =
            self.complete_nullable_optional(pending, NullableQuantifierContinuation::Repeat)?;
        let after = self.instructions.len();
        self.finish_nullable_optional(fallback, after);
        Ok(())
    }

    fn begin_nullable_optional(
        &mut self,
        preference: QuantifierPreference,
    ) -> Result<PendingNullableQuantifierProgress, RegExpCompileError> {
        let split_pc = self.instructions.len();
        self.push(RegExpInstruction::progress_split(0, 0, preference))?;
        Ok(PendingNullableQuantifierProgress {
            split_pc,
            attempt_pc: self.instructions.len(),
            preference,
        })
    }

    fn complete_nullable_optional(
        &mut self,
        pending: PendingNullableQuantifierProgress,
        continuation: NullableQuantifierContinuation,
    ) -> Result<PendingNullableQuantifierFallback, RegExpCompileError> {
        let check_pc = self.instructions.len();
        let continuation_pc = match continuation {
            NullableQuantifierContinuation::NextInstruction => check_pc + 1,
            NullableQuantifierContinuation::Repeat => pending.split_pc,
        };
        self.push(RegExpInstruction::progress_check(
            pending.split_pc,
            continuation_pc,
        ))?;
        Ok(PendingNullableQuantifierFallback {
            split_pc: pending.split_pc,
            attempt_pc: pending.attempt_pc,
            preference: pending.preference,
        })
    }

    fn finish_nullable_optional(
        &mut self,
        pending: PendingNullableQuantifierFallback,
        fallback_pc: usize,
    ) {
        self.instructions[pending.split_pc] =
            RegExpInstruction::progress_split(pending.attempt_pc, fallback_pc, pending.preference);
    }

    fn atom(&mut self, atom: &ParsedAtom) -> Result<(), RegExpCompileError> {
        match atom {
            ParsedAtom::Instruction(instruction) => self.push(*instruction),
            ParsedAtom::FiniteClassSet(atom) => {
                self.finite_class_set_atom(atom, RegExpMatchDirection::Forward)
            }
            ParsedAtom::Capture {
                id,
                body,
                subtree_end,
            } => {
                self.push(RegExpInstruction::clear_capture_range(*id, *subtree_end))?;
                self.push(RegExpInstruction::capture_start(*id))?;
                self.alternatives(body)?;
                self.push(RegExpInstruction::capture_end(*id))
            }
            ParsedAtom::NonCapture {
                body,
                subtree_start,
                subtree_end,
            } => {
                if subtree_start != subtree_end {
                    self.push(RegExpInstruction::clear_capture_range(
                        *subtree_start,
                        *subtree_end,
                    ))?;
                }
                self.alternatives(body)
            }
            ParsedAtom::NamedBackreference {
                name,
                offset,
                folding,
            } => {
                let name_id = self
                    .named_groups
                    .iter()
                    .position(|group| group.name == *name)
                    .ok_or_else(|| {
                        RegExpCompileError::invalid_syntax(
                            SyntaxRule::UnknownGroupName,
                            *offset,
                            format!("unknown named backreference `{name}`"),
                        )
                    })?;
                self.push(RegExpInstruction::named_backreference(
                    name_id as u32,
                    *folding,
                ))
            }
            ParsedAtom::NumberedBackreference {
                capture_id,
                folding,
            } => self.push(RegExpInstruction::numbered_backreference(
                *capture_id,
                *folding,
            )),
            ParsedAtom::Lookaround {
                polarity,
                direction,
                body,
                subtree_start,
                subtree_end,
            } => self.lookaround(
                polarity,
                *direction,
                RegExpMatchDirection::Forward,
                body,
                *subtree_start..*subtree_end,
            ),
        }
    }

    /// Lower an unsatisfiable repetition to an always-failing zero-width pair.
    ///
    /// `(?=)(?!)`: the positive empty assertion succeeds without consuming
    /// input and the negative empty assertion then always fails, so the pair
    /// fails with no captures or side effects. Used for minimums no
    /// addressable input can satisfy.
    fn never_match(
        &mut self,
        parent_direction: RegExpMatchDirection,
    ) -> Result<(), RegExpCompileError> {
        let empty: Vec<Vec<ParsedTerm>> = vec![Vec::new()];
        self.lookaround(
            &LookaroundPolarity::Positive,
            RegExpMatchDirection::Forward,
            parent_direction,
            &empty,
            0..0,
        )?;
        self.lookaround(
            &LookaroundPolarity::Negative,
            RegExpMatchDirection::Forward,
            parent_direction,
            &empty,
            0..0,
        )
    }

    fn lookaround(
        &mut self,
        polarity: &LookaroundPolarity,
        direction: RegExpMatchDirection,
        parent_direction: RegExpMatchDirection,
        body: &[Vec<ParsedTerm>],
        captures: std::ops::Range<u32>,
    ) -> Result<(), RegExpCompileError> {
        if !captures.is_empty() {
            self.push(RegExpInstruction::clear_capture_range(
                captures.start,
                captures.end,
            ))?;
        }
        self.push(RegExpInstruction::lookaround_start(direction))?;
        let sentinel = self.instructions.len();
        self.push(RegExpInstruction::split(0, 0))?;
        let body_start = self.instructions.len();
        match direction {
            RegExpMatchDirection::Forward => self.alternatives(body)?,
            RegExpMatchDirection::Reverse => self.reverse_alternatives(body)?,
        }
        let end = self.instructions.len();
        self.push(RegExpInstruction::lookaround_end(
            0,
            0,
            polarity,
            parent_direction,
        ))?;
        let failure = self.instructions.len();
        self.push(RegExpInstruction::lookaround_failure(
            0,
            polarity,
            parent_direction,
        ))?;
        let after = self.instructions.len();
        self.instructions[sentinel] = RegExpInstruction::split(body_start, failure);
        self.instructions[end] =
            RegExpInstruction::lookaround_end(failure, after, polarity, parent_direction);
        self.instructions[failure] =
            RegExpInstruction::lookaround_failure(after, polarity, parent_direction);
        Ok(())
    }

    fn finite_class_set_atom(
        &mut self,
        atom: &FiniteClassSetAtom,
        direction: RegExpMatchDirection,
    ) -> Result<(), RegExpCompileError> {
        let alternative_count =
            atom.multi_code_point_strings.len() + 1 + usize::from(atom.contains_empty);
        let mut exits = Vec::with_capacity(alternative_count.saturating_sub(1));
        for index in 0..alternative_count {
            if index + 1 == alternative_count {
                self.finite_class_set_alternative(atom, index, direction)?;
                break;
            }
            let split = self.instructions.len();
            self.push(RegExpInstruction::split(0, 0))?;
            let primary = self.instructions.len();
            self.finite_class_set_alternative(atom, index, direction)?;
            let exit = self.instructions.len();
            self.push(RegExpInstruction::jump(0))?;
            let fallback = self.instructions.len();
            self.instructions[split] = RegExpInstruction::split(primary, fallback);
            exits.push(exit);
        }
        let after = self.instructions.len();
        for exit in exits {
            self.instructions[exit] = RegExpInstruction::jump(after);
        }
        Ok(())
    }

    fn finite_class_set_alternative(
        &mut self,
        atom: &FiniteClassSetAtom,
        index: usize,
        direction: RegExpMatchDirection,
    ) -> Result<(), RegExpCompileError> {
        if let Some(string) = atom.multi_code_point_strings.get(index) {
            match direction {
                RegExpMatchDirection::Forward => {
                    for instruction in string {
                        self.push(*instruction)?;
                    }
                }
                RegExpMatchDirection::Reverse => {
                    for instruction in string.iter().rev() {
                        self.push(*instruction)?;
                    }
                }
            }
            return Ok(());
        }
        if index == atom.multi_code_point_strings.len() {
            return self.push(atom.singleton);
        }
        debug_assert!(atom.contains_empty);
        Ok(())
    }

    fn reverse_alternatives(
        &mut self,
        alternatives: &[Vec<ParsedTerm>],
    ) -> Result<(), RegExpCompileError> {
        if pure_epsilon::alternatives_are_pure_epsilon(alternatives) {
            return Ok(());
        }
        let mut exits = Vec::new();
        for (index, sequence) in alternatives.iter().enumerate() {
            if index + 1 == alternatives.len() {
                self.reverse_sequence(sequence)?;
                break;
            }
            let split = self.instructions.len();
            self.push(RegExpInstruction::split(0, 0))?;
            let primary = self.instructions.len();
            self.reverse_sequence(sequence)?;
            let exit = self.instructions.len();
            self.push(RegExpInstruction::jump(0))?;
            let fallback = self.instructions.len();
            self.instructions[split] = RegExpInstruction::split(primary, fallback);
            exits.push(exit);
        }
        let after = self.instructions.len();
        for exit in exits {
            self.instructions[exit] = RegExpInstruction::jump(after);
        }
        Ok(())
    }

    fn reverse_sequence(&mut self, terms: &[ParsedTerm]) -> Result<(), RegExpCompileError> {
        for term in terms.iter().rev() {
            match term {
                ParsedTerm::Quantified {
                    atom,
                    quantifier,
                    quantifier_offset,
                } => self.reverse_quantified(atom, quantifier.clone(), *quantifier_offset)?,
                ParsedTerm::LegacyUtf16Pair {
                    pair,
                    trail_quantifier,
                    quantifier_offset,
                } => {
                    self.reverse_quantified(
                        &ParsedAtom::Instruction(pair.trail_instruction()),
                        trail_quantifier.clone(),
                        *quantifier_offset,
                    )?;
                    self.push(pair.lead_instruction())?;
                }
            }
        }
        Ok(())
    }

    fn reverse_quantified(
        &mut self,
        atom: &ParsedAtom,
        quantifier: Quantifier,
        offset: usize,
    ) -> Result<(), RegExpCompileError> {
        self.error_offset = offset;
        if pure_epsilon::atom_is_pure_epsilon(atom) {
            return Ok(());
        }
        let progress = OptionalAtomProgress::for_atom(atom);
        if quantifier.has_impossible_consuming_minimum()
            && matches!(progress, OptionalAtomProgress::MustAdvance)
        {
            return self.never_match(RegExpMatchDirection::Reverse);
        }
        let trivial = match quantifier.body(matches!(
            progress,
            OptionalAtomProgress::MayRemainAtSameIndex,
        )) {
            QuantifierBody::Counted(bounds) => {
                let pending = self.begin_counted_repeat(bounds, quantifier.preference)?;
                self.reverse_atom(atom)?;
                return self.finish_counted_repeat(pending);
            }
            QuantifierBody::Trivial(trivial) => trivial,
        };
        // Mirror of the forward single-copy `X+` loop above.
        if trivial.required
            && matches!(trivial.optional, TrivialQuantifierOptional::Unbounded)
            && matches!(progress, OptionalAtomProgress::MustAdvance)
        {
            let body = self.instructions.len();
            self.reverse_atom(atom)?;
            let split = self.instructions.len();
            self.push(RegExpInstruction::split(0, 0))?;
            let after = self.instructions.len();
            self.instructions[split] = match quantifier.preference {
                QuantifierPreference::Greedy => RegExpInstruction::split(body, after),
                QuantifierPreference::Lazy => RegExpInstruction::split(after, body),
            };
            return Ok(());
        }
        if trivial.required {
            self.reverse_atom(atom)?;
        }
        match trivial.optional {
            TrivialQuantifierOptional::None => {}
            TrivialQuantifierOptional::Once => match progress {
                OptionalAtomProgress::MustAdvance => {
                    self.reverse_optional(atom, quantifier.preference)?
                }
                OptionalAtomProgress::MayRemainAtSameIndex => {
                    self.reverse_nullable_optional(atom, quantifier.preference)?
                }
            },
            TrivialQuantifierOptional::Unbounded => match progress {
                OptionalAtomProgress::MustAdvance => {
                    self.reverse_star(atom, quantifier.preference)?;
                }
                OptionalAtomProgress::MayRemainAtSameIndex => {
                    self.reverse_nullable_star(atom, quantifier.preference)?;
                }
            },
        }
        Ok(())
    }

    fn reverse_optional(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let split = self.instructions.len();
        self.push(RegExpInstruction::split(0, 0))?;
        let attempt = self.instructions.len();
        self.reverse_atom(atom)?;
        let after = self.instructions.len();
        self.instructions[split] = match preference {
            QuantifierPreference::Greedy => RegExpInstruction::split(attempt, after),
            QuantifierPreference::Lazy => RegExpInstruction::split(after, attempt),
        };
        Ok(())
    }

    fn reverse_star(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let split = self.instructions.len();
        self.push(RegExpInstruction::split(0, 0))?;
        let attempt = self.instructions.len();
        self.reverse_atom(atom)?;
        self.push(RegExpInstruction::jump(split))?;
        let after = self.instructions.len();
        self.instructions[split] = match preference {
            QuantifierPreference::Greedy => RegExpInstruction::split(attempt, after),
            QuantifierPreference::Lazy => RegExpInstruction::split(after, attempt),
        };
        Ok(())
    }

    fn reverse_nullable_optional(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let pending = self.begin_nullable_optional(preference)?;
        self.reverse_atom(atom)?;
        let fallback = self
            .complete_nullable_optional(pending, NullableQuantifierContinuation::NextInstruction)?;
        self.finish_nullable_optional(fallback, self.instructions.len());
        Ok(())
    }

    fn reverse_nullable_star(
        &mut self,
        atom: &ParsedAtom,
        preference: QuantifierPreference,
    ) -> Result<(), RegExpCompileError> {
        let pending = self.begin_nullable_optional(preference)?;
        self.reverse_atom(atom)?;
        let fallback =
            self.complete_nullable_optional(pending, NullableQuantifierContinuation::Repeat)?;
        let after = self.instructions.len();
        self.finish_nullable_optional(fallback, after);
        Ok(())
    }

    fn reverse_atom(&mut self, atom: &ParsedAtom) -> Result<(), RegExpCompileError> {
        match atom {
            ParsedAtom::Instruction(instruction) => self.push(*instruction),
            ParsedAtom::FiniteClassSet(atom) => {
                self.finite_class_set_atom(atom, RegExpMatchDirection::Reverse)
            }
            ParsedAtom::Capture {
                id,
                body,
                subtree_end,
            } => {
                self.push(RegExpInstruction::clear_capture_range(*id, *subtree_end))?;
                self.push(RegExpInstruction::capture_end(*id))?;
                self.reverse_alternatives(body)?;
                self.push(RegExpInstruction::capture_start(*id))
            }
            ParsedAtom::NonCapture {
                body,
                subtree_start,
                subtree_end,
            } => {
                if subtree_start != subtree_end {
                    self.push(RegExpInstruction::clear_capture_range(
                        *subtree_start,
                        *subtree_end,
                    ))?;
                }
                self.reverse_alternatives(body)
            }
            ParsedAtom::Lookaround {
                polarity,
                direction,
                body,
                subtree_start,
                subtree_end,
            } => self.lookaround(
                polarity,
                *direction,
                RegExpMatchDirection::Reverse,
                body,
                *subtree_start..*subtree_end,
            ),
            ParsedAtom::NamedBackreference { .. } | ParsedAtom::NumberedBackreference { .. } => {
                self.atom(atom)
            }
        }
    }
}

fn add_ascii_range(bitmap_low: &mut u64, bitmap_high: &mut u64, start: u8, end: u8) {
    for member in start..=end {
        add_ascii_member(bitmap_low, bitmap_high, member);
    }
}

fn add_ascii_member(bitmap_low: &mut u64, bitmap_high: &mut u64, member: u8) {
    if member < 64 {
        *bitmap_low |= 1_u64 << member;
    } else {
        *bitmap_high |= 1_u64 << (member - 64);
    }
}

/// `SyntaxCharacter :: one of ^ $ \ . * + ? ( ) [ ] { } |` (22.2.1), and
/// nothing else.
///
/// Named for the production rather than for a vague notion of "characters with
/// special meaning", which is what let DEFECT 1 hide: read as "metacharacter",
/// this looked like a plausible spelling of the Unicode identity-escape rule,
/// and the missing `/` alternative was invisible for as long as the name did
/// not say which production it was. Three call sites, each a *different*
/// grammar rule built on top of this one, and none of them may widen it:
/// [`is_unicode_identity_escape`], [`is_class_identity_escape`], and the
/// unsupported-metacharacter fallthrough in `parse_instruction_atom`. Adding
/// `/` here would have turned a bare `/` in `new RegExp("a/b")` into an
/// `UnsupportedFeature` verdict.
fn is_syntax_character(byte: u8) -> bool {
    matches!(
        byte,
        b'^' | b'$'
            | b'\\'
            | b'.'
            | b'*'
            | b'+'
            | b'?'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'|'
    )
}

/// `IdentityEscape[+UnicodeMode] :: SyntaxCharacter | `/`` (22.2.1).
///
/// The `/` alternative exists because a RegExp *literal* must be able to escape
/// its own delimiter, and `\/` is therefore an everyday pattern rather than a
/// corner case. `test262/vendor/test262/test/built-ins/RegExp/unicode_identity_escape.js`
/// asserts both alternatives, in `AtomEscape` and in `ClassEscape`.
fn is_unicode_identity_escape(byte: u8) -> bool {
    is_syntax_character(byte) || byte == b'/'
}

#[cfg(test)]
mod catalog_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_folding_tables_match_full_scalar_reference() {
        for folding in [CaseFolding::Legacy, CaseFolding::Unicode] {
            // Keep the previous scalar scan as an independent reference for
            // domain coverage, identity filtering and exact table ordering.
            let expected: Vec<_> = (0..=char::MAX as u32)
                .filter_map(|character| {
                    let canonical = folding.canonicalize(character);
                    (canonical != character).then_some((character, canonical))
                })
                .collect();
            assert_eq!(folding.mappings(), expected, "{folding:?}");
        }
    }

    fn compile(pattern: &str) -> RegExpProgram {
        RegExpProgram::compile(pattern, "").expect("pattern should compile")
    }

    #[test]
    fn compiles_literals_and_class_ranges() {
        let program = compile("t[a-b|q-s]");
        assert_eq!(program.instructions.len(), 3);
        assert_eq!(
            program.instructions[0],
            RegExpInstruction::literal_ascii(b't')
        );
        let class = program.instructions[1];
        for member in b"ab|qrs" {
            assert!(class.positive_ascii_class_contains(*member));
        }
        assert!(!class.positive_ascii_class_contains(b'c'));
        assert_eq!(program.instructions[2], RegExpInstruction::accept());
    }

    #[test]
    fn empty_pattern_compiles_to_accept() {
        assert_eq!(compile("").instructions, vec![RegExpInstruction::accept()]);
    }

    #[test]
    fn ascii_ignore_case_expands_literals_and_classes() {
        let program = RegExpProgram::compile("a[B-c]", "i").expect("pattern should compile");
        for (instruction, lowercase, uppercase) in [
            (program.instructions[0], b'a', b'A'),
            (program.instructions[1], b'b', b'B'),
        ] {
            assert!(instruction.positive_ascii_class_contains(lowercase));
            assert!(instruction.positive_ascii_class_contains(uppercase));
        }
    }

    #[test]
    fn unicode_singleton_class_compiles_as_one_scalar() {
        let program = RegExpProgram::compile("[𝌆]", "u").expect("pattern should compile");
        assert_eq!(
            program.instructions,
            vec![
                RegExpInstruction::literal_code_point(0x1d306),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn annex_b_quantified_lookaheads_collapse_zero_width_repetitions() {
        assert_eq!(compile(".(?=Z)*"), compile("."));
        assert_eq!(compile(".(?=Z)+"), compile(".(?=Z)"));
        assert_eq!(compile("[a-e](?!Z){2,3}"), compile("[a-e](?!Z)"));
    }

    #[test]
    fn compiles_range_classes_at_ascii_bitmap_boundaries() {
        let program = compile("[a-f]d");
        let class = program.instructions[0];
        assert!(class.positive_ascii_class_contains(b'a'));
        assert!(class.positive_ascii_class_contains(b'f'));
        assert!(!class.positive_ascii_class_contains(b'g'));
        assert_eq!(
            program.instructions[1],
            RegExpInstruction::literal_ascii(b'd')
        );

        let program = compile("[a-z]n");
        let class = program.instructions[0];
        assert!(class.positive_ascii_class_contains(b'a'));
        assert!(class.positive_ascii_class_contains(b'z'));
        assert!(!class.positive_ascii_class_contains(b'A'));
    }

    #[test]
    fn compiles_empty_character_classes() {
        assert_eq!(
            compile("[]").instructions,
            vec![
                RegExpInstruction::positive_ascii_class(0, 0),
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            compile("[^]").instructions,
            vec![
                RegExpInstruction::negative_ascii_class(0, 0),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn compiles_singleton_class_members() {
        let program = compile("[Nn]evermore");
        let class = program.instructions[0];
        assert!(class.positive_ascii_class_contains(b'N'));
        assert!(class.positive_ascii_class_contains(b'n'));
        assert!(!class.positive_ascii_class_contains(b'm'));
        assert_eq!(
            program.instructions[1],
            RegExpInstruction::literal_ascii(b'e')
        );
    }

    #[test]
    fn class_hyphens_are_literal_when_not_range_separators() {
        let leading = compile("[-a]").instructions[0];
        assert!(leading.positive_ascii_class_contains(b'-'));
        assert!(leading.positive_ascii_class_contains(b'a'));
        assert!(!leading.positive_ascii_class_contains(b'.'));

        let after_range = compile("[a-b-c]").instructions[0];
        for member in b"ab-c" {
            assert!(after_range.positive_ascii_class_contains(*member));
        }
        assert!(!after_range.positive_ascii_class_contains(b'd'));

        let trailing = compile("[a-]").instructions[0];
        assert!(trailing.positive_ascii_class_contains(b'a'));
        assert!(trailing.positive_ascii_class_contains(b'-'));

        let leading_dash_range = compile("[--a]").instructions[0];
        for member in b'-'..=b'a' {
            assert!(leading_dash_range.positive_ascii_class_contains(member));
        }
        assert!(!leading_dash_range.positive_ascii_class_contains(b'b'));
    }

    #[test]
    fn annex_b_class_escapes_and_set_ranges_compile_to_ascii_bitmaps() {
        let controls = compile(r"[\c0\c1\c8\c9\c_]").instructions[0];
        for member in [0x10, 0x11, 0x18, 0x19, 0x1f] {
            assert!(controls.positive_ascii_class_contains(member));
        }

        let decimal_range = compile(r"[\12-\14]").instructions[0];
        for member in 0x0a..=0x0c {
            assert!(decimal_range.positive_ascii_class_contains(member));
        }

        let union_range = compile(r"[\d-a]").instructions[0];
        for member in b'0'..=b'9' {
            assert!(union_range.positive_ascii_class_contains(member));
        }
        assert!(union_range.positive_ascii_class_contains(b'-'));
        assert!(union_range.positive_ascii_class_contains(b'a'));
    }

    #[test]
    fn whitespace_classes_retain_non_ascii_members_in_every_mode() {
        for flags in ["", "u", "v"] {
            let program = RegExpProgram::compile(r"[\s]", flags)
                .expect("whitespace class has a complete character domain");
            assert_eq!(
                program.instructions[0].opcode,
                REGEXP_OPCODE_UNICODE_PROPERTY
            );
            for member in [
                0x09, 0x20, 0xA0, 0x1680, 0x2000, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000,
                0xFEFF,
            ] {
                assert!(
                    ranges_contain(&program.ranges, member),
                    "{flags}: U+{member:04X}"
                );
            }
            for nonmember in [0x00, 0x85, 0x180E, 0x200B, 0xFFFF] {
                assert!(
                    !ranges_contain(&program.ranges, nonmember),
                    "{flags}: U+{nonmember:04X}"
                );
            }
        }
    }

    #[test]
    fn legacy_octal_classes_select_a_representation_covering_the_decoded_byte() {
        let low = compile(r"[\177]").instructions[0];
        assert!(low.positive_ascii_class_contains(0x7F));
        for (pattern, expected) in [
            (r"[\200]", vec![(0x80, 0x80)]),
            (r"[\377]", vec![(0xFF, 0xFF)]),
            (r"[\177-\377]", vec![(0x7F, 0xFF)]),
        ] {
            let program = compile(pattern);
            assert_eq!(
                program.instructions[0].opcode,
                REGEXP_OPCODE_UNICODE_PROPERTY
            );
            assert_eq!(program.ranges, expected, "{pattern}");
            for flags in ["u", "v"] {
                assert_eq!(
                    RegExpProgram::compile(pattern, flags)
                        .expect_err("octal class escapes remain Annex B only")
                        .kind,
                    RegExpCompileErrorKind::InvalidSyntax
                );
            }
        }
        assert_eq!(
            RegExpProgram::compile(r"[\377-\200]", "")
                .expect_err("decoded octal range endpoints retain source order")
                .rule,
            Some(SyntaxRule::ClassRangeOrder)
        );
    }

    #[test]
    fn negated_ascii_classes_compile_to_negative_class_instructions() {
        let instruction = compile(r"[^\d]").instructions[0];
        assert_eq!(instruction.opcode, REGEXP_OPCODE_NEGATIVE_ASCII_CLASS);
        assert_ne!(instruction.operand0 & (1_u64 << b'0'), 0);
        assert_ne!(instruction.operand0 & (1_u64 << b'9'), 0);
    }

    #[test]
    fn class_membership_rejects_non_ascii_code_units() {
        let class = RegExpInstruction::positive_ascii_class(0, u64::MAX);
        assert!(class.positive_ascii_class_contains(127));
        assert!(!class.positive_ascii_class_contains(128));
        assert!(!class.positive_ascii_class_contains(255));
    }

    #[test]
    fn escaped_syntax_characters_are_literal_atoms() {
        let program = compile(r"\.\^\$\*\+\?\(\)\[\]\{\}\|");
        let literals = program.instructions[..program.instructions.len() - 1]
            .iter()
            .map(|instruction| instruction.operand0 as u8)
            .collect::<Vec<_>>();
        assert_eq!(literals, b".^$*+?()[]{}|");
        assert!(program.instructions[..program.instructions.len() - 1]
            .iter()
            .all(|instruction| instruction.opcode == REGEXP_OPCODE_LITERAL_ASCII));
    }

    #[test]
    fn annex_b_extended_literals_identity_escapes_and_octal_escapes_compile() {
        assert_eq!(
            compile(r"]{}\C\8\9\377").instructions,
            vec![
                RegExpInstruction::literal_ascii(b']'),
                RegExpInstruction::literal_ascii(b'{'),
                RegExpInstruction::literal_ascii(b'}'),
                RegExpInstruction::literal_ascii(b'C'),
                RegExpInstruction::literal_ascii(b'8'),
                RegExpInstruction::literal_ascii(b'9'),
                RegExpInstruction::literal_code_point(0xff),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn legacy_incomplete_control_escapes_keep_the_standalone_backslash() {
        for (source, characters) in [
            (r"\c", b"\\c".as_slice()),
            (r"\c0", b"\\c0".as_slice()),
            (r"\c_", b"\\c_".as_slice()),
        ] {
            let program = compile(source);
            assert_eq!(
                program.instructions,
                characters
                    .iter()
                    .map(|unit| RegExpInstruction::literal_ascii(*unit))
                    .chain([RegExpInstruction::accept()])
                    .collect::<Vec<_>>(),
                "{source}"
            );
            for flags in ["u", "v"] {
                assert_eq!(
                    RegExpProgram::compile(source, flags).unwrap_err().kind,
                    RegExpCompileErrorKind::InvalidSyntax
                );
            }
        }
        let quantified = compile(r"\c+");
        assert_eq!(
            quantified.instructions,
            vec![
                RegExpInstruction::literal_ascii(b'\\'),
                RegExpInstruction::literal_ascii(b'c'),
                RegExpInstruction::split(1, 3),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn numbered_escape_uses_an_existing_capture_before_legacy_octal() {
        let program = compile(r"(.)\1");
        assert!(program
            .instructions
            .contains(&RegExpInstruction::numbered_backreference(
                1,
                CaseFolding::Sensitive
            )));
        assert!(!program
            .instructions
            .contains(&RegExpInstruction::literal_ascii(1)));
    }

    #[test]
    fn numbered_capture_backreferences_use_progress_for_unbounded_quantification() {
        let program = compile(r"^(a+)\1*,\1+$");
        assert!(program
            .instructions
            .contains(&RegExpInstruction::numbered_backreference(
                1,
                CaseFolding::Sensitive
            )));
        assert_eq!(
            program
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == REGEXP_OPCODE_PROGRESS_SPLIT)
                .count(),
            1
        );
        assert_eq!(
            program
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == REGEXP_OPCODE_PROGRESS_CHECK)
                .count(),
            1
        );
        assert_eq!(
            program
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == REGEXP_OPCODE_REPEAT_BEGIN)
                .count(),
            1,
            "nullable + shares one body instead of a required and optional copy"
        );
    }

    #[test]
    fn annex_b_identity_k_and_forward_numbered_backreferences_use_whole_pattern_syntax() {
        let identity = compile(r"\k<a>");
        let literals = identity.instructions[..identity.instructions.len() - 1]
            .iter()
            .map(|instruction| instruction.operand0 as u8)
            .collect::<Vec<_>>();
        assert_eq!(literals, b"k<a>");

        let forward = compile(r"\1(b)");
        assert_eq!(
            forward.instructions[0],
            RegExpInstruction::numbered_backreference(1, CaseFolding::Sensitive)
        );

        for pattern in [
            r"\k<a>(?<=>)a",
            r"(?<=>)\k<a>",
            r"\k<a>(?<!a)a",
            r"(?<!a>)\k<a>",
        ] {
            RegExpProgram::compile(pattern, "").unwrap_or_else(|error| {
                panic!("{pattern} should compile: {error:?}");
            });
        }
    }

    #[test]
    fn digit_escape_compiles_to_exact_ascii_bitmap() {
        let program = compile(r"\d");
        let instruction = program.instructions[0];
        assert_eq!(instruction.opcode, REGEXP_OPCODE_POSITIVE_ASCII_CLASS);
        for member in b'0'..=b'9' {
            assert!(instruction.positive_ascii_class_contains(member));
        }
        for member in [b'/', b':', b'A', 0, 127] {
            assert!(!instruction.positive_ascii_class_contains(member));
        }
        assert_eq!(program.encode().len(), 2 * REGEXP_INSTRUCTION_WIDTH);
        assert_eq!(
            &program.encode()[8..16],
            &instruction.operand0.to_le_bytes()
        );
        assert_eq!(
            &program.encode()[16..24],
            &instruction.operand1.to_le_bytes()
        );
    }

    #[test]
    fn digit_escape_integrates_with_greedy_quantifiers() {
        let digit_class = RegExpInstruction::positive_ascii_class(((1_u64 << 10) - 1) << 48, 0);
        assert_eq!(
            compile(r"\d+").instructions,
            vec![
                digit_class,
                RegExpInstruction::split(0, 2),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn captures_emit_boundaries_around_existing_quantified_atom_programs() {
        let digit_class = RegExpInstruction::positive_ascii_class(((1_u64 << 10) - 1) << 48, 0);
        let program = compile(r"(\d+)");
        assert_eq!(program.capture_count, 1);
        assert_eq!(
            program.instructions,
            vec![
                RegExpInstruction::clear_capture_range(1, 2),
                RegExpInstruction::capture_start(1),
                digit_class,
                RegExpInstruction::split(2, 4),
                RegExpInstruction::capture_end(1),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn numbers_sequential_captures_and_keeps_surrounding_terms() {
        let program = compile(r"x(a)(\d)y");
        assert_eq!(program.capture_count, 2);
        assert_eq!(
            program.instructions,
            vec![
                RegExpInstruction::literal_ascii(b'x'),
                RegExpInstruction::clear_capture_range(1, 2),
                RegExpInstruction::capture_start(1),
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::capture_end(1),
                RegExpInstruction::clear_capture_range(2, 3),
                RegExpInstruction::capture_start(2),
                RegExpInstruction::positive_ascii_class(((1_u64 << 10) - 1) << 48, 0),
                RegExpInstruction::capture_end(2),
                RegExpInstruction::literal_ascii(b'y'),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn supports_empty_groups_and_alternatives() {
        assert_eq!(compile("()").capture_count, 1);
        assert_eq!(compile("(?:)").capture_count, 0);
        assert_eq!(compile("(|)").capture_count, 1);
        assert_eq!(compile("|a").capture_count, 0);
        assert_eq!(compile("a|").capture_count, 0);
        assert_eq!(compile("(?:a)").capture_count, 0);
        assert_eq!(compile("(?<x>a)").named_groups[0].name, "x");
    }

    #[test]
    fn lowers_ordered_alternation_and_nested_capture_ranges() {
        assert_eq!(
            compile("(a|b)").instructions,
            vec![
                RegExpInstruction::clear_capture_range(1, 2),
                RegExpInstruction::capture_start(1),
                RegExpInstruction::split(3, 5),
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::jump(6),
                RegExpInstruction::literal_ascii(b'b'),
                RegExpInstruction::capture_end(1),
                RegExpInstruction::accept(),
            ]
        );
        let nested = compile("((a))");
        assert_eq!(nested.capture_count, 2);
        assert_eq!(
            nested.instructions[0],
            RegExpInstruction::clear_capture_range(1, 3)
        );
        assert_eq!(
            nested.instructions[2],
            RegExpInstruction::clear_capture_range(2, 3)
        );
    }

    #[test]
    fn compiles_capture_alternation_and_quantifier_targets() {
        let first = compile("((1)|(12))((3)|(23))");
        assert_eq!(first.capture_count, 6);
        assert!(first
            .instructions
            .iter()
            .any(|i| *i == RegExpInstruction::clear_capture_range(1, 4)));
        let star = compile("(aa|aabaac|ba|b|c)*");
        assert_eq!(star.capture_count, 1);
        assert_eq!(
            star.instructions[0],
            RegExpInstruction::split(1, star.instructions.len() - 1)
        );
        let nested_star = compile("(z)((a+)?(b+)?(c))*");
        assert_eq!(nested_star.capture_count, 5);
        assert!(nested_star
            .instructions
            .iter()
            .any(|i| *i == RegExpInstruction::clear_capture_range(2, 6)));
    }

    #[test]
    fn nullable_quantifier_progress() {
        fn progress_pairs(program: &RegExpProgram) -> Vec<(usize, usize)> {
            program
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(check_pc, instruction)| {
                    (instruction.opcode == REGEXP_OPCODE_PROGRESS_CHECK)
                        .then_some((instruction.operand0 as usize, check_pc))
                })
                .collect()
        }

        let exact = compile("(a?b??)*");
        let exact_pairs = progress_pairs(&exact);
        assert_eq!(exact_pairs.len(), 1);
        let (split_pc, check_pc) = exact_pairs[0];
        let split = exact.instructions[split_pc];
        let check = exact.instructions[check_pc];
        assert_eq!(split.opcode, REGEXP_OPCODE_PROGRESS_SPLIT);
        assert_eq!(split.operand1 & 1, QuantifierPreference::Greedy.word());
        assert_eq!((split.operand1 >> 1) as usize, check_pc + 1);
        assert_eq!(check.operand0 as usize, split_pc);
        assert_eq!(check.operand1 as usize, split_pc);

        let lazy = compile("(?:a?)*?");
        let lazy_pairs = progress_pairs(&lazy);
        assert_eq!(lazy_pairs.len(), 1);
        let (lazy_split_pc, lazy_check_pc) = lazy_pairs[0];
        assert_eq!(
            lazy.instructions[lazy_split_pc].operand1 & 1,
            QuantifierPreference::Lazy.word()
        );
        assert_eq!(
            lazy.instructions[lazy_check_pc].operand1 as usize,
            lazy_split_pc
        );

        let finite = compile("(){1,3}");
        assert!(progress_pairs(&finite).is_empty());
        assert_eq!(finite.instructions[0], RegExpInstruction::repeat_begin(0));
        assert_eq!(
            finite
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == REGEXP_OPCODE_CAPTURE_START)
                .count(),
            1,
            "required and optional iterations share one capture-clearing body"
        );
        ValidatedRegExpProgram::from_program(&finite).expect("certified nullable count lifecycle");

        let nested = compile("(?:(?:a?)*)*");
        let nested_pairs = progress_pairs(&nested);
        assert_eq!(nested_pairs.len(), 2);
        assert_ne!(nested_pairs[0].0, nested_pairs[1].0);
        for (split_pc, check_pc) in nested_pairs {
            assert_eq!(nested.instructions[check_pc].operand0 as usize, split_pc);
        }

        let reverse = compile("(?<=(?:a?)*)b");
        let reverse_pairs = progress_pairs(&reverse);
        assert_eq!(reverse_pairs.len(), 1);
        let (reverse_split_pc, reverse_check_pc) = reverse_pairs[0];
        assert_eq!(
            reverse.instructions[reverse_check_pc].operand0 as usize,
            reverse_split_pc
        );
        assert_eq!(
            reverse.instructions[reverse_check_pc].operand1 as usize,
            reverse_split_pc
        );
    }

    #[test]
    fn reports_malformed_group_delimiters_as_invalid_syntax() {
        for pattern in ["(a", "a)"] {
            let error = RegExpProgram::compile(pattern, "").expect_err(pattern);
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
        }
    }

    #[test]
    fn non_unicode_property_syntax_is_an_identity_escape() {
        let program = compile(r"\p{Decimal_Number}");
        assert_eq!(
            program.instructions[0],
            RegExpInstruction::literal_ascii(b'p')
        );
        assert_eq!(
            program.instructions.last(),
            Some(&RegExpInstruction::accept())
        );
    }

    #[test]
    fn annex_b_malformed_hex_escape_is_an_identity_escape() {
        assert_eq!(
            compile(r"\x").instructions,
            vec![
                RegExpInstruction::literal_ascii(b'x'),
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            RegExpProgram::compile(r"\x", "u").unwrap_err().kind,
            RegExpCompileErrorKind::InvalidSyntax
        );
        for (pattern, expected) in [(r"\xa", b"xa".as_slice()), (r"\ua", b"ua".as_slice())] {
            let program = compile(pattern);
            let literals = program.instructions[..program.instructions.len() - 1]
                .iter()
                .map(|instruction| instruction.operand0 as u8)
                .collect::<Vec<_>>();
            assert_eq!(literals, expected);
        }
        assert_eq!(
            RegExpProgram::compile(r"\u", "u").unwrap_err().kind,
            RegExpCompileErrorKind::InvalidSyntax
        );
    }

    #[test]
    fn word_escapes_compile_to_ascii_class_opcodes() {
        let word_bitmap_low = ((1_u64 << 10) - 1) << 48;
        let word_bitmap_high = ((1_u64 << 26) - 1) << 1 | ((1_u64 << 26) - 1) << 33 | (1_u64 << 31);
        assert_eq!(
            compile(r"\w").instructions,
            vec![
                RegExpInstruction::positive_ascii_class(word_bitmap_low, word_bitmap_high),
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            compile(r"\W").instructions,
            vec![
                RegExpInstruction::negative_ascii_class(word_bitmap_low, word_bitmap_high),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn complemented_character_class_escapes_compile() {
        let non_digit = compile(r"\D").instructions[0];
        assert_eq!(non_digit.opcode, REGEXP_OPCODE_NEGATIVE_ASCII_CLASS);
        assert_eq!(
            compile(r"\S").instructions[0],
            RegExpInstruction::not_whitespace()
        );
    }

    #[test]
    fn whitespace_escape_compiles_to_zero_operand_opcode() {
        let program = compile(r"\s");
        assert_eq!(
            program.instructions,
            vec![RegExpInstruction::whitespace(), RegExpInstruction::accept()]
        );
        assert_eq!(program.instructions[0].operand0, 0);
        assert_eq!(program.instructions[0].operand1, 0);
        assert_eq!(program.instructions[0].opcode, REGEXP_OPCODE_WHITESPACE);
    }

    #[test]
    fn line_assertions_compile_to_zero_width_opcodes() {
        let program = compile("^a$");
        assert_eq!(
            program.instructions,
            vec![
                RegExpInstruction::assert_start(),
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::assert_end(),
                RegExpInstruction::accept(),
            ]
        );
    }

    #[test]
    fn dot_compiles_to_a_canonical_zero_operand_instruction() {
        let program = compile(".");
        assert_eq!(
            program.instructions,
            vec![RegExpInstruction::dot(), RegExpInstruction::accept()]
        );
        assert_eq!(program.instructions[0].opcode, REGEXP_OPCODE_DOT);
        assert_eq!(program.instructions[0].operand0, 0);
        assert_eq!(program.instructions[0].operand1, 0);
    }

    #[test]
    fn whitespace_escape_integrates_with_a3_t4_pattern() {
        let program = compile(r"([Nn]?ever|([Nn]othing\s{1,}))more");
        assert_eq!(program.capture_count, 2);
        assert!(program
            .instructions
            .iter()
            .any(|instruction| instruction.opcode == REGEXP_OPCODE_WHITESPACE));
    }

    #[test]
    fn escaped_braces_can_still_be_postfix_quantified() {
        assert_eq!(
            compile(r"\{{2}").instructions,
            vec![
                RegExpInstruction::repeat_begin(0),
                RegExpInstruction::repeat_guard(3, 0, QuantifierPreference::Greedy),
                RegExpInstruction::literal_ascii(b'{'),
                RegExpInstruction::repeat_end(0),
                RegExpInstruction::repeat_exit(0),
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            compile(r"\}{1,2}").instructions.len(),
            6,
            "escaped closing brace remains an atom for postfix quantification"
        );
    }

    #[test]
    fn encodes_instructions_as_deterministic_little_endian_words() {
        let program = compile("[?@]");
        let encoded = program.encode();
        assert_eq!(encoded.len(), 2 * REGEXP_INSTRUCTION_WIDTH);
        assert_eq!(
            &encoded[0..8],
            &REGEXP_OPCODE_POSITIVE_ASCII_CLASS.to_le_bytes()
        );
        let expected_low = 1_u64 << 63;
        assert_eq!(&encoded[8..16], &expected_low.to_le_bytes());
        assert_eq!(&encoded[16..24], &1_u64.to_le_bytes());
        assert_eq!(&encoded[24..32], &REGEXP_OPCODE_ACCEPT.to_le_bytes());
        assert_eq!(&encoded[32..48], &[0; 16]);
    }

    #[test]
    fn quantifiers_encode_ordered_backtracking_programs() {
        assert_eq!(
            compile("a?").instructions,
            vec![
                RegExpInstruction::split(1, 2),
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::accept()
            ]
        );
        assert_eq!(
            compile("a??").instructions,
            vec![
                RegExpInstruction::split(2, 1),
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::accept()
            ]
        );
        assert_eq!(
            compile("a+").instructions,
            vec![
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::split(0, 2),
                RegExpInstruction::accept()
            ]
        );
        assert_eq!(
            compile("a+?").instructions,
            vec![
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::split(2, 0),
                RegExpInstruction::accept()
            ]
        );
        assert_eq!(
            compile("a{0}").instructions,
            vec![RegExpInstruction::accept()]
        );
        assert_eq!(
            compile("a{1}").instructions,
            vec![
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::accept()
            ]
        );
        for (source, minimum, maximum, preference) in [
            ("a{2}", 2, Some(2), QuantifierPreference::Greedy),
            ("a{2,4}", 2, Some(4), QuantifierPreference::Greedy),
            ("a{2,}", 2, None, QuantifierPreference::Greedy),
            ("a{2,4}?", 2, Some(4), QuantifierPreference::Lazy),
        ] {
            let program = compile(source);
            assert_eq!(
                program.repeat_bounds,
                vec![RegExpRepeatBounds::new(
                    RegExpNatural::from_u64(minimum),
                    maximum.map_or(RegExpRepeatMaximum::Unbounded, |value| {
                        RegExpRepeatMaximum::Finite(RegExpNatural::from_u64(value))
                    }),
                )
                .unwrap()]
            );
            assert_eq!(
                program.instructions,
                vec![
                    RegExpInstruction::repeat_begin(0),
                    RegExpInstruction::repeat_guard(3, 0, preference),
                    RegExpInstruction::literal_ascii(b'a'),
                    RegExpInstruction::repeat_end(0),
                    RegExpInstruction::repeat_exit(0),
                    RegExpInstruction::accept(),
                ],
                "{source}"
            );
        }
    }

    #[test]
    fn quantifier_errors_are_precise_and_bounded() {
        for pattern in ["a{4,2}", "a**", "a+*", "*", "+", "?"] {
            assert_eq!(
                RegExpProgram::compile(pattern, "").expect_err(pattern).kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
        }
        // The original decimal is too large for any consuming match; that
        // proof admits never-match without admitting a saturated nullable count.
        assert!(
            RegExpProgram::compile("a{184467440737095516160}", "").is_ok(),
            "overflowed minimum compiles to never-match"
        );
        assert!(
            RegExpProgram::compile("b{9007199254740991}", "u").is_ok(),
            "MAX_SAFE_INTEGER minimum compiles"
        );
        assert!(
            RegExpProgram::compile("b{0,9007199254740991}", "").is_ok(),
            "MAX_SAFE_INTEGER maximum remains an exact finite counted bound"
        );
        assert_eq!(
            RegExpProgram::compile("a{184467440737095516160,5}", "")
                .expect_err("reversed-overflow")
                .kind,
            RegExpCompileErrorKind::InvalidSyntax
        );
        let counted =
            RegExpProgram::compile("a{32768}", "").expect("bounds no longer expand source bodies");
        assert_eq!(counted.instructions.len(), 6);
        let nullable =
            RegExpProgram::compile("(){4294967296}", "").expect("exact nullable minimum");
        assert_eq!(nullable.repeat_bounds[0].minimum().digits(), b"4294967296");
        ValidatedRegExpProgram::from_program(&nullable).expect("source-sized exact descriptor");
    }

    #[test]
    fn incomplete_braces_are_legacy_literals() {
        for pattern in ["a{b}", "a{", "a{,2}", "a{1", "a{1,x}", "a{1,2"] {
            let program = compile(pattern);
            let literals = program.instructions[..program.instructions.len() - 1]
                .iter()
                .map(|instruction| instruction.operand0 as u8)
                .collect::<Vec<_>>();
            assert_eq!(literals, pattern.as_bytes(), "{pattern}");
        }
        for pattern in ["}", "a}", "a{}"] {
            let program = compile(pattern);
            let literals = program.instructions[..program.instructions.len() - 1]
                .iter()
                .map(|instruction| instruction.operand0 as u8)
                .collect::<Vec<_>>();
            assert_eq!(literals, pattern.as_bytes(), "{pattern}");
        }
    }

    #[test]
    fn braced_quantifiers_require_a_preceding_atom() {
        for pattern in ["{1}", "{1,}", "{1,2}", "{4,2}"] {
            assert_eq!(
                RegExpProgram::compile(pattern, "").expect_err(pattern).kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
        }
        assert_eq!(
            RegExpProgram::compile("{", "u")
                .expect_err("Unicode brace")
                .kind,
            RegExpCompileErrorKind::InvalidSyntax
        );
    }

    #[test]
    fn incomplete_braces_after_quantifiers_remain_literals() {
        for pattern in ["a{1}{b}", "a?{b}", "a*{", "a+{1,x}"] {
            compile(pattern);
        }
        for pattern in ["a{1}{2}", "a?{1}", "a*{1,}"] {
            assert_eq!(
                RegExpProgram::compile(pattern, "").expect_err(pattern).kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
        }
    }

    #[test]
    fn preserves_global_and_sticky_flags_in_either_order() {
        for flags in ["gy", "yg"] {
            let program = RegExpProgram::compile("a", flags).expect("flags should compile");
            assert_eq!(
                program.flags,
                RegExpFlags {
                    has_indices: false,
                    global: true,
                    ignore_case: false,
                    multiline: false,
                    dot_all: false,
                    sticky: true,
                    unicode_mode: RegExpUnicodeMode::Legacy,
                }
            );
            assert_eq!(
                program.instructions,
                vec![
                    RegExpInstruction::literal_ascii(b'a'),
                    RegExpInstruction::accept()
                ]
            );
            assert_eq!(program.capture_count, 0);
        }
    }

    #[test]
    fn unicode_literals_and_escaped_surrogates_are_code_points() {
        let paired = RegExpProgram::compile(r"\uD842\uDFB7", "u").unwrap();
        assert_eq!(
            paired.instructions[0],
            RegExpInstruction::literal_code_point(0x20BB7)
        );
        let lone = RegExpProgram::compile(r"\uDFFF", "u").unwrap();
        assert_eq!(
            lone.instructions[0],
            RegExpInstruction::literal_code_point(0xDFFF)
        );
        let direct = RegExpProgram::compile("𠮷", "u").unwrap();
        assert_eq!(
            direct.instructions[0],
            RegExpInstruction::literal_code_point(0x20BB7)
        );
    }

    #[test]
    fn malformed_unicode_escape_is_invalid_syntax() {
        for pattern in [r"\u12", r"\u12G4"] {
            assert_eq!(
                RegExpProgram::compile(pattern, "u").unwrap_err().kind,
                RegExpCompileErrorKind::InvalidSyntax
            );
        }
    }

    #[test]
    fn distinguishes_unicode_and_unicode_sets_flags() {
        let unicode = RegExpProgram::compile("𠮷", "u").unwrap();
        assert_eq!(unicode.flags.unicode_mode, RegExpUnicodeMode::Unicode);
        let unicode_sets = RegExpProgram::compile("𠮷", "v").unwrap();
        assert_eq!(
            unicode_sets.flags.unicode_mode,
            RegExpUnicodeMode::UnicodeSets
        );
        assert_eq!(unicode.instructions, unicode_sets.instructions);
    }

    #[test]
    fn compiles_unicode_properties_into_the_range_pool() {
        for flags in ["u", "v"] {
            let program = RegExpProgram::compile(r"\p{ASCII}\P{ASCII}", flags).unwrap();
            assert_eq!(
                program.instructions,
                vec![
                    RegExpInstruction::code_point_range_set(0, 1, false),
                    RegExpInstruction::code_point_range_set(1, 1, false),
                    RegExpInstruction::accept(),
                ]
            );
            assert_eq!(program.ranges, vec![(0, 0x7f), (0x80, 0x10ffff)]);
            let encoded = program.encode();
            assert_eq!(&encoded[..8], &REGEXP_OPCODE_UNICODE_PROPERTY.to_le_bytes());
            assert_eq!(
                encoded.len(),
                program.instructions.len() * REGEXP_INSTRUCTION_WIDTH
                    + program.ranges.len() * REGEXP_RANGE_ENTRY_WIDTH
            );
            assert_eq!(
                &encoded[encoded.len() - 8..],
                &[0x80, 0, 0, 0, 0xff, 0xff, 0x10, 0]
            );
        }
    }

    #[test]
    fn resolves_general_category_and_script_property_escapes() {
        let letters = RegExpProgram::compile(r"\p{L}", "u").unwrap();
        assert!(ranges_contain(&letters.ranges, u32::from(b'a')));
        assert!(ranges_contain(&letters.ranges, 0x00e9));
        assert!(!ranges_contain(&letters.ranges, u32::from(b'0')));

        let han = RegExpProgram::compile(r"\p{Script=Han}", "u").unwrap();
        assert!(ranges_contain(&han.ranges, 0x4e00));
        assert!(!ranges_contain(&han.ranges, u32::from(b'a')));

        let not_han = RegExpProgram::compile(r"\P{Script=Han}", "u").unwrap();
        assert!(!ranges_contain(&not_han.ranges, 0x4e00));
        assert!(ranges_contain(&not_han.ranges, u32::from(b'a')));
    }

    #[test]
    fn rejects_unknown_and_malformed_unicode_properties() {
        for pattern in [r"\p{NotAProperty}", r"\p{script=Han}", r"\p{Foo=Bar}"] {
            assert_eq!(
                RegExpProgram::compile(pattern, "u").unwrap_err().kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
        }
        for pattern in [r"\p", r"\pASCII", r"\p{ASCII", r"\p{}"] {
            assert_eq!(
                RegExpProgram::compile(pattern, "v").unwrap_err().kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
        }
        assert!(RegExpProgram::compile(r"\p{ASCII}", "").is_ok());
    }

    #[test]
    fn compiles_unicode_sets_set_operations() {
        let intersection = RegExpProgram::compile(r"[\p{ASCII}&&\p{L}]", "v").unwrap();
        assert!(ranges_contain(&intersection.ranges, u32::from(b'a')));
        assert!(!ranges_contain(&intersection.ranges, 0x00e9));
        assert!(!ranges_contain(&intersection.ranges, u32::from(b'0')));

        let difference = RegExpProgram::compile(r"[[a-f]--[c-d]]", "v").unwrap();
        assert_eq!(difference.ranges, vec![(0x61, 0x62), (0x65, 0x66)]);

        let chained_intersection = RegExpProgram::compile(r"[[a-c]&&[b-d]&&[c-e]]", "v").unwrap();
        assert_eq!(chained_intersection.ranges, vec![(0x63, 0x63)]);

        let chained_subtraction = RegExpProgram::compile(r"[[a-c]--b--c]", "v").unwrap();
        assert_eq!(chained_subtraction.ranges, vec![(0x61, 0x61)]);
    }

    #[test]
    fn unicode_sets_expression_shape_is_closed() {
        assert!(RegExpProgram::compile("[]", "v").unwrap().ranges.is_empty());
        assert_eq!(
            RegExpProgram::compile("[abc]", "v").unwrap().ranges,
            vec![(0x61, 0x63)]
        );
        assert_eq!(
            RegExpProgram::compile("[a-c]", "v").unwrap().ranges,
            vec![(0x61, 0x63)]
        );

        let invalid = [
            ("[a&&b--c]", 5),
            ("[a--b&&c]", 5),
            ("[ab&&c]", 3),
            ("[a&&bc]", 5),
            ("[&&a]", 1),
            ("[a&&]", 2),
            ("[--a]", 1),
            ("[a--]", 2),
            ("[a&&&b]", 4),
            ("[a&&&]", 4),
        ];
        for (pattern, offset) in invalid {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("invalid ClassSetExpression shape must be rejected");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(
                error.rule,
                Some(SyntaxRule::ClassSetExpression),
                "{pattern}"
            );
            assert_eq!(error.offset, offset, "{pattern}");
        }
    }

    #[test]
    fn unicode_sets_operands_validate_class_set_characters() {
        let empty_intersection = RegExpProgram::compile(r"[a&&\&]", "v").unwrap();
        assert!(empty_intersection.ranges.is_empty());

        let escaped_intersection = RegExpProgram::compile(r"[\&&&\&]", "v").unwrap();
        assert_eq!(escaped_intersection.ranges, vec![(0x26, 0x26)]);

        // Every member may occur raw when it is not doubled. Prefixing `a`
        // keeps `^` from being parsed as the class-negation marker.
        let reserved_double_members = [
            b'&', b'!', b'#', b'$', b'%', b'*', b'+', b',', b'.', b':', b';', b'<', b'=', b'>',
            b'?', b'@', b'^', b'`', b'~',
        ];
        assert_eq!(reserved_double_members.len(), 19);
        for punctuator in reserved_double_members {
            let pattern = format!("[a{}]", punctuator as char);
            let program = RegExpProgram::compile(&pattern, "v")
                .unwrap_or_else(|error| panic!("raw singleton `{pattern}` must compile: {error}"));
            assert!(
                ranges_contain(&program.ranges, u32::from(punctuator)),
                "`{pattern}` must contain `{}`",
                punctuator as char
            );
        }

        for punctuator in reserved_double_members {
            let pattern = format!("[a{0}{0}]", punctuator as char);
            let error = RegExpProgram::compile(&pattern, "v")
                .expect_err("a reserved double punctuator must not become two operands");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(
                error.rule,
                Some(if punctuator == b'&' {
                    SyntaxRule::ClassSetExpression
                } else {
                    SyntaxRule::ClassSetCharacter
                }),
                "{pattern}"
            );
        }

        for (pattern, offset) in [("[a&&-]", 4), ("[a---]", 4), ("[!!]", 1)] {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("raw UnicodeSets syntax must be rejected");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ClassSetCharacter), "{pattern}");
            assert_eq!(error.offset, offset, "{pattern}");
        }

        for pattern in ["[a(]", "[a)]", "[a{]", "[a}]", "[a/]", "[a-]", "[a|]"] {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("raw ClassSetSyntaxCharacter must be rejected");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ClassSetCharacter), "{pattern}");
            assert_eq!(error.offset, 2, "{pattern}");
        }

        let nul = RegExpProgram::compile(r"[\0a]", "v").unwrap();
        assert!(ranges_contain(&nul.ranges, 0));
        assert!(ranges_contain(&nul.ranges, u32::from(b'a')));
        for pattern in [r"[\00]", r"[\01]", r"[\09]"] {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("`\\0` followed by DecimalDigit is not CharacterEscape");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ClassSetCharacter), "{pattern}");
            assert_eq!(error.offset, 1, "{pattern}");
        }
    }

    #[test]
    fn unicode17_delta_covers_new_scripts_and_range_changes() {
        // Names added in Unicode 17 resolve with their exact range sets.
        for (pattern, witness) in [
            (r"\p{Script=Beria_Erfe}", 0x16ea0),
            (r"\p{Script=Berf}", 0x16ed3),
            (r"\p{scx=Tayo}", 0x1e6c0),
            (r"\p{Script_Extensions=Tolong_Siki}", 0x11db0),
            (r"\p{Script=Sidt}", 0x10940),
        ] {
            let ranges = unicode_property_ranges(
                pattern
                    .strip_prefix(r"\p{")
                    .unwrap()
                    .strip_suffix('}')
                    .unwrap(),
            )
            .unwrap_or_else(|| panic!("{pattern} should resolve"));
            assert!(
                ranges.iter().any(|&(s, e)| s <= witness && witness <= e),
                "{pattern} should cover U+{witness:05X}: {ranges:?}"
            );
        }
        // Unicode 16 -> 17 membership changes apply in both directions.
        let alphabetic = unicode_property_ranges("Alphabetic").expect("Alphabetic resolves");
        assert!(alphabetic.iter().any(|&(s, e)| s <= 0x88f && 0x88f <= e));
        let cased = unicode_property_ranges("Cased").expect("Cased resolves");
        assert!(!cased.iter().any(|&(s, e)| s <= 0x295 && 0x295 <= e));
        // The delta never resurrects invalid names or drops valid aliases.
        assert!(unicode_property_ranges("Script=Letter").is_none());
        assert!(unicode_property_ranges("space").is_some());
        assert!(unicode_property_ranges("No_Such_Property").is_none());
    }

    #[test]
    fn unicode_sets_validates_class_strings_before_finite_lowering() {
        for pattern in [
            r"[\q{}]",
            r"[\q{a|b}]",
            r"[\q{|a||b|}]",
            r"[\q{\&|\}|\|}]",
            r"[\q{a|b}&&a]",
            r"[^\q{a}]",
            r"[^\q{ab}&&a]",
            r"[^a--\q{ab}]",
            r"[\q{\0a}]",
            r"[\q{a}]b",
            r"([\q{a}])",
            r"(?:[\q{a}]|)*",
        ] {
            RegExpProgram::compile(pattern, "v")
                .unwrap_or_else(|error| panic!("legal finite class string `{pattern}`: {error}"));
        }

        for pattern in [r"[\q]", r"[\q{a]", r"[\q{a", r"[\q{a\}]"] {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("malformed class-string delimiters must be syntax errors");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(
                error.rule,
                Some(SyntaxRule::ClassStringDisjunction),
                "{pattern}"
            );
            assert_eq!(error.offset, 1, "{pattern}");
        }

        for (pattern, offset) in [(r"[\q{a!!b}]", 5), (r"[\q{\d}]", 4), (r"[\q{/}]", 4)] {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("malformed class-string contents must be syntax errors");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ClassSetCharacter), "{pattern}");
            assert_eq!(error.offset, offset, "{pattern}");
        }

        for pattern in [r"[\q{\00}]", r"[\q{\01}]", r"[\q{\09}]"] {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("class-string characters apply the `\\0` lookahead restriction");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ClassSetCharacter), "{pattern}");
            assert_eq!(error.offset, 4, "{pattern}");
        }
    }

    #[test]
    fn unicode_sets_finite_string_algebra() {
        fn finite_atom(pattern: &str) -> FiniteClassSetAtom {
            let mut parsed =
                parse_pattern(pattern.as_bytes(), RegExpUnicodeMode::UnicodeSets, false).unwrap();
            let term = parsed
                .alternatives
                .pop()
                .and_then(|mut sequence| sequence.pop())
                .expect("one finite class atom");
            match term {
                ParsedTerm::Quantified {
                    atom: ParsedAtom::FiniteClassSet(atom),
                    ..
                } => atom,
                _ => panic!("`{pattern}` did not retain a finite class-set atom"),
            }
        }

        let singleton = RegExpProgram::compile(r"[\q{a}&&a]", "v").unwrap();
        let ordinary = RegExpProgram::compile("[a&&a]", "v").unwrap();
        assert_eq!(singleton.instructions, ordinary.instructions);
        assert_eq!(singleton.ranges, ordinary.ranges);

        let subtracted = finite_atom(r"[\q{ab|cd|a}--\q{cd|a}]");
        assert_eq!(
            subtracted.multi_code_point_strings,
            vec![vec![
                RegExpInstruction::literal_code_point(u32::from(b'a')),
                RegExpInstruction::literal_code_point(u32::from(b'b')),
            ]]
        );
        assert_eq!(subtracted.singleton.operand1, 0);
        assert!(!subtracted.contains_empty);

        let direct = finite_atom(r"[\q{ab|abc|ab|}]");
        let nested = finite_atom(r"[[\q{ab|abc|ab|}]&&[\q{ab|abc|}]]");
        assert_eq!(direct, nested);
        assert_eq!(
            direct
                .multi_code_point_strings
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![3, 2]
        );
        assert!(direct.contains_empty);
        assert!(atom_nullable(&ParsedAtom::FiniteClassSet(direct.clone())));

        let mut forward = Vec::new();
        ProgramLowerer::new(&mut forward, &mut Vec::new(), 0, &[])
            .finite_class_set_atom(&direct, RegExpMatchDirection::Forward)
            .unwrap();
        let mut reverse = Vec::new();
        ProgramLowerer::new(&mut reverse, &mut Vec::new(), 0, &[])
            .finite_class_set_atom(&direct, RegExpMatchDirection::Reverse)
            .unwrap();
        let literals = |instructions: &[RegExpInstruction]| {
            instructions
                .iter()
                .filter(|instruction| {
                    instruction.opcode == REGEXP_OPCODE_LITERAL_CODE_POINT
                        || instruction.opcode == REGEXP_OPCODE_LITERAL_ASCII
                })
                .map(|instruction| instruction.operand0)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            literals(&forward),
            [b'a', b'b', b'c', b'a', b'b'].map(u64::from)
        );
        assert_eq!(
            literals(&reverse),
            [b'c', b'b', b'a', b'b', b'a'].map(u64::from)
        );
        assert!(forward
            .iter()
            .all(|instruction| instruction.opcode != REGEXP_OPCODE_PROGRESS_SPLIT));

        let negated = RegExpProgram::compile(r"[^\q{ab}--\q{ab}]", "v")
            .expect_err("static MayContainStrings is independent of the empty product");
        assert_eq!(
            negated.rule,
            Some(SyntaxRule::NegatedClassMayContainStrings)
        );

        let keycaps = finite_atom(r"\p{Emoji_Keycap_Sequence}");
        assert_eq!(keycaps.multi_code_point_strings.len(), 12);
        assert!(keycaps.multi_code_point_strings.iter().all(|string| {
            string.len() == 3 && string[1].operand0 == 0xFE0F && string[2].operand0 == 0x20E3
        }));
        assert!(!keycaps.contains_empty);

        let keycaps_v = RegExpProgram::compile(r"\p{Emoji_Keycap_Sequence}", "v").unwrap();
        let keycaps_iv = RegExpProgram::compile(r"\p{Emoji_Keycap_Sequence}", "iv").unwrap();
        assert_eq!(keycaps_iv.instructions, keycaps_v.instructions);
        assert_eq!(keycaps_iv.ranges, keycaps_v.ranges);

        let keycap_intersection = finite_atom(r"[\p{Emoji_Keycap_Sequence}&&\q{0\uFE0F\u20E3|x}]");
        assert_eq!(keycap_intersection.multi_code_point_strings.len(), 1);
        assert_eq!(
            keycap_intersection.multi_code_point_strings[0]
                .iter()
                .map(|instruction| instruction.operand0)
                .collect::<Vec<_>>(),
            vec![u64::from(b'0'), 0xFE0F, 0x20E3]
        );

        let keycap_difference = finite_atom(r"[\p{Emoji_Keycap_Sequence}--\q{0\uFE0F\u20E3}]");
        assert_eq!(keycap_difference.multi_code_point_strings.len(), 11);

        let negated_keycaps = RegExpProgram::compile(r"[^\p{Emoji_Keycap_Sequence}]", "v")
            .expect_err("a finite property of strings still has string cardinality");
        assert_eq!(
            negated_keycaps.rule,
            Some(SyntaxRule::NegatedClassMayContainStrings)
        );

        // Every property of strings lowers finitely; the ZWJ and full
        // RGI sets are the largest (12k/18k instructions with the
        // single-copy `+` loop) and size the matcher-program cap.
        for (property_name, expected_strings) in [
            ("Basic_Emoji", 207),
            ("RGI_Emoji_Flag_Sequence", 259),
            ("RGI_Emoji_Modifier_Sequence", 665),
            ("RGI_Emoji_Tag_Sequence", 3),
            ("RGI_Emoji_ZWJ_Sequence", 1614),
            ("RGI_Emoji", 2760),
        ] {
            let pattern = format!(r"\p{{{property_name}}}");
            let atom = finite_atom(&pattern);
            assert_eq!(
                atom.multi_code_point_strings.len(),
                expected_strings,
                "{property_name}"
            );
        }

        let oversized = format!(r"[\q{{{}}}]", "a".repeat(REGEXP_MAX_INSTRUCTIONS + 1));
        let error = RegExpProgram::compile(&oversized, "v")
            .expect_err("finite class strings remain under the matcher-program cap");
        assert_eq!(error.kind, RegExpCompileErrorKind::UnsupportedFeature);
    }

    #[test]
    fn unicode_string_property_names_are_strict_and_closed() {
        for (property_name, expected_strings) in [
            ("Emoji_Keycap_Sequence", 12),
            ("Basic_Emoji", 207),
            ("RGI_Emoji_Flag_Sequence", 259),
            ("RGI_Emoji_Modifier_Sequence", 665),
            ("RGI_Emoji_Tag_Sequence", 3),
            ("RGI_Emoji_ZWJ_Sequence", 1614),
            ("RGI_Emoji", 2760),
        ] {
            let source = format!(r"\p{{{property_name}}}");
            let mut cursor = 0;
            let value = parse_unicode_property_of_strings(
                source.as_bytes(),
                &mut cursor,
                CaseFolding::Sensitive,
            )
            .unwrap()
            .expect("recognized finite property of strings");
            assert_eq!(cursor, source.len(), "{property_name}");
            let finite = value.finite;
            assert_eq!(finite.strings.len(), expected_strings, "{property_name}");
        }

        for source in [
            br"\p{emoji_keycap_sequence}".as_slice(),
            br"\p{Emoji_Keycap_Sequence_}".as_slice(),
            br"\p{RGIEmoji}".as_slice(),
        ] {
            let mut cursor = 0;
            assert!(
                parse_unicode_property_of_strings(source, &mut cursor, CaseFolding::Sensitive)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(cursor, 0);
        }
    }

    #[test]
    fn unicode_sets_class_strings_do_not_bypass_enclosing_class_validation() {
        let invalid = [
            (r"[\q{a}", SyntaxRule::UnclosedCharacterClass, 0),
            (r"[\q{a}&&]", SyntaxRule::ClassSetExpression, 6),
            (r"[\q{a}-b]", SyntaxRule::ClassRangeBound, 1),
            (r"[a-\q{b}]", SyntaxRule::ClassRangeBound, 1),
            (r"[\q{a}!!]", SyntaxRule::ClassSetCharacter, 6),
            (r"[\q{a}])", SyntaxRule::StrayClosingParenthesis, 7),
            (r"[\q{a}](", SyntaxRule::UnclosedGroup, 7),
            (r"[\q{a}]\k<missing>", SyntaxRule::UnknownGroupName, 7),
            (r"(?:[\q{a}]|)*)", SyntaxRule::StrayClosingParenthesis, 13),
            (r"(?:[\q{a}]|)*(", SyntaxRule::UnclosedGroup, 13),
            (
                r"(?:[\q{a}]|)*\k<missing>",
                SyntaxRule::UnknownGroupName,
                13,
            ),
            (r"[^\q{ab}]", SyntaxRule::NegatedClassMayContainStrings, 0),
            (r"[^\q{}]", SyntaxRule::NegatedClassMayContainStrings, 0),
            (
                r"[^\q{ab}--a]",
                SyntaxRule::NegatedClassMayContainStrings,
                0,
            ),
            (
                r"[^\q{ab}&&\q{cd}]",
                SyntaxRule::NegatedClassMayContainStrings,
                0,
            ),
        ];
        for (pattern, rule, offset) in invalid {
            let error = RegExpProgram::compile(pattern, "v")
                .expect_err("a class string must not short-circuit enclosing validation");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(rule), "{pattern}");
            assert_eq!(error.offset, offset, "{pattern}");
        }
    }

    #[test]
    fn modifier_groups_accept_empty_removal_after_added_flags() {
        for flags in ["", "u", "v"] {
            for added in ["i", "m", "s", "im", "is", "ms", "ims"] {
                let with_dash = RegExpProgram::compile(&format!("(?{added}-:a)"), flags)
                    .expect("added flags may precede an empty removal list");
                let without_dash = RegExpProgram::compile(&format!("(?{added}:a)"), flags)
                    .expect("equivalent modifier group");
                assert_eq!(with_dash, without_dash);
            }
        }
    }

    #[test]
    fn invalid_group_prefixes_are_syntax_errors_without_rejecting_legal_lookaheads() {
        for pattern in [
            "(?1:a)", "(?I:a)", "(?u:a)", "(?é:a)", "(?-:a)", "(?ii:a)", "(?i-i:a)",
        ] {
            let error = RegExpProgram::compile(pattern, "").expect_err("invalid modifier syntax");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ModifierFlags), "{pattern}");
        }
        for pattern in ["(?=ab)", "(?!ab)"] {
            RegExpProgram::compile(pattern, "").expect("legal lookahead compiles");
        }
    }

    #[test]
    fn modifier_groups_scope_case_insensitivity() {
        let program = RegExpProgram::compile("(?i:a)b", "").unwrap();
        assert_eq!(
            program.instructions.last(),
            Some(&RegExpInstruction::accept())
        );
        assert!(program.instructions[0].positive_ascii_class_contains(b'A'));
        assert!(program.instructions[0].positive_ascii_class_contains(b'a'));
        assert_eq!(
            program.instructions[1],
            RegExpInstruction::literal_ascii(b'b')
        );

        let disabled = RegExpProgram::compile("(?-i:a)b", "i").unwrap();
        assert_eq!(
            disabled.instructions[0],
            RegExpInstruction::literal_ascii(b'a')
        );
        assert!(disabled.instructions[1].positive_ascii_class_contains(b'B'));
    }

    #[test]
    fn modifier_groups_encode_dot_all_and_multiline_overrides() {
        let dot_all_on = RegExpProgram::compile("(?s:.)", "").unwrap();
        assert_eq!(
            dot_all_on.instructions[0].operand0,
            RegExpModifierOverride::ForceOn.operand_code()
        );

        let dot_all_off = RegExpProgram::compile("(?-s:.)", "s").unwrap();
        assert_eq!(
            dot_all_off.instructions[0].operand0,
            RegExpModifierOverride::ForceOff.operand_code()
        );

        let multiline_on = RegExpProgram::compile("(?m:^$)", "").unwrap();
        assert_eq!(
            multiline_on.instructions[0].operand0,
            RegExpModifierOverride::ForceOn.operand_code()
        );
        assert_eq!(
            multiline_on.instructions[1].operand0,
            RegExpModifierOverride::ForceOn.operand_code()
        );

        let multiline_off = RegExpProgram::compile("(?-m:^$)", "m").unwrap();
        assert_eq!(
            multiline_off.instructions[0].operand0,
            RegExpModifierOverride::ForceOff.operand_code()
        );
        assert_eq!(
            multiline_off.instructions[1].operand0,
            RegExpModifierOverride::ForceOff.operand_code()
        );
    }

    #[test]
    fn nested_modifier_groups_restore_outer_dot_all_override() {
        let program = RegExpProgram::compile("(?s:(?-s:.).).", "").unwrap();
        let dot_operands = program
            .instructions
            .iter()
            .filter(|instruction| instruction.opcode == REGEXP_OPCODE_DOT)
            .map(|instruction| instruction.operand0)
            .collect::<Vec<_>>();

        assert_eq!(
            dot_operands,
            [
                RegExpModifierOverride::ForceOff.operand_code(),
                RegExpModifierOverride::ForceOn.operand_code(),
                RegExpModifierOverride::Inherit.operand_code(),
            ]
        );
    }

    #[test]
    fn direct_non_unicode_source_quantifies_only_its_utf16_trail_unit() {
        let lead = RegExpInstruction::literal_code_point(0xD842);
        let trail = RegExpInstruction::literal_code_point(0xDFB7);
        assert_eq!(
            RegExpProgram::compile("é𠮷", "").unwrap().instructions,
            vec![
                RegExpInstruction::literal_code_point(0xE9),
                lead,
                trail,
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            RegExpProgram::compile("𠮷?", "").unwrap().instructions,
            vec![
                lead,
                RegExpInstruction::split(2, 3),
                trail,
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            RegExpProgram::compile("𠮷??", "").unwrap().instructions,
            vec![
                lead,
                RegExpInstruction::split(3, 2),
                trail,
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            RegExpProgram::compile("𠮷{0}", "").unwrap().instructions,
            vec![lead, RegExpInstruction::accept()]
        );
        assert_eq!(
            RegExpProgram::compile("𠮷{2}", "").unwrap().instructions,
            vec![
                lead,
                RegExpInstruction::repeat_begin(0),
                RegExpInstruction::repeat_guard(4, 0, QuantifierPreference::Greedy),
                trail,
                RegExpInstruction::repeat_end(1),
                RegExpInstruction::repeat_exit(1),
                RegExpInstruction::accept()
            ]
        );
        assert_eq!(
            RegExpProgram::compile("𠮷?", "u").unwrap().instructions,
            vec![
                RegExpInstruction::split(1, 2),
                RegExpInstruction::literal_code_point(0x20BB7),
                RegExpInstruction::accept(),
            ],
            "Unicode mode quantifies the whole scalar"
        );
    }

    #[test]
    fn legacy_non_ascii_identity_atoms_retain_utf16_quantifier_ownership() {
        let lead = RegExpInstruction::literal_code_point(0xD842);
        let trail = RegExpInstruction::literal_code_point(0xDFB7);
        for (source, expected) in [
            (
                r"\é",
                vec![
                    RegExpInstruction::literal_code_point(0xE9),
                    RegExpInstruction::accept(),
                ],
            ),
            (
                r"\𠮷?",
                vec![
                    lead,
                    RegExpInstruction::split(2, 3),
                    trail,
                    RegExpInstruction::accept(),
                ],
            ),
            (
                r"\𠮷??",
                vec![
                    lead,
                    RegExpInstruction::split(3, 2),
                    trail,
                    RegExpInstruction::accept(),
                ],
            ),
            (r"\𠮷{0}", vec![lead, RegExpInstruction::accept()]),
            (
                r"\𠮷{2}",
                vec![
                    lead,
                    RegExpInstruction::repeat_begin(0),
                    RegExpInstruction::repeat_guard(4, 0, QuantifierPreference::Greedy),
                    trail,
                    RegExpInstruction::repeat_end(1),
                    RegExpInstruction::repeat_exit(1),
                    RegExpInstruction::accept(),
                ],
            ),
        ] {
            let program = RegExpProgram::compile(source, "").unwrap();
            assert_eq!(program.instructions, expected, "{source}");
            ValidatedRegExpProgram::from_program(&program).unwrap();
        }
        let grouped = RegExpProgram::compile(r"(?:\𠮷){2}", "").unwrap();
        assert_eq!(
            grouped.instructions,
            vec![
                RegExpInstruction::repeat_begin(0),
                RegExpInstruction::repeat_guard(4, 0, QuantifierPreference::Greedy),
                lead,
                trail,
                RegExpInstruction::repeat_end(0),
                RegExpInstruction::repeat_exit(0),
                RegExpInstruction::accept(),
            ],
            "a grouping term owns both code units before repetition"
        );
        ValidatedRegExpProgram::from_program(&grouped).unwrap();
    }

    #[test]
    fn unicode_non_ascii_identity_atoms_use_the_syntax_error_owner() {
        for flags in ["u", "v"] {
            for (source, offset) in [(r"\é", 0), (r"x\é", 1), (r"\𠮷?", 0)] {
                let error = RegExpProgram::compile(source, flags).unwrap_err();
                assert_eq!(
                    error.kind,
                    RegExpCompileErrorKind::InvalidSyntax,
                    "{source}"
                );
                assert_eq!(error.rule, Some(SyntaxRule::IdentityEscape));
                assert_eq!(error.offset, offset, "{source}");
            }
        }
    }

    #[test]
    fn reports_invalid_syntax_with_offsets() {
        let invalid_cases = [("[a", 0), ("[z-a]", 2), ("\\", 0)];
        for (pattern, offset) in invalid_cases {
            let error = RegExpProgram::compile(pattern, "").expect_err("pattern should be invalid");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.offset, offset, "{pattern}");
        }
    }

    /// No "unsupported" category any more: the `first_unsupported` arm this test
    /// was named for was deleted with the rest of the flag-verdict audit, and
    /// `parse_flags` can now only answer `InvalidSyntax`. The test stayed green
    /// through that deletion because it never asserted the third category — which
    /// is why the *name* was the only thing left claiming it exists.
    #[test]
    fn distinguishes_duplicate_and_unknown_flags() {
        for flags in ["gg", "yy", "ii", "igi"] {
            let error = RegExpProgram::compile("a", flags).expect_err("flags should be invalid");
            assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
        }

        let error = RegExpProgram::compile("a", "z").expect_err("flag should be unknown");
        assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
        assert_eq!(error.offset, 0);

        for flags in ["uv", "vu"] {
            let error = RegExpProgram::compile("a", flags)
                .expect_err("unicode modes should be mutually exclusive");
            assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
            assert_eq!(error.offset, 1);
        }

        let flags = RegExpProgram::compile("a", "imsv").unwrap().flags;
        assert!(flags.ignore_case);
        assert!(flags.multiline);
        assert!(flags.dot_all);
        assert_eq!(flags.unicode_mode, RegExpUnicodeMode::UnicodeSets);
    }

    #[test]
    fn named_groups_preserve_source_order_and_legal_duplicate_mappings() {
        let program =
            RegExpProgram::compile("(?:(?<x>a)|(?<y>a)(?<x>b))(?:(?<z>c)|(?<z>d))", "d").unwrap();
        assert!(program.flags.has_indices);
        assert_eq!(program.capture_count, 5);
        assert_eq!(
            program.named_groups,
            vec![
                RegExpNamedGroup {
                    name: "x".into(),
                    capture_ids: vec![1, 3]
                },
                RegExpNamedGroup {
                    name: "y".into(),
                    capture_ids: vec![2]
                },
                RegExpNamedGroup {
                    name: "z".into(),
                    capture_ids: vec![4, 5]
                },
            ]
        );
    }

    #[test]
    fn named_backreferences_resolve_forward_and_are_nullable() {
        let forward = compile(r"\k<x>(?<x>a)");
        assert_eq!(
            forward.instructions[0],
            RegExpInstruction::named_backreference(0, CaseFolding::Sensitive)
        );
        let repeated = compile(r"(?:(?:(?<x>a)|(?<x>b)|c)\k<x>){2}");
        assert_eq!(repeated.capture_count, 2);
        assert_eq!(repeated.named_groups[0].capture_ids, vec![1, 2]);
        assert!(repeated
            .instructions
            .iter()
            .any(
                |instruction| instruction.opcode == REGEXP_OPCODE_NAMED_BACKREFERENCE
                    && instruction.operand0 == 0
                    && instruction.operand1 == 0
            ));
        assert_eq!(
            repeated.instructions[2],
            RegExpInstruction::clear_capture_range(1, 3)
        );
        assert_eq!(
            repeated
                .instructions
                .iter()
                .filter(|instruction| {
                    **instruction == RegExpInstruction::clear_capture_range(1, 3)
                })
                .count(),
            2
        );
    }

    #[test]
    fn named_group_identifiers_use_unicode_id_properties_and_canonical_names() {
        let program = RegExpProgram::compile(
            r"(?<π>a)(?<ಠ_ಠ>b)(?<ͺ>c)(?<$𐒤>d)(?<a\uD801\uDCA4>e)(?<_\u200C>f)(?<_\u200D>g)\k<\u03C0>\k<ಠ_ಠ>\k<ͺ>\k<$\u{104A4}>\k<a𐒤>\k<_\u200C>\k<_\u200D>",
            "du",
        )
        .unwrap();

        assert_eq!(
            program
                .named_groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            vec!["π", "ಠ_ಠ", "ͺ", "$𐒤", "a𐒤", "_\u{200C}", "_\u{200D}"]
        );
        assert_eq!(
            program
                .instructions
                .iter()
                .filter(|instruction| instruction.opcode == REGEXP_OPCODE_NAMED_BACKREFERENCE)
                .count(),
            7
        );
    }

    #[test]
    fn named_group_identifier_escapes_are_canonical_across_groups_and_references() {
        let non_unicode = RegExpProgram::compile(r"(?<\u{03C0}>a)\k<π>", "").unwrap();
        assert_eq!(non_unicode.named_groups[0].name, "π");
        assert_eq!(
            non_unicode.instructions[4],
            RegExpInstruction::named_backreference(0, CaseFolding::Sensitive)
        );

        let unicode_sets = RegExpProgram::compile(r"(?<\u03C0>a)\k<\u{03C0}>", "v").unwrap();
        assert_eq!(unicode_sets.named_groups[0].name, "π");

        let duplicate = RegExpProgram::compile(r"(?:(?<π>a)|(?<\u03C0>b))", "u").unwrap();
        assert_eq!(duplicate.named_groups[0].name, "π");
        assert_eq!(duplicate.named_groups[0].capture_ids, vec![1, 2]);

        let same_path = RegExpProgram::compile(r"(?<π>a)(?<\u{03C0}>b)", "u")
            .expect_err("decoded duplicate name should be rejected");
        assert_eq!(same_path.kind, RegExpCompileErrorKind::InvalidSyntax);
    }

    #[test]
    fn invalid_named_group_identifier_code_points_and_escapes_are_syntax_errors() {
        let invalid = [
            ("(?<1>a)", 3),
            (r"(?<\u0031>a)", 3),
            (r"(?<\u{31}>a)", 3),
            ("(?<❤>a)", 3),
            (r"(?<\u200C>a)", 3),
            (r"(?<\u200D>a)", 3),
            (r"(?<\x41>a)", 3),
            (r"(?<\u12G4>a)", 7),
            (r"(?<\u{}>a)", 6),
            (r"(?<\u{D800}>a)", 3),
            (r"(?<\u{110000}>a)", 3),
            (r"(?<\uD801>a)", 3),
            (r"(?<\uDCA4>a)", 3),
            (r"(?<a\uD801\u0041>a)", 4),
            (r"(?<x>a)\k<\uD801>", 10),
        ];
        for (pattern, offset) in invalid {
            let error = RegExpProgram::compile(pattern, "")
                .expect_err("invalid identifier should be rejected");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{pattern}"
            );
            assert_eq!(error.offset, offset, "{pattern}: {error}");
        }
    }

    #[test]
    fn lowers_supported_lookbehind_to_reverse_matcher_instructions() {
        let program = RegExpProgram::compile(r"(?<=\w+)f", "").unwrap();
        assert_eq!(
            program.instructions[0],
            RegExpInstruction::lookaround_start(RegExpMatchDirection::Reverse)
        );
        assert_eq!(program.instructions[1], RegExpInstruction::split(2, 5));
        assert_eq!(program.instructions[3], RegExpInstruction::split(2, 4));
        assert_eq!(
            program.instructions[4],
            RegExpInstruction::lookaround_end(
                5,
                6,
                &LookaroundPolarity::Positive,
                RegExpMatchDirection::Forward
            )
        );
        assert_eq!(
            program.instructions[5],
            RegExpInstruction::lookaround_failure(
                6,
                &LookaroundPolarity::Positive,
                RegExpMatchDirection::Forward
            )
        );
        assert_eq!(
            program.instructions[6],
            RegExpInstruction::literal_ascii(b'f')
        );
    }

    #[test]
    fn rejects_invalid_named_groups_and_same_path_duplicates() {
        for pattern in ["(?<1>x)", "(?<x>a)(?<x>b)"] {
            let error = RegExpProgram::compile(pattern, "").expect_err(pattern);
            assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
        }
        for pattern in [r"\k<missing>", r"\k<x"] {
            let error = RegExpProgram::compile(pattern, "u").expect_err(pattern);
            assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
        }
        assert!(RegExpProgram::compile("(?:(?<x>a)|(?<x>b))", "").is_ok());
    }

    /// DEFECT 1: `IdentityEscape[+UnicodeMode] :: SyntaxCharacter | `/``.
    ///
    /// The SOLIDUS alternative was missing, so `/\//u` — the way a RegExp
    /// literal escapes its own delimiter, and half of every URL pattern ever
    /// written — was answered `InvalidSyntax`, i.e. a `SyntaxError` for a legal
    /// program. `test262/vendor/test262/test/built-ins/RegExp/unicode_identity_escape.js`
    /// line 35 is `assert(/\//u.test("/"), …)`.
    #[test]
    fn unicode_identity_escape_accepts_the_solidus() {
        for flags in ["u", "v"] {
            let program = RegExpProgram::compile(r"\/", flags)
                .unwrap_or_else(|error| panic!("`\\/` under `{flags}` must compile: {error}"));
            assert_eq!(
                program.instructions,
                vec![
                    RegExpInstruction::literal_ascii(b'/'),
                    RegExpInstruction::accept(),
                ],
                "{flags}"
            );
        }

        // The two class representations must both keep the rule. The `\w` is
        // load-bearing: it forces the code-point range path while bare `[\/]`
        // takes the ASCII bitmap path. Both now receive the same closed
        // ordinary-class grammar mode.
        assert!(!class_needs_code_point_ranges(br"[\/]", 0));
        assert!(!class_needs_code_point_ranges(br"[\/A]", 0));
        assert!(class_needs_code_point_ranges(br"[\/\w]", 0));
        assert!(RegExpProgram::compile(r"[\/\w]", "u").is_ok());

        // The guard against the WRONG fix. Adding `/` to `is_syntax_character`
        // would satisfy the assertions above and simultaneously turn the bare
        // `/` in `new RegExp("a/b")` into an `UnsupportedFeature` verdict via
        // the metacharacter fallthrough in `parse_instruction_atom`.
        let unescaped = RegExpProgram::compile("a/b", "").expect("`a/b` must stay legal");
        assert_eq!(
            unescaped.instructions,
            vec![
                RegExpInstruction::literal_ascii(b'a'),
                RegExpInstruction::literal_ascii(b'/'),
                RegExpInstruction::literal_ascii(b'b'),
                RegExpInstruction::accept(),
            ]
        );

        // And `\q` is still not an identity escape, so the fix widened the set
        // by exactly one character rather than deleting the rule.
        assert_eq!(
            RegExpProgram::compile(r"\q", "u").unwrap_err().rule,
            Some(SyntaxRule::IdentityEscape)
        );
    }

    /// DEFECT 2: `ClassSetCharacter :: `\` ClassSetReservedPunctuator` in
    /// `v` mode.
    ///
    /// `parse_class_set` passed a literal `true` for what was a `unicode: bool`
    /// parameter, so `v`-mode class atoms were checked against the `u`-mode
    /// `ClassEscape` rule. All thirteen punctuators that `u` mode does not
    /// already accept were rejected as `SyntaxError`s.
    #[test]
    fn unicode_sets_class_accepts_reserved_punctuator_escapes() {
        // The full production is `& - ! # % , : ; < = > @ ` ~`; `-` is already a
        // `u`-mode `ClassEscape` alternative, so these are the thirteen that
        // were new in `v` mode and the thirteen HEAD refused.
        let punctuators = [
            b'&', b'!', b'#', b'%', b',', b':', b';', b'<', b'=', b'>', b'@', b'`', b'~',
        ];
        assert_eq!(punctuators.len(), 13);
        for punctuator in punctuators {
            let pattern = format!("[\\{}]", punctuator as char);
            let program = RegExpProgram::compile(&pattern, "v")
                .unwrap_or_else(|error| panic!("`{pattern}` under `v` must compile: {error}"));
            assert!(
                ranges_contain(&program.ranges, u32::from(punctuator)),
                "`{pattern}` must match `{}`",
                punctuator as char
            );
        }
        // `-` is the fourteenth alternative and is legal in `u` mode too, which
        // is why it was the one punctuator HEAD already accepted.
        //
        // Under `v` a bare `[\-]` reaches `parse_unicode_sets_class`
        // unconditionally. Under `u`, the bare and `\w`-suffixed spellings
        // deliberately exercise the bitmap and range representations under the
        // same ordinary-class grammar mode.
        assert!(!class_needs_code_point_ranges(br"[\-]", 0));
        assert!(class_needs_code_point_ranges(br"[\-\w]", 0));
        for flags in ["u", "v"] {
            for pattern in [r"[\-]", r"[\-\w]"] {
                assert!(
                    RegExpProgram::compile(pattern, flags).is_ok(),
                    "the `-` alternative must not regress for `{pattern}` under `{flags}`"
                );
            }
        }

        // `u` mode is unchanged: these are NOT `u`-mode class escapes, and
        // widening both modes together would have been the wrong fix.
        for punctuator in punctuators {
            let pattern = format!("[\\{}\\u0041]", punctuator as char);
            let error = RegExpProgram::compile(&pattern, "u")
                .expect_err("`u` mode must keep rejecting reserved punctuator escapes");
            assert_eq!(error.rule, Some(SyntaxRule::ClassEscape), "{pattern}");
        }
    }

    /// DEFECT 3: `RegExpUnicodeEscapeSequence[+UnicodeMode] :: `u{` CodePoint `}``.
    ///
    /// Found while picking a witness for [`SyntaxRule::CodePointEscape`], and
    /// the same shape as DEFECT 1: only the class parser implemented the braced
    /// alternative, so `/[\u{41}]/u` compiled while `/\u{41}/u` — and therefore
    /// every astral pattern written the ordinary way — was a `SyntaxError`.
    #[test]
    fn unicode_mode_accepts_braced_code_point_escapes() {
        assert_eq!(
            RegExpProgram::compile(r"\u{41}", "u")
                .expect("`\\u{41}` must compile")
                .instructions,
            vec![
                RegExpInstruction::literal_ascii(b'A'),
                RegExpInstruction::accept(),
            ]
        );
        assert_eq!(
            RegExpProgram::compile(r"\u{1F600}", "v")
                .expect("an astral braced escape must compile")
                .instructions,
            vec![
                RegExpInstruction::literal_code_point(0x1_f600),
                RegExpInstruction::accept(),
            ]
        );
        // The class path, which always had this alternative, must still agree.
        assert!(RegExpProgram::compile(r"[\u{41}]", "u").is_ok());

        // Out of range stays a Syntax Error: the fix added the production, it
        // did not delete its early error.
        assert_eq!(
            RegExpProgram::compile(r"\u{110000}", "u").unwrap_err().rule,
            Some(SyntaxRule::CodePointEscapeRange)
        );
        // And in legacy mode `\u{41}` is still the Annex B identity escape `u`
        // followed by a literal brace group, not a code point.
        assert!(RegExpProgram::compile(r"\u{41}", "").is_ok());
    }

    /// One pinned `(pattern, flags)` witness per [`SyntaxRule`], asserted to
    /// produce that rule.
    ///
    /// This is the audit's standing form. A rejection site is a claim that a
    /// conforming engine refuses the pattern, and the cheapest way to keep that
    /// claim honest is to require a pattern that demonstrates it. A new
    /// `SyntaxRule` variant fails the exhaustive `match` in
    /// [`SyntaxRule::citation`]; once added there it fails HERE until it has a
    /// witness. A site that cannot produce one is not an `InvalidSyntax` site.
    ///
    /// Each witness is asserted to produce its OWN rule, not merely some
    /// `InvalidSyntax`, so a witness pattern that starts being answered by a
    /// different rule fails here rather than being silently absorbed.
    ///
    /// **This is per-RULE, not per-SITE, and it is not a site map.** 86
    /// `invalid_syntax` call sites map onto 28 rules and 28 witnesses:
    /// `RegExpIdentifierName` alone has 14 sites sharing one witness,
    /// `UnclosedCharacterClass` has 14, `ClassSetExpression` has 7 and
    /// `CharacterEscape` has 3. A site given the WRONG variant is invisible to
    /// this test whenever some other site already witnesses both rules — which
    /// is exactly how `parse_modifier_group_prefix` shipped an `UnclosedGroup`
    /// citation on a `ModifierFlags` violation. Since `Display` puts
    /// `citation()` on the product path, that class of error is user-visible; a
    /// message-versus-citation read of each site is the only thing that catches
    /// it.
    #[test]
    fn every_syntax_rule_has_a_pinned_witness() {
        // (rule, pattern, flags). Written as one table rather than one test per
        // rule so that the coverage assertion below can be total.
        let witnesses: &[(SyntaxRule, &str, &str)] = &[
            (SyntaxRule::UnclosedGroup, "(a", ""),
            (SyntaxRule::StrayClosingParenthesis, "a)", ""),
            (SyntaxRule::ModifierFlags, "(?ii:a)", ""),
            (SyntaxRule::QuantifierWithoutAtom, "*", ""),
            (SyntaxRule::QuantifierAfterQuantifier, "a**", ""),
            (SyntaxRule::QuantifierBounds, "a{2,1}", ""),
            (SyntaxRule::UnescapedSyntaxCharacter, "{", "u"),
            (SyntaxRule::NamedBackreferenceSyntax, r"\ka", "u"),
            (SyntaxRule::UnknownGroupName, r"\k<missing>", "u"),
            (SyntaxRule::DuplicateGroupName, "(?<x>a)(?<x>b)", ""),
            (SyntaxRule::RegExpIdentifierName, "(?<1>a)", ""),
            (SyntaxRule::Flags, "a", "gg"),
            (SyntaxRule::CharacterEscape, "\\", ""),
            (SyntaxRule::IdentityEscape, r"\q", "u"),
            // This deliberately takes the ASCII bitmap path. The ordinary
            // class mode must enforce `ClassEscape` before representation can
            // affect the verdict.
            (SyntaxRule::ClassEscape, r"[\q]", "u"),
            (SyntaxRule::ClassSetCharacter, "[!!]", "v"),
            (SyntaxRule::ClassStringDisjunction, r"[\q]", "v"),
            (SyntaxRule::ClassSetExpression, "[a&&b--c]", "v"),
            (SyntaxRule::NegatedClassMayContainStrings, r"[^\q{}]", "v"),
            (SyntaxRule::HexEscapeSequence, r"\xZZ", "u"),
            (SyntaxRule::UnicodeEscapeSequence, r"\uZZZZ", "u"),
            (SyntaxRule::CodePointEscape, r"\u{}", "u"),
            (SyntaxRule::CodePointEscapeRange, r"\u{110000}", "u"),
            (SyntaxRule::UnicodePropertyEscape, r"\p{ASCII", "u"),
            (SyntaxRule::UnicodePropertyName, r"\p{NotAProperty}", "u"),
            (SyntaxRule::UnclosedCharacterClass, "[a", ""),
            (SyntaxRule::ClassRangeOrder, "[z-a]", ""),
            (SyntaxRule::ClassRangeBound, r"[\D-a]", "u"),
        ];

        for (rule, pattern, flags) in witnesses {
            let Err(error) = RegExpProgram::compile(pattern, flags) else {
                panic!("`{pattern}` under `{flags}` must be rejected by {rule:?}");
            };
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "`{pattern}` under `{flags}`: {error}"
            );
            assert_eq!(
                error.rule,
                Some(*rule),
                "`{pattern}` under `{flags}` reached the wrong site: {error}"
            );
            assert!(
                !rule.citation().is_empty(),
                "{rule:?} must cite a production"
            );
        }

        for rule in SyntaxRule::ALL {
            assert!(
                witnesses.iter().any(|(witness, ..)| *witness == rule),
                "{rule:?} has no pinned witness. A rejection site whose rule \
                 cannot be demonstrated by a pattern is an UnsupportedFeature, \
                 not an InvalidSyntax -- see the SyntaxRule doc comment."
            );
        }
    }

    /// [`SyntaxRule::ALL`] is hand-maintained, so it is checked rather than
    /// trusted: sorted, duplicate-free, and the length the constant declares.
    ///
    /// It cannot be derived, and a copy-paste omission there would silently
    /// exempt a rule from the witness table above. Sorted-and-unique is the
    /// strongest property available without a derive.
    #[test]
    fn all_syntax_rules_are_listed_once() {
        let mut deduplicated = SyntaxRule::ALL.to_vec();
        deduplicated.sort_unstable();
        deduplicated.dedup();
        assert_eq!(
            deduplicated.len(),
            SyntaxRule::ALL.len(),
            "SyntaxRule::ALL contains a duplicate, so some rule is exempt from \
             the witness table"
        );
        // Declaration order is `Ord` here, so "sorted" means "listed in the
        // order the enum declares". A variant appended to the enum but inserted
        // in the middle of `ALL` -- or forgotten and then noticed later -- shows
        // up as an ordering failure rather than as a silent gap.
        assert!(
            SyntaxRule::ALL.windows(2).all(|pair| pair[0] < pair[1]),
            "SyntaxRule::ALL must list every variant once, in declaration order"
        );
        // Every rule's citation must be non-empty and must name a clause. This
        // is the cheap half of the audit: the expensive half is the witness.
        for rule in SyntaxRule::ALL {
            let citation = rule.citation();
            assert!(
                citation.starts_with("22.2."),
                "{rule:?} cites `{citation}`, which is not an ECMA-262 clause \
                 number. A rejection that cannot name one is an \
                 UnsupportedFeature."
            );
        }
    }

    /// The parser picks between an ASCII bitmap class and a code-point range
    /// class by inspecting the class body ([`class_needs_code_point_ranges`]),
    /// and the two are separate parsers with separate escape handling. A
    /// pattern's VERDICT must not depend on which one the compiler chose.
    ///
    /// Appending `A` adds `A` to the set and nothing else, and the `\u`
    /// is what forces the code-point path, so each witness is compiled through
    /// both representations and the two verdicts are compared.
    ///
    /// Both accepted and rejected witnesses are pinned: choosing a smaller
    /// instruction representation cannot admit Annex B grammar under `u`.
    #[test]
    fn class_verdicts_do_not_depend_on_the_class_representation() {
        let bodies = [
            r"\/", r"\^", r"\$", r"\\", r"\.", r"\*", r"\+", r"\?", r"\(", r"\)", r"\[", r"\]",
            r"\{", r"\}", r"\|", r"\-", r"\b", r"\d", r"a-f", "abc",
        ];
        for body in bodies {
            // The premise the test's NAME rests on, asserted rather than
            // assumed: the two spellings must reach two different parsers.
            // `class_needs_code_point_ranges` is what routes them, and adding
            // any of `d`, `b`, `/` or `-` to its trigger set — a plausible edit,
            // since `\d` is already modelled differently by the two parsers —
            // would send BOTH forms to `parse_class` and leave every assertion
            // below green while compiling one parser twice.
            let bitmap_source = format!("[{body}]");
            let ranges_source = format!("[{body}\\u0041]");
            assert!(
                !class_needs_code_point_ranges(bitmap_source.as_bytes(), 0),
                "`{bitmap_source}` must take the ASCII bitmap parser"
            );
            assert!(
                class_needs_code_point_ranges(ranges_source.as_bytes(), 0),
                "`{ranges_source}` must take the code-point range parser"
            );
            for flags in ["", "u"] {
                let bitmap = RegExpProgram::compile(&bitmap_source, flags);
                let ranges = RegExpProgram::compile(&ranges_source, flags);
                assert!(
                    bitmap.is_ok(),
                    "`[{body}]` under `{flags}` must compile: {:?}",
                    bitmap.unwrap_err()
                );
                assert!(
                    ranges.is_ok(),
                    "`[{body}\\u0041]` under `{flags}` must compile through the \
                     code-point path too: {:?}",
                    ranges.unwrap_err()
                );
            }
            // `v` has one parser and no representation choice, but it must not
            // disagree with the other two about a legal class either.
            assert!(
                RegExpProgram::compile(&format!("[{body}]"), "v").is_ok(),
                "`[{body}]` under `v` must compile"
            );
        }

        for body in [r"\q", r"\c", r"\c0", r"\1", r"\8", r"\01"] {
            let bitmap_source = format!("[{body}]");
            let ranges_source = format!("[{body}\\u0041]");
            assert!(
                !class_needs_code_point_ranges(bitmap_source.as_bytes(), 0),
                "`{bitmap_source}` must take the ASCII bitmap parser"
            );
            assert!(
                class_needs_code_point_ranges(ranges_source.as_bytes(), 0),
                "`{ranges_source}` must take the code-point range parser"
            );

            for source in [&bitmap_source, &ranges_source] {
                let error = RegExpProgram::compile(source, "u")
                    .expect_err("Annex B class escape must be rejected under `u`");
                assert_eq!(
                    error.kind,
                    RegExpCompileErrorKind::InvalidSyntax,
                    "{source}"
                );
                assert_eq!(error.rule, Some(SyntaxRule::ClassEscape), "{source}");

                assert!(
                    RegExpProgram::compile(source, "").is_ok(),
                    "legacy grammar must keep accepting `{source}`"
                );
            }
        }
        // The complete named-capture census must reach both class encoders
        // and both range endpoints, even when the group appears afterwards.
        for body in [r"\k", r"\k-m", r"i-\k"] {
            let bitmap_source = format!("[{body}]");
            let ranges_source = format!(r"[{body}\u0041]");
            assert!(!class_needs_code_point_ranges(bitmap_source.as_bytes(), 0));
            assert!(class_needs_code_point_ranges(ranges_source.as_bytes(), 0));
            for class_source in [&bitmap_source, &ranges_source] {
                assert!(
                    RegExpProgram::compile(class_source, "").is_ok(),
                    "{class_source}"
                );
                for pattern in [
                    format!("(?<named>a){class_source}"),
                    format!("{class_source}(?<named>a)"),
                ] {
                    let error = RegExpProgram::compile(&pattern, "")
                        .expect_err("named-capture context excludes escaped k inside classes");
                    assert_eq!(
                        error.kind,
                        RegExpCompileErrorKind::InvalidSyntax,
                        "{pattern}"
                    );
                    assert_eq!(error.rule, Some(SyntaxRule::ClassEscape), "{pattern}");
                }
            }
        }
    }

    #[test]
    fn legacy_bare_class_control_escape_preserves_backslash_and_c() {
        let bitmap_source = r"[\c]";
        let ranges_source = r"[\c\u0041]";
        assert!(!class_needs_code_point_ranges(bitmap_source.as_bytes(), 0));
        assert!(class_needs_code_point_ranges(ranges_source.as_bytes(), 0));

        let bitmap = RegExpProgram::compile(bitmap_source, "").expect("legacy bitmap `\\c`");
        assert_eq!(
            bitmap.instructions[0],
            RegExpInstruction::positive_ascii_class(
                0,
                (1_u64 << (b'\\' - 64)) | (1_u64 << (b'c' - 64)),
            )
        );

        let ranges = RegExpProgram::compile(ranges_source, "").expect("legacy range `\\c`");
        assert_eq!(
            ranges.ranges,
            vec![
                (u32::from(b'A'), u32::from(b'A')),
                (u32::from(b'\\'), u32::from(b'\\')),
                (u32::from(b'c'), u32::from(b'c')),
            ]
        );
    }

    #[test]
    fn class_control_underscore_has_mode_and_representation_invariant_semantics() {
        // Prefix both the Pattern and class body so the asserted source offset
        // cannot accidentally be the class opener or its first member.
        let bitmap_source = r"x[ab\c_]";
        let ranges_source = r"x[ab\c_\u0041]";
        assert!(!class_needs_code_point_ranges(bitmap_source.as_bytes(), 1));
        assert!(class_needs_code_point_ranges(ranges_source.as_bytes(), 1));

        for source in [bitmap_source, ranges_source] {
            let error = RegExpProgram::compile(source, "u")
                .expect_err("`\\c_` is an Annex B class escape, not Unicode grammar");
            assert_eq!(
                error.kind,
                RegExpCompileErrorKind::InvalidSyntax,
                "{source}"
            );
            assert_eq!(error.rule, Some(SyntaxRule::ClassEscape), "{source}");
            assert_eq!(error.offset, 4, "{source}");
        }

        let bitmap = RegExpProgram::compile(bitmap_source, "").expect("legacy bitmap `\\c_`");
        assert_eq!(
            bitmap.instructions[1],
            RegExpInstruction::positive_ascii_class(
                1_u64 << 0x1f,
                (1_u64 << (b'a' - 64)) | (1_u64 << (b'b' - 64)),
            )
        );

        let ranges = RegExpProgram::compile(ranges_source, "").expect("legacy range `\\c_`");
        assert_eq!(
            ranges.ranges,
            vec![
                (0x1f, 0x1f),
                (u32::from(b'A'), u32::from(b'A')),
                (u32::from(b'a'), u32::from(b'b')),
            ]
        );
    }

    #[test]
    fn class_escape_values_do_not_depend_on_the_class_representation() {
        assert!(!class_needs_code_point_ranges(br"[\cA]", 0));
        assert!(class_needs_code_point_ranges(br"[\cA\u0002]", 0));
        for flags in ["", "u"] {
            let bitmap = RegExpProgram::compile(r"[\cA]", flags)
                .unwrap_or_else(|error| panic!("bitmap `\\cA` under `{flags}`: {error}"));
            assert!(bitmap.instructions[0].positive_ascii_class_contains(0x01));
            assert!(!bitmap.instructions[0].positive_ascii_class_contains(b'c'));
            assert!(!bitmap.instructions[0].positive_ascii_class_contains(b'A'));

            let ranges = RegExpProgram::compile(r"[\cA\u0002]", flags)
                .unwrap_or_else(|error| panic!("range `\\cA` under `{flags}`: {error}"));
            assert!(ranges_contain(&ranges.ranges, 0x01));
            assert!(!ranges_contain(&ranges.ranges, u32::from(b'c')));
            assert!(!ranges_contain(&ranges.ranges, u32::from(b'A')));
        }

        assert!(!class_needs_code_point_ranges(br"[\0]", 0));
        assert!(class_needs_code_point_ranges(br"[\0\u0002]", 0));
        let bitmap = RegExpProgram::compile(r"[\0]", "u").expect("`\\0` is valid under `u`");
        assert!(bitmap.instructions[0].positive_ascii_class_contains(0));
        let ranges = RegExpProgram::compile(r"[\0\u0002]", "u")
            .expect("range-path `\\0` is valid under `u`");
        assert!(ranges_contain(&ranges.ranges, 0));

        for source in [r"[\/]", r"[\-]", r"[\^]", r"[\\]"] {
            assert!(
                RegExpProgram::compile(source, "u").is_ok(),
                "legal Unicode class identity escape `{source}` must remain accepted"
            );
        }
    }

    fn folded_class_value(pattern: &str) -> ClassSetValue {
        let mut cursor = 0;
        let parsed = parse_class_set(pattern.as_bytes(), &mut cursor, CaseFolding::Unicode)
            .unwrap_or_else(|error| panic!("{pattern}: {error}"));
        assert_eq!(cursor, pattern.len());
        parsed.into_nested_value()
    }

    #[test]
    fn unicode_sets_class_strings_fold_singletons_before_algebra() {
        for (direct, ordinary) in [
            (r"[\q{a}&&A]", "[a&&A]"),
            (r"[\q{K}&&\u212A]", r"[K&&\u212A]"),
            (r"[\q{a}--A]", "[a--A]"),
            (r"[^\q{K}]", "[^K]"),
            (r"[[^\q{K}]&&\q{K|b}]", "[[^K]&&[Kb]]"),
        ] {
            let direct = RegExpProgram::compile(direct, "iv").unwrap();
            let ordinary = RegExpProgram::compile(ordinary, "iv").unwrap();
            assert_eq!(direct.instructions, ordinary.instructions);
            assert_eq!(direct.ranges, ordinary.ranges);
            super::program::ValidatedRegExpProgram::from_program(&direct).unwrap();
        }
        let complemented = folded_class_value(r"[^\q{K}]");
        for code_point in [u32::from(b'K'), u32::from(b'k'), 0x212a] {
            assert!(!ranges_contain(&complemented.finite.ranges, code_point));
        }
        assert!(ranges_contain(&complemented.finite.ranges, u32::from(b'b')));
    }

    #[test]
    fn unicode_sets_class_strings_fold_sequences_before_algebra() {
        let union = folded_class_value(r"[\q{Ab|aB}\q{AB|CD}]");
        assert_eq!(
            union.finite.strings,
            BTreeSet::from([
                vec![u32::from(b'a'), u32::from(b'b')],
                vec![u32::from(b'c'), u32::from(b'd')]
            ])
        );
        let intersection = folded_class_value(r"[\q{Ab|CD}&&\q{aB|ef}]");
        assert_eq!(intersection.finite.strings, BTreeSet::from([vec![97, 98]]));
        let subtraction = folded_class_value(r"[\q{Ab|CD}--\q{aB|ef}]");
        assert_eq!(subtraction.finite.strings, BTreeSet::from([vec![99, 100]]));
        assert!(folded_class_value(r"[\q{Ab}--\q{aB}]")
            .finite
            .strings
            .is_empty());
    }

    #[test]
    fn unicode_sets_class_strings_simple_folding_preserves_code_point_lengths() {
        let value = folded_class_value(
            r"[\q{\u017F\u212A|\u03A3\u03C2|\u{10400}\u{10428}|\uD800X|\u00DF\u00DF|SS|}]",
        );
        assert_eq!(
            value.finite.strings,
            BTreeSet::from([
                vec![115, 107],
                vec![0x3c3, 0x3c3],
                vec![0x10428, 0x10428],
                vec![0xd800, 120],
                vec![0xdf, 0xdf],
                vec![115, 115],
                vec![],
            ])
        );
        assert!(value.may_contain_strings);
        assert!(value.finite.ranges.is_empty());
    }

    #[test]
    fn unicode_sets_folded_strings_lower_each_position_through_existing_instructions() {
        let mut parsed =
            parse_pattern(br"[\q{\u212A\u017F}]", RegExpUnicodeMode::UnicodeSets, true).unwrap();
        let term = parsed.alternatives.pop().unwrap().pop().unwrap();
        let ParsedTerm::Quantified {
            atom: ParsedAtom::FiniteClassSet(atom),
            ..
        } = term
        else {
            panic!("one multi-code-point atom expected");
        };
        assert_eq!(atom.multi_code_point_strings.len(), 1);
        assert_eq!(atom.multi_code_point_strings[0].len(), 2);
        let members = |instruction: RegExpInstruction| {
            assert_eq!(instruction.opcode, REGEXP_OPCODE_UNICODE_PROPERTY);
            let first = instruction.operand0 as usize;
            let count = (instruction.operand1 >> 1) as usize;
            parsed.ranges[first..first + count].to_vec()
        };
        let kelvin = members(atom.multi_code_point_strings[0][0]);
        let long_s = members(atom.multi_code_point_strings[0][1]);
        for code_point in [75, 107, 0x212a] {
            assert!(ranges_contain(&kelvin, code_point));
        }
        for code_point in [83, 115, 0x17f] {
            assert!(ranges_contain(&long_s, code_point));
        }
        assert!(!ranges_contain(&kelvin, 115));
        assert!(!ranges_contain(&long_s, 107));
        let mut forward = Vec::new();
        ProgramLowerer::new(&mut forward, &mut Vec::new(), 0, &[])
            .finite_class_set_atom(&atom, RegExpMatchDirection::Forward)
            .unwrap();
        let mut reverse = Vec::new();
        ProgramLowerer::new(&mut reverse, &mut Vec::new(), 0, &[])
            .finite_class_set_atom(&atom, RegExpMatchDirection::Reverse)
            .unwrap();
        assert_eq!(forward[1..3], atom.multi_code_point_strings[0]);
        assert_eq!(
            reverse[1..3],
            [
                atom.multi_code_point_strings[0][1],
                atom.multi_code_point_strings[0][0]
            ]
        );
    }

    #[test]
    fn unicode_sets_folded_class_strings_keep_empty_and_static_negation_rules() {
        let empty = folded_class_value(r"[\q{AB|}--\q{ab}]");
        assert_eq!(empty.finite.strings, BTreeSet::from([vec![]]));
        let legal = RegExpProgram::compile(r"[^\q{ab}&&A]", "iv").unwrap();
        super::program::ValidatedRegExpProgram::from_program(&legal).unwrap();
        for pattern in [r"[^\q{ab}--\q{AB}]", r"[^\q{}]", r"[^\q{ab}&&\q{CD}]"] {
            let error = RegExpProgram::compile(pattern, "iv").unwrap_err();
            assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
            assert_eq!(error.rule, Some(SyntaxRule::NegatedClassMayContainStrings));
        }
        for (pattern, rule) in [
            (r"[\q{ab}]\k<missing>", SyntaxRule::UnknownGroupName),
            (r"[\q{ab}](", SyntaxRule::UnclosedGroup),
            (r"[\q{ab}--]", SyntaxRule::ClassSetExpression),
        ] {
            let error = RegExpProgram::compile(pattern, "iv").unwrap_err();
            assert_eq!(error.kind, RegExpCompileErrorKind::InvalidSyntax);
            assert_eq!(error.rule, Some(rule));
        }
    }

    #[test]
    fn unicode_sets_folded_strings_keep_longest_first_and_scoped_modifiers() {
        let mut parsed =
            parse_pattern(br"[\q{A|Ab|ABC|}]", RegExpUnicodeMode::UnicodeSets, true).unwrap();
        let term = parsed.alternatives.pop().unwrap().pop().unwrap();
        let ParsedTerm::Quantified {
            atom: ParsedAtom::FiniteClassSet(atom),
            ..
        } = term
        else {
            panic!("a mixed finite class atom expected");
        };
        assert_eq!(
            atom.multi_code_point_strings
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![3, 2]
        );
        assert!(atom.contains_empty);
        assert_eq!(atom.singleton.opcode, REGEXP_OPCODE_UNICODE_PROPERTY);
        let first = atom.singleton.operand0 as usize;
        let count = (atom.singleton.operand1 >> 1) as usize;
        let singleton_ranges = &parsed.ranges[first..first + count];
        assert!(ranges_contain(singleton_ranges, u32::from(b'a')));
        assert!(ranges_contain(singleton_ranges, u32::from(b'A')));
        let direct = RegExpProgram::compile(r"[\q{Ab}]", "iv").unwrap();
        let scoped = RegExpProgram::compile(r"(?i:[\q{Ab}])", "v").unwrap();
        assert_eq!(direct.instructions, scoped.instructions);
        assert_eq!(direct.ranges, scoped.ranges);
        let sensitive = RegExpProgram::compile(r"[\q{Ab}]", "v").unwrap();
        let disabled = RegExpProgram::compile(r"(?-i:[\q{Ab}])", "iv").unwrap();
        assert_eq!(sensitive.instructions, disabled.instructions);
        assert_eq!(sensitive.ranges, disabled.ranges);
    }

    #[test]
    fn unicode_sets_folded_property_strings_share_direct_operand_keys() {
        let original = vec![0x24c2, 0xfe0f];
        let canonical = vec![0x24dc, 0xfe0f];
        for property_name in ["Basic_Emoji", "RGI_Emoji"] {
            let property = format!(r"\p{{{property_name}}}");
            let folded = folded_class_value(&format!("[{property}]"));
            assert!(
                folded.finite.strings.contains(&canonical),
                "{property_name}"
            );
            assert!(
                !folded.finite.strings.contains(&original),
                "{property_name}"
            );
            let intersection = format!(r"[{property}&&\q{{\u24C2\uFE0F}}]");
            assert_eq!(
                folded_class_value(&intersection).finite.strings,
                BTreeSet::from([canonical.clone()]),
                "{property_name}"
            );
            let subtraction = format!(r"[{property}--\q{{\u24DC\uFE0F}}]");
            assert!(
                !folded_class_value(&subtraction)
                    .finite
                    .strings
                    .contains(&canonical),
                "{property_name}"
            );
            let reverse_subtraction = format!(r"[\q{{\u24C2\uFE0F}}--{property}]");
            assert!(
                folded_class_value(&reverse_subtraction)
                    .finite
                    .strings
                    .is_empty(),
                "{property_name}"
            );
            let mut cursor = 0;
            let sensitive =
                parse_class_set(intersection.as_bytes(), &mut cursor, CaseFolding::Sensitive)
                    .unwrap()
                    .into_nested_value();
            assert_eq!(cursor, intersection.len());
            assert_eq!(sensitive.finite.strings, BTreeSet::from([original.clone()]));
            let lower_intersection = format!(r"[{property}&&\q{{\u24DC\uFE0F}}]");
            let mut cursor = 0;
            let sensitive_lower = parse_class_set(
                lower_intersection.as_bytes(),
                &mut cursor,
                CaseFolding::Sensitive,
            )
            .unwrap()
            .into_nested_value();
            assert!(sensitive_lower.finite.strings.is_empty());
            let direct = RegExpProgram::compile(&intersection, "iv").unwrap();
            let enabled = RegExpProgram::compile(&format!("(?i:{intersection})"), "v").unwrap();
            assert_eq!(direct.instructions, enabled.instructions);
            assert_eq!(direct.ranges, enabled.ranges);
            let direct_sensitive = RegExpProgram::compile(&intersection, "v").unwrap();
            let disabled = RegExpProgram::compile(&format!("(?-i:{intersection})"), "iv").unwrap();
            assert_eq!(direct_sensitive.instructions, disabled.instructions);
            assert_eq!(direct_sensitive.ranges, disabled.ranges);
            for pattern in [property.clone(), format!("[{property}]")] {
                let v = RegExpProgram::compile(&pattern, "v").unwrap();
                let iv = RegExpProgram::compile(&pattern, "iv").unwrap();
                super::program::ValidatedRegExpProgram::from_program(&v).unwrap();
                super::program::ValidatedRegExpProgram::from_program(&iv).unwrap();
                assert_ne!(v.instructions, iv.instructions, "{property_name}");
            }
        }
    }
}
