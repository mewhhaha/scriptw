# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-08**, React **19.3.1**, source fingerprint `0c2c10dac9c0`. The manifest-bound dirty source snapshot is based on Git `dfefb3222737`; that base commit alone does not identify the measured implementation. Both Flight outputs match Node exactly. The 32-row catalog async form adds a resolved Promise and extra markup; neither request performs I/O.

| Current policy | Sync · ms (median; min–max) | Async · ms (median; min–max) |
| --- | ---: | ---: |
| Scriptw Fast | 4.663 (4.586–4.745) | 5.068 (4.975–5.251) |
| Scriptw Speed | 3.695 (3.601–3.821) | 4.033 (3.957–4.159) |
| Node 24 / V8 | 0.490 (0.477–0.505) | 0.400 (0.383–0.510) |

![Current RSC request timings](assets/rsc-current/runtime.svg)

The campaign uses 8 fresh processes per policy in 2 balanced blocks, with 3 warmups and 7 timed requests per mode. It records 640 exact-checked Flight calls. Each process retains one Module/Realm; setup precedes the request window, while JSON encoding and exact-output validation follow it. Rows show medians of process medians, with process-median bounds. The chart contains current Fast, Speed and V8 only; the published baseline remains a separately identified raw control.

Fast and Speed are separately scored native compilation policies. Their source, executable, P, R and request inputs match; Cranelift optimization and register allocation differ together.

| Cold phase · median | Fast | Speed |
| --- | ---: | ---: |
| Front end and lowering · s | 2.679 | 2.678 |
| Wasm emission · s | 0.432 | 0.432 |
| Native preparation · s | 5.588 | 7.534 |
| Source to Wasm · s | 3.112 | 3.111 |
| Realm and instantiation · ms | 0.572 | 0.627 |
| Module initialization · ms | 2.052 | 1.698 |

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

The emitted program P is 4,327,610 bytes (SHA-256 `8d69007a874b606b13d1e840b72c7f75b56fa8875c853e86d7efa47f422e637e`). Packaged runtime R is 38,769,947 bytes (SHA-256 `4f8d4a6182181d74b527394cd06015a7c723b81b0dabe95964628922516b6f64`). These are artifact sizes, not allocation or resident-memory measurements. Exact executable and producer identities are in [the current report](rsc-current.json).

### RSC profiling evidence

Fresh request counters for compiler `0c2c10dac9c0`. Values are unscored diagnostics: medians of three process medians, 100 measured requests and 3 warmups per process. Only the authenticated caller/main thread is counted, including control edges; normal JIT and GC remain enabled.

| Mode | Actor | Cycles/request | Instructions/request | Branches/request | Branch misses/request |
| --- | --- | ---: | ---: | ---: | ---: |
| sync | fast | 18,489,146.0 | 39,879,202.5 | 5,586,053.5 | 22,926.5 |
| sync | speed | 13,774,270.0 | 36,761,818.0 | 5,453,693.0 | 17,228.0 |
| sync | v8 | 1,366,392.5 | 2,512,669.5 | 496,588.5 | 5,108.5 |
| async | fast | 21,126,330.5 | 43,419,419.0 | 6,167,511.0 | 25,579.5 |
| async | speed | 14,832,464.5 | 40,134,144.0 | 6,028,109.0 | 19,001.5 |
| async | v8 | 1,445,997.5 | 2,658,970.5 | 526,222.5 | 5,578.5 |

| Mode | Instrumented target | Baseline 2255/request | Candidate/request |
| --- | --- | ---: | ---: |
| sync | `builtin::Object.prototype.hasOwnProperty` | 163 | 163 |
| sync | `helper::ordinary_own_descriptor` | 4,304 | 4,074 |
| sync | `helper::value_to_property_key` | 7,458 | 7,357 |
| sync | `helper::dynamic_property_read` | 136 | 136 |
| async | `builtin::Object.prototype.hasOwnProperty` | 165 | 165 |
| async | `helper::ordinary_own_descriptor` | 4,594 | 4,360 |
| async | `helper::value_to_property_key` | 7,656 | 7,552 |
| async | `helper::dynamic_property_read` | 141 | 141 |

Per-target builtin/helper populations overlap. Do not add them into a total source-entry, call, allocation, or extent count.

The older leaf samples remain historical source `5aa48ad8a545` evidence in archived member `reports/react-attribution-current.json` from `scriptw-property-kernels-evidence-2026-11-07.tar.xz`. They do not describe the current candidate or 2255 baseline.

Fast/Speed/V8 PMU windows use raw nonmultiplexed caller/main-thread counters. Speed combines optimization and register allocation changes. Physical allocation has not been recounted.

Fresh caller profiles use 12 independent processes: Fast/Speed × sync/async × three repetitions, each with 3 warmups and 100 request windows. The table shows the largest disjoint leaf period weights; inclusive caller tables, host/unresolved symbols and unavailable callchains remain in the archived raw summary. These are cycle-sample locations, not wall-time shares or exact call counts.

| Native policy / mode | Leaf owner | Sample-period weight |
| --- | --- | ---: |
| fast / sync | `wasm:js::vn#f133` | 11.6% |
| fast / sync | `wasm:helper::object_read` | 7.7% |
| fast / sync | `wasm:js::ft#f123` | 5.8% |
| fast / sync | `wasm:helper::ordinary_property_find` | 4.0% |
| fast / async | `wasm:js::vn#f133` | 11.7% |
| fast / async | `wasm:helper::object_read` | 7.1% |
| fast / async | `wasm:js::ft#f123` | 5.6% |
| fast / async | `wasm:helper::ordinary_property_find` | 3.9% |
| speed / sync | `wasm:helper::object_read` | 9.1% |
| speed / sync | `wasm:helper::ordinary_property_find` | 4.7% |
| speed / sync | `wasm:helper::for_in_advance` | 4.6% |
| speed / sync | `wasm:js::vn#f133` | 4.4% |
| speed / async | `wasm:helper::object_read` | 8.7% |
| speed / async | `wasm:helper::ordinary_property_find` | 4.5% |
| speed / async | `wasm:js::vn#f133` | 4.2% |
| speed / async | `wasm:helper::intrinsic_property_find` | 4.1% |

The matched Fast comparison measured no meaningful RSC speedup despite the lower helper-entry counts. [Attribution data](react-attribution-current.json) and the [raw UTF-8 evidence archive](scriptw-performance-predicate-transport-evidence-2026-11-08.tar.xz) retain identities, counts, caller profiles and replay tools.

## Focused performance situations

Measured **2026-11-08** on source `0c2c10dac9c0`. All 21 programs use two balanced blocks and eight fresh processes per actor. Each program compares Fast, Speed and V8.

| Program · retained call | Fast · ms (median; min–max) | Speed · ms (median; min–max) | V8 · ms (median; min–max) |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.005205 (0.00511–0.00528) | 0.0051195 (0.00508–0.005179) | 0.000371 (0.000351–0.000381) |
| `global-shallow.js` | 2.15091 (2.13457–2.47145) | 1.80062 (1.79018–1.95783) | 0.972221 (0.939644–1.00079) |
| `global-deep8.js` | 2.72953 (2.69883–2.94225) | 1.96657 (1.94707–2.16408) | 0.945736 (0.931108–1.04589) |
| `global-bound-deep.js` | 0.641253 (0.638393–0.645846) | 0.214242 (0.212894–0.215489) | 0.004248 (0.003788–0.004288) |
| `global-getter-deep.js` | 2.95835 (2.88347–3.03896) | 2.17241 (2.15927–2.41108) | 0.972397 (0.962918–0.999088) |
| `frame-compact.js` | 1.93303 (1.92307–2.02891) | 0.973859 (0.969121–0.987565) | 0.057569 (0.056668–0.077147) |
| `frame-branches32.js` | 8.13099 (8.0468–8.59473) | 5.8768 (5.84858–6.00526) | 0.0825565 (0.079331–0.088568) |
| `frame-branches128.js` | 8.45608 (8.41308–8.71366) | 5.98491 (5.94302–6.22377) | 0.113025 (0.111662–0.114798) |
| `frame-branches512.js` | 11.3498 (10.6404–11.9646) | 6.00733 (5.92427–6.25072) | 0.178554 (0.176395–0.181375) |
| `args-unused.js` | 0.102355 (0.100131–0.104889) | 0.0554 (0.054904–0.059693) | 0.003136 (0.003066–0.003306) |
| `args-observed.js` | 1.04961 (1.03896–1.19029) | 0.736148 (0.73173–0.747149) | 0.0030405 (0.002996–0.003096) |
| `property-stable.js` | 0.150642 (0.149835–0.151518) | 0.0708595 (0.070274–0.142842) | 0.071571 (0.06868–0.072007) |
| `property-mutated.js` | 0.149985 (0.149144–0.153893) | 0.072813 (0.072397–0.073009) | 0.0622535 (0.061667–0.06338) |
| `metadata-ordinary.js` | 0.197961 (0.196253–0.199288) | 0.08143 (0.081295–0.082477) | 0.0122035 (0.012123–0.012263) |
| `metadata-function.js` | 0.193849 (0.191154–0.196223) | 0.080984 (0.079742–0.082496) | 0.203922 (0.202715–0.20544) |
| `metadata-intrinsic.js` | 0.195567 (0.193748–0.206071) | 0.082642 (0.082426–0.162679) | 0.22587 (0.218616–0.245426) |
| `typeof-dispatch.js` | 0.931439 (0.92668–0.954012) | 0.370939 (0.363239–0.387375) | 0.013806 (0.013665–0.025579) |
| `singleton-dispatch.js` | 0.886544 (0.883688–0.896413) | 0.365474 (0.360455–0.381705) | 0.0106655 (0.006292–0.012584) |
| `strict-equality-numbers.js` | 0.0803275 (0.079501–0.156547) | 0.033208 (0.03163–0.034155) | 0.0124985 (0.011001–0.02103) |
| `strict-equality-mixed.js` | 0.088293 (0.087757–0.08958) | 0.0343355 (0.033854–0.034585) | 0.034721 (0.034134–0.035227) |
| `for-in-wide.js` | 0.643732 (0.636078–0.755054) | 0.562733 (0.5582–0.57922) | 0.038633 (0.03683–0.055555) |

All scores time the public retained call after preparation; each program keeps its own workload and exact-output checks. No workload ratio or blended score is reported. [Full identities and observations](situations-current.json).

### Native program image sizes

| Retained program | Fast native P · KiB | Speed native P · KiB | Portable Wasm P · KiB |
| --- | ---: | ---: | ---: |
| `frame-compact.js` | 2252.2 | 968.1 | 193.1 |
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

The current situation cohort has 504 process observations and 60,984 exact calls. The current RSC cohort has 32 processes and 640 exact Flight calls. The fixed-yield for-in width control remains historical on source `0ca455c47636` and its earlier archive.

Scoped verification (**2026-11-08**, source `0c2c10dac9c0`): 2,828 broad checks, 380 source guards, and 9 CLI smokes pass. Full CI, Test262, and private units were not rerun; existing format, Intl, and four AOT failures remain owned.

**Broad native scopes:** emission, structure_builtins, structure_language, structure_async, runtime_link, modules_realms, intl_temporal, structure, aot_builtins, aot_language, aot_async, aot_realm_modules, aot_gc_entries.

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
