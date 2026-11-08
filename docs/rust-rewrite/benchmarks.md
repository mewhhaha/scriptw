# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-08**, React **19.3.1**, source fingerprint `2255a0080df2`. The manifest-bound dirty source snapshot is based on Git `7b9841501050`; that base commit alone does not identify the measured implementation. Both Flight outputs match Node exactly. The 32-row catalog async form adds a resolved Promise and extra markup; neither request performs I/O.

| Current policy | Sync · ms (median; min–max) | Async · ms (median; min–max) |
| --- | ---: | ---: |
| Scriptw Fast | 4.714 (4.644–5.044) | 5.107 (5.056–5.518) |
| Scriptw Speed | 3.726 (3.645–3.802) | 4.056 (4.004–4.182) |
| Node 24 / V8 | 0.493 (0.470–0.546) | 0.398 (0.322–0.542) |

![Current RSC request timings](assets/rsc-current/runtime.svg)

The campaign uses 8 fresh processes per policy in 2 balanced blocks, with 3 warmups and 7 timed requests per mode. It records 640 exact-checked Flight calls. Each process retains one Module/Realm; setup precedes the request window, while JSON encoding and exact-output validation follow it. Rows show medians of process medians, with process-median bounds. The chart contains current Fast, Speed and V8 only; the published baseline remains a separately identified raw control.

Fast and Speed are separately scored native compilation policies. Their source, executable, P, R and request inputs match; Cranelift optimization and register allocation differ together.

Against the matched prior Fast control, request medians improve 2.2% sync and 2.1% async. The selected implementation keeps property caches in shared R, transports up to five arguments in locals, fuses admitted captured calls into Number loops and writes mutable binding values directly. Broad P cache inlining was rejected: it grew P 33% and regressed Fast requests by 6–7%. A thinner P path gained only 1–2% in an unscored screen while growing P 21%. Source snapshots and raw results are in the [evidence archive](scriptw-performance-levers-evidence-2026-11-08.tar.xz).

| Cold phase · median | Fast | Speed |
| --- | ---: | ---: |
| Front end and lowering · s | 2.667 | 2.668 |
| Wasm emission · s | 0.431 | 0.431 |
| Native preparation · s | 5.559 | 7.511 |
| Source to Wasm · s | 3.098 | 3.099 |
| Realm and instantiation · ms | 0.573 | 0.607 |
| Module initialization · ms | 2.061 | 1.742 |

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

The emitted program P is 4,327,610 bytes (SHA-256 `8d69007a874b606b13d1e840b72c7f75b56fa8875c853e86d7efa47f422e637e`). Packaged runtime R is 38,768,915 bytes (SHA-256 `95ad9e205549dab9a64cb841319bf866e0da37afdfef7507ff00eb10908bb4ec`). These are artifact sizes, not allocation or resident-memory measurements. Exact executable and producer identities are in [the current report](rsc-current.json).

### RSC profiling evidence

Fresh request counters for compiler `2255a0080df2`. Values are unscored diagnostics.

| Mode | Actor | Cycles/request | Instructions/request | Branches/request | Branch misses/request |
| --- | --- | ---: | ---: | ---: | ---: |
| sync | fast | 17,589,548.5 | 39,944,688.0 | 5,593,143.0 | 22,645.5 |
| sync | speed | 13,563,641.5 | 36,827,263.0 | 5,460,766.0 | 15,901.5 |
| sync | v8 | 1,356,714.5 | 2,541,786.5 | 500,716.5 | 5,037.5 |
| async | fast | 19,146,436.5 | 43,486,086.5 | 6,174,704.0 | 25,239.5 |
| async | speed | 14,914,027.5 | 40,200,914.0 | 6,035,331.5 | 18,316.5 |
| async | v8 | 1,445,820.5 | 2,669,739.5 | 528,488.5 | 5,534.0 |

| Mode | Native entry | Prior/request | Current/request |
| --- | --- | ---: | ---: |
| sync | `js::vn#f133` | 276 | 276 |
| sync | `js::ft#f123` | 242 | 242 |
| sync | `helper::function_call` | 2,538 | 1,879 |
| sync | `helper::cached_property_read` | 2,548 | 2,548 |
| async | `js::vn#f133` | 291 | 291 |
| async | `js::ft#f123` | 254 | 254 |
| async | `helper::function_call` | 2,691 | 1,998 |
| async | `helper::cached_property_read` | 2,698 | 2,698 |

| Mode | Native body | Prior symbol bytes | Current symbol bytes | Prior mapping bytes | Current mapping bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| sync | `js::vn#f133` | 1,506,048 | 1,572,240 | 31,842,304 | 31,776,768 |
| sync | `js::ft#f123` | 556,048 | 574,512 | 31,842,304 | 31,776,768 |
| async | `js::vn#f133` | 1,506,048 | 1,572,240 | 31,842,304 | 31,776,768 |
| async | `js::ft#f123` | 556,048 | 574,512 | 31,842,304 | 31,776,768 |

The two source bodies total 518/545 baseline native entries per sync/async request. These counts overlap helper entries and do not count vectors or physical allocations.

No fresh leaf sampling or physical allocation census. [Archived leaf sampling for compiler `5aa48ad8a545`](scriptw-property-kernels-evidence-2026-11-07.tar.xz) retains its old compiler identity.

Fast/Speed/V8 PMU windows use raw nonmultiplexed caller/main-thread counters. Speed combines optimization and register allocation changes. Native symbol extents are code sizes; containing executable mappings are broader regions, and neither measures stack frames.


## Focused performance situations

Measured **2026-11-08** on source `2255a0080df2`. All 21 programs use two balanced blocks and eight fresh processes per actor. Fast, Speed and V8 are current policies; this is not a before/after series.

| Program · retained call | Fast · ms (median; min–max) | Speed · ms (median; min–max) | V8 · ms (median; min–max) |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.005255 (0.005139–0.00531) | 0.0052 (0.005069–0.00544) | 0.000361 (0.0003–0.000381) |
| `global-shallow.js` | 2.24274 (2.21636–2.3037) | 1.82358 (1.78822–1.87597) | 0.984215 (0.925338–1.00343) |
| `global-deep8.js` | 2.74447 (2.71951–2.79397) | 2.06074 (2.03936–2.07554) | 0.996508 (0.947129–1.06036) |
| `global-bound-deep.js` | 0.650165 (0.644765–0.654413) | 0.216902 (0.214107–0.244004) | 0.004308 (0.002345–0.004579) |
| `global-getter-deep.js` | 2.97213 (2.92984–2.99675) | 2.19718 (2.16867–2.27933) | 0.985147 (0.938443–1.02057) |
| `frame-compact.js` | 2.05863 (2.04008–2.15498) | 0.975989 (0.963751–0.982236) | 0.057765 (0.057349–0.058772) |
| `frame-branches32.js` | 8.26369 (8.11337–8.30669) | 5.89935 (5.85895–6.21956) | 0.082802 (0.081575–0.086334) |
| `frame-branches128.js` | 8.66395 (8.57749–8.84299) | 5.98293 (5.85009–6.43743) | 0.113651 (0.112935–0.116281) |
| `frame-branches512.js` | 10.7892 (10.3446–12.4362) | 6.11094 (6.05184–6.18282) | 0.181585 (0.17898–0.190362) |
| `args-unused.js` | 0.106662 (0.105761–0.109588) | 0.0566475 (0.055916–0.088809) | 0.003156 (0.003046–0.02647) |
| `args-observed.js` | 1.09131 (1.05632–1.12091) | 0.770353 (0.754343–0.817573) | 0.003141 (0.003066–0.007735) |
| `property-stable.js` | 0.151273 (0.149374–0.15244) | 0.07098 (0.069843–0.071596) | 0.0717865 (0.071556–0.072427) |
| `property-mutated.js` | 0.151027 (0.149634–0.152299) | 0.0729285 (0.072688–0.073259) | 0.0621835 (0.061998–0.062439) |
| `metadata-ordinary.js` | 0.200371 (0.199169–0.207965) | 0.0822365 (0.080783–0.086524) | 0.012278 (0.012193–0.015429) |
| `metadata-function.js` | 0.194444 (0.191474–0.197415) | 0.0812745 (0.080223–0.083298) | 0.204293 (0.202845–0.208767) |
| `metadata-intrinsic.js` | 0.199028 (0.196683–0.201232) | 0.083664 (0.081956–0.087266) | 0.224967 (0.215299–0.227723) |
| `typeof-dispatch.js` | 0.922192 (0.914487–0.982176) | 0.375543 (0.36839–0.418986) | 0.013912 (0.013746–0.025348) |
| `singleton-dispatch.js` | 0.894254 (0.877456–1.12056) | 0.397335 (0.393137–0.42091) | 0.010585 (0.006332–0.012744) |
| `strict-equality-numbers.js` | 0.0815095 (0.079421–0.082928) | 0.033934 (0.032472–0.033975) | 0.012339 (0.011352–0.014908) |
| `strict-equality-mixed.js` | 0.0894245 (0.087847–0.090322) | 0.0348715 (0.034054–0.035598) | 0.034415 (0.033594–0.035628) |
| `for-in-wide.js` | 0.635617 (0.631931–0.680623) | 0.562704 (0.557199–0.576255) | 0.037942 (0.036509–0.060385) |

All scores time the public retained call after preparation; each program keeps its own workload and exact-output checks. No workload ratio or blended score is reported. [Full identities and observations](situations-current.json).

### Native program image sizes

| Retained program | Fast native P · KiB | Speed native P · KiB | Portable Wasm P · KiB |
| --- | ---: | ---: | ---: |
| `frame-compact.js` | 2260.2 | 972.1 | 193.2 |
| `frame-branches32.js` | 2195.2 | 967.2 | 210.3 |
| `frame-branches128.js` | 2854.1 | 1233.4 | 312.5 |
| `frame-branches512.js` | 5486.8 | 2330.2 | 727.7 |

Native serialized sizes include code metadata and data; they do not measure peak RSS or runtime heap.


### Global lookup

Six constructor values are read over 1,024 passes. Deep lookup retains eight
padding captures; bound aliases sit beside the worker. Shallow/deep also differ
in padding arithmetic, and V8 may hoist aliases or global reads. Getter controls
include calls and counter increments. These differences are not isolated lookup costs.

![Global lookup](assets/situations-current/global-lookup.svg)

### Control-flow and call-site scaling

These programs execute 8,192 logical source applications across selected sites with matching checksums. Fast and Speed may fuse admitted calls into the loop body, so logical applications are not a direct-call count. Compact dispatch uses one polymorphic site and a dynamic key; switch arms use constant keys and separate sites. This does not isolate stack-probe cost.
The current 512-site native P image is **5.36 MiB Fast /
2.28 MiB Speed**;
portable P is **0.71 MiB**.
Native serialized sizes include metadata and data.

![Call-site scaling](assets/situations-current/frame-size.svg)

![Native program image growth](assets/situations-current/frame-native-image.svg)

### Arguments observation

Both programs evaluate 1,024 logical strict calls, reassign a formal and compute the same
result. The observed case additionally reads the original arguments and length.
Admitted calls may be fused. The difference includes the observed case's timed work and is not an allocation count.

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

All 5,808 calls in that cohort passed. Raw programs, timings and the separate
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

The current situation cohort passes 504 processes and 60,984 exact calls. The current RSC cohort passes 32 processes and 640 exact Flight calls. The fixed-yield for-in width control remains historical on source `0ca455c47636` and its earlier archive.

Latest scoped verification (**2026-11-08**): 2,550 broad checks across 12 scopes; AOT unit checks pass 671 with 4 known failures; Engine unit checks pass 1,093 with 0 failures. Source guards pass 380; production CLI smoke checks pass 8. Full workspace CI and Test262 were not rerun. Eleven pre-existing formatting failures, fourteen stale Intl recipes and the existing host guard's historical-selector inventory failure remain [owned](current-inventory.md).

Benchmarks retain compiler `2255a0080df2`; the separately built CLI records `cd9a276f1c38`. The intervening fingerprinted change corrects one inline `cfg(test)` arity fixture, with its production prefix byte-identical. Six subsequent external-test edits only sort imports. Exact source snapshots, retained passing scopes, failures and both producer identities are qualified in [the current report](rsc-current.json) and its archive; binary equality is not inferred.

The situation runner’s wrapper retained its original terminal status: failed at stale 5aa orchestration assertion; underlying campaign complete and all pins/resources retained. The pinned successful native campaign was qualified without rerunning measurements.

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
