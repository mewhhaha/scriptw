# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Next work

Use the [16 focused programs and scores](benchmarks.md#focused-performance-situations)
to measure these hills individually; [the runner](../../CONTRIBUTING.md#focused-performance-situations)
accepts `--case <file.js>`.

- Measure remaining call transport after canonical local-argument entries and
  guarded direct calls. Attribute executed entry coverage, argument vectors and
  activation allocation before widening the two-formal admission or inlining
  larger source bodies. React's five-formal `vn`/`ft` helpers are outside that
  transport bound; exclusion alone does not establish their cost. Preserve
  callee identity, Realm, captures, receiver, depth and abrupt completion.
  Owner: IR/AOT source-call planning and dispatch.
- Test compile-time sharing of equivalent immutable compact birth layouts.
  The eight-literal/one-factory control retains the same timed read body apart
  from four binding-slot constants; its layout identities differ. Intern ordered
  primitive keys and attributes in the existing ProgramInstance shape table,
  retaining per-object values/prototypes and live mutation/promotion guards.
  Measure dynamic cache misses and the unchanged original workload before
  assigning causal cost or predicting RSC gains. Owner: compact data planning.
- Measure remaining RSC property, descriptor and allocation work after typed
  internal OwnKeys and canonical Module direct-cell capture admission. Keep
  [instruction counts and sampled callers](react-attribution-current.json)
  separate from scored timings; inclusive sample shares overlap. Preserve live
  import aliases, TDZ, cross-Realm errors and Proxy invariants. Total executed
  allocation remains unmeasured. Owner: IR/AOT lowering and object operations.
- Measure remaining descriptor snapshots, name-to-slot selection and executed
  allocation after private `[[GetOwnProperty]]` transport. Use the matched
  ordinary/JavaScript-function/native-constructor read situations and
  [sampled callers](react-attribution-current.json); preserve Proxy ordering,
  namespace TDZ, mapped arguments, typed arrays and execution Realm.
  Owner: AOT object operations.
- Attribute Fast frame capacity before changing emitter value transport.
  Under pinned regalloc2 0.15.2 / Cranelift 0.134.0, separate strictly block-local
  spill homes from block parameters, branch arguments, other cross-block homes,
  scratch and explicit GC-root slots. Compare persistent plus maximum block-local
  capacity with the actual frame. Overlay local arenas only if that census
  supports it; retain within-block uniqueness, size/alignment, edge-copy scratch,
  output/debug locations and explicit GC stack maps. Validate allocator checks,
  forced GC, exceptions, loops and recursion; measure preparation and requests.
  [Allocator-only controls](react-attribution-current.json) remain separate from
  source changes. Frame capacity is distinct from spill instruction count.
  Owner: pinned native allocator and AOT code shape.
- Move embedding-only benchmark hosts to Engine examples so emitter iterations
  do not rebuild the Test262-linked CLI library. Reuse native producer/consumer
  runs for retained RSC requests while measuring cold setup separately. Prefer
  the existing external emission target for public artifact acceptance checks;
  keep private unit checks at frozen qualification. Measure identical edits
  before claiming build savings. Owner: benchmark tools/runtime-image build.
- Reduce [raw RSC Wasm expansion](rsc-wasm-size-current.json) and cold setup.
  Keep exact P/R identities and code/data accounting separate from native image
  size and request time. The older [SSR checkpoint](react-codegen-20261102.json)
  retains a 1,110,482-byte body. Owner: AOT codegen/native compilation.
- Requalify affected operators before a new full conformance publication.
  The [shared-kernel checkpoint](react-codegen-20261102.json) passes all 79 pinned
  modulus variants; the complete pinned Test262 matrix remains a separate gate.
- V8 execution and process-memory parity remain open. The fresh 408-process
  [reference](performance-gaps-20261101.json) wins 17/17 Scriptc and 6/17 V8
  cases in both statistics and blocks; mixed join's Scriptc lead is narrow.
  The calibrated 30-case screen has no repeated wall/worker-CPU flags, but all
  30 peak-RSS checks regress by roughly 4.25–5.28 MiB. A zero-trip probe locates
  4.39 MiB of extra clean host-executable residency; anonymous memory differs
  by 2 KiB and native R backing residency is unchanged. Diagnose the kernel/link
  layout mechanism and remaining V8 gaps; do not report a passing memory gate
  or add another heap/cache. Preserve setup boundaries and original workloads.
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
- Expand Module storage admission only with evidence. Real captures, observed
  arguments, lexical bindings, arrows, eval and non-simple/resumable protocols
  retain required storage. Preserve live exports, Realm-relative lookup,
  arguments snapshots, capture hops and source-name collisions. Owner: IR
  environment planning; measure each expansion with the focused programs.
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
- Repair four pre-existing AOT bootstrap assertions: compact native metadata,
  deferred Error demand, intrinsic property-layout allocation and eager layout
  ownership. All four reproduce on the frozen prior test executable; they remain
  failures in the latest focused checkpoint. Owner: AOT runtime bootstrap recipes
  and their encoded-allocation assertions. Preserve the original expected and
  actual values while determining whether recipes or assertions need correction.
- Repair pre-existing workspace formatting, stale shortcut inventory, identity
  and module-boundary audit findings. The RSC checkpoint still reports 14 stale
  Intl source-identity recipes. Preserve original evidence and compiler
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
