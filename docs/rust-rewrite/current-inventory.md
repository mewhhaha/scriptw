# Current work

[README](../../README.md) owns published product, benchmark and Test262 status.
Each measurement belongs to its recorded compiler and settings; later source
changes do not refresh it. [Contributing](../../CONTRIBUTING.md) owns development
commands and resource policy. [Acceptance](conformance-acceptance.md) and
[ownership](conformance-ownership.md) define durable completion requirements.

## Next work

Use the [21 focused programs and scores](benchmarks.md#focused-performance-situations)
to measure these hills individually; [the runner](../../CONTRIBUTING.md#focused-performance-situations)
accepts `--case <file.js>`.

- Bring retained RSC sync and async below 1 ms, and at least to V8 without its
  JIT. The source with chain walks, the computed-key write kernel and the
  call-free `instanceof` entry (ABI 68), with the host-side reductions,
  static site records kept in P globals (ABI 69), inline funcref reads,
  identifier cells, rounding-free truncation, single casts and shorter GC
  access code (implicit null checks, wrapping element offsets), captured
  cells read without import links in programs that import nothing,
  `instanceof` site records in P (ABI 70), a young generation and strict
  `arguments` reads without the object (4.87M median since),
  runs a Speed sync request in 5.05M user instructions (median; mean 5.17M
  over 900 requests, 0.11M above the median against Node's 0.04M) against
  4.38M (and 2.68M cycles) for Node 24
  `--jitless`
  ([per-operation costs](benchmarks.md#read-records-and-per-operation-costs-2026-11-11),
  [hit entries](benchmarks.md#record-hit-entries-2026-11-12),
  [chain walks](benchmarks.md#chain-walks-and-computed-keys-2026-11-13),
  [host-side costs](benchmarks.md#host-side-costs-2026-11-13),
  [site records](benchmarks.md#site-records-in-p-2026-11-13),
  [identifier cells and truncation](benchmarks.md#identifier-cells-truncation-and-single-casts-2026-11-13),
  [GC access code](benchmarks.md#gc-access-code-2026-11-14),
  [young generation](benchmarks.md#young-generation-2026-11-14)).
  Decide whether static read sites keep their inline own-slot read: on this
  request it saves about 17K instructions (0.2%) for 272 KB of P (6%) and
  1.6 s of native preparation, while inline writes pay for themselves.
  Next, in measured order: `instanceof`, about 505 instructions per missing
  test (685 before the shorter GC access code), of which the serializer runs
  about twenty per plain object;
  WeakMap operations, which still cross into the host (about 1,000
  instructions of a `has`, most of it Wasmtime's typed host-call boundary;
  about 5% of profiled cycles); the collector, now a minor collection of
  about 0.47M instructions every 4.3 requests (a quarter of requests collect;
  about 110K of it for the WeakMap entries React Flight creates, and 263
  instructions per survivor in the copying loop) and a full one of about 5M
  every 260–300 requests; the embedding's remaining per-call type checks
  (dynamic argument checks, struct field type checks, `RegisteredType` roots;
  about 2%);
  accessors, which no record covers (typed-array `byteLength`, Set `size`,
  module getters); computed reads of an own property, about twice V8; a
  computed add, about 1,000 instructions; identifier reads that start at the
  current environment and `typeof` of a missing global (969 against 239);
  inline stores for proven captured cells; lazy function `prototype`,
  `length` and `name`; closure creation and `Object.keys`; the Wasm
  `index < length` trap that repeats the source's own bounds branch on every
  array element read, and the `is_subtype` libcall a cast to a non-final type
  takes on a mismatch (about 0.6% of profiled cycles).
  Arguments reads without the object and proven cells for arrow functions
  then take the sync request from 5.095M to 4.844M and async from 5.588M to
  5.324M, in Callgrind instruction references on a machine without hardware counters (about 0.8% above the
  hardware counts), and the same tool attributes the rest by native
  instruction: one `instanceof` test in `vn` is about 390 instructions, 150
  of them the global read of its constructor, and a third of `vn`'s
  plain-object path is stack traffic
  ([attribution](benchmarks.md#arguments-reads-and-instruction-attribution-2026-11-15)).
  Site records for global reads and `instanceof`, a home slot for the GC heap
  base, chained read records, weak collection entries found by identity and
  operand tests before the generic add and equality kernels then take it to
  4.017M and 4.411M
  ([measurement](benchmarks.md#read-record-chains-weak-entries-and-operand-tests-2026-11-15)),
  and direct JSON members, ASCII run kernels, bulk typed-array copies and a
  chain walk for composite receivers to 3.646M and 4.011M
  ([measurement](benchmarks.md#json-byte-runs-and-composite-receivers-2026-11-15)).
  Stores into fresh literals and arrays, arguments objects that no child
  can name and prototype casts take it to 3.475M and 3.827M
  ([measurement](benchmarks.md#stores-arguments-objects-and-prototype-casts-2026-11-16)),
  and frame reloads, proven references and threaded branches in the
  vendored translation to 3.073M and 3.382M
  ([measurement](benchmarks.md#frame-reloads-proven-references-and-threaded-branches-2026-11-16)).
  Node 24 `--jitless` runs about 3.82M and 3.94M there. In wall clock the
  sync request was then level with Node and the async request 7.8% behind:
  the compiled request fetches its instructions from 828 KiB of distinct
  code and misses the instruction cache about twenty times as often as the
  interpreter
  ([measurement](benchmarks.md#wall-clock-against-node-and-code-locality-2026-11-16)).
  A branch hinting section that lays unlikely code out behind each
  function, a Symbol row count in intrinsic layouts, direct stores on
  collection and Promise receivers, direct Array species creation, object
  pattern parameters without an arguments object and funcref stores without
  a call (ABI 77) take it to 2.893M and 3.131M, and to the V8 goal on the
  cloud machine: 279 to 286 µs against Node's 308 to 309 sync, and 309 to
  315 against 329 to 335 async, with the means ahead as well, in two runs of
  twelve processes each
  ([measurement](benchmarks.md#branch-hints-the-async-requests-extra-work-and-node-2026-11-16)).
  A third run, on source `875fb7647`, has 268 against 308 µs sync and 333
  against 329 µs async, so the async request is level with `--jitless`
  rather than reliably ahead. The eight-process campaign on the same cloud
  machine is the published panel: Speed takes 0.271 and 0.341 ms, Fast
  0.776 and 0.818 ms, and Node with its JIT 0.164 and 0.177 ms
  ([measurement](benchmarks.md#react-server-components)). It has no
  hardware counters or CPU isolation; a campaign on a quiet local machine
  would tighten it. Still to decide: whether Speed or Fast is the default
  policy (every instruction count above is Speed) and whether the CPU
  baseline stays x86-64 without SSE4.1. The kernel, compilation and edge
  panels need Scriptc 0.4.5 and Zig to refresh, and the focused situations
  need the pinned Node 24.12.0.
  What remains, by size on that request: read record hits
  (`cached_property_read`, 274K own for 1,259 calls),
  `computed_property_write` (113K, about 245 an append), the general Get
  (118K async), `for_in_advance` (45K) and `hasOwnProperty.call` (34K),
  Promise species and construction in the async request (37K),
  `createElement`'s environment for its `arguments` cell (30K), register
  allocator spills and reloads (about 490K instructions a request), and
  JavaScript-level untaken arms between hot code, which no emitter hint
  names; the hints' own effect on time is not established. Not started:
  function environments at six objects each and the builtin call site
  (`proxy_call`, 40K over 160 calls).
 Source `89a934d06891`
  screens at 2.871/3.349 ms Fast and 2.298/2.486 ms Speed, with four fresh
  processes per actor/case rather than final qualification; qualify it against
  V8 with the eight-process campaign before refreshing the published panel.
  [Fault-driven deadlines](benchmarks.md#fault-driven-deadlines-2026-11-11) and
  [host-boundary screen](benchmarks.md#host-boundary-screen-2026-11-11). The complete
  single-reference ABI prototype and its local-copy refinement regress both
  cases and remain excluded. Measure conversion, classification and temporary
  transport together before another representation change; wrapper-read counts
  alone do not predict a win. Preserve exact number bits, TDZ, callable
  distinctions, specialized numeric paths, natural GC and the complete clock.
  [Results and measurement boundaries](benchmarks.md#single-reference-value-experiment-2026-11-10).
  Owner: RSC compiler/runtime.
- Close the remaining execution-deadline gaps. A deadline now removes access
  to the Store's GC heap and private linear memory instead of relying on epoch
  checks ([contract](contracts/execution-deadlines.md)). 570 runtime loops
  touch neither and rely on being bounded; add a structural guard that
  classifies the runtime module's loops and rejects a new one of that shape.
  Delivery is exercised on Linux x86-64 only, and other hosts reject a
  timeout. The pinned Test262 status has not been rerun with this mechanism.
  Owner: Engine native policy.
- Shorten typed funcref reads from GC structs further. Each `struct.get` of an
  `ExecutableCode` entry now reads the interned table inline (bounds check,
  entry load, registered-type compare) and calls Wasmtime only for a subtype
  or a bad id ([host-side costs](benchmarks.md#host-side-costs-2026-11-13)).
  A funcref table shared by R and P with an `i32` entry index and
  `call_indirect` would drop the type compare and the extra loads; it changes
  the `ExecutableCode` layout, publication, host-created code and the runtime
  ABI. Owner: AOT call lowering.
- Reduce copying-collection cost per request. A retained RSC Realm's compiled
  code allocates about 10,200 objects and 480 KB per sync request (11,200 and
  535 KB async; StoredValue boxes are 30% of the objects) with about 1.67 MB
  live; the existing larger-capacity profiles trade resident memory for fewer
  collections. The per-copy ephemeron search and `memmove` calls are gone
  ([host-side costs](benchmarks.md#host-side-costs-2026-11-13)); what remains
  is copying the same long-lived startup graph every time. A nursery with a
  card-marking store barrier would stop that. A sync request makes about
  12,200 reference stores into existing objects (13,200 async), 4% of them
  into objects older than the last collection, so an unfiltered card mark
  would add roughly 0.5% to every request's instructions. Owner: runtime
  value/storage representation and collector patch.
- Reduce the current Fast gaps on `args-unused.js` and `frame-compact.js`. Measure remaining full Number conversions, classify rows during projection, preserve original payloads, and branch once to integer or existing Number loops. Carry conditional domains through call slots and exact Add/Sub bounds; retain fractions, negative zero, nonfinite values, overflow and fallback. Owner: scalar-call planning.
- Test amortizing `frame-compact.js` projection across its closed outer loop. Recount projection allocations and own-data probes on the current source, then prove intervening effects cannot invalidate rows before hoisting. Owner: scalar-call planning.
- Reduce cross-block value transport or establish a stronger spill-home compaction hypothesis. The isolated block-local prototype reduces current Fast `vn`/`ft` fixed frames 213,856→200,864 B / 85,664→81,696 B, but request medians move only −0.35% / −0.97% (sync/async). Keep the fork outside the product. The anonymous static census lacks named-function/module and dynamic-frequency attribution; precompiled Backtracking R is outside it. Preserve CFG lifetimes, explicit GC roots, debug locations, argument slots, alignment and move order. [Evidence](react-architecture-current.json). Owner: compiler backend/spill homes.
- Measure a guarded dictionary Set region for the same ordinary target/receiver. Existing own-data updates perform three lookups and construct four records after descriptor-edge reuse; these are source-path counts, not measured frequency. Reuse PreparedOrdinaryDefinition for one lookup and a live VALUE-edge update. Creation still needs its key/value, descriptor and entry records; admit it only after proving prototype absence. Preserve accessor/readonly/exotic fallback, snapshots, insertion order and deferred values. Measure eligible updates and creates separately before changing dictionary storage. Owner: ordinary object storage.

- Measure type classification and singleton dispatch after strict-pair period
  projection. Prefer private binding facts and existing semantic kernels before
  adding a general Value region. Preserve arithmetic order, input effects,
  invalidation, zero-trip state and complete fallback.
  Owner: AOT scalar-loop planning and value refinement.
- Measure Script function storage caused by analysis-wide arguments visibility.
  Establish a passing current SSR baseline before changing recovery. Preserve
  global records, eval/prelude modes, captures and arguments; Module-only lookup
  and cell authorities must remain scoped to Modules. Owner: IR storage planning.
- Expand strict read-only arguments snapshots only with complete observer proofs
  and measured gains. Length and index reads now answer from the argument
  vector, the object is built when another use needs it, and method calls on
  other receivers are admitted
  ([measurement](benchmarks.md#arguments-reads-and-instruction-attribution-2026-11-15)).
  Still refused: passing `arguments` to `apply`, and a `try` statement. An
  admitted function still allocates its activation record for the
  `arguments` cell (about 480 instructions at `createElement`'s entry).
  Preserve actual count, the unchanged vector and complete fallback.
  Owner: arguments binding and source-call planning.
- Classify property-cache fallback families before widening coverage. Current source `9f2788b4af9f` request windows still count 2,548/2,698 cached-helper entries and 1,275/1,362 generic fallbacks (sync/async); other exits are not a hit count. Separate eligibility, cold/evicted sites, compact/dictionary receivers, computed/static keys, inherited and absent results. Preserve live guards and the complete exotic fallback. [Counters](react-architecture-current.json). Owner: ordinary object storage.
- Attribute StoredValue decoding and binding writes before another list-storage
  rewrite. Current sync requests still allocate 12,625 physical StoredValue records;
  the census does not identify origins or prove a representation change wins. Preserve deferred recipes, descriptor ownership
  tokens and full numeric semantics; measure total request time and allocation.
  Owner: runtime value/storage representation.
- Re-run fixed-yield width controls on the current source before attributing
  remaining for-in OwnKeys, descriptor and value-read costs. The existing width
  table and SVGs retain source `0ca455c47636` and are historical. Investigate a
  closed scalar loop region with actual ordinary/null-prototype guards before
  omitting visited tracking. Preserve Number arithmetic, head bindings, fallback
  progress, live descriptors, Proxy ordering and suspended cursors.
  Owner: AOT loop lowering and IR facts.
- Reduce duplicate descriptor snapshots on read-only packed-Array queries.
  Keep backing storage, values, attributes, holes and prototype behavior intact,
  with materialization retained for real writes. Measure repeated queries as well
  as isolated reads. The current physical census records 3,728 descriptor
  allocations per synchronous request; it does not attribute them to this path. Owner: AOT Array internal methods.
- Attribute the three remaining async Reflect definitions and remaining descriptor
  snapshots, name-to-slot selection and executed allocation after direct internal
  definition and private `[[GetOwnProperty]]` transport. Use the matched
  ordinary/JavaScript-function/native-constructor read situations and
  [sampled callers](react-attribution-current.json); preserve Proxy ordering,
  namespace TDZ, mapped arguments, typed arrays and execution Realm.
  Owner: AOT object operations.
- Test a connected NormalValue/Throw operation region across Get, conversion and callbacks before changing the global Completion ABI. A single boundary adapter can preserve StatementList/finally/resumable completions while removing repeated normal kind/target transport. Measure native traffic first: Wasmtime Tail ABI can return the five integer-class lanes in registers; they do not automatically imply sret. Native EH must preserve Realm, activation, invocation-depth cleanup and arbitrary thrown values. Owner: AOT operation/call lowering.

- Move embedding-only benchmark hosts to Engine examples so emitter iterations
  do not rebuild the Test262-linked CLI library. Reuse native producer/consumer
  runs for retained RSC requests while measuring cold setup separately. Prefer
  the existing external emission target for public artifact acceptance checks;
  keep private unit checks at frozen qualification. Measure identical edits
  before claiming build savings. Owner: benchmark tools/runtime-image build.
- Reduce [current RSC Wasm and native expansion](react-architecture-current.json) and cold setup. Measure code, data and reachable-helper contribution to P and R separately; the [older encoded-byte breakdown](rsc-wasm-size-current.json) retains its recorded source. Keep raw Wasm, native image size, resident memory and request time separate. Owner: AOT codegen/native compilation.

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
  ownership. Three eager-inventory checks predate the Web API constructors
  (34 versus 24 constructors; 81 versus 61 layouts). Eager DOMException prototype
  initialization reads Error.prototype and forces the Error graph. Preserve
  DOMException inheritance while repairing that demand boundary and the inventory
  checks. All four remain failures. Owner: AOT runtime bootstrap recipes and
  their encoded-allocation assertions.
- Repair pre-existing workspace formatting, stale shortcut inventory, identity
  and module-boundary audit findings. The RSC checkpoint still reports 14 stale
  Intl source-identity recipes. Preserve original evidence and compiler
  invariants; focused native verification does not clear these separate gates.
- Repair pinned Wasmtime's owned-reference downcast: `AnyRef::as_struct` can
  construct a LIFO `Rooted` from an `OwnedRooted` index. Collector tests use a
  scoped view after collection; that preserves their coverage without repairing
  the separate public API defect.
- **Deleted global behind a script `var`:** a write to a top-level `var`
  whose configurable global property was deleted re-creates the property
  where strict code must throw a ReferenceError (`var Map; delete
  globalThis.Map; var Map = 1`, also as a `for (var ... in ...)` target), and
  a top-level `for (var n of ...)` body reads a stale local copy after
  `globalThis.n = 9`. Both come from keeping script `var` bindings in locals
  mirrored to the global object by an unchecked Set
  (`emit_global_property_write`); reads of such names are exact since the
  run-time global resolution. Owner: the local-copy-plus-mirror design of
  script `var` bindings.
- Five vendored `pagemap::tests::*` unit tests fail, and the emitter's
  `thousand_method_program_shares_only_the_native_bodies` measures a
  method initializer of 1,441,914 bytes against its 1,400,000 limit; both
  predate the RSC work. On the cloud machine only,
  `runtime_cache::embedded_runtime_executes_fresh_programs_without_a_native_runtime_compile`
  fails (its trace has no "runtime-native-bundle hit" line there). Owner:
  vendored Wasmtime patches, native program templates and the runtime
  cache test's host assumptions.

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
