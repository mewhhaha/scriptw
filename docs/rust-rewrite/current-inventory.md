# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Latest verification

The [binary-data and scalar-loop checkpoint](binary-scalar-20261029.json) records
**5,081 native passes across 21 complete targets**, eight CLI benchmark-host
tests, 359 source guards and **191 selected original pinned Test262 executions**.
Local production binaries belong to `d0a134cfd`; test corrections at `f19370fcf`
retain that production logic. Checks ran sequentially within the local one-CPU,
4-GiB/no-swap limit. Original failed diagnostic attempts remain in the evidence.

Across the 17 original local workloads, geometric means fell **8.5% in execution
time, 3.7% in post-execution RSS and 4.3% in PSS** against `a5448d285`.
Integer execution fell about 34%; some other controls were flat or slower.
Whole-process changes are descriptive and are not attributed solely to one
compiler change. These local results remain separate from the
[CI benchmark cohorts](benchmark-refresh-20261028.md). The
[full pinned publication](test262-current.md) remains bound to `b9ecfe64b`.

## Next work

- Every-case Scriptc time/RSS parity remains open. Continue from the
  [runtime ownership contract](contracts/runtime-byte-ownership.md), current
  checkpoint captures and [performance queue](../performance-hills.md).
  Integer-local retention and binary-data demand initialization are implemented.
  The consumer capability control reduced Scriptw RSS by 8.0% with near-flat
  execution. Whole-lifecycle join self-samples identify object reads, pooled
  strings and the packed join kernel as targets; isolate execution-only costs
  before choosing the next change.
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
