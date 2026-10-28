# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Latest verification

The [retained-object checkpoint](retained-runtime-20261028.json) passed **4,937
native tests across 20 complete affected targets** and **216 original Test262
executions** at `65ffec2bf`. Its collector checkpoint passed 17 checks, retaining
16 unchanged passes after one test-only correction. The final guard short circuit
at `dce404dd1` passed all 34 emission tests and four strict/sloppy semantic controls;
the broader checkpoint retains its earlier source identity. The final local
performance corpus passed 144 processes, with another 52 scaling/control processes.
All ran serially within the local 4-GiB/no-swap limit. The separate
[full pinned publication](test262-current.md) remains bound to `b9ecfe64b`.

## Next work

- Continue memory work from the [runtime ownership contract](contracts/runtime-byte-ownership.md)
  and [performance queue](../performance-hills.md), using current source-bound
  measurements to choose the next change. The consumer experiment leaves a
  5.32-MiB RSS gap before Store creation that persists after drop; investigate
  host/runtime residency next. Retain all benchmark graphs and their
  evidence. The [8-MiB GC capacity probe](memory-gc-capacity-20261028.md) already
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
