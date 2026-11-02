# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Next work

- Repair Fast native frame/code expansion before a new full conformance run.
  The pinned `language/expressions/modulus/bigint-arithmetic.js` crashes in strict
  and sloppy mode on both the accepted compiler and the private-counter candidate;
  the additional remainder scope is 77/79, not a passing scope. Owner: AOT native
  compilation policy and source-operation expansion. A 368-local Wasm function
  expands to about 117 MB of native code and a 33.44 MB frame, exhausting the
  32-MiB budget during shallow assertion calls. Compare allocation/expansion
  policies against the retained native prologues; do not increase the stack cap
  or skip the tests. The [gap diagnosis](performance-gaps-20261101.json) binds both
  failed snapshots and the exact frame evidence.
- Every-case V8/Scriptc execution and RSS parity remains open. The
  [gap diagnosis](performance-gaps-20261101.json) separates source-bound reference
  cohorts, LLVM/final-Wasm inspection and worker-IP diagnostics. Private counter
  proofs pass the 480-process A/A, 30-case screen and all three separate control
  cohorts. The fresh 408-process three-runtime entry reference wins 14/17 cases
  against Scriptc (median geometric means 0.671–0.676×) and 6/17 against V8
  (1.595–1.604×). V8 context creation is outside its entry timer; Scriptw's
  bootstrap remains inside. Preserve these API boundaries and supplemental
  context timings when comparing the remaining sorting, join and object gaps.
  Next, test guarded reads of already-observed intrinsic DATA rows: object_read
  accounts for 26–33% of user-cycle samples in three current mixed-join windows.
  Reuse complete Array prototype layouts and their existing value cache, with
  live own-absence/prototype/storage guards and ordinary fallback. Preserve
  method lookup before arguments. Source literal Strings already use the Program
  cache; avoid another cache or heap. Retain failed historical controls and keep
  the primitive-append region parked.
- Attribute remaining cold compiler emission before changing P type generation.
  The [catalog checkpoint](regexp-catalog-20261101.json) removes repeated ICU
  Script-range enumeration after the earlier executable-identity removal.
  An intrusive diagnostic still places about 55 ms in catalog construction
  inside about 75 ms of emission; those spans overlap. Ordinary P still collects
  its roughly 2-MiB compiler-owned pool. Exact-R sharing is a warm multi-program
  hypothesis; first population still admits and freezes that prefix.
- Profile the remaining cold `run` cost for
  [runtime-free primitive programs](standalone-pool-20261101.json). The
  [literal checkpoint](literal-admission-20261101.json) uses passive UTF-16 data
  and 204–205-byte main bodies on its seven literal inputs, without compiler
  pools or linear memory. Exact completions retain the existing WasmGC representation.
  The [cache-identity checkpoint](cache-image-20261101.json) promotes full-image
  BLAKE3 after an ordinary CLI-only comparison. It still reads the whole loaded
  executable. Attribute remaining hashing, P lookup and Wasmtime loading before
  changing cache policy; never substitute a source-only key. Measure cheap P
  regeneration separately: bypassing native caches can activate Cranelift and
  increase RSS. Earlier retained-object RSS rises and slow wall outliers still
  need attribution within their original cohorts.
  Broader programs and rooted snapshots still require R. Extending admission is
  separate correctness work, including numeric relation lowering distinctions.
- Isolate dependency-heavy CLI benchmark examples from the product's Cargo graph.
  A combined CLI/example build activated Wasmtime async/component features and
  failed the RSS gate; the ordinary CLI passed with matching dependency features.
  Preserve host/target feature checks and source-bound native-scope reuse while
  measuring whether a separate benchmark crate reduces build duplication.
- Reduce repeated Rust compilation across Cargo graphs using the
  [source-bound unit timings](regexp-catalog-20261101.json). The IR unit-test
  target took about 204 seconds, and its separate ordinary library took about
  149 seconds before integration linking. Measure feature/host-role duplication
  before choosing a smaller crate or different build invocation. Preserve
  complete current-source receipts and bounded verification.
- Attribute the roughly 2.45 ms P loading span: cache lookup/read/digest,
  deserialization and type registration. Preserve the one-Module/64-MiB cap;
  counting two Modules as one bundle would bypass it. Measure precompiled R
  decode/digest and private-file writes separately from ordinary P loading.
  Intrinsic descriptor templates are a later hypothesis for the roughly 0.22 ms
  fresh-entry floor; the Realm share is not isolated;
  the [namespace experiment](deferred-namespaces-20261101.json) remains rejected.
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
