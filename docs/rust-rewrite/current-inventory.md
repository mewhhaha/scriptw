# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Latest verification

The [loader-admission checkpoint](loader-admission-20261029.json) records
**1,389 native passes at `afe83e312` across four complete executables**: 1,063
engine, 300 AOT Intl, 18 runtime image and 8 default-feature CLI shared-host
example tests. The retained **665 Intl passes belong to intermediate `dc827285d`**.
The batch also passed 356 source guards and 40 inert tooling tests. Verification
ran serially within the local 4-GiB/no-swap limit; these affected scopes do not
establish full workspace or pinned-suite acceptance.

Across the 17 original local workloads, geometric means fell **4.7% in
post-execution RSS and 5.4% in PSS** against accepted `e8ef6700a`; private dirty
memory and execution were broadly flat. Focused integer timings were 0.2–2.9%
slower, so there is no universal CPU improvement. Paired WASI control RSS rose
1.7%. Both raw R profiles retain exact standard/data sections; only the Intl
identity custom section changes.
All 19 P modules remain byte-identical to accepted E8. These local results retain
their frozen producer identity separately from the [fresh CI publication](benchmark-refresh-20261028.md)
at `afe83e312`.

The historical [Temporal checkpoint](temporal-demand-20261029.json) retains
5,006 unique native passes across 20 targets and 884 original pinned Test262
executions across 197 filters at `e8ef6700a`. Verification-only corrections at
`c80fd9fbb` preserve that production source. Benchmark cohorts keep their own
recorded identities; the [full pinned publication](test262-current.md) remains
bound to `b9ecfe64b`.

## Next work

- Every-case Scriptc time/RSS parity remains open. Continue from the
  [runtime ownership contract](contracts/runtime-byte-ownership.md), current
  checkpoint captures and [performance queue](../performance-hills.md).
  Trace retained host type metadata from R's `Module::from_parts` through postcard
  `ModuleTypes` decoding to `Engine::register_and_canonicalize_types`. Earlier
  phase deltas (~204 KiB private decode, ~300 KiB R registry versus ~92 KiB P
  registry) do not prove duplication. Demonstrate at least **256 KiB of avoidable
  retained ownership** before choosing a patch; preserve all required admission
  and type registration.
  Fresh [CI mappings](benchmark-refresh-20261028.md) bind exact R/P image, text
  and section ranges; local `e8ef6700a` captures retain narrower attribution.
  Native-function ownership and execution attribution remain
  deferred; a fault-around cause remains unproven. Preserve reflection, later
  compiled programs and retained embedding access. Warm before-Store endpoints
  include earlier executions and must not be called cold floors.
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
