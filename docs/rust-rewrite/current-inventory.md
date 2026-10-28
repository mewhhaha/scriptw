# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Latest verification

The [consumer capability checkpoint](memory-consumer-capability-20261028.json)
at `a5da57330` passed **46 capability-disabled and six capability-enabled engine
tests**, **280 source-only checks**, and all 15 Intl identity checks. The same-source
memory comparison passed all 136 processes. This focused checkpoint does not
refresh full native or Test262 coverage; rejected heap experiments retain their
own source-bound results in its evidence archive.

The [runtime error-tail checkpoint](memory-runtime-error-tail-20261028.md)
records **4,917 fresh native passes across 19 complete affected targets** at
`0658e6ae6`, retaining all 4,912 predecessor IDs and five additions. It credits
no original Test262 executions. The separate [full pinned publication](test262-current.md)
remains bound to compiler `b9ecfe64b`; the October 28 benchmark refresh does not
refresh either native coverage or Test262.

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
- Repair pre-existing identity and module-boundary audit findings without rewriting
  original evidence or weakening compiler invariants. This cleanup does not clear them.

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
