# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Latest verification

The [Temporal demand-initialization checkpoint](temporal-demand-20261029.json)
records **5,006 unique native passes across 20 complete affected
release/default-feature targets** and **884 original pinned Test262 executions
across 197 filters**. Frozen producer binaries and receipts retain source
`e8ef6700a`. Final source `c80fd9fbb` corrects four verification-only files;
its production files are identical. Passing scopes and original failed receipts
remain recorded. Verification ran serially within the local 4-GiB/no-swap limit.
These selected scopes do not establish full workspace or pinned-suite acceptance.
Both refreshed CI benchmark cohorts measure `c80fd9fbb` and retain their own
settings in the [refresh record](benchmark-refresh-20261028.md). The separate
[full pinned publication](test262-current.md) remains bound to `b9ecfe64b`.

Across the 17 original local workloads, geometric means changed by **−9.8% in
execution time, −3.3% in private dirty memory and +3.6% in RSS**. Separate startup
and eight-Realm control timings fell **22.1% and 26.6%**. These mixed memory
endpoints do not establish an overall memory gain. An ELF grouping-only placement
trial failed the RSS acceptance gate and was reverted.

## Next work

- Every-case Scriptc time/RSS parity remains open. Continue from the
  [runtime ownership contract](contracts/runtime-byte-ownership.md), current
  checkpoint captures and [performance queue](../performance-hills.md).
  Fresh [CI mappings](benchmark-refresh-20261028.md) bind exact R/P image, text
  and section ranges; local `e8ef6700a` captures retain narrower attribution.
  Measure individual Wasmtime loading phases to find what makes the extra clean
  read-only/executable host pages resident. Native-function ownership and execution
  attribution remain deferred; a fault-around cause remains unproven. Use phase
  measurements to select a production change. Preserve reflection, later compiled
  programs and retained embedding access. Warm before-Store endpoints include
  earlier executions and must not be called cold floors.
  The [8-MiB GC capacity probe](memory-gc-capacity-20261028.md) already
  found growth back to 16 MiB and slower retained-object execution; ordinary
  execution remains at 16 MiB.
- Complete current-source default and optimized release acceptance. Historical
  complete all-feature coverage and later affected-target checkpoints establish
  only their recorded scopes and sources.
- Finish retention, real serial/shard/resume checks, debug/release differential
  and robustness campaigns, supported-host reproducibility and calibrated
  performance acceptance. Preserve original IDs, budgets and failed receipts.
- Complete the release gate's two fresh current-pin Wasm-AOT families and
  deterministic backlog generation. The [published full run](test262-current.md)
  is a complete measured baseline, not green acceptance or acceptance of every
  later compiler change.
- The historical 5,365-case failure replay lacks its original aggregate input.
  Keep that replay blocked until the genuine input is available; do not synthesize
  a substitute or infer its results from newer runs.
- Repair pre-existing workspace formatting, stale shortcut inventory, identity
  and module-boundary audit findings. Preserve original evidence and compiler
  invariants; focused native verification does not clear these separate gates.
- Repair pinned Wasmtime's owned-reference downcast: `AnyRef::as_struct` can
  construct a LIFO `Rooted` from an `OwnedRooted` index. Collector tests use a
  scoped view after collection; that preserves their coverage without repairing
  the separate public API defect.

## Conformance boundaries

Keep the four original upstream disagreements counted as failures. Their exact
cases and reasons remain in [the failure record](failed-case-replay-20261021.md)
and [upstream tracking](../../test262/errata/proposals/upstream-status-20261021.md).
Local fixture/specification proposals do not alter original results or authorize
upstream posting. Fake suites, selected replays and full pinned families remain
separate evidence.

Wasm is the sole product compilation target. Dynamic source cases requiring a
parser/interpreter inside emitted Wasm remain explicit unsupported AOT cases;
the native-host path does not establish standalone support. See
[runtime requirements](runtime-requirements.md).

Completed maintenance work and dated implementation receipts remain evidence,
not additional release gates. Open acceptance criteria survive removal of old
handoff documents and speculative checklists.
