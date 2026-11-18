# Current Test262 baseline — 2026-11-18

The complete pinned suite ran through the optimized default-feature Wasm-AOT
compiler: **102,990 Success / 4 Bug / 0 Crash / 0 NotImplemented**, covering
**all 748 matrix nodes and 102,994 mode-specific executions**. The four failures
are the same four tracked upstream disagreements as the 2026-10-23 run; no new
failure appeared. This is not a green suite.

No filters, skips, fixture patches or timeout extensions were used. Three
reusable case workers kept fresh per-case state, one compiler job each and the
normal 60-second deadline, on a shared four-vCPU cloud machine with its
inherited 13.4 GiB memory cap, one matrix node per invocation. The run started
on 2026-11-17 and resumed from its checkpoints after two container restarts,
with the same family, compiler and settings; publication exited zero on
2026-11-18.

Measured product: **`41983e248feab2f731d7ac54cfa66a4c41032c9f`**.

- Compiler fingerprint scheme: `scriptw-program-cache-compiler-v3`.
- Compiler fingerprint: `9c84626c7197122eb63b94f1a4f15e465337e2534cdf5445a6a4e45ac8db9129`.
- Executing image SHA-256: `c9abd4d4d8bafc77c94c3856d849d19eba07699ce0814a01cb2bed9bae41f50c`.
- Test262 pin: `6a4268a9354a545d41c3b62efebc478ed8c521f4`.
- ECMA-262 pin: `5345883164f463e87f8b40aca4956157ecba8783`.

Changes after `41983e248` (the read entry's hint dispatch, completion state
written only on abrupt edges, the embedding policy default and the
byte-identical Cranelift and regalloc2 work) are not covered by this run;
each ran its affected suites and Test262 subtrees.

The [canonical manifest](../../test262/snapshots/published-status-wasm-aot.manifest.json)
selects the immutable native [JSON](../../test262/snapshots/published-status-wasm-aot-3416bca62d18a38665345d3dcfb41da025087a4df11e5361b3df5e12f4e11902.json)
and [text](../../test262/snapshots/published-status-wasm-aot-3416bca62d18a38665345d3dcfb41da025087a4df11e5361b3df5e12f4e11902.txt)
members, imported unchanged from the frozen checkout, and the
[aggregate snapshot](../../test262/snapshots/cloud-41983e248-reuse-20261117-aggregate-13327628081868727304.json)
they name. The [portable chart record](test262-chart-full-20261022.json)
embeds the aggregate and all 748 leaf snapshots, verifies their identities,
and applies the unchanged 429-execution scope audit (its filename is the
first full campaign's). The README bars contain **102,561 in-scope passes /
4 failures / 0 unimplemented / 429 out of standalone Wasm scope**. All 429
scoped-out cases passed through the native host. No lossless log archive was
retained for this run.
[Scope policy](runtime-requirements.md#out-of-scope-tests).

## Remaining failures

| Original execution ID | Outcome | Explanation / tracking |
| --- | --- | --- |
| `sloppy-script:annexB/language/function-code/block-decl-func-skip-arguments.js` | Bug | Implicit arguments binding conflict; Test262 #5113 / PR #5112 |
| `sloppy-script:intl402/Temporal/ZonedDateTime/links.js` | Bug | Johnston/Honolulu identity conflict; related ECMA-402 #806 |
| `strict-script:intl402/Temporal/ZonedDateTime/links.js` | Bug | Same timezone conflict in strict mode |
| `sloppy-script:staging/sm/lexical-environment/block-scoped-functions-annex-b-label.js` | Bug | Labelled function visibility; related ECMA-262 #1595, no exact issue found |

[Reasons, exact controls and upstream links](../../test262/errata/proposals/upstream-status-20261021.md)
retain the 2026-10-21 upstream status check. No new upstream-status check is
claimed by this suite run. By user decision, all four remain ordinary failures.

## Historical evidence

### 2026-10-23 full run

The complete pinned suite ran through the optimized default-feature Wasm-AOT
compiler: **102,990 Success / 4 Bug / 0 Crash / 0 NotImplemented**, covering
**all 748 matrix nodes and 102,994 mode-specific executions**. The four failures
are exactly the previously tracked upstream disagreements; the twelve repaired
failures stay fixed and no new failures appeared. This is not a green suite.

No filters, skips, fixture patches or timeout extensions were used. Four reusable
case workers kept fresh per-case state, one compiler job each and the normal
60-second deadline within the cloud machine's four-CPU/32-GiB budget. The same
family resumed at a read-only barrier with 16 sequential nodes per invocation,
preserving each checkpoint and all worker/case settings. Native publication
exited zero; the intentional first-segment pause (exit130) remains in the record.

Measured product: **`b9ecfe64bdac63e0553b8c38a178229056e38f73`**.

- Compiler fingerprint scheme: `scriptw-program-cache-compiler-v3`.
- Compiler fingerprint: `7337ccbc2834d0d17902382431789227b6dddf8fda40842bf19c4243161d56f9`.
- Executing image SHA-256: `32ee09a9a33daaf7d27fcb1b649db7223fcbd3ce00f0943cf19cce5ef886101b`.
- Frozen checkout: `4aac260e76f88a503e39479df661856f448bcf64`, with subsequent test/doc/tool changes separated from the compiled product identity.
- Test262 pin: `6a4268a9354a545d41c3b62efebc478ed8c521f4`.
- ECMA-262 pin: `5345883164f463e87f8b40aca4956157ecba8783`.

The [canonical manifest](../../test262/snapshots/published-status-wasm-aot.manifest.json)
selects immutable native [JSON](../../test262/snapshots/published-status-wasm-aot-857165c90f0fb4ac3ec0493a3b4470977007808c5b1c10ecf5512840edc80f46.json)
and [text](../../test262/snapshots/published-status-wasm-aot-857165c90f0fb4ac3ec0493a3b4470977007808c5b1c10ecf5512840edc80f46.txt)
members, imported byte-for-byte from the frozen checkout. Their original absolute
snapshot paths remain provenance locators. The [lossless archive](../../test262/snapshots/full-test262-20261023.tar.gz)
retains native aggregate/leaf snapshots, text reports, provenance, raw logs,
memory samples, commands and an inventory. The [verification receipt](test262-full-20261023.json)
records its SHA-256 and exact census/failure checks.

Its portable chart record (`test262-chart-full-20261022.json` as committed at `183953284`; the file now holds the 2026-11-18 run) independently embeds
the aggregate and all 748 leaves, verifies identities and disjoint execution IDs,
and applies the unchanged 429-execution scope audit. The filename identifies the
campaign start; its measurement date is **2026-10-23**. The README bars contain
**102,561 in-scope passes / 4 failures / 0 unimplemented / 429 out of standalone
Wasm scope**. All 429 scoped-out cases passed through the native host in this run;
the partition neither hides failures nor establishes standalone dynamic eval.
No historical failure replay is mixed into these fresh full-run counts.
[Scope policy](runtime-requirements.md#out-of-scope-tests).

The sampled whole-cgroup maximum was **34,264,653,824 bytes**, with zero OOM or
OOM-kill events. This includes shared files and tmpfs; it is neither process RSS
nor JavaScript heap usage. Broader native/default/all-feature, release and
performance acceptance remain open in the [inventory](current-inventory.md).

The original **2026-10-20** full run measured compiler `c5053c9d4` at
**102,978 Success / 16 Bug / 0 Crash / 0 NotImplemented**. Its byte-identical
[JSON](../../test262/snapshots/published-status-wasm-aot.json),
[text](../../test262/snapshots/published-status-wasm-aot.txt),
[archive](../../test262/snapshots/current-test262-20261020.tar.gz) and
[16-failure backlog](../../test262/backlog/6a4268a9354a545d41c3b62efebc478ed8c521f4/wasm-aot.json)
remain historical. The [failure replay](failed-case-replay-20261021.md) repaired
12 original IDs; the [old chart reconciliation](test262-chart-reconciliation-20261021.json)
retains that distinct baseline-plus-replay calculation. Neither is relabelled
as the current full run. Older charts on different pins/path accounting are not
directly comparable. See the [publication contract](reproducible-publication-driver.md).

## Reproduce

Build the ordinary release compiler, then run the full provenance-bound matrix:

```sh
THREADS=4 JOBS=1 REUSE_CASE_WORKERS=1 MAX_MATRIX_NODES=16 \
SCRIPTW_BIN=./target/release/scriptw \
SNAPSHOT_DIR=test262/snapshots \
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  ./scripts/publish-real-status-low-ram.sh wasm-aot fresh-current-family
python3 -B scripts/generate-test-graph.py --refresh-evidence --full-evidence
python3 -B scripts/generate-test-graph.py --check
```

Use the same family/settings when resuming; completed execution IDs and literal
failures stay in their original checkpoints. A fresh compiler or suite requires
a fresh family. Canonical publication remains gated on full native proof.
For a frozen checkout, import its immutable generation and manifest unchanged
before capturing the graph, preserving historical members and aliases.
