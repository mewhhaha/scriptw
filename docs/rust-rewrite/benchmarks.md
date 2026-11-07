# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-07**, React **19.3.1**, source fingerprint `5aa48ad8a545`. The exact source is the manifest-bound patch and untracked emission test on Git base `6ae49d87bd9e`; the base revision alone is not the measured source identity. Sync and async Flight outputs match V8 exactly. The 32-row catalog's async form adds a resolved Promise and extra markup; neither path performs I/O.

| Engine | Sync · ms | Async · ms |
| --- | ---: | ---: |
| Scriptw Fast · current | 5.077 | 5.429 |
| Node 24 / V8 | 0.494 | 0.402 |

![Retained RSC request timings](assets/rsc-current/runtime.svg)

The balanced campaign has eight fresh processes per engine in two blocks, three warmups and seven timed requests per mode. It covers 24 processes and 480 exact Flight checks. A process retains one Module/Realm and runs sync then async. Timing includes full Flight serialization, consumption and the fixture's sanity parsing. Compilation and initialization precede requests; JSON encoding, host copying and exact-output checks follow request timing. Rows show medians of process medians. Against the matched prior build, request time falls 11.4% / 11.6%. The chart shows current Fast and V8.

P uses Wasmtime 47 **Fast / None / SinglePass**; shared R uses
**SpeedAndSize / Backtracking**. Both retain the required experimental backend,
generic target, one CPU and 16-MiB copying GC under a 4-GiB cap with no swap.
Cold candidate phase medians are **2.752 s front end/lowering**, **0.442 s Wasm
emission**, and **5.687 s native preparation**. JavaScript-to-Wasm totals
**3.195 s**; native preparation includes P/R admission, packaged R loading and P
compilation. Realm/instantiation takes **0.568 ms** and module initialization
**2.069 ms**, separately from retained requests. Peak child RSS includes this
setup and process history, rather than measuring live guest heap.

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

The emitted program P is 4,232,136 bytes, identical to the matched baseline. Shared runtime R is 38,738,181 bytes, 5,574 bytes larger than baseline. Exact executable, Wasm and producer identities remain in [the current report](rsc-current.json). No fresh physical allocation census was taken. The linked allocation and deep Wasm-size reports retain their earlier source, executable and P/R identities; they do not describe this build.

### RSC request attribution

The v4 attribution cohort is unscored. Three fresh processes per actor/mode count 100 requests after three warmups; PMU targets the authenticated Scriptw caller or Node main thread, includes control edges and excludes background threads. No group was multiplexed. Values are medians of process medians. `new-speed` changes Cranelift optimization and register allocation together; it is a diagnostic policy, not a request-time score.

| Actor / policy | Sync instructions · M | Async instructions · M | Sync cycles · M | Async cycles · M |
| --- | ---: | ---: | ---: | ---: |
| Matched prior Fast | 48.747 | 52.611 | 22.653 | 25.108 |
| Current Fast | 41.229 | 44.816 | 20.759 | 20.642 |
| Current Speed diagnostic | 37.764 | 41.166 | 14.004 | 16.097 |
| Node / V8 | 2.526 | 2.686 | 1.363 | 1.460 |

Against the matched prior Fast build, current instructions fall 15.4% sync and 14.8% async; cycles fall 8.4% and 17.8%. Fast still uses 16.3× / 16.7× Node's instructions. Speed is a combined optimization/allocator setting, not evidence for either component alone.

Three current Fast samples per mode cover 200 checked requests each. Mean per-process own-leaf shares:

| Sampled code location | Sync | Async |
| --- | ---: | ---: |
| Object/property/enumeration functions | 33.84% | 32.70% |
| Compiled JavaScript bodies/adapters | 29.48% | 29.17% |
| Other compiled runtime functions | 18.58% | 18.55% |
| Other resolved host functions | 7.26% | 8.41% |
| Wasmtime GC support | 3.71% | 4.02% |
| Wasmtime subtype routines | 3.35% | 3.32% |
| Selected Promise/stream/text-codec functions | 1.93% | 2.04% |
| Unresolved samples | 1.84% | 1.79% |

The hottest own-leaf functions are stable across three samples. Shares below are mean per-process values; ranges show the low and high process share.

| Function | Sync mean (range) | Async mean (range) |
| --- | ---: | ---: |
| `vn#f133` | 11.16% (10.89–11.33%) | 11.38% (11.27–11.46%) |
| `helper::object_read` | 7.34% (7.27–7.40%) | 7.32% (7.08–7.51%) |
| `ft#f123` | 6.36% (6.09–6.63%) | 6.09% (5.63–6.50%) |
| `helper::ordinary_property_find` | 3.74% (3.59–3.90%) | 3.54% (3.47–3.66%) |
| `helper::for_in_advance` | 3.40% (3.22–3.63%) | 3.03% (2.99–3.09%) |
| `js::f42` | 3.24% (3.15–3.32%) | 3.08% (2.89–3.32%) |

Function samples identify code locations, not allocation origins or removable time. Inlined allocation, guards and value transport remain charged to their containing body. GC support includes allocation/reference operations and collection. Caller stacks can truncate; category shares are disjoint, while inclusive stacks overlap. Node's function-map/interpreter-stack samples and literal-null controls are retained from the explicitly identified older diagnostic build; the current Node PMU counts were freshly measured.

Hardware execute breakpoints count helper entries over seven request windows per mode. Each repeated count is exact. The matched prior column is the old measured RSC binary, not a reconstruction. Three implemented mechanisms reduce internal descriptor or receiver work while retaining exotic fallback: ordinary receivers use canonical definition without a JavaScript descriptor carrier; ordinary Get reuses a successful absence/accessor probe; and scalar observations ask for existence/enumerability without cloning a full descriptor. The table reports matched whole-request entry counts; it does not assign every reduction to one mechanism.

| Native entry per request | Sync prior → current | Async prior → current |
| --- | ---: | ---: |
| `helper::object_read` | 7,823 → 7,030 | 8,269 → 7,457 |
| `helper::ordinary_property_find` | 10,604 → 6,799 | 11,187 → 7,231 |
| `helper::ordinary_own_descriptor` | 9,711 → 4,304 | 10,207 → 4,594 |
| `helper::ordinary_property_append` | 1,707 → 914 | 1,790 → 978 |
| `helper::intrinsic_property_find` | 5,410 → 4,330 | 5,704 → 4,561 |
| `helper::object_has_property` | 3,034 → 1,804 | 3,184 → 1,912 |
| `builtin::Reflect.defineProperty` | 240 → 35 | 250 → 38 |
| `helper::function_call` | 2,743 → 2,538 | 2,903 → 2,691 |

These are overlapping helper-entry populations, not JavaScript expression or allocation counts. Source-derived writes and physical heap census are separate measurements. No fresh allocation census or cache-miss event group is included. The legacy allocation census and Wasm section accounting remain available with their original identities: [allocation report](react-allocation-current.json) · [Wasm size report](rsc-wasm-size-current.json).

[Fresh v4 counters, samples, entries and limits](react-attribution-current.json) · [Inert raw samples, maps, controls and drivers](scriptw-property-kernels-evidence-2026-11-07.tar.xz).

## Focused performance situations

Measured **2026-11-07**: **21** standalone Wasm programs, source fingerprint `5aa48ad8a545`. Each score is independent; no aggregate combines arbitrary operation counts. Table entries are the median of eight process medians from two balanced blocks.

| Program · retained call | Scriptw Fast · ms | Scriptw Speed · ms | V8 · ms |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.005746 | 0.005666 | 0.000396 |
| `global-shallow.js` | 2.46178 | 1.92994 | 1.04968 |
| `global-deep8.js` | 3.03218 | 2.23049 | 1.04976 |
| `global-bound-deep.js` | 0.744395 | 0.250296 | 0.0043885 |
| `global-getter-deep.js` | 3.24457 | 2.37507 | 1.06291 |
| `frame-compact.js` | 8.7721 | 6.30566 | 0.062223 |
| `frame-branches32.js` | 8.84522 | 6.22213 | 0.0877265 |
| `frame-branches128.js` | 9.32291 | 6.39185 | 0.119502 |
| `frame-branches512.js` | 13.9557 | 6.72274 | 0.191324 |
| `args-unused.js` | 0.694215 | 0.401548 | 0.003367 |
| `args-observed.js` | 1.20796 | 0.845263 | 0.003271 |
| `property-stable.js` | 0.163932 | 0.0779335 | 0.0773925 |
| `property-mutated.js` | 0.162504 | 0.0791355 | 0.0670225 |
| `metadata-ordinary.js` | 0.214107 | 0.088013 | 0.013155 |
| `metadata-function.js` | 0.210325 | 0.087962 | 0.220034 |
| `metadata-intrinsic.js` | 0.228174 | 0.0911485 | 0.235277 |
| `typeof-dispatch.js` | 1.03294 | 0.405637 | 0.014878 |
| `singleton-dispatch.js` | 0.978796 | 0.427288 | 0.0114165 |
| `strict-equality-numbers.js` | 0.0873505 | 0.03674 | 0.0128545 |
| `strict-equality-mixed.js` | 0.095011 | 0.0418195 | 0.03692 |
| `for-in-wide.js` | 0.690513 | 0.609815 | 0.039831 |

All **504** processes passed output checks: 100 warmups and 21 timed calls in each of eight fresh processes per actor and program. Each process initializes one Module/Realm, then times the public `run()` after untimed preparation. The 60,984 timed and warmup calls all passed exact checks. Fast uses None / SinglePass; Speed uses Speed / Backtracking. Node 24.12.0/V8 uses the same source with normal JIT and natural GC. Fast, Speed and V8 are current configurations, not before/after compilers. [Full identities and raw observations](situations-current.json).

Four prepared inputs rotate identically across actors. Wasmtime consumers have
their compiler disabled; P and shared R are prepared before execution. The public
clock includes the bridge and print conversion; output draining and validation
follow it. Node uses `vm.SourceTextModule`; 100 warmups do not prove optimized
steady state. All actors retain one CPU, a 4-GiB cap and no swap. Scriptw uses
16-MiB copying GC; setup can influence later GC. The empty control is not subtracted.

### Global lookup

Six constructor values are read over 1,024 passes. Deep lookup retains eight
padding captures; bound aliases sit beside the worker. Shallow/deep also differ
in padding arithmetic, and V8 may hoist aliases or global reads. Getter controls
include calls and counter increments. These differences are not isolated lookup costs.

![Global lookup](assets/situations-current/global-lookup.svg)

### Control-flow and call-site scaling

All programs make 8,192 entries and exercise every selected site with matching
checksums. Compact dispatch uses one polymorphic site and a dynamic key; switch
arms use constant keys and separate sites. This does not isolate stack-probe cost.
The current 512-site native P image is **5.43 MiB Fast /
2.30 MiB Speed**;
portable P is **0.72 MiB**.
Native serialized sizes include metadata and data.

![Call-site scaling](assets/situations-current/frame-size.svg)

![Native program image growth](assets/situations-current/frame-native-image.svg)

### Arguments observation

Both programs make 1,024 strict calls, reassign a formal and compute the same
result. The observed case additionally reads the original arguments and length.
The difference includes that timed work and is not an allocation count.

![Arguments observation](assets/situations-current/arguments-observation.svg)

### Property mutation history

Both programs make 8,192 variable-key reads over 64 records. Untimed mutation
adds an accessor, deletes properties and restores values. Reads retain live
storage guards and complete conversion/accessor/prototype/Proxy fallback.
Indexing, key production and record reads all contribute to the score.

![Property mutation history](assets/situations-current/property-mutation-history.svg)

### Packed-array descriptor reads

An additional three-program diagnostic times one indexed descriptor snapshot,
its value read and checksum output. Fresh array construction is untimed. These
recorded packed-array results (compiler `92d17baf84cb`) use eight processes per
engine, 100 warmups and 21 samples:

| Array elements | Scriptw Speed · µs | V8 · µs |
| --- | ---: | ---: |
| 64 | 10.78 | 0.706 |
| 1,024 | 11.77 | 0.692 |
| 4,096 | 12.43 | 0.721 |

All 5,808 current-cohort calls passed. Raw programs, timings and the separate
prior-build control are in the [evidence archive](situations-evidence-2026-11-05.tar.xz).
This descriptor cohort was not rerun for caller-thread entry; its original
source and producer identities remain in that archive.

### Function property reads

Each program reads own length, own name and the name string's length over 8,192
passes and eight targets. Ordinary records, JavaScript functions and native
constructors have matching values/checksums. Array indexing, object-kind dispatch,
string reads and V8 feedback remain part of these scores.

![Function property reads](assets/situations-current/function-properties.svg)

### Type classification

The typeof program performs 18,432 literal comparisons across 4,096 mixed-value
passes. The strict singleton program performs 16,384 null/undefined comparisons.
Inputs cover primitive and object values, bound functions and revoked callable
Proxies. Array access, branches and loop arithmetic remain timed. All earlier
program sources and checksums are unchanged. Independent semantic controls also
cover operand Throws/TDZ and HTMLDDA, outside these timed programs.

![Type classification](assets/situations-current/type-tests.svg)

### Dynamic strict equality

Both programs execute 8,192 strict comparisons over prepared pairs. Number pairs
include NaN, signed zero and infinities. Mixed pairs cover String/BigInt content,
Symbol and object identity, boxed values, functions and revoked Proxies. Operand
coercion must not run. Preparation precedes timing; pair indexing, branches and
checksum arithmetic remain timed. Independent models check every input.

![Dynamic strict equality](assets/situations-current/strict-equality.svg)

### Enumeration

Sixteen for-in loops visit 64 own enumerable properties, checking 1,024
yielded keys and numeric value reads. Private hashed String Set storage removes
visited-prefix copying. Live descriptor checks, nonenumerable shadowing,
deleted-key prototype behavior, Proxy ordering and suspended cursors retain
the compiled Wasm object model. Ordinary scalar reads observe existence and
enumerability without a full descriptor clone; other descriptor-query paths
retain their own work.
Hash collisions still require exact String equality.

![For-in enumeration](assets/situations-current/enumeration.svg)

The **historical, unscored width diagnostic** fixes 1,024 yields per call over
null-prototype records: counting keys and summing values are separate programs.
It retains compiler `0ca455c47636`, its original evidence archive and its
original SVG assets; its table and charts do not describe the current source.

| Own keys | Body | Scriptw Fast · ms | Scriptw Speed · ms | V8 · ms |
| ---: | --- | ---: | ---: | ---: |
| 8 | count | 0.471 | 0.438 | 0.0358 |
| 8 | value | 0.624 | 0.523 | 0.0478 |
| 32 | count | 0.442 | 0.42 | 0.0293 |
| 32 | value | 0.591 | 0.509 | 0.0387 |
| 64 | count | 0.46 | 0.427 | 0.0272 |
| 64 | value | 0.629 | 0.55 | 0.0411 |
| 256 | count | 0.447 | 0.423 | 0.0285 |
| 256 | value | 0.583 | 0.51 | 0.0416 |

![Historical for-in width · source 0ca455c47636](assets/situations-current/enumeration-width.svg)

All 360 balanced width processes and 43,560 calls
pass exact checks. [Width data and paired image receipts](for-in-width-current.json).
This diagnostic does not isolate OwnKeys, hashing, property reads or GC costs.
Its baseline is the separately qualified set-only build. One wide-value Speed
block is 2% slower, despite an improved overall median; block variation remains
part of the result.

### Public-call control and verification

All 504 current situation processes and 60,984 calls pass exact checks. The balanced RSC campaign adds 24 processes and 480 exact Flight checks. The fixed-yield width control remains historical on source `0ca455c47636` and its earlier archive.

Latest scoped verification (**2026-11-07**, source fingerprint `5aa48ad8a545`): 12 fresh broad scopes pass 2,535 cases; fresh Engine/AOT unit inventories add 1,757 passes with four known AOT failures. Source guards pass 380 checks; all eight production CLI smokes pass. The broad total comprises 71 emission, 674 structural across seven targets (637 emitter-side and 37 Engine structure), and 1,790 Engine AOT integration cases across four targets. The focused and broad totals describe separate scopes. Full workspace CI and full Test262 were not rerun. Eleven formatting failures and fourteen stale Intl recipes remain [owned](current-inventory.md). The earlier 1,630 IR unit and 771 IR integration counts are historical and retain their original source/archive identities.

<!-- runtime-comparison-details:start -->
## Runtime comparison

**Measured 2026-11-02 locally**, compiler fingerprint `87385fff34d1`.
V8 **13.6.233.17-node.37**, Node **24.12.0**.
Scriptw is faster than Scriptc in **17/17 workloads** in both blocks; block execution geometric means are **0.455–0.458× Scriptc**. Scriptw is faster than V8 in **6/17 workloads** in both blocks; block execution geometric means are **1.098–1.106× V8**.

This compares Wasm public entry with V8 `runInContext`. Scriptw and Scriptc compile
Wasm before the measured processes and share a Wasmtime host with compilation disabled.
Wasmtime 47.0.0 uses size-optimized/backtracking
native code and a generic target.
Timed Wasm entry includes Scriptw Realm/intrinsic bootstrap. V8 parses one `vm.Script`
per process and creates a fresh `vm.Context` before each invocation outside entry timing;
lazy compilation, JIT tiering and natural GC may occur inside. Every entry checks output.

Each bar shows the median of eight process medians: four processes per balanced
block, each with 21 measured entries after ten checked warmups. Memory bars show
median GNU-time whole-child **peak RSS** for all three runtimes, including host,
code, runtime and warmup history. Warmups do not establish optimized steady state.
Each panel uses its own zero-based scale. Chart data also retains V8 context-creation
time as a supplemental measurement, excluded from bars and ratios.

<table>
<tr>
<td><img src="assets/benchmarks-current/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; Scriptw: 0.091 ms, 21.6 MiB RSS; Scriptc: 0.133 ms, 17.2 MiB RSS; V8: 0.023 ms, 59.3 MiB RSS"></td>
<td><img src="assets/benchmarks-current/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; Scriptw: 0.133 ms, 21.7 MiB RSS; Scriptc: 0.340 ms, 17.5 MiB RSS; V8: 0.069 ms, 59.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; Scriptw: 0.133 ms, 21.7 MiB RSS; Scriptc: 0.338 ms, 17.2 MiB RSS; V8: 0.070 ms, 59.8 MiB RSS"></td>
<td><img src="assets/benchmarks-current/to-sorted-512.svg" width="440" alt="to-sorted-512.js; Scriptw: 0.143 ms, 21.9 MiB RSS; Scriptc: 0.375 ms, 17.4 MiB RSS; V8: 0.069 ms, 59.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/join-1024.svg" width="440" alt="join-1024.js; Scriptw: 0.235 ms, 22.2 MiB RSS; Scriptc: 0.410 ms, 17.5 MiB RSS; V8: 0.055 ms, 59.5 MiB RSS"></td>
<td><img src="assets/benchmarks-current/join-4096.svg" width="440" alt="join-4096.js; Scriptw: 0.699 ms, 22.1 MiB RSS; Scriptc: 1.547 ms, 17.8 MiB RSS; V8: 0.233 ms, 67.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; Scriptw: 0.269 ms, 21.4 MiB RSS; Scriptc: 0.271 ms, 17.2 MiB RSS; V8: 0.096 ms, 60.5 MiB RSS"></td>
<td><img src="assets/benchmarks-current/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; Scriptw: 0.068 ms, 20.9 MiB RSS; Scriptc: 0.095 ms, 16.6 MiB RSS; V8: 0.142 ms, 64.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; Scriptw: 0.069 ms, 20.7 MiB RSS; Scriptc: 0.184 ms, 16.9 MiB RSS; V8: 0.698 ms, 66.7 MiB RSS"></td>
<td><img src="assets/benchmarks-current/folded-string-4096.svg" width="440" alt="folded-string-4096.js; Scriptw: 0.170 ms, 21.1 MiB RSS; Scriptc: 0.808 ms, 16.4 MiB RSS; V8: 0.126 ms, 60.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; Scriptw: 0.189 ms, 20.6 MiB RSS; Scriptc: 0.285 ms, 16.7 MiB RSS; V8: 0.193 ms, 65.7 MiB RSS"></td>
<td><img src="assets/benchmarks-current/integer.svg" width="440" alt="integer.js; Scriptw: 0.070 ms, 21.1 MiB RSS; Scriptc: 0.074 ms, 16.6 MiB RSS; V8: 2.546 ms, 66.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/objects.svg" width="440" alt="objects.js; Scriptw: 0.242 ms, 21.6 MiB RSS; Scriptc: 0.679 ms, 17.5 MiB RSS; V8: 0.067 ms, 59.6 MiB RSS"></td>
<td><img src="assets/benchmarks-current/retained-objects.svg" width="440" alt="retained-objects.js; Scriptw: 6.235 ms, 25.8 MiB RSS; Scriptc: 34.484 ms, 23.4 MiB RSS; V8: 3.584 ms, 84.6 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/join-empty-1024.svg" width="440" alt="join-empty-1024.js; Scriptw: 0.077 ms, 21.4 MiB RSS; Scriptc: 0.283 ms, 17.5 MiB RSS; V8: 0.062 ms, 59.4 MiB RSS"></td>
<td><img src="assets/benchmarks-current/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; Scriptw: 0.078 ms, 21.3 MiB RSS; Scriptc: 0.130 ms, 16.6 MiB RSS; V8: 0.098 ms, 60.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/benchmarks-current/join-small-1024.svg" width="440" alt="join-small-1024.js; Scriptw: 0.077 ms, 21.4 MiB RSS; Scriptc: 0.178 ms, 16.8 MiB RSS; V8: 0.168 ms, 60.1 MiB RSS"></td>
<td></td>
</tr>
</table>

[Chart data and runtime identities](execution-current.json) ·
[Complete evidence](performance-gaps-20261101.json) ·
[Workflow](../../CONTRIBUTING.md#benchmark-charts).
<!-- runtime-comparison-details:end -->

<!-- application-benchmarks-details:start -->
## Application workloads

Measured **2026-11-02**, compiler `87385fff34d1`.
Six synchronous request/response kernels use plain records, without networking or
Web APIs. The seventh case bundles real React 19.3.1
legacy `renderToString` with hooks/context and 32 escaped rows.
Times cover complete batches: 64 requests for JSON/routing/query/middleware,
32 HTML requests, 48 nested-result requests, or one 32-row SSR render.

Two balanced blocks, eight fresh processes per available actor/case; each process
has seven measured entries after three checked warmups. Cards show the median of
process entry medians and median whole-child peak RSS. Wasm compilation/loading
and V8 source/context setup stay outside entry timing; Scriptw Realm bootstrap
stays inside. Failed actors have no timing or memory bar.

Across the **5 completed Scriptc comparisons**, Scriptw batch medians
are **3.8–9.3× Scriptc**. Failed cases receive no speed score.

<table>
<tr>
<td><img src="assets/applications-current/edge-json-api.svg" width="440" alt="edge-json-api.js; Scriptw: 3.999 ms, 26.4 MiB RSS; Scriptc: 0.793 ms, 17.7 MiB RSS; V8: 0.340 ms, 55.3 MiB RSS"></td>
<td><img src="assets/applications-current/edge-router.svg" width="440" alt="edge-router.js; Scriptw: 2.637 ms, 25.2 MiB RSS; Scriptc: 0.694 ms, 17.5 MiB RSS; V8: 0.204 ms, 54.6 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/applications-current/edge-query.svg" width="440" alt="edge-query.js; Scriptw: 6.298 ms, 27.5 MiB RSS; Scriptc: 1.268 ms, 17.6 MiB RSS; V8: 0.653 ms, 55.0 MiB RSS"></td>
<td><img src="assets/applications-current/edge-middleware.svg" width="440" alt="edge-middleware.js; Scriptw: 2.426 ms, 25.6 MiB RSS; Scriptc: 0.622 ms, 17.8 MiB RSS; V8: 0.170 ms, 54.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/applications-current/edge-html.svg" width="440" alt="edge-html.js; Scriptw: 10.564 ms, 30.8 MiB RSS; Scriptc: 1.142 ms, 17.8 MiB RSS; V8: 1.004 ms, 56.6 MiB RSS"></td>
<td><img src="assets/applications-current/edge-nested-result.svg" width="440" alt="edge-nested-result.js; Scriptw: 7.307 ms, 29.7 MiB RSS; Scriptc: failed (execution); V8: 0.390 ms, 56.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="assets/applications-current/react-ssr.svg" width="440" alt="react-ssr.js; Scriptw: failed (timed out); Scriptc: failed (build); V8: 0.762 ms, 56.7 MiB RSS"></td>
<td></td>
</tr>
</table>

Real Server Components separately serialize and consume Flight streams in Node,
including an async component. Web references use Node's Request/Response/URL APIs.
The browser cases use real React DOM in Chromium: mount, state update and hydration,
with commit plus forced layout timed in the page. RSC renders 32 rows, Web API
cases process 32 requests, and client cases render 64 rows. Setup and golden-output comparisons are
outside these timers. These host references have different timing boundaries;
they establish no engine parity. Browser RSS covers all three cases in one process
and is not assigned to individual cases.

![Node Flight and Web API references](assets/applications-current/react-server.svg)
![Chromium React DOM references](assets/applications-current/react-browser.svg)

| Reference | Entry median · ms |
| --- | ---: |
| RSC Flight · sync | 0.505 |
| RSC Flight · async | 0.571 |
| URL / Request | 2.016 |
| JSON / Request | 3.797 |
| React mount | 1.137 |
| React update | 1.347 |
| React hydration | 1.945 |

Actual standalone build/execution attempts remain visible, including failures:

| Capability attempt | Scriptw | Scriptc | V8 host reference |
| --- | --- | --- | --- |
| RSC Flight · sync | Execution failed | Build failed | Node checked |
| RSC Flight · async | Execution failed | Build failed | Node checked |
| URL / Request | Execution failed | Build failed | Node checked |
| JSON / Request | Execution failed | Build failed | Node checked |
| React client mount | Timed out (60 s) | Build failed | Chromium checked |

Standalone run timeouts include compilation and startup; they do not isolate
rendering time. Scriptw's portable build and ordinary NativeHost run are
separate probes; the run recompiles source instead of loading the dumped pair.
At this frozen benchmark revision, Scriptw lacked DOM, encoding, Web Streams,
Fetch and URL APIs, as recorded by its capability probe. Current-source RSC
measurements are reported in the [README](../../README.md#benchmarks).

The frozen [failure diagnosis](applications-current.json) records
missing `TextEncoder` during RSC initialization. Corrected Scriptc Web probes
reach `SC3002` (WASI API admission), and its nested-result case reports `SC2020`
(dynamic `Array.slice` end unsupported). React bundles retain type-check errors.
Scriptw's SSR output has a 2-MB function, triggering size-optimized native
compilation; a debugger sample finds Cranelift optimization active at 20 seconds.

[Data, identities and diagnostics](applications-current.json) ·
[Inert raw evidence](application-benchmarks-20261102-inert.tar.xz) ·
[Workflow](../../CONTRIBUTING.md#react-and-edge-benchmarks).
<!-- application-benchmarks-details:end -->

## Compilation time


**JavaScript → portable Wasm**, measured 2026-11-02 with the same compiler
fingerprint as the runtime charts. Scriptw is faster in **17/17 workloads**,
taking **0.524x Scriptc's time** by geometric mean of workload medians.

![JavaScript to portable Wasm compilation time](assets/compilation-current.svg)

Four fresh CLI builds per compiler/workload across two balanced blocks
(**136 processes**). Includes startup and output I/O: Scriptw P plus its shared R
sidecar; Scriptc standalone WASI. Application caches are fresh; OS and Zig toolchain
caches remain. Runtime production, Wasm-to-native compilation and execution are
excluded. [Data and tool identities](compilation-current.json) ·
[Reproduce](../../CONTRIBUTING.md#benchmark-charts).
