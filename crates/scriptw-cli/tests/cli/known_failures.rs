//! The tracked ledger of expected non-green outcomes, and the hygiene tests
//! that keep it honest.
//!
//! # Why this module exists
//!
//! Rung 1c (the whole CLI suite) was documented for three batches as "compare
//! against `crates/scriptw-cli/tests/known-failures.txt`". That file never
//! existed: `.gitignore` line 3 is a bare `*.txt`, so `git add -A` dropped it
//! silently while two documents went on citing it. Nobody noticed, because
//! nobody ever got the suite to terminate -- one test hangs forever, so the
//! documented invocation carried a `--skip` and the run was never a gate.
//!
//! This module replaces "a document tells you what to expect" with three
//! mechanisms, each placed at the cheapest rung that can hold it:
//!
//! 1. **File existence -> compile time.** [`LEDGER`] is an `include_str!`.
//!    Delete, rename or re-gitignore `tests/known-failures.tsv` and
//!    `cargo check --all-targets` fails. The exact defect above becomes
//!    unreintroducible.
//! 2. **Test existence -> compile time.** One `const _` line per `cli`-target
//!    row, below. Renaming or deleting a listed test is then an E0425/E0603,
//!    not a ledger quietly pointing at nothing. [`ledger_is_well_formed`]
//!    checks the other direction: a row with no matching line, or a line with
//!    no row, fails.
//! 3. **Outcome -> libtest.** Every `fail`/`hang` row's test carries a
//!    `should_panic` attribute with a non-empty `expected` substring. That
//!    gives all three delta directions for free, with no end-of-run machinery:
//!    an expected failure with the expected message passes; a now-passing test
//!    fails with "test did not panic as expected"; a test that fails for a
//!    *different* reason fails on message mismatch. Rung 1c's exit code alone
//!    is then the gate.
//!
//! # Why `should_panic` is acceptable here and only here
//!
//! AGENTS.md bans permanent skip lists and silent expected failures, and a
//! `should_panic` attribute on its own is exactly such a silent expected
//! failure. It is acceptable in this crate *because* the ledger makes it
//! non-silent (owner task, reason, evidence) and non-permanent (the
//! `unfilled-allowed-until` expiry, and a row whose test starts passing turns
//! the suite red). Remove the ledger and keep the attributes and this crate has
//! built the thing AGENTS.md bans. The **bare** form is rejected outright: it
//! passes on any panic at all, which converts a genuine new defect into a green
//! test.
//!
//! # Today's inventory, counted rather than estimated
//!
//! Recounted at the head of batch 4, after the T24 retirement and the two
//! `throw_propagation` tests. These are the numbers rung 1c is measured
//! against, so they are re-derived rather than carried forward:
//!
//! - 601 `#[test]` attributes across `tests/cli/*.rs`; 8 of them behind
//!   `#[cfg(feature = "spec-exec-oracle")]` in `frontend.rs`, so **593 compile
//!   under default features and 592 execute** (one is ignored, in `heap.rs`).
//!   593 is the figure `--list` reports and the one a rung-1c log's
//!   `running N tests` line should show.
//!
//!   Recounted at the head of batch 5, with the exact-line `awk` above:
//!   **615** across `tests/cli/*.rs`. The two additions since batch 4 are the
//!   13 in the new `iterator_helpers.rs` and the one added to this module
//!   ([`rung_1c_chunks_cover_every_cli_area_module`], so `known_failures::` is
//!   now a **5**-test chunk, not 4). `main.rs` declares `mod iterator_helpers;`,
//!   so **607 compile** (615 minus the 8 behind `spec-exec-oracle`). Settle it
//!   with `--list`, never by arithmetic on this comment.
//!
//!   Recounted again by the batch-6 integrator: **617** across
//!   `tests/cli/*.rs`, so **609 compile** under default features and 608 run.
//!   The two additions are one `iterator_helpers` test and one `date` test; the
//!   18th area module, `frontend_test262_subset`, is a *move* of the one
//!   8.7 GiB `frontend` test into a chunk of its own and adds no test. **609 is
//!   the number each chunk's `ran + filtered_out` must now sum to**; every
//!   chunk banked before batch 6 recorded 607 and was right at its own head.
//!
//!   The source scan below reads `tests/cli/*.rs` from disk, so it sees all 617
//!   whether or not the `mod` line landed — while the compiled target would see
//!   none of an undeclared module's tests. That gap is not hypothetical:
//!   `iterator_helpers.rs` shipped with 13 tests, its own `run_chunk` line, and
//!   no `mod` declaration, and every check in this file was green while its
//!   chunk selected nothing and banked. [`rung_1c_chunks_cover_every_cli_area_module`]
//!   now reads `main.rs`'s `mod` list and asserts it against the files on disk
//!   in both directions, so that state is a sub-millisecond red rather than a
//!   paragraph somebody has to remember.
//!
//!   Recounted at the batch-7 **integration** head, same exact-line `awk`:
//!   **620** across `tests/cli/*.rs`, so **612 compile** under default features
//!   and **611 execute**. The three additions are in two modules, not one:
//!   `regexp.rs` 33 -> 35 and `date.rs` 17 -> 18. The batch-7 split of
//!   `language.rs` into `language` (45), `language_errors` (29) and
//!   `language_numerics` (31) is a **pure move** and adds nothing: the three
//!   files' `awk` counts sum to 105, and the set of function names across them
//!   is identical to the pre-split `language.rs`. It takes the area-module
//!   count 18 -> 20. **612 is the number each chunk's `ran + filtered_out`
//!   must now sum to**, and unlike every previous batch that half is
//!   *measured*: `--list | tail -1` printed `612 tests` at integration and
//!   620 - 8 agrees independently.
//!
//!   The write lane's figure was 619/611/610 and it was right when it counted;
//!   it went stale **inside the batch**, because a concurrent `date.rs` lane
//!   landed the 620th test in the same commit. That is the reason this file
//!   records how each number was obtained rather than only what it is — a
//!   number here can be wrong without anyone editing the line it sits on.
//! - 3 more in `tests/perf.rs`, 1 in `tests/async_generator.rs`, and 1 in
//!   `tests/cache.rs`. Do not
//!   restate the crate-wide total here as a literal: it read `605` across at
//!   least three batches while the true figure moved 621 -> 624, because no
//!   assertion reads it (the only floor is [`MINIMUM_SCANNED_TESTS`] = 500) so
//!   nothing ever caught it. Recount instead, with the same exact-line `awk`
//!   over `crates/scriptw-cli/tests/cli/*.rs crates/scriptw-cli/tests/*.rs`.
//! - 4 ignore attributes: `heap.rs` (1) and `perf.rs` (3).
//! - **0** `should_panic` attributes. It was 0 before this module existed and 2
//!   after it; the batch-3 T24 row was retired in batch 4 and the batch-5 T17
//!   row (`binary_data::run_wasm_backend_succeeds_for_atomics_wait_core_fixture`,
//!   the declared `Atomics.wait` hang) in batch 6, when it started passing and
//!   libtest reported "test did not panic as expected". Zero is the target
//!   state, not an omission: every remaining non-green outcome in this crate is
//!   an `ignore` with an owner.
//!
//! Reproduce with the **exact-line** form, which is what [`scan_source`] itself
//! matches. A substring `grep -h '#\[test\]'` over these files currently returns
//! extra hits, because it matches prose lines *in this module* that name the
//! attribute — including this one. The `--list` form is the authority for the
//! compiled/executing split, because it is the only one that resolves `cfg`.
//!
//! ```sh
//! awk '/^[[:space:]]*#\[test\][[:space:]]*$/{n++} END{print n}' \
//!   crates/scriptw-cli/tests/cli/*.rs crates/scriptw-cli/tests/*.rs
//! grep -rn '#\[ignore' crates/scriptw-cli/tests/
//! grep -rn '#\[should_panic' crates/scriptw-cli/tests/
//! cargo test -p scriptw-cli --test cli -- --list | tail -1
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The ledger, pulled in at compile time. See enforcement level 1 above.
const LEDGER: &str = include_str!("../known-failures.tsv");

/// The batch this checkout belongs to.
///
/// Bumped by hand at the start of a batch. That single edit is what turns the
/// `unfilled-allowed-until` header into a real expiry rather than a comment: an
/// `unfilled` row that outlives its deadline fails [`ledger_is_well_formed`].
///
/// # Why the `unfilled` row outlived its first deadline, said out loud
///
/// This read 3 through batches 4 and 5, because the header in
/// `known-failures.tsv` read `batch-4` and bumping the constant while the
/// `unfilled` row is alive turns rung 1c red. The bump and the fill are ONE
/// edit, and the fill needs a completed rung 1c. Batch 7 took the alternative
/// the assertion names in its own message for the second time: the constant
/// went to 7 and the header was extended to `batch-8`.
///
/// # Batch 8: the set is complete, and the `unfilled` row is gone
///
/// The row's own text set the condition for deleting it rather than filling it:
/// *"the moment `language`, `language_errors` and `language_numerics` all bank
/// green"*. Batch 7's rung 5 met it with the following recorded results:
///
/// - `language` **45**, `language_errors` **29**, `language_numerics` **31**,
///   each `ok` with zero failures, each from ONE invocation. `language::` had
///   never produced a single-invocation verdict at any head across four
///   batches -- at 105 tests in one libtest process it OOM-SIGKILLed three
///   times, and batch 6 could only reach 105/105 by the UNION of a chunk run
///   and a tail run, which this row correctly refused to accept as a verdict.
/// - `rung1c: all chunks done`. **All 20 chunks hold a verdict at that head**,
///   and every one of the eight chunks that executed printed
///   `ran + filtered_out = 612`, the compiled-test total, so no chunk silently
///   selected a subset.
/// - The only declared non-pass outcome across the suite is the `heap` ignore
///   (T05), which has its own row. The surviving set is therefore empty, and an
///   `unfilled` placeholder for it would now be a claim about nothing.
///
/// So batch 8 deleted the row. The three companion edits the row's recipe
/// called for were made in the same patch, and are recorded here because their
/// *absence* is what a later reader would misread:
///
/// 1. The `# unfilled-allowed-until: batch-8` header STAYS. Its absence is
///    `MissingExpiryHeader` whether or not an `unfilled` row exists; with no
///    row it simply goes inert without becoming wrong.
/// 2. `CURRENT_BATCH` STAYS at 7. No second bump is owed, and the deadline
///    check lives inside `if unfilled_rows > 0`, so with no `unfilled` row it
///    is not merely satisfied but never evaluated. Bumping it to 8 would make
///    the surviving comparison `8 < 8` and turn rung 1c red for no reason.
/// 3. The `heap` ignore row is untouched.
///
/// # If a later batch needs a placeholder again
///
/// Add a real row, never a second `unfilled` one. A `fail`/`hang` row names a
/// real test and carries `should_panic` with a non-empty `expected`, so libtest
/// enforces it in both delta directions; an `unfilled` row enforces nothing and
/// only borrows time against the expiry header. If a genuinely unmeasured set
/// reappears, extend the header in the same patch that adds the row, and write
/// the delete condition into the row's own reason column the way the batch-3
/// row did -- that one sentence is what let batch 8 close this without
/// re-litigating what "done" meant.
///
/// # Counting, at the batch-8 write head
///
/// Recount rather than trusting this paragraph. Batch 7 recorded a figure that
/// went stale INSIDE the batch because a concurrent lane landed a test in the
/// same commit, and the same is true here: at the moment the RE-VERDICT lane
/// counted, the exact-line `awk` over `tests/cli/*.rs` returned **622** -- 620
/// at the batch-7 integration head, plus one from that lane
/// (`regexp::run_wasm_backend_succeeds_for_regexp_identity_escape_solidus_fixture`,
/// taking `regexp.rs` 35 -> 36) and one from a concurrent batch-8 lane. 8 stay
/// behind `spec-exec-oracle` in `frontend.rs` and one is `#[ignore]`d in
/// `heap.rs`. Settle the compiled/executing split with `--list`, never by
/// arithmetic on this comment, and expect the `regexp` chunk to re-run on the
/// counts guard.
const CURRENT_BATCH: u32 = 7;

/// Header line carrying the `unfilled` expiry, e.g.
/// `# unfilled-allowed-until: batch-7`.
const UNFILLED_HEADER_PREFIX: &str = "# unfilled-allowed-until: batch-";

/// The only test name an `unfilled` row may carry. Keeping the sentinel out of
/// the real name space stops a placeholder from shadowing a real test.
const UNFILLED_SENTINEL: &str = "UNFILLED";

/// Number of tab-separated columns in a ledger data row.
const LEDGER_COLUMNS: usize = 6;

/// Column names, in order, for diagnostics.
const LEDGER_COLUMN_NAMES: [&str; LEDGER_COLUMNS] = [
    "target",
    "test",
    "state",
    "owner_task",
    "reason",
    "evidence",
];

/// Attribute spellings the source scanner recognises.
///
/// These are ordinary string constants rather than attributes, so the lines
/// they sit on do not begin with `#[` and the scanner does not mistake this
/// file's own constants for declarations.
const TEST_ATTRIBUTE: &str = "#[test]";
const SHOULD_PANIC_PREFIX: &str = "#[should_panic";
const IGNORE_PREFIX: &str = "#[ignore";

/// Prefix of the enforcement-level-2 statements further down this file, once
/// their whitespace has been normalised to single spaces.
const CONST_ASSERT_PREFIX: &str = "const _: fn() = crate::";

/// What an enforcement-level-2 statement opens with, before `crate::`.
///
/// The scan keys on this rather than on [`CONST_ASSERT_PREFIX`] because
/// `rustfmt` wraps a long assertion after the `=`, and because this file's own
/// declaration of `CONST_ASSERT_PREFIX` contains that prefix as a string
/// literal — a substring scan would match the literal and read the ledger back
/// as one bogus name. A line opening `const _: fn()` is an assertion and
/// nothing else is.
const CONST_ASSERT_OPENER: &str = "const _: fn()";

/// Floor on the number of test declarations the source scan must find.
///
/// An anti-vacuity guard, not a budget. The scan reads sources from
/// `CARGO_MANIFEST_DIR` at run time; if that ever resolved somewhere
/// unexpected it would find nothing and every hygiene check below would pass
/// for the worst possible reason. Today's count is 602 across the three
/// targets. This bound fails when the count *shrinks*, which is the direction
/// that means the scan broke; a growing suite must never require an edit here.
const MINIMUM_SCANNED_TESTS: usize = 500;

/// The tracked runner that executes rung 1c as resumable per-module chunks,
/// repo-root relative.
///
/// Rung 1c is ~2.5 h on a 4-CPU box and libtest has no resume, so the suite is
/// run one area module at a time with a done-file. That makes the chunk SET a
/// load-bearing artefact: a module with no chunk is silently never run, and the
/// result still reads as a complete rung 1c.
/// [`rung_1c_chunks_cover_every_cli_area_module`] reads this file back and
/// closes that hole at rung 0, per AGENTS.md ("code invariants before test
/// invariants").
const RUNG_1C_RUNNER: &str = "scripts/rung1c-chunks.sh";

/// What a chunk declaration in [`RUNG_1C_RUNNER`] opens with.
///
/// The shell function's own definition line reads `run_chunk() {`, with no
/// space before the parenthesis, so it is not mistaken for a declaration.
const RUN_CHUNK_OPENER: &str = "run_chunk ";

/// The libtest flag a chunk uses to exclude an overlapping module.
const SKIP_FLAG: &str = "--skip";

/// Floor on the number of chunks parsed out of [`RUNG_1C_RUNNER`].
///
/// An anti-vacuity guard, exactly like [`MINIMUM_SCANNED_TESTS`]: if the parse
/// ever found nothing, the coverage check below would pass for the worst
/// possible reason. Today there are 20 (`grep -c '^run_chunk ' ` the runner —
/// this line said 17 while the answer was 18, and then 20).
///
/// The floor deliberately stays well below the true count. Raising it to track
/// the count would create an edit obligation on every split, and it would buy
/// nothing: [`rung_1c_chunks_cover_every_cli_area_module`] already fails on a
/// missing chunk for any real module, which is the property that matters.
const MINIMUM_RUNG_1C_CHUNKS: usize = 10;

/// The four states a ledger row can be in.
///
/// Closed domain. [`FromStr`] rejects everything else and every consumer
/// matches exhaustively -- no `_` arm, per AGENTS.md and
/// `docs/rust-rewrite/contracts/closed-name-domains.md`. Spelling this column
/// as `&str` is precisely the mistake those contracts exist to prevent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum State {
    /// The test runs and fails. It must carry a `should_panic` attribute with a
    /// non-empty `expected` substring.
    Fail,
    /// The test runs and never returns. It must carry a `should_panic`
    /// attribute with a non-empty `expected` substring, and `main.rs` routes it
    /// through the guarded subprocess path so the hang becomes a bounded
    /// failure instead of a spinning suite.
    Hang,
    /// The test is excluded from the default run by an `ignore` attribute.
    Ignored,
    /// A placeholder for an unmeasured failure set. Expires.
    Unfilled,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Fail => "fail",
            State::Hang => "hang",
            State::Ignored => "ignored",
            State::Unfilled => "unfilled",
        }
    }

    /// Does a row in this state require an outcome-enforcing `should_panic`?
    fn requires_should_panic(self) -> bool {
        match self {
            State::Fail | State::Hang => true,
            State::Ignored | State::Unfilled => false,
        }
    }

    /// Does a row in this state require an `ignore` attribute?
    fn requires_ignore(self) -> bool {
        match self {
            State::Ignored => true,
            State::Fail | State::Hang | State::Unfilled => false,
        }
    }

    /// Does a row in this state name a test that must actually exist?
    fn names_a_real_test(self) -> bool {
        match self {
            State::Fail | State::Hang | State::Ignored => true,
            State::Unfilled => false,
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for State {
    type Err = LedgerError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "fail" => Ok(State::Fail),
            "hang" => Ok(State::Hang),
            "ignored" => Ok(State::Ignored),
            "unfilled" => Ok(State::Unfilled),
            other => Err(LedgerError::UnknownState(other.to_string())),
        }
    }
}

/// The cargo test targets of this crate. Closed domain.
///
/// A libtest name is rooted at its *target's* crate root, so the same bare
/// function name can legitimately exist in two targets. Carrying the target as
/// its own column, rather than smuggling it into the name, is what keeps
/// `(target, test)` a real key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TestTarget {
    /// `tests/async_generator.rs`.
    AsyncGenerator,
    /// `tests/cache.rs`.
    Cache,
    /// `tests/cli/` -- `main.rs` plus the area modules.
    Cli,
    /// `tests/cli_output_ending_structure.rs`.
    CliOutputEndingStructure,
    /// `tests/conformance_closure.rs`.
    ConformanceClosure,
    /// `tests/differential_corpus.rs`.
    DifferentialCorpus,
    /// `tests/differential_robustness.rs`.
    DifferentialRobustness,
    /// `tests/differential_scenario_pair.rs`.
    DifferentialScenarioPair,
    /// `tests/differential_worker.rs`.
    DifferentialWorker,
    /// `tests/fake_suite_publication.rs`.
    FakeSuitePublication,
    /// `tests/perf.rs`.
    Perf,
    /// `tests/product_artifact_execution.rs`.
    ProductArtifactExecution,
    /// `tests/test262_case_deadline.rs`.
    Test262CaseDeadline,
    /// `tests/test262_execution_list.rs`.
    Test262ExecutionList,
    /// `tests/test262_verdict_command_structure.rs`.
    Test262VerdictCommandStructure,
}

impl TestTarget {
    fn as_str(self) -> &'static str {
        match self {
            TestTarget::AsyncGenerator => "async_generator",
            TestTarget::Cache => "cache",
            TestTarget::Cli => "cli",
            TestTarget::CliOutputEndingStructure => "cli_output_ending_structure",
            TestTarget::ConformanceClosure => "conformance_closure",
            TestTarget::DifferentialCorpus => "differential_corpus",
            TestTarget::DifferentialRobustness => "differential_robustness",
            TestTarget::DifferentialScenarioPair => "differential_scenario_pair",
            TestTarget::DifferentialWorker => "differential_worker",
            TestTarget::FakeSuitePublication => "fake_suite_publication",
            TestTarget::Perf => "perf",
            TestTarget::ProductArtifactExecution => "product_artifact_execution",
            TestTarget::Test262CaseDeadline => "test262_case_deadline",
            TestTarget::Test262ExecutionList => "test262_execution_list",
            TestTarget::Test262VerdictCommandStructure => "test262_verdict_command_structure",
        }
    }

    /// Map a `tests/<stem>.rs` file to its target.
    ///
    /// `Cli` is deliberately absent: that target's root is `tests/cli/main.rs`,
    /// found by directory rather than by file stem. A new `tests/<name>.rs`
    /// therefore lands here as `None` and fails the scan loudly instead of
    /// being silently unaudited.
    fn from_top_level_stem(stem: &str) -> Option<Self> {
        match stem {
            "async_generator" => Some(TestTarget::AsyncGenerator),
            "cache" => Some(TestTarget::Cache),
            "cli_output_ending_structure" => Some(TestTarget::CliOutputEndingStructure),
            "conformance_closure" => Some(TestTarget::ConformanceClosure),
            "differential_corpus" => Some(TestTarget::DifferentialCorpus),
            "differential_robustness" => Some(TestTarget::DifferentialRobustness),
            "differential_scenario_pair" => Some(TestTarget::DifferentialScenarioPair),
            "differential_worker" => Some(TestTarget::DifferentialWorker),
            "fake_suite_publication" => Some(TestTarget::FakeSuitePublication),
            "perf" => Some(TestTarget::Perf),
            "product_artifact_execution" => Some(TestTarget::ProductArtifactExecution),
            "test262_case_deadline" => Some(TestTarget::Test262CaseDeadline),
            "test262_execution_list" => Some(TestTarget::Test262ExecutionList),
            "test262_verdict_command_structure" => Some(TestTarget::Test262VerdictCommandStructure),
            _ => None,
        }
    }
}

impl fmt::Display for TestTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TestTarget {
    type Err = LedgerError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "async_generator" => Ok(TestTarget::AsyncGenerator),
            "cache" => Ok(TestTarget::Cache),
            "cli" => Ok(TestTarget::Cli),
            "cli_output_ending_structure" => Ok(TestTarget::CliOutputEndingStructure),
            "conformance_closure" => Ok(TestTarget::ConformanceClosure),
            "differential_corpus" => Ok(TestTarget::DifferentialCorpus),
            "differential_robustness" => Ok(TestTarget::DifferentialRobustness),
            "differential_scenario_pair" => Ok(TestTarget::DifferentialScenarioPair),
            "differential_worker" => Ok(TestTarget::DifferentialWorker),
            "fake_suite_publication" => Ok(TestTarget::FakeSuitePublication),
            "perf" => Ok(TestTarget::Perf),
            "product_artifact_execution" => Ok(TestTarget::ProductArtifactExecution),
            "test262_case_deadline" => Ok(TestTarget::Test262CaseDeadline),
            "test262_execution_list" => Ok(TestTarget::Test262ExecutionList),
            "test262_verdict_command_structure" => Ok(TestTarget::Test262VerdictCommandStructure),
            other => Err(LedgerError::UnknownTarget(other.to_string())),
        }
    }
}

// Use the compiler's permanent ownership domain; report text is validated once.
use scriptw_engine::ConformanceOwner;

/// One ledger row. Borrows from [`LEDGER`], which is `'static`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row {
    pub(crate) target: TestTarget,
    pub(crate) test: &'static str,
    pub(crate) state: State,
    pub(crate) owner_task: ConformanceOwner,
    pub(crate) reason: &'static str,
    pub(crate) evidence: &'static str,
    /// 1-based line in `known-failures.tsv`, for diagnostics only.
    pub(crate) line: usize,
}

impl Row {
    fn sort_key(&self) -> (&'static str, &'static str) {
        (self.target.as_str(), self.test)
    }
}

/// The parsed ledger.
#[derive(Clone, Debug)]
pub(crate) struct Ledger {
    /// Batch number taken from the `unfilled-allowed-until` header.
    unfilled_allowed_until: u32,
    rows: Vec<Row>,
}

impl Ledger {
    fn row_for(&self, target: TestTarget, test: &str) -> Option<&Row> {
        self.rows
            .iter()
            .find(|row| row.target == target && row.test == test)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LedgerError {
    UnknownState(String),
    UnknownTarget(String),
    MalformedConformanceOwner(String),
    WrongColumnCount { line: usize, found: usize },
    EmptyColumn { line: usize, column: &'static str },
    PaddedColumn { line: usize, column: &'static str },
    OutOfOrder { line: usize },
    Duplicate { line: usize, test: String },
    UnfilledNameMismatch { line: usize, test: String },
    MissingExpiryHeader,
    DuplicateExpiryHeader { line: usize },
    MalformedExpiryHeader { line: usize, value: String },
    NoRows,
}

impl fmt::Display for LedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = "crates/scriptw-cli/tests/known-failures.tsv";
        match self {
            LedgerError::UnknownState(found) => write!(
                formatter,
                "{file}: unknown state `{found}`; the closed set is fail | hang | ignored | unfilled"
            ),
            LedgerError::UnknownTarget(found) => write!(
                formatter,
                "{file}: unknown target `{found}`; the closed set is async_generator | cli | perf. \
                 A new cargo test target means extending TestTarget, not widening this column"
            ),
            LedgerError::MalformedConformanceOwner(found) => write!(
                formatter,
                "{file}: owner_task `{found}` is not a conformance owner; expected T00 through T29"
            ),
            LedgerError::WrongColumnCount { line, found } => write!(
                formatter,
                "{file}:{line}: found {found} tab-separated columns, expected {} \
                 (target, test, state, owner_task, reason, evidence)",
                LEDGER_COLUMNS
            ),
            LedgerError::EmptyColumn { line, column } => {
                write!(formatter, "{file}:{line}: column `{column}` is empty")
            }
            LedgerError::PaddedColumn { line, column } => write!(
                formatter,
                "{file}:{line}: column `{column}` has leading or trailing whitespace; \
                 the separator is a tab, so padding is always a mistake"
            ),
            LedgerError::OutOfOrder { line } => write!(
                formatter,
                "{file}:{line}: rows must be sorted ascending by (target, test); \
                 this row is not greater than the one before it"
            ),
            LedgerError::Duplicate { line, test } => write!(
                formatter,
                "{file}:{line}: duplicate row for `{test}`; (target, test) is the key"
            ),
            LedgerError::UnfilledNameMismatch { line, test } => write!(
                formatter,
                "{file}:{line}: state `unfilled` requires the test column to be exactly `{}`, \
                 found `{test}`. A placeholder must never shadow a real test name",
                UNFILLED_SENTINEL
            ),
            LedgerError::MissingExpiryHeader => write!(
                formatter,
                "{file}: no `{}<n>` header; without it an `unfilled` row would be permanent",
                UNFILLED_HEADER_PREFIX
            ),
            LedgerError::DuplicateExpiryHeader { line } => write!(
                formatter,
                "{file}:{line}: a second `{}<n>` header; there is exactly one deadline",
                UNFILLED_HEADER_PREFIX
            ),
            LedgerError::MalformedExpiryHeader { line, value } => write!(
                formatter,
                "{file}:{line}: expiry header value `{value}` is not a batch number"
            ),
            LedgerError::NoRows => write!(
                formatter,
                "{file}: parsed zero data rows. Either every row was lost or the parser is \
                 reading the wrong file; both are failures"
            ),
        }
    }
}

/// Parse [`LEDGER`].
///
/// Returns `Err` on anything the closed domains do not admit. The one caller
/// that runs inside every test ([`execution_path`]) treats an error
/// conservatively rather than panicking; [`ledger_is_well_formed`] is where a
/// malformed ledger is reported.
pub(crate) fn parse_ledger() -> Result<Ledger, LedgerError> {
    let mut rows: Vec<Row> = Vec::new();
    let mut unfilled_allowed_until: Option<u32> = None;

    for (index, raw) in LEDGER.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            if let Some(value) = trimmed.strip_prefix(UNFILLED_HEADER_PREFIX) {
                if unfilled_allowed_until.is_some() {
                    return Err(LedgerError::DuplicateExpiryHeader { line });
                }
                let value = value.trim();
                let batch =
                    value
                        .parse::<u32>()
                        .map_err(|_| LedgerError::MalformedExpiryHeader {
                            line,
                            value: value.to_string(),
                        })?;
                unfilled_allowed_until = Some(batch);
            }
            continue;
        }

        let columns: Vec<&'static str> = raw.split('\t').collect();
        if columns.len() != LEDGER_COLUMNS {
            return Err(LedgerError::WrongColumnCount {
                line,
                found: columns.len(),
            });
        }
        for (column, name) in columns.iter().zip(LEDGER_COLUMN_NAMES) {
            if column.is_empty() {
                return Err(LedgerError::EmptyColumn { line, column: name });
            }
            if column.trim() != *column {
                return Err(LedgerError::PaddedColumn { line, column: name });
            }
        }

        let row = Row {
            target: columns[0].parse::<TestTarget>()?,
            test: columns[1],
            state: columns[2].parse::<State>()?,
            owner_task: columns[3]
                .parse::<ConformanceOwner>()
                .map_err(|_| LedgerError::MalformedConformanceOwner(columns[3].to_owned()))?,
            reason: columns[4],
            evidence: columns[5],
            line,
        };

        if (row.state == State::Unfilled) != (row.test == UNFILLED_SENTINEL) {
            return Err(LedgerError::UnfilledNameMismatch {
                line,
                test: row.test.to_string(),
            });
        }

        if let Some(previous) = rows.last() {
            if previous.sort_key() == row.sort_key() {
                return Err(LedgerError::Duplicate {
                    line,
                    test: row.test.to_string(),
                });
            }
            if previous.sort_key() > row.sort_key() {
                return Err(LedgerError::OutOfOrder { line });
            }
        }
        rows.push(row);
    }

    if rows.is_empty() {
        return Err(LedgerError::NoRows);
    }
    let unfilled_allowed_until = unfilled_allowed_until.ok_or(LedgerError::MissingExpiryHeader)?;
    Ok(Ledger {
        unfilled_allowed_until,
        rows,
    })
}

// -------------------------------------------------------------------------
// Enforcement level 2: test existence, checked by the compiler.
//
// One line per `cli`-target row that names a real test. A rename or a deletion
// is then E0425 (no such function) or E0603 (private), caught by `cargo xc` in
// seconds rather than by a suite run measured in hours that nobody would think
// to attribute to a stale ledger. `ledger_is_well_formed` checks the reverse
// direction by reading these very lines back out of this file.
//
// `perf` and `async_generator` are separate crates, so `crate::` cannot reach
// them; their rows are covered by the source scan instead.
// -------------------------------------------------------------------------

// The T17 assertion
// (`binary_data::run_wasm_backend_succeeds_for_atomics_wait_core_fixture`) was
// retired together with its ledger row in batch 6, when the declared hang
// started passing and libtest produced the "test did not panic as expected"
// signal the row existed to produce. `Atomics.wait` now returns instead of
// blocking, so the test needs no guarded child, no `should_panic` and no row.
const _: fn() = crate::heap::run_wasm_backend_succeeds_for_heap_page_boundary_stress_fixture;
// The T24 assertion
// (`language_errors::run_wasm_backend_gives_a_runtime_error_a_message_distinct_from_its_name`
// -- it was in `language::` until batch 7 split that module three ways for
// memory; see the header of `tests/cli/language.rs`)
// was retired together with its ledger row when the repair landed:
// `emit_runtime_error_object` now defines `message` from its message argument,
// and `data.rs`'s `RUNTIME_ERROR_MESSAGE_LITERALS` interns the strings that
// makes possible. `ledger_is_well_formed` rejects an assertion with no row, so
// this had to go in the same patch as the row. The test itself stays and now
// passes.

// -------------------------------------------------------------------------
// Routing: which `scriptw` invocations must run as a guarded subprocess.
// -------------------------------------------------------------------------

/// How `main.rs`'s `Command::output` should execute a `scriptw` invocation.
///
/// Closed domain, matched exhaustively at its single call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionPath {
    /// The fast path: call `scriptw_cli::run_cli_capture` in this process. No
    /// process spawn.
    ///
    /// Bounded, but by a leaked worker thread rather than by a kill: `main.rs`
    /// runs the call on its own thread and gives up after the hang timeout. A
    /// blocked call therefore fails its test instead of consuming a libtest
    /// worker forever. That matters because this is the path a **new**,
    /// undeclared hang takes — the guarded path is reachable only for a test the
    /// ledger already names — so leaving it unbounded left rung 1c able to spin
    /// forever under the documented `--test-threads=2` invocation.
    InProcess,
    /// Spawn the real `scriptw` binary, poll it, and kill it after the hang
    /// timeout. Costs a process spawn; survives a test that never returns.
    GuardedSubprocess,
}

/// Thread name libtest uses when it does not spawn a worker per test, i.e.
/// under `--test-threads=1`. The test's own name is unavailable there.
const MAIN_THREAD_NAME: &str = "main";

/// Decide how to run a `scriptw` invocation, given the current thread's name.
///
/// libtest names each worker thread after the test it is running, so the thread
/// name *is* the libtest name, and it is the only routing key reachable from
/// inside a test body without threading state through ~590 call sites.
///
/// Unknown names and a malformed ledger take the guarded path. Serial libtest
/// uses `main`, where the individual test name is unavailable. That route may
/// use the bounded in-process path only when the ledger declares no CLI hang.
/// Three consequences worth knowing:
///
/// - Under `--test-threads=1`, one declared CLI hang guards every invocation
///   from `main`. With no declared CLI hangs, serial tests use the same bounded
///   in-process path as named non-hang tests. Each call still creates a fresh
///   Engine and ECMAScript Realm. Explicit process controls remain subprocesses.
/// - If the ledger fails to parse, every test takes the guarded path. That is
///   loud rather than silent: [`ledger_is_well_formed`] reports the parse error
///   in the same run — but read that one diagnostic early, because the visible
///   symptom is 588 cold subprocess spawns and a run that looks merely slow.
/// - Routing by row means a hang in an *undeclared* test never reaches the
///   guarded path. It is bounded on the other path instead; see
///   [`ExecutionPath::InProcess`].
pub(crate) fn execution_path(thread_name: Option<&str>) -> ExecutionPath {
    execution_path_with_ledger(thread_name, parse_ledger())
}

fn execution_path_with_ledger(
    thread_name: Option<&str>,
    ledger: Result<Ledger, LedgerError>,
) -> ExecutionPath {
    let Some(name) = thread_name else {
        return ExecutionPath::GuardedSubprocess;
    };
    if name.is_empty() {
        return ExecutionPath::GuardedSubprocess;
    }
    let Ok(ledger) = ledger else {
        return ExecutionPath::GuardedSubprocess;
    };
    if name == MAIN_THREAD_NAME {
        return if ledger
            .rows
            .iter()
            .any(|row| row.target == TestTarget::Cli && row.state == State::Hang)
        {
            ExecutionPath::GuardedSubprocess
        } else {
            ExecutionPath::InProcess
        };
    }
    match ledger.row_for(TestTarget::Cli, name) {
        None => ExecutionPath::InProcess,
        Some(row) => match row.state {
            State::Hang => ExecutionPath::GuardedSubprocess,
            State::Fail | State::Ignored | State::Unfilled => ExecutionPath::InProcess,
        },
    }
}

// -------------------------------------------------------------------------
// Source scanning.
// -------------------------------------------------------------------------

/// A `should_panic` attribute as written.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ShouldPanicAttribute {
    /// No arguments. Always rejected: it passes on ANY panic, which turns a
    /// genuine new defect into a green test.
    Bare,
    /// `(expected = "...")`, carrying the substring.
    Expected(String),
}

/// An `ignore` attribute as written.
#[derive(Clone, Debug, PartialEq, Eq)]
enum IgnoreAttribute {
    /// No reason. Rejected: an undocumented skip.
    Bare,
    /// `= "..."`, carrying the reason.
    Reason(String),
}

/// One test function found in a source file.
#[derive(Clone, Debug)]
struct TestDeclaration {
    target: TestTarget,
    /// libtest name within the target: `module::function` for the `cli`
    /// target's area modules, bare `function` for a target whose tests live in
    /// its crate root.
    name: String,
    /// Repo-relative source path, for diagnostics.
    source: String,
    /// 1-based line of the `fn` item.
    line: usize,
    should_panic: Option<ShouldPanicAttribute>,
    ignore: Option<IgnoreAttribute>,
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    manifest_dir()
        .parent()
        .and_then(Path::parent)
        .expect("CARGO_MANIFEST_DIR is crates/scriptw-cli, so it has two ancestors")
        .to_path_buf()
}

fn repo_relative(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// `.rs` files directly inside `directory`, sorted so diagnostics are stable.
fn rust_sources_in(directory: &Path) -> Vec<PathBuf> {
    let entries = std::fs::read_dir(directory).unwrap_or_else(|error| {
        panic!("{}: could not read directory: {error}", directory.display())
    });
    let mut paths: Vec<PathBuf> = entries
        .map(|entry| {
            entry
                .unwrap_or_else(|error| {
                    panic!("{}: could not read entry: {error}", directory.display())
                })
                .path()
        })
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("rs")
        })
        .collect();
    paths.sort();
    paths
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .expect("a .rs file has a stem")
        .to_string_lossy()
        .into_owned()
}

/// Scan every test source of every target in this crate.
///
/// Panics with a pointed message rather than skipping anything: a source this
/// function cannot classify is an unaudited test file, which is the failure
/// mode the whole module exists to prevent.
fn scan_test_sources() -> Vec<TestDeclaration> {
    let tests_dir = manifest_dir().join("tests");
    let mut declarations = Vec::new();

    for path in rust_sources_in(&tests_dir) {
        let stem = file_stem(&path);
        let relative = repo_relative(&path);
        let target = TestTarget::from_top_level_stem(&stem).unwrap_or_else(|| {
            panic!(
                "{relative}: `{stem}` is a cargo integration target that TestTarget does not know \
                 about. Add a variant -- and, if it can produce a non-green outcome, a ledger row \
                 -- rather than leaving a whole test target unaudited."
            )
        });
        declarations.extend(scan_source(target, None, &path));
    }

    let cli_dir = tests_dir.join("cli");
    let mut saw_this_file = false;
    for path in rust_sources_in(&cli_dir) {
        let stem = file_stem(&path);
        if stem == "known_failures" {
            saw_this_file = true;
        }
        // `main.rs` is the `cli` target's crate root, so anything declared
        // there would carry a bare libtest name.
        let module = if stem == "main" { None } else { Some(stem) };
        declarations.extend(scan_source(TestTarget::Cli, module, &path));
    }
    assert!(
        saw_this_file,
        "the source scan did not find tests/cli/known_failures.rs. CARGO_MANIFEST_DIR is not \
         resolving to this crate, so every hygiene check here would pass vacuously."
    );

    assert!(
        declarations.len() >= MINIMUM_SCANNED_TESTS,
        "the source scan found only {} test declarations, below the anti-vacuity floor of {}. \
         Either the scan is reading the wrong tree or the parser stopped recognising the test \
         attribute.",
        declarations.len(),
        MINIMUM_SCANNED_TESTS
    );

    declarations
}

/// Extract the function name from an already-trimmed item line.
fn function_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    let rest = rest.strip_prefix("fn ")?;
    let end =
        rest.find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))?;
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn parse_should_panic(attribute: &str, source: &str, line: usize) -> ShouldPanicAttribute {
    let rest = attribute
        .strip_prefix(SHOULD_PANIC_PREFIX)
        .expect("caller matched the prefix");
    if rest == "]" {
        return ShouldPanicAttribute::Bare;
    }
    let expected = rest
        .strip_prefix("(expected = \"")
        .and_then(|text| text.strip_suffix("\")]"))
        .unwrap_or_else(|| {
            panic!(
                "{source}:{line}: unsupported attribute spelling `{attribute}`. This hygiene check \
                 understands only the rustfmt form with a single `expected = \"...\"` string, so \
                 that the expected substring can be read back and checked."
            )
        });
    ShouldPanicAttribute::Expected(expected.to_string())
}

fn parse_ignore(attribute: &str, source: &str, line: usize) -> IgnoreAttribute {
    let rest = attribute
        .strip_prefix(IGNORE_PREFIX)
        .expect("caller matched the prefix");
    if rest == "]" {
        return IgnoreAttribute::Bare;
    }
    let reason = rest
        .strip_prefix(" = \"")
        .and_then(|text| text.strip_suffix("\"]"))
        .unwrap_or_else(|| {
            panic!(
                "{source}:{line}: unsupported attribute spelling `{attribute}`. Use the \
                 `= \"<owner task> <reason>\"` form so the reason can be read back."
            )
        });
    IgnoreAttribute::Reason(reason.to_string())
}

/// Parse one source file into its test declarations.
///
/// The parser is deliberately literal: attributes must be single-line, which
/// every attribute in this crate's tests already is. A multi-line attribute
/// panics rather than being silently mis-attributed, and the per-file count
/// assertion below catches any declaration the walk fails to see.
fn scan_source(target: TestTarget, module: Option<String>, path: &Path) -> Vec<TestDeclaration> {
    let source = repo_relative(path);
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{source}: could not read: {error}"));

    let mut declarations = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();

        if trimmed.starts_with("#[") {
            assert!(
                trimmed.ends_with(']'),
                "{source}:{line}: multi-line attribute `{trimmed}`. Keep attributes on one line in \
                 this crate's tests; the ledger hygiene checks read them textually."
            );
            pending.push(trimmed.to_string());
            continue;
        }
        // Comments and blank lines may legally separate an attribute from its
        // item, so they do not reset the pending set.
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        let Some(name) = function_name(trimmed) else {
            pending.clear();
            continue;
        };
        if !pending
            .iter()
            .any(|attribute| attribute.as_str() == TEST_ATTRIBUTE)
        {
            pending.clear();
            continue;
        }

        let should_panic = pending
            .iter()
            .find(|attribute| attribute.starts_with(SHOULD_PANIC_PREFIX))
            .map(|attribute| parse_should_panic(attribute, &source, line));
        let ignore = pending
            .iter()
            .find(|attribute| attribute.starts_with(IGNORE_PREFIX))
            .map(|attribute| parse_ignore(attribute, &source, line));

        let libtest_name = match &module {
            Some(module) => format!("{module}::{name}"),
            None => name.to_string(),
        };
        declarations.push(TestDeclaration {
            target,
            name: libtest_name,
            source: source.clone(),
            line,
            should_panic,
            ignore,
        });
        pending.clear();
    }

    // Anti-vacuity: the walk must see every test attribute in the file. If the
    // two ever disagree the parser has silently dropped declarations, and every
    // check downstream is weaker than it looks.
    let attribute_count = text
        .lines()
        .filter(|raw| raw.trim() == TEST_ATTRIBUTE)
        .count();
    assert_eq!(
        declarations.len(),
        attribute_count,
        "{source}: the scan recognised {} test functions but the file contains {} `{}` \
         attributes. The attribute walk lost declarations.",
        declarations.len(),
        attribute_count,
        TEST_ATTRIBUTE
    );

    declarations
}

/// The enforcement-level-2 targets, read back out of this file as
/// `module::function`.
///
/// Statement-based, not line-based. `rustfmt` wraps an assertion whose path is
/// long enough to cross the column limit:
///
/// ```text
/// const _: fn() =
///     crate::language_errors::run_wasm_backend_gives_a_runtime_error_a_message_distinct_from_its_name;
/// ```
///
/// A line-based scan finds neither half, so `ledger_is_well_formed` reports a
/// row as unasserted when the assertion is right there — which is exactly what
/// it did, on the first row long enough to wrap. Since `cargo fmt --check` is
/// enforced, the wrap is not something a shorter line can avoid; the scan has
/// to read the statement.
fn declared_const_assertions() -> BTreeSet<String> {
    let path = manifest_dir().join("tests/cli/known_failures.rs");
    let source = repo_relative(&path);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{source}: could not read: {error}"));
    let mut names = BTreeSet::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if !line.trim_start().starts_with(CONST_ASSERT_OPENER) {
            continue;
        }
        let mut statement = line.trim().to_string();
        while !statement.contains(';') {
            let continuation = lines.next().unwrap_or_else(|| {
                panic!("{source}: compile-time assertion `{statement}` is never terminated")
            });
            statement.push(' ');
            statement.push_str(continuation.trim());
        }
        let normalised = statement.split_whitespace().collect::<Vec<_>>().join(" ");
        let rest = normalised
            .strip_prefix(CONST_ASSERT_PREFIX)
            .unwrap_or_else(|| {
                panic!(
                    "{source}: `{normalised}` opens like a compile-time assertion but does not \
                     read `{CONST_ASSERT_PREFIX}<module>::<test>;`"
                )
            });
        let name = rest.strip_suffix(';').unwrap_or_else(|| {
            panic!("{source}: compile-time assertion `{rest}` does not end in a semicolon")
        });
        names.insert(name.to_string());
    }
    assert!(
        !names.is_empty(),
        "{source}: no compile-time test-existence assertions found. Either every one was deleted, \
         or this function is reading the wrong file and its check is vacuous."
    );
    names
}

/// The chunk declarations in [`RUNG_1C_RUNNER`], as
/// `area module -> modules that chunk excludes with `--skip``.
///
/// Parsed rather than duplicated, so the runner and this check cannot drift.
fn rung_1c_chunks() -> BTreeMap<String, BTreeSet<String>> {
    let path = repo_root().join(RUNG_1C_RUNNER);
    let source = repo_relative(&path);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{source}: could not read the tracked rung-1c runner: {error}. Rung 1c is run as \
             resumable per-module chunks because the whole suite outlives a container window; \
             without this script the chunk set is untracked again and nothing can check that it \
             covers the suite."
        )
    });

    let mut chunks: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix(RUN_CHUNK_OPENER) else {
            continue;
        };
        let arguments: Vec<&str> = rest.split_whitespace().collect();
        assert!(
            arguments.len() >= 2,
            "{source}:{line}: `{trimmed}` declares a chunk with fewer than two arguments; the \
             form is `run_chunk <module> <module>:: [--skip <other>::]...`"
        );
        let name = arguments[0];
        let filter = arguments[1];
        let expected_filter = format!("{name}::");
        assert_eq!(
            filter, expected_filter,
            "{source}:{line}: chunk `{name}` filters on `{filter}`, not `{expected_filter}`. The \
             first argument is the done-file key and the second is the libtest filter; letting \
             them differ means the done-file records a chunk that ran something else."
        );

        let mut skipped = BTreeSet::new();
        let mut remaining = arguments[2..].iter();
        while let Some(flag) = remaining.next() {
            assert_eq!(
                *flag, SKIP_FLAG,
                "{source}:{line}: unexpected argument `{flag}` in chunk `{name}`. Only \
                 `{SKIP_FLAG} <module>::` may follow the filter; anything else changes what the \
                 chunk runs in a way this check cannot account for."
            );
            let target = remaining.next().unwrap_or_else(|| {
                panic!("{source}:{line}: `{SKIP_FLAG}` in chunk `{name}` has no argument")
            });
            let target = target.strip_suffix("::").unwrap_or_else(|| {
                panic!(
                    "{source}:{line}: `{SKIP_FLAG} {target}` in chunk `{name}` does not end in \
                     `::`. A bare module name is a substring of far more libtest names than \
                     intended."
                )
            });
            skipped.insert(target.to_string());
        }

        assert!(
            chunks.insert(name.to_string(), skipped).is_none(),
            "{source}:{line}: `{name}` is declared as a chunk twice. Chunks partition the suite, \
             so a repeated key means those tests run twice and the done-file skips the second \
             declaration's filter entirely."
        );
    }

    assert!(
        chunks.len() >= MINIMUM_RUNG_1C_CHUNKS,
        "{source}: parsed only {} chunk(s), below the anti-vacuity floor of {}. Either the runner \
         was gutted or the `{RUN_CHUNK_OPENER}` spelling changed, and this check is now vacuous.",
        chunks.len(),
        MINIMUM_RUNG_1C_CHUNKS
    );
    chunks
}

fn parsed_ledger_or_panic() -> Ledger {
    parse_ledger().unwrap_or_else(|error| panic!("{error}"))
}

fn declaration_for<'a>(
    declarations: &'a [TestDeclaration],
    target: TestTarget,
    name: &str,
) -> Option<&'a TestDeclaration> {
    declarations
        .iter()
        .find(|declaration| declaration.target == target && declaration.name == name)
}

// -------------------------------------------------------------------------
// The hygiene tests.
// -------------------------------------------------------------------------

#[test]
fn ledger_is_well_formed() {
    let ledger = parsed_ledger_or_panic();

    // A parsed owner is one of the compiler's permanent closed areas.
    // The shared registry checker validates its durable technical document.
    for row in &ledger.rows {
        assert!(!row.owner_task.area().is_empty());
        assert!(ConformanceOwner::ALL.contains(&row.owner_task));
    }

    for row in &ledger.rows {
        // A reason short enough to fit in a shrug is not a reason.
        assert!(
            row.reason.len() >= 20,
            "known-failures.tsv:{}: reason `{}` is too short to be a reason",
            row.line,
            row.reason
        );
        // Evidence is a path, or a path plus a symbol -- never a line number.
        // This ledger already lost one file to silent rot; line-numbered
        // evidence rots the same way, one edit at a time.
        assert!(
            !row.evidence
                .as_bytes()
                .windows(2)
                .any(|pair| pair[0] == b':' && pair[1].is_ascii_digit()),
            "known-failures.tsv:{}: evidence `{}` cites a line number. Cite a path, or a path and \
             a symbol; line numbers go stale without anything failing.",
            row.line,
            row.evidence
        );

        // ...and the path has to resolve, in this checkout, outside `target/`.
        //
        // This column was free text with one negative check, in a ledger whose
        // entire purpose is that a path nothing consumes rots silently. It
        // reproduced that shape in its own schema: the `unfilled` row cited
        // `target/watched/b2-cli.log`, which is gitignored, so on any other
        // checkout it simply does not exist and nothing notices. Contrast the
        // columns that *are* consumed -- `owner_task` against the closed `ConformanceOwner` domain
        // and `test` against the source scan -- and both bite.
        let evidence_path = row.evidence.split_whitespace().next().unwrap_or("");
        assert!(
            !evidence_path.is_empty(),
            "known-failures.tsv:{}: evidence must start with a path",
            row.line
        );
        assert!(
            !evidence_path.starts_with("target/"),
            "known-failures.tsv:{}: evidence `{}` points into `target/`, which is gitignored. \
             Nobody else's checkout has that file, so the citation is unverifiable by \
             construction. Cite a tracked path.",
            row.line,
            evidence_path
        );
        let resolved = repo_root().join(evidence_path);
        assert!(
            resolved.exists(),
            "known-failures.tsv:{}: evidence path `{}` does not exist ({}). Cite something that \
             is actually in the tree.",
            row.line,
            evidence_path,
            resolved.display()
        );
    }

    // The `unfilled` placeholder expires.
    let unfilled_rows = ledger
        .rows
        .iter()
        .filter(|row| row.state == State::Unfilled)
        .count();
    if unfilled_rows > 0 {
        assert!(
            CURRENT_BATCH < ledger.unfilled_allowed_until,
            "known-failures.tsv still has {} `unfilled` row(s), the deadline is batch-{}, and this \
             checkout is batch-{}. Run rung 1c, replace the placeholder with real rows, and delete \
             it. Extending the deadline instead is possible, but it is a visible edit to the \
             header, which is the point.",
            unfilled_rows,
            ledger.unfilled_allowed_until,
            CURRENT_BATCH
        );
    }

    // Enforcement level 2, both directions: every `cli` row naming a real test
    // has a compile-time assertion, and every assertion has a row.
    let asserted = declared_const_assertions();
    let expected: BTreeSet<String> = ledger
        .rows
        .iter()
        .filter(|row| row.target == TestTarget::Cli && row.state.names_a_real_test())
        .map(|row| row.test.to_string())
        .collect();
    for name in &expected {
        assert!(
            asserted.contains(name),
            "known-failures.tsv names `{name}` in the cli target, but known_failures.rs has no \
             compile-time existence assertion for it. Without one, renaming or deleting the test \
             leaves a live ledger row pointing at nothing and nothing fails."
        );
    }
    for name in &asserted {
        assert!(
            expected.contains(name),
            "known_failures.rs asserts the existence of `{name}`, but no cli ledger row names it. \
             Delete the orphan assertion or add the row."
        );
    }
}

#[test]
fn every_expected_failure_carries_a_should_panic() {
    let ledger = parsed_ledger_or_panic();
    let declarations = scan_test_sources();

    // Forward: a declared fail/hang must be enforced by libtest itself.
    for row in &ledger.rows {
        if !row.state.names_a_real_test() {
            continue;
        }
        let declaration =
            declaration_for(&declarations, row.target, row.test).unwrap_or_else(|| {
                panic!(
                "known-failures.tsv:{}: no test function named `{}` in target `{}`. A renamed or \
                 deleted test must not leave a live ledger row behind.",
                row.line, row.test, row.target
            )
            });
        if !row.state.requires_should_panic() {
            continue;
        }
        match &declaration.should_panic {
            None => panic!(
                "known-failures.tsv:{}: `{}` is declared `{}`, but {}:{} carries no should_panic \
                 attribute. libtest would report the failure as an ordinary red test and rung 1c \
                 could not be its own gate.",
                row.line, row.test, row.state, declaration.source, declaration.line
            ),
            Some(ShouldPanicAttribute::Bare) => panic!(
                "{}:{}: bare should_panic on `{}`. It passes on ANY panic, so a genuine new defect \
                 in this test would show up green. Give it the substring the current failure \
                 actually prints.",
                declaration.source, declaration.line, row.test
            ),
            Some(ShouldPanicAttribute::Expected(expected)) => assert!(
                !expected.trim().is_empty(),
                "{}:{}: should_panic on `{}` has an empty expected string, which matches every \
                 panic and is exactly as vacuous as the bare form.",
                declaration.source,
                declaration.line,
                row.test
            ),
        }
    }

    // Reverse: every should_panic in this crate's tests is declared.
    for declaration in &declarations {
        let Some(attribute) = &declaration.should_panic else {
            continue;
        };
        let row = ledger
            .row_for(declaration.target, &declaration.name)
            .unwrap_or_else(|| {
                panic!(
                    "{}:{}: `{}` carries {:?}, but has no row in known-failures.tsv. An expected \
                     failure with no owner, no reason and no expiry is the silent expected failure \
                     AGENTS.md bans.",
                    declaration.source, declaration.line, declaration.name, attribute
                )
            });
        assert!(
            row.state.requires_should_panic(),
            "{}:{}: `{}` carries a should_panic attribute, but its ledger row says `{}`. The row \
             and the attribute must agree on what this test does.",
            declaration.source,
            declaration.line,
            declaration.name,
            row.state
        );
    }
}

#[test]
fn every_ignored_test_is_declared() {
    let ledger = parsed_ledger_or_panic();
    let declarations = scan_test_sources();

    // Reverse: every ignore attribute in `tests/cli/*.rs` and `tests/*.rs` is
    // declared with an owner. Today that is one in the `cli` target (the T05
    // allocation-stress case in heap.rs) and three in `perf`.
    for declaration in &declarations {
        let Some(attribute) = &declaration.ignore else {
            continue;
        };
        match attribute {
            IgnoreAttribute::Bare => panic!(
                "{}:{}: `{}` is ignored with no reason. An undocumented skip becomes a permanent \
                 skip.",
                declaration.source, declaration.line, declaration.name
            ),
            IgnoreAttribute::Reason(reason) => assert!(
                !reason.trim().is_empty(),
                "{}:{}: `{}` is ignored with an empty reason.",
                declaration.source,
                declaration.line,
                declaration.name
            ),
        }
        let row = ledger
            .row_for(declaration.target, &declaration.name)
            .unwrap_or_else(|| {
                panic!(
                    "{}:{}: `{}` is ignored, but has no row in known-failures.tsv. Add one with an \
                     owner task: a reason inside the attribute is invisible to anyone reading the \
                     suite result.",
                    declaration.source, declaration.line, declaration.name
                )
            });
        assert!(
            row.state.requires_ignore(),
            "{}:{}: `{}` carries an ignore attribute, but its ledger row says `{}`.",
            declaration.source,
            declaration.line,
            declaration.name,
            row.state
        );
    }

    // Forward: an `ignored` row whose test is no longer ignored is stale.
    for row in &ledger.rows {
        if !row.state.requires_ignore() {
            continue;
        }
        let declaration =
            declaration_for(&declarations, row.target, row.test).unwrap_or_else(|| {
                panic!(
                    "known-failures.tsv:{}: no test function named `{}` in target `{}`.",
                    row.line, row.test, row.target
                )
            });
        assert!(
            declaration.ignore.is_some(),
            "known-failures.tsv:{}: `{}` is declared ignored, but {}:{} no longer carries an \
             ignore attribute. Delete the row: the ledger must not outlive what it describes.",
            row.line,
            row.test,
            declaration.source,
            declaration.line
        );
    }
}

#[test]
fn routing_takes_the_guarded_path_whenever_the_test_name_is_unknown() {
    // The hang-to-fail conversion is what lets rung 1c drop `--skip`, and it is
    // keyed on the libtest thread name. Getting the bias backwards restores the
    // hang, so it is asserted rather than left to a comment.
    assert_eq!(execution_path(None), ExecutionPath::GuardedSubprocess);
    assert_eq!(execution_path(Some("")), ExecutionPath::GuardedSubprocess);
    let ledger = parsed_ledger_or_panic();
    let hangs: Vec<&Row> = ledger
        .rows
        .iter()
        .filter(|row| row.state == State::Hang && row.target == TestTarget::Cli)
        .collect();
    // There is deliberately no `assert!(!hangs.is_empty())` here any more. The
    // batch-5 form carried one, with the message "if the hang is genuinely fixed,
    // delete this assertion along with the row" — and in batch 6 it was: the T17
    // `Atomics.wait` row was retired when the test started passing, leaving zero
    // cli hang rows. The loop below is then vacuous *by design*, which is the
    // correct end state; the unknown-name assertions above are what keep
    // this test from asserting nothing. Re-add the non-empty check only if a new
    // hang row is ever declared and you want its routing pinned.
    assert_eq!(
        execution_path(Some(MAIN_THREAD_NAME)),
        if hangs.is_empty() {
            ExecutionPath::InProcess
        } else {
            ExecutionPath::GuardedSubprocess
        },
        "serial libtest must guard every invocation when any CLI hang is declared"
    );
    for row in hangs {
        assert_eq!(
            execution_path(Some(row.test)),
            ExecutionPath::GuardedSubprocess,
            "`{}` is declared a hang but would run in-process, which is the configuration that \
             spins forever",
            row.test
        );
    }

    // A name with no row keeps the fast path; otherwise the whole suite would
    // pay a process spawn per test.
    assert_eq!(
        execution_path(Some("array::a_name_that_is_not_in_the_ledger")),
        ExecutionPath::InProcess
    );
}

#[test]
fn serial_routing_guards_declared_hangs_and_invalid_ledgers() {
    let mut ledger = parsed_ledger_or_panic();
    ledger.rows.clear();
    let route = |ledger: &Ledger, name| execution_path_with_ledger(name, Ok(ledger.clone()));
    assert_eq!(
        route(&ledger, Some(MAIN_THREAD_NAME)),
        ExecutionPath::InProcess
    );
    ledger.rows.push(Row {
        target: TestTarget::Perf,
        test: "test_only_hang",
        state: State::Hang,
        owner_task: ConformanceOwner::T02,
        reason: "routing control",
        evidence: "synthetic ledger; no fixture is executed",
        line: 1,
    });
    assert_eq!(
        route(&ledger, Some(MAIN_THREAD_NAME)),
        ExecutionPath::InProcess
    );
    ledger.rows[0].target = TestTarget::Cli;
    ledger.rows[0].state = State::Fail;
    assert_eq!(
        route(&ledger, Some(MAIN_THREAD_NAME)),
        ExecutionPath::InProcess
    );
    ledger.rows[0].state = State::Hang;
    assert_eq!(
        route(&ledger, Some(MAIN_THREAD_NAME)),
        ExecutionPath::GuardedSubprocess
    );
    assert_eq!(
        route(&ledger, Some("test_only_hang")),
        ExecutionPath::GuardedSubprocess
    );
    assert_eq!(
        route(&ledger, Some("test_only_nonhang")),
        ExecutionPath::InProcess
    );
    assert_eq!(route(&ledger, None), ExecutionPath::GuardedSubprocess);
    assert_eq!(route(&ledger, Some("")), ExecutionPath::GuardedSubprocess);
    for name in [Some(MAIN_THREAD_NAME), Some("test_only_nonhang"), None] {
        assert_eq!(
            execution_path_with_ledger(name, Err(LedgerError::UnknownState("invalid".into()))),
            ExecutionPath::GuardedSubprocess
        );
    }
}

#[test]
fn rung_1c_chunks_cover_every_cli_area_module() {
    // Rung 1c is the gate this ledger exists to make meaningful, and on this
    // hardware it is only reachable as resumable per-module chunks. That turns
    // "the chunk set covers the suite" into a load-bearing claim that was, until
    // now, checked by nobody: a new area module is simply never selected by any
    // chunk filter, every chunk still reports `ok`, and the run reads as a
    // complete rung 1c while skipping a whole file. This test is the difference
    // between a partition and a subset masquerading as one.
    let chunks = rung_1c_chunks();

    let cli_dir = manifest_dir().join("tests/cli");
    let modules: BTreeSet<String> = rust_sources_in(&cli_dir)
        .into_iter()
        .map(|path| file_stem(&path))
        // `main.rs` is the target's crate root, not an area module.
        .filter(|stem| stem.as_str() != "main")
        .collect();
    assert!(
        !modules.is_empty(),
        "no area modules found under {}. CARGO_MANIFEST_DIR is not resolving to this crate, so \
         the coverage check below would pass vacuously.",
        cli_dir.display()
    );

    // A file on disk with a chunk of its own is NOT enough, and the gap is not
    // hypothetical: `iterator_helpers.rs` shipped with 13 tests, a `run_chunk`
    // line, and no `mod iterator_helpers;` in `main.rs`. The file existed, the
    // chunk existed, this test was green -- and the chunk selected nothing,
    // libtest exited 0 on `0 passed`, and the done-file banked a chunk that
    // measured nothing. Undeclared modules are not compiled, so the disk scan
    // above cannot see the difference; only `main.rs` can.
    let root_source = cli_dir.join("main.rs");
    let root_text = std::fs::read_to_string(&root_source).unwrap_or_else(|error| {
        panic!(
            "{}: could not read the cli target's crate root: {error}",
            repo_relative(&root_source)
        )
    });
    let declared: BTreeSet<String> = root_text
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("mod "))
        .filter_map(|rest| rest.strip_suffix(';'))
        .map(str::to_string)
        .collect();
    for module in &modules {
        assert!(
            declared.contains(module),
            "crates/scriptw-cli/tests/cli/main.rs does not declare `mod {module};`, so \
             tests/cli/{module}.rs is not compiled into the `cli` target at all. Its chunk \
             `{module}::` then selects nothing, libtest exits 0 on `0 passed`, and the chunked \
             rung 1c banks it and reads as a complete suite. Add `mod {module};` to main.rs."
        );
    }
    for module in &declared {
        assert!(
            modules.contains(module),
            "crates/scriptw-cli/tests/cli/main.rs declares `mod {module};` but there is no \
             tests/cli/{module}.rs. Delete the declaration or restore the file."
        );
    }

    for module in &modules {
        assert!(
            chunks.contains_key(module),
            "{RUNG_1C_RUNNER} has no chunk for tests/cli/{module}.rs, so the chunked rung 1c never \
             runs those tests and still reports a complete suite. Add `run_chunk {module} \
             {module}::` to the runner."
        );
    }
    for chunk in chunks.keys() {
        assert!(
            modules.contains(chunk),
            "{RUNG_1C_RUNNER} runs a chunk `{chunk}::`, but there is no tests/cli/{chunk}.rs. The \
             filter selects nothing, libtest exits 0 on `0 passed`, and the done-file records a \
             chunk that measured nothing. Delete the `run_chunk {chunk}` line or restore the \
             module."
        );
    }

    // libtest filters are SUBSTRINGS. A chunk filter `a::` therefore also
    // selects module `b` whenever `b::` ends with `a::` -- which is exactly why
    // the `array` chunk carries `--skip typed_array::`. Left unchecked, the
    // overlap double-runs those tests and, worse, makes `passed + filtered out`
    // stop summing to the suite size, which is the arithmetic that proves the
    // chunking complete.
    for (chunk, skipped) in &chunks {
        for other in &modules {
            if other == chunk {
                continue;
            }
            if !format!("{other}::").ends_with(&format!("{chunk}::")) {
                continue;
            }
            assert!(
                skipped.contains(other),
                "{RUNG_1C_RUNNER}: chunk `{chunk}::` is a substring of every `{other}::` test \
                 name, so it selects tests/cli/{other}.rs too. Add `{SKIP_FLAG} {other}::` to the \
                 `run_chunk {chunk}` line."
            );
        }
    }
    for (chunk, skipped) in &chunks {
        for target in skipped {
            assert!(
                modules.contains(target),
                "{RUNG_1C_RUNNER}: chunk `{chunk}` skips `{target}::`, which is not an area \
                 module. A misspelled skip silently excludes nothing."
            );
        }
    }

    // The chunk filters are all `<module>::`, so a test declared in the target's
    // crate root would carry a bare libtest name that no chunk selects.
    let root_declarations = scan_source(TestTarget::Cli, None, &cli_dir.join("main.rs"));
    assert!(
        root_declarations.is_empty(),
        "tests/cli/main.rs declares {} test(s). Their libtest names carry no `module::` prefix, so \
         no chunk filter selects them and the chunked rung 1c skips them silently. Move them into \
         an area module.",
        root_declarations.len()
    );
}
