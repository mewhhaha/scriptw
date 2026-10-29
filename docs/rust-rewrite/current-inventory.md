# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Latest verification

The [execution checkpoint](execution-parity-20261029.json) records **5,063 native
passes across 21 complete targets**, eight default-feature CLI benchmark-host
tests, 356 source guards and **1,214 selected original pinned Test262 executions**.
Local production binaries belong to `a5448d285`;
two test-only corrections at `54a28246e` preserve that production logic. Passing
unaffected scopes were retained after correcting structural assertions. Checks
ran sequentially within the local one-CPU, 4-GiB/no-swap limit.

Across the 17 original local workloads, geometric means fell **27.4% in execution
time, 4.3% in post-execution RSS and 5.0% in PSS** against `afe83e312`. Private
dirty memory was broadly flat. Every Scriptw execution median improved in this
run; every-case Scriptc parity remains open. Whole-process RSS changes are not
attributed solely to the compiler changes. These local results remain separate
from the [CI benchmark cohorts](benchmark-refresh-20261028.md). The
[full pinned publication](test262-current.md) remains bound to `b9ecfe64b`.

## Next work

- Every-case Scriptc time/RSS parity remains open. Continue from the
  [runtime ownership contract](contracts/runtime-byte-ownership.md), current
  checkpoint captures and [performance queue](../performance-hills.md).
  Next examine integer representation changes inside the existing scalar-loop
  path and demand initialization of binary-data intrinsics. Preserve negative
  zero, exact Number values, defining-Realm identities and the shared
  `Array.prototype.toString` / `%TypedArray%.prototype.toString` callable.
  Host type metadata remains a separate hypothesis: demonstrate at least
  **256 KiB of avoidable retained ownership** before changing its decoding or
  registration. Phase deltas do not prove duplication or fault-around behavior.
  Preserve reflection, later programs and retained embedding access. Warm
  before-Store endpoints include prior executions and are not cold floors.
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
