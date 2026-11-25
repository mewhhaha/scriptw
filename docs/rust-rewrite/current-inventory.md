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

- Builtin entry guards (landed 2026-11-23; follow-ups, correctness first):
  - **What landed:** an ordinary function body is lowered twice, from the
    builtin facts of the state it is lowered in and from none. When the two
    lowerings differ, both are emitted: the first checks at entry, by reads
    alone, the builtin globals its lowering consulted and tail-calls the
    second on a miss
    ([IR](../../crates/scriptw-ir/src/lowering/builtin_entry_guard.rs),
    [entry](../../crates/scriptw-aot-wasm/src/functions/builtin_entry_guard.rs)).
    A fact holds while the binding is an own data property of the global
    object that no global lexical declaration shadows, its value is the
    Realm's own intrinsic, and the intrinsic objects the fact reads through
    keep their intrinsic rows: `Math`, `JSON`, `Reflect` and `Atomics`
    themselves, a constructor allocated with a complete layout,
    %Object.prototype%, %Function.prototype%, %String.prototype% and
    %Array.prototype%
    ([Realm protectors](../../crates/scriptw-aot-wasm/src/objects/realm_protectors.rs)).
    Generators, async bodies and class elements have no guard and assume no
    builtin fact. `Math.imul(2,3)+1` after
    `globalThis["Ma"+"th"] = {imul(){return "x"}}` is `x1`.
  - **Still stale (wrong result possible, as before):**
    - a prototype with no protector record edited before a dormant body
      reads through it: `Number.prototype`, `Boolean.prototype`, `Symbol`,
      `BigInt`, `Promise`, `Map`, `Set`, the Error family, `Date`, `RegExp`,
      `ArrayBuffer` and the typed arrays (for example
      `Number.prototype.toFixed = f` before a body calls `x.toFixed()` on a
      proven Number), and a constructor the Realm allocates without a layout;
    - an own property of a builtin method's function object (`length`,
      `name`, an added `call`);
    - summaries read across bodies: a callee with an exact target (a class
      static method, a method of a `const` object) keeps the return, effect
      and parameter kinds inferred from fresh builtin facts, and a body
      reads a captured `const` with the kind its initializer had under them;
    - an intrinsic whose [[Prototype]] `Object.setPrototypeOf` changed (the
      entry reads storage, not the link);
    - prepared `eval`/`Function` units and `array_prototype_mutated`-style
      flags, which are not global property facts.
    Each needs either a protector record for the object or a summary that
    carries the facts it was inferred from.
  - **Never assumed (exact, at a precision cost):** a body that consults
    `Intl`, `Temporal` or a host namespace such as `$262` is lowered
    proof-free.
  - **Whole proofs (same day):** an entry that proved every fact keeps the
    proof for one builtin fact epoch, with the Environment, the Realm and
    the Environment's declarative name table it ran for, and the next entry
    compares those four instead of reading the facts. The epoch is one
    counter shared by R and P; it advances on a write through a dictionary
    entry a proof was read from (in-place Set, redefinition, replaced
    descriptor, deletion), on the promotion of intrinsic rows, when a
    deferred recipe is written back and when a Realm's global object is
    published
    ([writers](../../crates/scriptw-aot-wasm/src/objects/builtin_fact_epoch.rs)).
    A write to any other global, a script's own `var` included, leaves every
    proof standing.
  - **Cost, measured on 2026-11-23 against the same day's baseline host:**
    the retained RSC request is 2,339,863 Callgrind instructions sync
    without guards, 2,449,457 with per-fact entries (+4.7%) and 2,359,531
    with whole proofs (+0.8%), and 2,541,177, 2,657,029 (+4.6%) and
    2,562,007 (+0.8%) async; Flight's serializer `vn` is entered 276 times
    a request and its entry fell from about 400 to about 75 instructions.
    P is 3,820,278 bytes without guards and 4,138,051 with them (+8.3%):
    the second bodies, which whole proofs do not change. P grows where
    guarded bodies dominate (`edge-router.js` 157,523 to 200,292 bytes,
    `edge-html.js` 140,285 to 192,967, `integer.js` 67,089 to 86,915,
    `client.js` about 18.8 MB to 20.7 MB, measured before whole proofs) and
    not at all without one (`hello.js`, `objects.js`). Lowering runs twice
    per ordinary body.
  - **Follow-ups for the cost:** one fallback per source function instead of
    one per exact-context copy; a guard hoisted to the caller for an inlined
    captured call, a numeric leaf and a constant leaf, which a guarded body
    currently gives up; guards for class elements; the facts' proving code
    shared by a module's guards instead of repeated in each. Facts narrowed
    from "consulted by name" to "became a proof" need a lowering without
    side effects first: lowering a body again without each fact in turn
    took the RSC bundle's guards from 17 facts to 6 and P down 15 KB, but a
    body's lowering adds to the lowerer's finite-source candidates each
    time it runs, and `private_get_value_effects/optional_tails.js`
    exhausted 13 GB.

  Owner: IR lowering (global property facts) with the emitter.
- Speed versus V8: the [2026-11-22 deep dive](performance-deep-dive-20261122.md)
  attributes the situation, RSC, SSR and edge-kernel gaps and ranks five waves of
  fixes.
  - **Landed:** its waves 1 to 3 (protectors, plain `JSON.stringify`, fresh
    builtin results, `instanceof`/`typeof` chains, captured loop snapshots)
    and wave 4 (int32 domain for bitwise results, the direct `JSON.parse`
    builder, guarded routes such as `Math.imul` in tail position).
  - **Measured:** the RSC request is 13% fewer Callgrind instructions, and 20 of
    the 21 situations are faster in a local A/B
    ([batch results](benchmarks.md#performance-batch-2026-11-23)).
  - **Landed since (2026-11-24):** the GC pretenure window and cached funcref
    code addresses in the vendored runtime: a fresh React SSR entry runs 34%
    fewer instructions and its execution is 25% shorter
    ([measurements](benchmarks.md#pretenure-window-2026-11-24)).
  - **Next:**
    - the stale builtin fact entry guard (above);
    - regalloc2 split-limit campaign;
    - stack-map store placement;
    - shared call-site machinery;
    - preshaped sloppy ArgumentsObject;
    - WeakMap in the GC heap.
  - **Also open:** the 128-body recursion limit, and stale published SSR
    (about 3.3× V8) and `args-observed` (about 5×) numbers that need a
    republish.

  Owner: RSC compiler/runtime.
- Process startup: the [density panel](benchmarks.md#process-density) fits
  1.7× as many Scriptw processes as Node 24 in memory, but measured a Scriptw
  process at about 306 ms against Node's 42 ms: about 194 ms hashing the
  203 MiB executable and about 93 ms loading the runtime image. Both are gone
  from the product path. Compiler caches are keyed by the loaded image's
  linker build ID
  ([contract](contracts/compiler-cache-image-identity.md)), and the native
  runtime image is linked raw and page-aligned and mapped from the running
  executable
  ([contract](contracts/runtime-byte-ownership.md#native-backing-storage))
  instead of zstd-decoded (about 43 ms), SHA-256-admitted (about 31 ms) and
  written to a private unnamed file (about 14 ms) in every process. R now
  loads in about 2 ms. On the loaded shared machine, one CPU, 21 interleaved
  processes each, medians: warm `scriptw run` of the print-only program
  114 → 27 ms and the host's `execute-scriptw` 13 ms (103 ms before, from the
  previous measurement; that host was not rebuilt), against 32 ms for Node 24
  in the same rounds. Eight concurrent `scriptw run` processes held in a loop
  used 6.3 MiB of PSS per added process, from 9.9 MiB: the runtime's touched
  code pages are now shared file pages. The executable grew from 215.5 to
  275.4 MB (the host from 212.5 to 272.6 MB), because both native images are
  stored raw.

  Since 2026-11-24 a warm start no longer lists any cache directory, maps the
  cached program module from `native-modules-v1` instead of zstd-decoding it
  ([contract](contracts/runtime-byte-ownership.md#native-module-cache)), and
  hashes rec groups with an unkeyed word hasher in the vendored type registry.
  Warm `scriptw run` of the print-only program, same binary pair and method
  (51 interleaved processes each, pinned to one CPU while a Test262 run used
  the others; median, quartiles): 10.5 ms (10.3 to 10.7) → 6.5 ms (6.3 to
  6.6) with a 320-entry function cache, and 23.3 ms (22.8 to 24.0) → 6.5 ms
  (6.4 to 6.8) with 7,800 function, 640 program and 560 module entries: the
  cost no longer depends on what the cache holds. System calls 847 → 370 and
  9,647 → 370 (`statx` 353 and 9,044 → 12, `getdents64` 10 and 33 → 0,
  threads created 4 → 1); Callgrind instructions 41.1M → 24.8M and 56.3M →
  24.8M. These rounds are not comparable with the 27 ms above, which was
  taken under a different load.

  Private memory per process, also 2026-11-24 (the precompiled host held at
  Store 1's `after_execution` barrier, pooled Stores, one pinned CPU): R
  defines its private linear memory instead of importing a host-created one,
  so Wasmtime maps R's data segments from the executable as a copy-on-write
  image instead of copying 47 pages into every Store
  ([contract](contracts/runtime-byte-ownership.md#native-backing-storage);
  a [vendored Wasmtime patch](../../vendor/wasmtime-47.0.0/PATCHES.md#copy-on-write-image-beside-an-imported-memory)
  gives a module that imports another memory its image), and the host's
  execute commands run on one glibc malloc arena. Private dirty memory of
  `startup.js` fell from 1,944 to 1,672 KiB (R's memory 192 → 4 KiB, malloc
  arenas 88 KiB) and of `objects.js` from 2,112 to 1,848 KiB; the PSS added
  per process between one and four held processes fell from 2.04 to 1.80 MiB
  and from 2.21 to 1.98 MiB. A warm `scriptw run` did not slow down.

  What remains: rerun the panel on a quiet machine with its own method (the
  figures above are not the panel's). Of the 24.8M instructions of a warm
  start, 9.4M are SHA-256 as Valgrind runs it, without the CPU's SHA
  extension, and 4.6M register the two modules' types. The two raw Wasm
  packages (38 MiB each) are the next candidates if the executable's size
  matters more than its page-cache sharing. Keep the rule that no
  pre-existing, cache-directory or digest-selected native file is admitted.

  Owner: CLI and engine embedding.
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
- Lazy P literals (ABI 89) cut a React SSR fresh entry by about 4.7M
  instructions but add about 10K (0.45%) to each retained RSC request: every P
  literal read now has a cold resolver call, and Cranelift spills GC
  references live across any call at their definition and reloads them at
  each use (`js::ft` runs 17 more stack moves per call). Decide whether hot
  literal reads should drop the call, for example by building the literals P
  code reads at publication and keeping only source texts lazy. Owner: AOT
  codegen.
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
  The [cache-identity checkpoint](cache-image-20261101.json) promoted full-image
  BLAKE3 after an ordinary CLI-only comparison; the default is now the loaded
  image's linker build ID, which reads no executable bytes (see process startup
  above). Attribute P lookup and Wasmtime loading before changing cache policy
  further; never substitute a source-only key. Measure cheap P
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
  `normalize`, the URI and `escape` encoders, `String.raw`,
  `TextDecoder.prototype.decode` (which checks only `i32::MAX`, so decoding a
  200 MB Uint8Array traps), `RegExp.prototype.toString`, Intl formatting
  and the internal concatenations of names and messages. `join` checks the
  limit, but it keeps its element Strings live beside the growing result
  (about six bytes per unit, not the four the limit assumes), so a join of
  distinct Strings near the limit can still trap. The limit's 1 GiB premise
  holds only with the Store's own limiter (see the on-demand GC heap item).
  A template literal checks the limit at each concatenation, so a RangeError
  at `${a}${b}` precedes the side effects of a later `${c}`; ECMA-262
  concatenates only after evaluating every substitution, and V8 throws
  early as Scriptw does. A result past the
  limit there traps on heap exhaustion instead of throwing. Fix: give the
  accumulator's `reserve` an overflow exit that the serializer routes to a
  RangeError, and route each remaining builder's final length through the
  same check. Owner: string built-ins.
- Compile memory has no budget, and an embedder compiling untrusted source
  must run compilation under an OS memory limit (cgroup, `ulimit -v`,
  container): a few kilobytes of nested source can still take gigabytes. This
  is a product exposure, not only a performance note. Fixed root causes
  (2026-11-20, each measured one process under a 4 GiB cgroup):
  `cranelift-frontend`'s SSA builder kept a dense map over every block for
  every variable (a 2,000-term top-level `+` chain took 2.6 GB to compile with
  the Fast policy, now 387 MB; 2,000 nested `else if` branches no longer
  exceed 4 GiB), Wasmtime's frame-reload pass kept a known set per block that
  grew with nesting depth (500 `else if` branches: 955 MB to 216 MB; 1,000:
  3.3 GB to 392 MB), and Boa's parser cloned every parenthesized expression's
  whole subtree once per enclosing parenthesis. Still open, each growing
  faster than the source: native compilation time of one large function with
  the single-pass allocator (1,500 `else if` branches: 103 s; a 2,000-term
  top-level `+` chain: 27 s; a 10,000-term `+` chain at a module's top level,
  which the parser admits since module chains are no longer capped at 2,000
  operators: 493 s and 2.9 GB), backtracking allocation memory (1,500 `else if`:
  1.4 GB; a 1,000-term top-level `+` chain: 1.6 GB, 2.8 GB once the chain is
  split into temporaries; with the Speed policy, the embedding default, and one
  compilation job (`SCRIPTW_JOBS=1`), native memory follows the whole function
  body, chained or not, and more jobs compile separate functions concurrently:
  2,700 `o.m();` statements in one function took 1.5 GB, three 900-call chains 1.9 GB and
  three 300-call chains what one 900-call chain does, about 510 MB, while two
  separate 900-call functions peaked at 1.2 GB, so part of one large body's
  memory outlives its compilation), SSA values in deeply nested member chains (1,000
  links: 1.5 GB, from block parameters), one large body (a top-level script
  costs about 990 Wasm bytes per global `x++` statement, 250 in a function
  body; before these fixes 600 of them took 22.8 s and 400 MB with the Fast
  policy, and 800 `if (x == i) { x++ }` statements 97 s and 2.0 GB, 8.7 s and
  2.1 GB with the Speed policy), and the earlier measurements of object
  literals, binding patterns, templates, chained assignment and class heritage
  near the nesting limit (9.7, 9.2, 3.2, 4.1 GB at 997 levels and 3.5 GB at
  250 for heritage, before these fixes; source nested 997 levels did not
  compile everywhere: heritage at 997 never finished). The parser weights
  `else if` and member chains for the compile cost of one chain, not only
  their stack, but not a body's total (see the source nesting limit in
  [runtime requirements](runtime-requirements.md#implementation-limits)); the
  other constructs are not weighted yet. One Wasm function body is limited to
  7,654,321 bytes by the validator, so a top-level script of about 20,000
  global `+` terms is a compile error. The CLI's `scriptw build wasm` and
  `precompile` of the 64 KB RSC bundle (`target/rsc-fixtures/rsc.js`) exceed
  4 GiB with the binaries before and after these fixes. Fix: split a large
  script or function body into several Wasm functions at statement
  boundaries, find the single-pass allocator's superlinear step, weight the
  remaining constructs or make their emission linear, and give the embedding
  API a compile memory budget with a typed error. Owner: AOT codegen and
  native compilation.
- Left-deep binary, logical, `??` and comma chains are walked without
  recursion by the parser, Boa's visitors and the tree's Drop and Clone, and
  lowering splits them, and template literals' parts, into IR at most about
  256 levels above their operands (`ChainSegments`: a temporary every 256
  links and after a deep operand, arranged as a tree, so about two levels
  above one much deeper operand and log2(k) + 1 above k comparably deep
  ones). In generator and async function bodies, a chain with
  an operand that suspends is still lowered and planned by passes that
  recurse per operator, so each of its operators costs half a nesting level
  (about 2,000 per chain); a chain without one is as cheap as anywhere.
  Dynamic `Function` source candidates give up on `+` chains of more than 64
  operators (`MAX_SOURCE_CANDIDATE_CHAIN`): a `new Function` body built from
  more operands that mix literals and finite bindings is reported unsupported
  where it compiled before, a cut against combinatorial growth (literal-only
  chains still compile). Conditional (`a ? b : c ? ...`) chains still nest
  one AssignmentExpression per `?`, so about 1,000 compile. Every right
  operand of `||`, `&&` and `??` (like every conditionally reached
  expression) is lowered between two snapshots of the flow facts
  (`capture_conditional_flow_facts`). A sloppy function's `this` binding,
  often the global object whose shape lists every script global, is now
  shared rather than copied: a function returning a 2,000-link `||` chain
  took 32 s to lower and emit and now takes 0.33 s, 10,000 links 177 s and
  now 0.93 s. The other facts are still copied, so compile time grows with
  the conditionally reached expressions times the tracked bindings in scope:
  1,000 object-valued globals and a 1,000-link chain take 8 s, 2,000 of each
  34 s, a compile-time cost untrusted source can drive (see
  [runtime requirements](runtime-requirements.md#compile-memory)). Fix: keep
  the facts in persistent maps. Owner: front end and lowering.
- A template literal emits about 500 Wasm bytes per substitution into its
  function body, in each of the function's copies (ToString's completion
  transport and a concatenation with its length check, inline even for a
  known String), and its native compilation grows faster than its length: at
  a Script's top level 1,000 `${x}` substitutions took 47 s and 2.8 GB with
  the Fast policy (72 s and 6.6 GB before the parts were folded into bounded
  IR), 300 `${f(i)}` substitutions about 650 MB, and 10,000 `${x}`
  substitutions in a function did not fit in 4 GiB. From about 14,000
  substitutions in one function the body exceeds the validator's
  7,654,321-byte limit, a compile error in about a second, where Node runs
  100,000 (the IR is bounded now; 30,000 overflowed the compiler stack after a
  scan of minutes). Tagged templates pass their substitutions as call
  arguments and hit the same limit at 60,000, which Node runs. The engine
  test `template_bodies_past_the_wasm_body_size_limit_are_a_compile_error`
  tracks the limit at 20,000 substitutions and is inverted by the fix.
  Fix: emit each part through one out-of-line ToString-and-append helper, or
  split large bodies (see the compile memory item). Owner: AOT codegen.
- Property access chains (`a.b.c…`) and calls of call results (`f()()…`)
  compile natively in memory quadratic and superlinear in their length (450
  `.a` links: 0.5 GB with the Fast policy; 450 calls: 0.4 GB), while method
  call chains grow more slowly (Fast: 1,000 `.m()` calls 261 MB and 14 s;
  1,500: 1.25 GB and 37 s; 3,000, which Node runs, do not fit in 4 GiB;
  1,500 `Map#set` calls: 132 s). So the parser charges those links two
  nesting levels and a method call's links half a level each (about 1,000
  calls, 998 at a Script's top level). Measured with the Speed policy and one
  compilation job (`SCRIPTW_JOBS=1`) at bcfa01be7, the longest admitted chain
  stays under 0.7 GB: `.m()` at top level 500 calls 297 MB and 3 s, 750 445 MB and 6 s, 950 647 MB and 10 s;
  `Map#set` 500 378 MB and 10 s, 750 541 MB and 23 s, 950 680 MB and 51 s;
  900 calls in a function body 514 MB and 24 s. Their memory is the body's
  (see the compile memory item), which a chain weight cannot bound; a
  per-body budget would have to count every call, chained or not. Each call
  emits about 840 Wasm bytes and three new locals, in each of the function's
  two copies (the exact-context clone). Lowering a method chain in a function
  body is quadratic in its length (19 s of the 24 s at 900 calls):
  `lower_call` clones the receiver's whole IR into the callee, and dynamic
  `Function` source candidates (`function_source_value_candidates`) recurse
  through the chain at every link. Fix: find the per-call native cost and
  share one copy of the receiver, then lower the weight. Owner: AOT codegen
  and native compilation (lowering: front end and lowering).
- The frame-reload pass's budget is 64 known entries per reachable block on
  average; a function over it keeps all its loads, deterministically, and its
  Speed-policy cost is unmeasured (for example few blocks and many stack-map
  slots). Measure such a function and consider a debug-build record when the
  budget fires. Owner: native compilation (vendored Wasmtime translator).
- Boa's scope analysis (`boa_ast::scope::Scope`) finds a binding by a linear
  search of its scope, so a scope with n bindings costs O(n^2) to analyze:
  one function of 65,535 parameters (the limit) takes about 25 s to parse,
  and a script or function with tens of thousands of `var`s or parameters is
  correspondingly slow. Fix: index each scope's bindings by name. Owner:
  front end. (`async(...)` lookahead scanned the call's tokens quadratically
  as well; it now scans them once.)
- `annexB/language/function-code/block-decl-func-skip-arguments.js` (sloppy)
  fails in every Test262 run: it expects the implicit Arguments object after
  the block, while the pinned FunctionDeclarationInstantiation tests formal
  `paramNames` only, so the Annex B function copy replaces `arguments`; the
  unchanged staging test `block-scoped-functions-annex-b-arguments.js`
  expects that replacement and passes
  ([replay](failed-case-replay-20261021.md); Test262 #5113 and PR #5112,
  [status](../../test262/errata/proposals/upstream-status-20261021.md)). It
  is one of the four published failures; resolve it with the Test262 pin
  that settles which reading the suite keeps. Owner: front end and lowering
  (Annex B function declarations).
- Correctness-batch review items left open, each with its reason: the cost of
  the native recursion guard on the hot path (an entry test in `proxy_call` and
  an eight-instruction bracket around each counted call) has no Callgrind or
  P/R size measurement yet, and the entry test could move to the native-call
  arm so that `return_call` entries into source bodies skip it (owner: emitter,
  measured with the RSC Callgrind tooling); `open_counted_calls` takes a raw
  global index rather than the `NativeRecursionDepthGlobal` newtype, which
  prevents no mistake while its one caller passes the schema's own global
  (the scope is now closed on an error return too); the bind limit counts
  bound functions and callable Proxies, but a user function whose `name`
  getter returns its target's name can still forward a growing name between
  binds, which is ordinary user allocation bounded by the String limit and the
  heap; and regular expressions nest at most 1,000 groups (and `v`-mode classes)
  where V8 compiles about 10,000 (the pattern parsers recurse per level; a
  deeper literal is a SyntaxError, a deeper pattern from any other source is
  left to the runtime compiler). The earlier
  compile memory figures of 9 to 14 GB were taken above the 4 GiB
  verification cap; the figures in the compile memory item were taken under
  it. Owners: emitter (first two), runtime built-ins (bind), front end
  (pattern nesting).
- BigInt multiplication, division and radix conversion are quadratic
  (schoolbook digits; conversion divides by the largest radix power per
  pass). Values near `MAX_BIGINT_BITS` are bounded but slow: squaring a
  2^26-bit value takes hours. `**` throws at once when its result provably
  exceeds the limit (the bound is at most e / 2^20 + e / 2^30 + 1 bits below
  the true size, and exact for a power-of-two base), so
  only a power whose result fits, or comes within that margin, computes for
  long; a work budget that refused a fitting result would be a RangeError the
  spec does not allow, so an embedder bounds the time with its execution
  deadline. V8 uses Karatsuba/Toom/FFT and divide-and-conquer conversion.
  Owner: BigInt arithmetic (performance, not correctness).
- The BigInt limit is counted in 32-bit digits (3,997,695) while values are
  stored in 64-bit limbs, so the limit is not one exact size: `1n << 127926238n`
  throws though its result fits, a value with an odd number of digits at the
  limit decodes to one digit more than the limit, so `x > 0n` and `-x` of
  `(1n << 63n) << 127926175n` throw, and working buffers reserve a digit of
  slack. Fix: define the limit once in limbs, size buffers by what the
  normalized result can reach and check the normalized length. Owner: BigInt
  arithmetic.
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
  instead, and removing the branch waits on the test-harness change that
  gives every engine unit test an explicit job count); no test drops a pooled Store while a panic or trap
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
- Emitter batch E-2 carries over from E-1, each needing an R layout bump:
  E3c (release an `eqref` value local without its null store; the GC local
  write check must then cover `eqref`, so it lands after a debug-verifier
  Test262 sweep has run over E-1), E4 (bind immutable struct fields read in
  dominated blocks once: P about 6K and R about 11K reads, `StringValue`
  code units the largest), R hot-callee renumbering below index 128 (about
  80 KB) and R's 15 duplicate bodies (14.9 KB, needs index aliasing in
  `function_layout.rs`). Measured E-1 results are in
  [benchmarks](benchmarks.md#emitter-batch-e-1-2026-11-21). Owner: emitter.
- E-1 verification scope (the batch review's M2). Before E-1 landed, the
  debug-verifier Test262 run covered 30 directories through
  `built-ins/Proxy` (12,267 cases, no Bug or Crash). Not yet run with the
  verifier: `built-ins/{TypedArray, TypedArrayConstructors, Array, Object,
  Promise, RegExp, String, DataView, Function}` and
  `language/{statements,expressions}/class`; precompiled-consumer
  `end_to_end`; a full release `report-all` compared per case with the
  2026-11-18 publication. Owner: emitter (batch E-1 follow-up).
- Third-party notices: `THIRD-PARTY-NOTICES.md` reproduces only jemalloc's
  notice. The Rust dependencies under MIT, Apache-2.0 and Unicode-3.0 need a
  generated notice file, and CI uploads `cargo build --release` binaries for
  three systems as artifacts without any notice file, so neither a dist nor a
  release binary may be published as a release artifact yet.
  `scripts/build-dist.sh` copies the file next to its binary. Fix: generate
  the notices from `Cargo.lock` (cargo-about or cargo-deny's license data)
  and ship them with every uploaded binary. Owner: release engineering
  (dependency licences).
- Dist recipe residuals, each with its reason: no committed command compares a
  dist binary's outputs with release (the identity gate; until one exists,
  CONTRIBUTING asks for a manual comparison); the committed `pgo` mode has not
  completed end to end (the measured binary came from hand-resumed scripts);
  no CI job builds `--features jemalloc` or `--profile dist` (the recipe is
  local-only); `SCRIPTW_DIST_TARGET_DIR` refuses only this checkout's paths,
  not another checkout's target or an inherited `CARGO_TARGET_DIR` (the only
  deletion there is that directory's `--target` units); the lock descriptors
  are inherited by daemons a build starts (an sccache server would hold the
  lock after the run); `--check` creates the directories and lock files it
  validates and checks none of the pgo training inputs but `rsc.js`;
  `CARGO_PROFILE_*` overrides are neither refused nor recorded; and in a
  shared checkout `target/release/examples/compiler_precompiled_baseline`
  may still be the jemalloc build verification left there on 2026-11-20
  (`rsc_baseline` was rebuilt with default features on 2026-11-21): check
  for `_rjem_malloc` and rebuild with default features before a
  measurement. Owner: release engineering (dist recipe).

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
