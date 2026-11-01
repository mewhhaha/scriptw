# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Next work

- Every-case V8/Scriptc execution and RSS parity remains open. The
  [entry checkpoint](entry-bootstrap-20261101.json) remains production
  (`c5586f330`, ABI 42). The [unified six-kind namespace experiment](deferred-namespaces-20261101.json)
  is rejected: retained-object hot-R CP medians rose +4.946/+4.820% and means
  +3.671/+5.149% across the two blocks. Passing ordinary-target and first-touch
  gates do not override this veto. Next measure P-only compact-row reads under
  the [ownership contract](contracts/runtime-byte-ownership.md) and
  [performance queue](../performance-hills.md). Preserve fresh-entry timing,
  source/tool pins, slower control means and both
  [rejected](numeric-array-region-screen-20261101.json)
  [predecessors](joined-length-zero-trip-20261101.json).
- Continue measuring module function storage after private activation `var`
  bindings and simple parameters in strict ordinary functions. Lexical locals,
  arrows and other protocols retain their existing cells. Preserve live exports,
  Realm-relative lookup, actual captures and the conservative direct-eval,
  arguments and surrounding-`with` boundaries before expanding admission.
  The [local diagnostic](module-local-performance-20261030.json) preserves all
  17 retained-call results and the ordinary controls, including reverse-sort-512
  and join-4096 slowdowns in both balanced orders despite identical emitted Wasm.
- Host type metadata remains a separate hypothesis: demonstrate at least
  **256 KiB of avoidable retained ownership** before changing its decoding or
  registration. Phase deltas do not prove duplication or fault-around behavior.
  Preserve reflection, later programs and retained embedding access. Warm
  before-Store endpoints include prior executions and are not cold floors.
  Require a different measured hypothesis before revisiting the
  [8-MiB GC capacity probe](memory-gc-capacity-20261028.md); ordinary execution
  remains at 16 MiB.
- Complete current-source default and optimized release acceptance. Historical
  complete all-feature coverage and later affected-target checkpoints establish
  only their recorded scopes and sources.
- Repair the checked-initialization panic for
  `language/function-code/S10.2.1_A4_T2.js`: both modes crash at the Wasm backend's
  local-storage invariant on the unchanged performance baseline and candidate.
  Keep both failures in selected replay totals; the module-local change does not
  repair this existing hoisted-function/`var` case.
  Ordinary function-body hoisting emits lexical `let` initialization for backend
  `HoistedVar` storage without a checked initialization flag. Distinguish this
  hoisting boundary from true lexical initialization; preserve module/block
  lexicals, parameter collisions, captures and TDZ rather than weakening the
  backend's uninitialized-storage invariant.
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
