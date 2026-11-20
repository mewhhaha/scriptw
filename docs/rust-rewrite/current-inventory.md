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

- Retained RSC requests: the 1 ms goal is met, and against Node 24
  `--jitless` the sync request is ahead and the async request level
  ([runs](benchmarks.md#branch-hints-the-async-requests-extra-work-and-node-2026-11-16)).
  The published 2026-11-17 panel has Speed at 0.269/0.294 ms sync/async, Fast
  at 0.675/0.777 ms and Node 24 with its JIT at 0.166/0.178 ms (Speed 1.6×
  Node; [panel](benchmarks.md#react-server-components)). Remaining per-request
  cost, largest first, on the last attributed request (2,815,221 sync and
  3,046,837 async Callgrind instructions): register allocator spills and
  reloads (about 490K), read record hits (`cached_property_read`, 224K, now led
  by its frame prologue), the general Get (118K async),
  `computed_property_write` (113K), `for_in_advance` (45K), `hasOwnProperty.call`
  (34K), Promise species and construction in the async request (37K),
  `createElement`'s `arguments` cell (30K), about 440 runtime-builtin sites
  that copy a result into the completion before testing it, function
  environments at six objects each, and the `proxy_call` builtin call site
  (40K over 160 calls). Decide whether static read sites keep their inline
  own-slot read (about 17K instructions saved for 272 KB of P and 1.6 s of
  native preparation). The panel has no hardware counters or CPU isolation; a
  quiet-machine campaign would tighten it. The single-reference value ABI
  experiment stays rejected
  ([results](benchmarks.md#single-reference-value-experiment-2026-11-10)):
  measure conversion, classification and transport together before another
  representation change. Owner: RSC compiler/runtime.
- Profile-guided compilation: collection, the `.swprof` codec, admission and
  application are in and off by default
  ([contract](contracts/profile-guided-compilation.md)). Its first
  specialisation, inline read proofs at hot single-kind sites (S1), measured
  no-go on 2026-11-20: 37.7K of 2.79M sync and 38.5K of 3.02M async Callgrind
  instructions saved, against a threshold of 80K. Each inline guard costs
  nearly as much as the R entry proof it skips, and the 583 entry calls left
  are at mixed-kind sites. Computed appends (S2), profiled call targets (S3),
  arm hints (S4) and computed reads (S5) are unbuilt; cut the guard's own cost
  before building on S1. Owner: RSC compiler.
- Native preparation dominates a cold start (6.9 s for the RSC program under
  Speed on a busy machine; the backtracking allocator and the e-graph pass are
  the largest parts). The embedding API compiles serially although two jobs
  measured 1.7× faster: decide whether to enable parallel compilation and
  bound its memory. Fast's request-time deficit is its allocator (Fast about
  0.75 ms and 6.6 s to compile; optimization `None` with the backtracking
  allocator 0.33 ms and 8.7 s; Speed 0.31 to 0.39 ms and 9.5 s); the embedding
  API defaults to Speed while the CLI and Test262 keep Fast. Decide whether
  the CPU baseline stays x86-64 without SSE4.1. Owner: Engine native policy.
- Fresh entries cost about 0.05 ms before the program runs (Scriptc 0.03 ms;
  `integer.js` 135,512 instructions after ABI 82). Left: the pooled funcref
  table fill (about 31,000 instructions; decide between a chunked table and a
  faster null fill in the vendored runtime), Realm creation's remaining 7
  eager constructors and 39 prototypes, and the host's per-Store
  instantiation and Linker registration outside the timer (about 2.3M
  instructions) ([measurement](benchmarks.md#fresh-entries-and-compile-time-2026-11-17)).
  Owner: AOT runtime bootstrap.
- Program size and compile time: program Wasm is about 68 bytes per source
  byte, the inference prepass lowers every body twice, and the lowering maps
  use `String` keys. Owner: AOT codegen and IR lowering.
- Focused situations (2026-11-17): Speed is behind V8 in 12 of 21, most on
  observed `arguments` (29×), a bound global alias (16×), an empty retained
  call (3 µs against 0.24 µs), singleton and `typeof` dispatch and 32 to 128
  branch sites; the React SSR render takes 7.3 ms against V8's 0.63 ms
  ([situations](benchmarks.md#focused-performance-situations)). Owner: RSC
  compiler/runtime.
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

- V8 execution and process-memory parity remain open. On 2026-11-17 Scriptw
  is faster than V8 in 7 of the 17 kernels and than Scriptc in 15
  ([panel](benchmarks.md#runtime-comparison)). The 2026-11-01 calibrated
  30-case [screen](performance-gaps-20261101.json) has no repeated wall/worker-CPU flags, but all
  30 peak-RSS checks regress by roughly 4.25–5.28 MiB. A zero-trip probe locates
  4.39 MiB of extra clean host-executable residency; anonymous memory differs
  by 2 KiB and native R backing residency is unchanged. Diagnose the kernel/link
  layout mechanism and remaining V8 gaps; do not report a passing memory gate
  or add another heap/cache. Preserve setup boundaries and original workloads.
  Owner: unassigned.
- Attribute remaining cold compiler emission before changing P type generation.
  The [catalog checkpoint](regexp-catalog-20261101.json) removes repeated ICU
  Script-range enumeration after the earlier executable-identity removal.
  An intrusive diagnostic still places about 55 ms in catalog construction
  inside about 75 ms of emission; those spans overlap. Ordinary P still collects
  its roughly 2-MiB compiler-owned pool. Exact-R sharing is a warm multi-program
  hypothesis; first population still admits and freezes that prefix.
  Owner: unassigned.
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
  Owner: unassigned.
- Isolate dependency-heavy CLI benchmark examples from the product's Cargo graph.
  A combined CLI/example build activated Wasmtime async/component features and
  failed the RSS gate; the ordinary CLI passed with matching dependency features.
  Preserve host/target feature checks and source-bound native-scope reuse while
  measuring whether a separate benchmark crate reduces build duplication.
  Owner: unassigned.
- Reduce repeated Rust compilation across Cargo graphs using the
  [source-bound unit timings](regexp-catalog-20261101.json). The IR unit-test
  target took about 204 seconds, and its separate ordinary library took about
  149 seconds before integration linking. Measure feature/host-role duplication
  before choosing a smaller crate or different build invocation. Preserve
  complete current-source receipts and bounded verification. Owner: unassigned.
- Attribute the roughly 2.45 ms P loading span: cache lookup/read/digest,
  deserialization and type registration. Preserve the one-Module/64-MiB cap;
  counting two Modules as one bundle would bypass it. Measure precompiled R
  decode/digest and private-file writes separately from ordinary P loading.
  Intrinsic descriptor templates are a later hypothesis for the roughly 0.22 ms
  fresh-entry floor; the Realm share is not isolated;
  the [namespace experiment](deferred-namespaces-20261101.json) remains rejected.
  Owner: unassigned.
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
  remains at 16 MiB. Owner: unassigned.
- Complete current-source default and optimized release acceptance. Historical
  complete all-feature coverage and later affected-target checkpoints establish
  only their recorded scopes and sources. Owner: unassigned.
- Finish retention, real serial/shard/resume checks, debug/release differential
  and robustness campaigns, supported-host reproducibility and calibrated
  performance acceptance. Preserve original IDs, budgets and failed receipts.
  Owner: unassigned.
- Complete the release gate's two fresh current-pin Wasm-AOT families and
  deterministic backlog generation. The [published full run](test262-current.md)
  is a complete measured baseline, not green acceptance or acceptance of every
  later compiler change. Owner: unassigned.
- The historical 5,365-case failure replay lacks its original aggregate input.
  Keep that replay blocked until the genuine input is available; do not synthesize
  a substitute or infer its results from newer runs. Owner: unassigned.
- Repair four pre-existing AOT bootstrap assertions: compact native metadata,
  deferred Error demand, intrinsic property-layout allocation and eager layout
  ownership. Three eager-inventory checks predate the Web API constructors
  (34 versus 24 constructors; 81 versus 61 layouts). Eager DOMException prototype
  initialization reads Error.prototype and forces the Error graph. Preserve
  DOMException inheritance while repairing that demand boundary and the inventory
  checks. All four remain failures. Owner: AOT runtime bootstrap recipes and
  their encoded-allocation assertions.
- Make CI green again. Two of the cheap guard steps that `ci.yaml` runs before
  it builds (`scripts/ci-fast.sh` runs them locally) are still red; formatting,
  Intl identities, the shortcut audit and the other guard steps pass. The
  module-boundary guard now runs every rule and reports 51 findings: 20 line
  budgets (largest `objects.rs` 4526/4250, `json/stringify.rs` 1320/950,
  `gc_types/value/program_instance.rs` 646/420, `emit/module_assembly.rs`
  2184/1950), the removed Date TimeClip, JSON undefined-descriptor, fixed
  String trim and raw-key read authorities, the eight `intrinsics/<family>.rs`
  files folded into `property_layouts/eager.rs`, the stale engine-test selector
  baseline in `check-abi8-native-direct-eval-host.py`, and the native-backing
  `sync_data` that `59bc04411` removed. `check-scriptw-identity.sh` reports
  10,539 retired-name lines (paths and commands) in tracked evidence:
  `docs/rust-rewrite/*.json` (9,524), the `aa55200d1` backlog JSON (651) and
  `test262/replays/*.verification.json` (364). Self-hosted jobs are cancelled
  for want of a runner. Preserve original evidence and compiler invariants;
  focused native verification does not clear these gates. Owner: coordinator
  (this maintenance stream). The red steps span many crates and their evidence
  rather than one subsystem, so the stream that maintains the guards keeps them
  sound and takes each finding to the owner of the code it names.
- Repair pinned Wasmtime's owned-reference downcast: `AnyRef::as_struct` can
  construct a LIFO `Rooted` from an `OwnedRooted` index. Collector tests use a
  scoped view after collection; that preserves their coverage without repairing
  the separate public API defect. Owner: unassigned.
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
- Five vendored `pagemap::tests::*` unit tests were reported failing on the
  cloud machine. No repository command runs them: Cargo refuses
  `cargo test -p wasmtime` because the vendored crate is not a workspace
  member and has dev-dependencies, so they cannot enter the
  `known-failures.tsv` ledger, whose rows libtest enforces. Cause: the tests
  expect never-populated pages to be reported unwritten, and the cloud
  machine's 6.18 kernel reports them `WRITTEN` (a `PAGEMAP_SCAN` probe of an
  anonymous mapping returns category `0x2` for its holes); the product scan
  does not depend on it (see the pagemap entry in the Wasmtime
  [patch notes](../../vendor/wasmtime-47.0.0/PATCHES.md#pooled-gc-heaps)).
  Owner: vendored Wasmtime patches (make the tests kernel-independent and
  runnable from the repository).
- The swapped-page reset of pooled Stores has no end-to-end run:
  `a_swapped_out_dirty_page_reads_zero_in_the_next_store` is `#[ignore]`d
  because it needs an active swap device, and the cloud machine has none and
  the verification wrapper requires `memory.swap.max = 0`. The reset's
  classification and region handling do run, on modelled kernel categories
  (`the_reset_releases_swapped_dirty_pages_and_zeroes_present_ones`); what
  only the ignored test covers is the kernel reporting a real swapped page as
  the model says. Owner: pooled-store runtime (a CI runner with a swap device
  or zram running this test).
- On an on-demand Engine, a Store's GC heap is bounded only by its 4 GiB
  reservation unless the embedder attaches a limiter (`limits_mut` holds the
  1 GiB cap); the private and shared memories carry the cap in their types and
  a pooled Engine caps the heap. Wasmtime has no on-demand GC heap maximum
  and the embedder owns the Store's limiter, so closing it needs either
  Scriptw-owned limiting in the embedding API or a vendored heap maximum.
  Owner: embedding API.
- The String length limit (`MAX_STRING_LENGTH`, [implementation
  limits](runtime-requirements.md#implementation-limits)) is checked by
  `repeat`, `padStart`/`padEnd`, `+` and template literals, `concat`,
  `Array.prototype.join`, the replacement builders and RegExp `source`. The
  other builders still grow without it: JSON.stringify's output accumulator
  (whose growth path has no completion to throw into), case mapping,
  `normalize`, the URI and `escape` encoders, `String.raw`, Intl formatting
  and the internal concatenations of names and messages. A result past the
  limit there traps on heap exhaustion instead of throwing. Fix: give the
  accumulator's `reserve` an overflow exit that the serializer routes to a
  RangeError, and route each remaining builder's final length through the
  same check. Owner: string built-ins.
- Native compilation of one large Wasm function is superlinear in time and
  memory, so large single bodies and deep nesting stay costly though they no
  longer overflow a stack (measured 2026-11-20 with the product CLI, one
  process each). A top-level script body costs about 990 Wasm bytes per
  global `x++` statement (the global-binding write and its fallbacks; the
  same statements in a function body cost about 250): 600 of them make one
  594 KB function that `scriptw run` (Fast policy, single-pass register
  allocation) compiles in 22.8 s at 400 MB RSS (200: 2.7 s; 400: 8.0 s),
  and 800 `if (x == i) { x++ }` statements make 1.3 MB that take 97 s and
  2.0 GB. `precompile --policy speed` (backtracking allocation) compiles the
  latter in 8.7 s at 2.1 GB, `--policy fast` in 110 s at 1.9 GB, so the time
  is in the single-pass allocator and the memory in compiling the one
  function; Wasm emission takes 0.4 s and 51 MB. Source nested just below
  `MAX_NESTING_DEPTH` (997 levels) compiles and runs everywhere without a
  stack overflow, but its memory grows superlinearly with depth for some
  constructs: parentheses 81 MB, arrays 220 MB, blocks 53 MB, nested
  functions 304 MB, `else if` chains 1.2 GB, conditional chains 558 MB,
  templates 3.2 GB, chained assignment 4.1 GB, binding patterns 9.2 GB,
  object literals 9.7 GB (500 levels: 2.7 GB; emission itself 1 GB at 997),
  and nested `class extends` heritage already 3.5 GB and 62 s at 250 levels
  (emission 3.6 GB at 500). The parser limit is the stack bound, not a
  memory bound; lowering it to fit the deepest-cost construct in 4 GiB would
  reject ordinary 500-branch `else if` chains. Fix: split a large script or
  function body into several Wasm functions at statement boundaries (the
  emitter keeps one function per body today), find the single-pass
  allocator's superlinear step, and remove the per-level duplication behind
  class heritage and object literal emission. Owner: AOT codegen and native
  compilation.
- BigInt multiplication, division and radix conversion are quadratic
  (schoolbook digits; conversion divides by the largest radix power per
  pass). Values near `MAX_BIGINT_BITS` are bounded but slow: squaring a
  2^26-bit value takes hours. V8 uses Karatsuba/Toom/FFT and
  divide-and-conquer conversion. Owner: BigInt arithmetic (performance, not
  correctness).
- Smaller runtime review items left open, each with its reason: an
  embedder's own `Module::new` on a parallel Engine still compiles on Rayon's
  global pool (Scriptw cannot route it; foreign-modules.md says so);
  `compilation_pool` caches a failed Rayon pool build for the process (the
  failure is a thread spawn refused by the OS, which a retry in the same
  process would most likely meet again, while retrying would let two callers
  race to build two pools; the cached error names the cause on every use);
  `EngineError` keeps `store_pool_limit_reached: bool` beside mutually
  exclusive `Option` fields (part of the typed error kinds decision);
  `default_compilation_jobs` keeps a `cfg!(test)` branch (the library's unit
  tests share one process and most configure one job explicitly; without it
  an unconfigured test would fix a CPU-derived count first and every later
  one-job request would be refused; integration tests use `SCRIPTW_JOBS`
  instead, and removing the branch waits on the B4 test-harness change in
  target/review/PLAN.md); no test drops a pooled Store while a panic or trap
  unwinds out of a host call (the drop path is the same `Drop` that ordinary
  Store teardown runs and the pooled-store tests cover, and the trap case is
  covered by the interrupted-Store test; a panic through a host call needs a
  panicking host import that the product does not define, so the test needs
  a test-only import first); a killed `scriptw precompile`
  leaves its transaction file (removing files by a dead PID is not safe across
  PID namespaces). Owner: embedding API.
- Compiler review refactors not taken, each because it prevents no current
  mistake: `SpecOperationResultLocals::{Expression, Call}` as one struct,
  `ArgumentsProjectionRead::Index` carrying its key (26 sites), and an
  `Option<(projection, positional)>` pairing in the captured-call proof.
  Owner: emitter.
- The function completion kind is Normal at every normal program point, which
  is what licensed dropping the `set_completion_kind(Normal)` writes in
  `a9c732611`; it is still unchecked. A debug-only trap at `project` and at
  ExpressionStatement would change the bytes every debug build emits (and the
  exact emission fixtures CI runs in debug). An opt-in emission mode would
  change no default bytes, but it must reach the compiler as a compile option
  that the program cache key, the runtime-image build (R is emitted at build
  time) and the CLI cache all see, or a cached artifact silently drops or
  carries the checks; an environment switch read by the emitter does not.
  The better path is static: the code sink's GC local check shows a
  trace-based dataflow over a finished body is cheap, and the i32 fact flow
  already proves `Is(const)` for `i32.const; local.set`, so a completion-kind
  variant needs value sets through completion copies. Revisit when the next
  change removes completion writes. Owner: emitter completion protocol.

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
