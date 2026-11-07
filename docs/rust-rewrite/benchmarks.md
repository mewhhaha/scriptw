# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-07**, React **19.3.1**, compiler `0ca455c47636`.
Both sync and async Flight outputs match Node exactly.
32-row catalog; async adds a resolved Promise, with no I/O.

| Engine | Sync · ms | Async · ms |
| --- | ---: | ---: |
| Scriptw Fast | 7.148 | 7.783 |
| Node 24 / V8 | 0.578 | 0.460 |

![Retained RSC request timings](assets/rsc-current/runtime.svg)

Each engine uses eight fresh processes across two balanced blocks, with three
checked warmups and seven measured requests per mode. A process retains one
Module/Realm and runs sync then async. Requests include full Flight serialization
and consumption; JSON encoding and exact-output checks follow timing. Values
are medians of process medians. The frozen prior-build control stays in the
receipts; chart bars show the latest compiler and V8. Scriptc's React build
remains unqualified; client React/DOM is a separate open scope.

Cold Scriptw setup is **4.120 s JavaScript → Wasm**,
**7.189 s native preparation**, and
**2.666 ms module initialization**.
All setup precedes request timing. The linked size/allocation/PMU diagnostics
retain their original earlier build identities; their counts were not refreshed.
Child peak RSS includes compilation and process history; it is not live guest heap.

The matched prior-build control is essentially neutral for requests: sync time
is 0.4% higher and async time 1.2% lower. Cold JavaScript-to-Wasm time is 6.2%
higher and native preparation 3.9% higher in this cohort. Native preparation
includes P/R admission, packaged R loading and P compilation.

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

Scriptw compiles P with Wasmtime 47 **Fast / OptLevel::None / SinglePass**.
Shared R is built with **SpeedAndSize / Backtracking**. Its successful native
bundle admission is silent in scored runs. A separate four-process, 80-check trace
hit the packaged runtime every time. It did not reproduce the scored emission
spikes: candidate emission samples split around 505 and 740 ms, while traced
emission took 439–453 ms. The scored emission median is 32.6% higher; its cause
remains unassigned. The cohort uses
a generic target, one CPU, 16-MiB copying GC and the recorded stack/deadline/cache settings.
The **recorded baseline** [hardware-counter diagnostic](react-attribution-current.json) records
100 checked requests after three warmups in each of three fresh processes per
engine/mode. Scriptw retires **50.36 million sync /
54.26 million async user instructions per request**;
Node retires **2.54 / 2.80 million**.
Counters run without multiplexing. They count Scriptw's execution worker and
Node's main thread, including Node barrier/Promise-return edges; background
compilation/GC threads are excluded. These are unscored counts, not causal shares.

The **recorded packed-array baseline** allocation diagnostic admits the complete program
and runtime bytes before four off/on/on/off children. It records **80 exact
Flight checks** and **40 valid counting epochs** after three warmups.
Measured requests allocate a median **49,794 objects /
2,016,016 B** synchronously and
**53,305 /
2,209,728 B** asynchronously.
Physical StoredValue wrappers account for **19,935 /
637,920 B** and
**21,064 /
674,048 B**, respectively.
These aligned GC reservations include dead objects and host-bridge work;
copied survivors are excluded. They do not identify allocation origins or retained
memory. These counts retain their original compiler/host and exact P/R identities.
No fresh allocation census was run for the current cohort. Timings stay unscored.
[Counts, source identities and limits](react-allocation-current.json).

Recorded totals and authenticated physical labels use medians of process medians.
Per-type medians need not sum to the total. ValueArray is not an argument-origin
count; physical descriptor counts do not explain why those objects were allocated.

| Recorded total or physical type | Sync · objects / B | Async · objects / B |
| --- | ---: | ---: |
| BindingCell | 346 / 27,680 | 359 / 28,720 |
| CodeUnitArray | 3,371 / 282,416 | 3,925 / 358,576 |
| ForInEnumerationRecord | 99 / 3,168 | 101 / 3,232 |
| OrdinaryObject | 1,327 / 63,696 | 1,413 / 67,824 |
| OrdinaryPropertyIndex | 820 / 52,832 | 864 / 55,648 |
| OrdinaryPropertyStorage | 938 / 30,016 | 1,000 / 32,000 |
| PropertyDescriptor | 8,412 / 269,184 | 8,790 / 281,280 |
| PropertyEntry | 1,904 / 91,392 | 1,993 / 95,664 |
| PropertyKeyTable | 1,916 / 97,648 | 1,956 / 99,600 |
| PropertyTable | 820 / 39,536 | 864 / 41,648 |
| StoredValue | 19,935 / 637,920 | 21,064 / 674,048 |
| StringValue | 3,284 / 105,088 | 3,827 / 122,464 |
| ValueArray | 3,150 / 120,112 | 3,428 / 135,616 |

The earlier allocation and hardware-entry diagnostics remain baseline evidence
with their original runtime identities in the prior archive. Their counts were not
remeasured for caller-thread entry and do not establish removable boxing or time.

Top-level embedded calls use a cached guarded caller-thread fiber. Reentry keeps
an independent scoped thread; the 64-MiB host and 32-MiB Wasm stack limits remain.
Programs use the existing WasmGC value, environment, object and completion protocols.
Admitted numeric regions retain live guards and the complete semantic fallback;
preflight costs remain timed. Strict read-only arguments index-zero/length reads
use invocation snapshots; missing indices retain the real object and generic Get.
Bounded numeric bodies guard own Number data and captured cells before source work,
then preserve arithmetic order and immediate canonical writes. Refusals use the
original body. The current RSC source, program and runtime identities are recorded separately;
V8 parity remains open.

A recorded prior **one-process, unscored** Speed/Backtracking P diagnostic produced
20 exact Flight checks: 6.219 ms sync, 6.667 ms async, and 9.456 s native preparation.
It is not part of the Fast cohort and establishes no policy win or regression.

Recorded baseline user-cycle sampling attributes **25.51% sync /
24.99% async** to six property/enumeration helpers' disjoint
own samples. The `vn`/`ft` page-touch loops account for
**2.06% /
2.25%**. Each mode has one
sampled process and 200 checked requests after three warmups. Frame-pointer
chains can truncate; inclusive caller shares overlap. These observations do not
partition wall time or predict a change's gain. The linked diagnostic retains
raw samples, source identities and same-process prefixes. These worker-thread
diagnostics were not refreshed for caller-thread entry; control-edge boundaries differ.

Older allocator controls, source screens and rejected experiments retain their
own source/binary identities in the inert archive. The allocator census found
most SinglePass spill capacity in cross-block/edge homes; strictly block-local
reuse was rejected as the main hill. Entry stack budgets are distinct from spill
capacity, executed spill traffic and allocation counts. GC allocation
reservations are recorded above; their source origins remain unmeasured.

Runtime ABI 52 retains private hashed String Set storage for for-in cursors.
The runtime supplies bounded UTF-8 codecs, byte streams with default readers,
events, abort signals and tasks for these fixtures. Fetch/Request/URL, DOM nodes
and full WHATWG conformance remain unsupported or unverified.

[Current report and process receipts](rsc-current.json) ·
[Size accounting](rsc-wasm-size-current.json) ·
[Inert current evidence](situations-evidence-2026-11-07-for-in.tar.xz).

## Focused performance situations

Measured **2026-11-07**: **21** standalone Module programs, compiler `0ca455c47636`.
Node and Rust versions retain their exact identities in the report.
Each program has its own score; no aggregate combines arbitrary operation counts.

| Program · retained call | Scriptw Fast · ms | Scriptw Speed · ms | V8 · ms |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.00544 | 0.00535 | 0.000376 |
| `global-shallow.js` | 2.3 | 1.81 | 0.988 |
| `global-deep8.js` | 2.77 | 2.02 | 0.992 |
| `global-bound-deep.js` | 0.697 | 0.235 | 0.00426 |
| `global-getter-deep.js` | 3.07 | 2.26 | 0.979 |
| `frame-compact.js` | 8.26 | 5.79 | 0.0581 |
| `frame-branches32.js` | 8.43 | 6.08 | 0.0826 |
| `frame-branches128.js` | 8.88 | 5.96 | 0.113 |
| `frame-branches512.js` | 12.7 | 6.19 | 0.18 |
| `args-unused.js` | 0.65 | 0.369 | 0.00312 |
| `args-observed.js` | 1.11 | 0.79 | 0.00314 |
| `property-stable.js` | 0.152 | 0.072 | 0.0718 |
| `property-mutated.js` | 0.152 | 0.0742 | 0.0627 |
| `metadata-ordinary.js` | 0.201 | 0.0828 | 0.0125 |
| `metadata-function.js` | 0.195 | 0.0819 | 0.212 |
| `metadata-intrinsic.js` | 0.199 | 0.0837 | 0.226 |
| `typeof-dispatch.js` | 0.948 | 0.388 | 0.0139 |
| `singleton-dispatch.js` | 0.919 | 0.363 | 0.0105 |
| `strict-equality-numbers.js` | 0.0819 | 0.0341 | 0.0148 |
| `strict-equality-mixed.js` | 0.0889 | 0.0394 | 0.0352 |
| `for-in-wide.js` | 0.669 | 0.58 | 0.0382 |

Each value is the median of eight fresh-process medians across two balanced
blocks. Every process initializes one Module/Realm, then runs 100 checked
warmups and 21 measured calls. Four inputs rotate identically in every actor;
every call checks an independent integer checksum. Bars show current configurations;
whiskers span process medians. Block medians and process summaries remain in the
[data](situations-current.json).

**Fast = None / SinglePass**, **Speed = Speed / Backtracking**. Both use Wasmtime
47, a generic target, one CPU, 16-MiB copying GC, a 4-GiB cap and no swap.
The labels select P compilation; shared R is built with SpeedAndSize /
Backtracking and uses its recorded native-image admission path.
Compilation, initialization and `prepare(input)` precede the public `run()` clock.
Wasmtime consumers have their compiler disabled. Node uses the same source in
`vm.SourceTextModule`, with normal JIT and natural GC; 100 warmups do not prove
optimized steady state. Timings include the public bridge and print conversion;
output draining and validation follow timing. Setup can influence timed GC/JIT.
The empty control is shown without subtraction. File preparation and output validation remain outside the public-call timer.

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
The recorded prior 512-site native P image is **5.43 MiB Fast /
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

A separate direct comparison with the prior published build passes 4,840 calls
across 40 fresh processes. Fast is **3.01× faster** and Speed **3.40× faster**;
both balanced blocks improve. Its raw observations remain in the evidence archive.

The separate **unscored width diagnostic** fixes 1,024 yields per call over
null-prototype records: counting keys and summing values are separate programs.
Preparation uses fixed-length String keys and stays outside timing.
Bars and this table show the current compiler and V8; the balanced five-actor
control retains both compiler images with their original identities.

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

![For-in width](assets/situations-current/enumeration-width.svg)

All 360 balanced width processes and 43,560 calls
pass exact checks. [Width data and paired image receipts](for-in-width-current.json).
This diagnostic does not isolate OwnKeys, hashing, property reads or GC costs.
Its baseline is the separately qualified set-only build. One wide-value Speed
block is 2% slower, despite an improved overall median; block variation remains
part of the result.

### Public-call control and verification

![Public-call control](assets/situations-current/control.svg)

All **504** situation processes and **60,984** calls pass exact checks. The balanced RSC campaign adds 24 processes and 480 exact Flight checks.
The separate width control is documented above. Prior arguments/frame controls
remain in their original archive; they were not relabeled current.

Latest scoped verification (**2026-11-07**, compiler `0ca455c47636`): 1,093 Engine unit
and 1,781 integration cases pass; 70 emission, 637 structural
and 380 source-guard checks pass. AOT units pass 664 cases; four known
bootstrap assertions remain failures. 1,630 IR unit cases pass on this source; 771 IR integration
cases retain original source/binary receipts with an unchanged-input proof.
All 60,984 focused calls, 43,560 width-control calls
and 480 exact RSC checks pass.
These scopes do not replace the published full Test262 run. Full workspace CI
was not rerun; eleven formatting failures and fourteen stale Intl recipes
remain [owned](current-inventory.md).

[Generate/run a hill](../../CONTRIBUTING.md#focused-performance-situations) with
`--case <file.js>` and optional `--policies fast` or `speed`. Scriptc has no matched
retained-Module consumer here. These scores are separate from RSC and the older
fresh-entry kernels; they do not partition each mechanism's share of RSC cost.

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
