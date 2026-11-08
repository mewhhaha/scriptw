# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-08**, React **19.3.1**, source fingerprint `453a9074fcac`. The manifest-bound dirty source snapshot is based on Git `b8c7325438c4`; that base commit alone does not identify the measured implementation. Both Flight outputs match Node exactly. The 32-row catalog async form adds a resolved Promise and extra markup; neither request performs I/O.

| Current policy | Sync · ms (median; min–max) | Async · ms (median; min–max) |
| --- | ---: | ---: |
| Scriptw Fast | 4.554 (4.442–4.769) | 4.938 (4.846–5.271) |
| Scriptw Speed | 3.733 (3.630–3.796) | 4.067 (3.965–4.174) |
| Node 24 / V8 | 0.495 (0.477–0.716) | 0.398 (0.393–0.411) |

![Current RSC request timings](assets/rsc-current/runtime.svg)

The campaign uses 8 fresh processes per policy in 2 balanced blocks, with 3 warmups and 7 timed requests per mode. It records 800 exact-checked Flight calls. Each process retains one Module/Realm; setup precedes the request window, while JSON encoding and exact-output validation follow it. Rows show medians of process medians, with process-median bounds. The chart shows current Fast, Speed and V8. The two prior-source Fast/Speed controls remain separately identified in the raw report.

Fast and Speed are separately scored native compilation policies. Their source, executable, P, R and request inputs match; Cranelift optimization and register allocation differ together.

| Cold phase · median | Fast | Speed |
| --- | ---: | ---: |
| Front end and lowering · s | 2.682 | 2.687 |
| Wasm emission · s | 0.421 | 0.422 |
| Native preparation · s | 5.214 | 7.162 |
| Source to Wasm · s | 3.103 | 3.110 |
| Realm and instantiation · ms | 0.572 | 0.596 |
| Module initialization · ms | 2.079 | 1.717 |

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

The emitted program P is 4,169,697 bytes (SHA-256 `fef58c9a24bc24e438153a9d0936071e6a6947c86e09ad10dd1db6cc56aeba3f`). Packaged runtime R is 38,769,947 bytes (SHA-256 `4f8d4a6182181d74b527394cd06015a7c723b81b0dabe95964628922516b6f64`). These are artifact sizes, not allocation or resident-memory measurements. Exact executable and producer identities are in [the current report](rsc-current.json).

### Current native-policy diagnostic

This excluded 2×2 experiment uses six processes per mode, seven timed requests and three warmups per process. It varies program P compilation; all rows load the same precompiled runtime R. The product default is unchanged. These request timings are diagnostic; the scored Fast/Speed rows above remain the 40-process cohort.

| Mode | Cranelift opt | Register allocation | Sync · ms | Async · ms | Native preparation · s |
| --- | --- | --- | ---: | ---: | ---: |
| fast | None | SinglePass | 4.468 | 4.855 | 5.093 |
| fast-backtracking | None | Backtracking | 3.794 | 4.150 | 7.225 |
| speed | Speed | Backtracking | 3.636 | 3.961 | 7.070 |
| speed-single-pass | Speed | SinglePass | 4.202 | 4.554 | 5.643 |

#### Captured native stack frames

| Mode | `js::vn#f133` frame / probes | `js::ft#f123` frame / probes |
| --- | ---: | ---: |
| fast | 213,856 B / 53 page probes | 85,664 B / 21 page probes |
| fast-backtracking | 8,736 B / 2 unrolled probe stores | 2,736 B / 0 unrolled probe stores |
| speed | 6,656 B / 1 unrolled probe store | 2,480 B / 0 unrolled probe stores |
| speed-single-pass | 171,712 B / 42 looped probes | 68,976 B / 17 looped probes |

Perf-map JIT extents below are separate from native stack-frame sizes:

| Mode | `vn` perf-map extent · B | `ft` perf-map extent · B |
| --- | ---: | ---: |
| fast | 1,398,560 | 521,904 |
| fast-backtracking | 761,904 | 291,664 |
| speed | 531,472 | 195,184 |
| speed-single-pass | 1,044,736 | 388,384 |

With optimization fixed, Backtracking reduces request time about 15% and adds about 2.1 seconds of native preparation. The pinned regalloc2 0.15.2 SinglePass allocator assigns permanent homes to spilled SSA values and block parameters; freeing a stack allocation does not recycle its slot. The 223 declared Wasm locals in `vn` therefore do not bound its native frame. Safe spill-home reuse is an unmeasured follow-up; the experiment also changes spill/reload code and does not isolate page-probe time.

The historical cache census below is pinned to source `0c2c10dac9c0`; these entry/fallback counts are not current-source measurements and do not count property-cache hits.

| Historical 0c2c mode | Cached read entries | Generic `object_read` fallback | Fallback share |
| --- | ---: | ---: | ---: |
| sync | 2,548 | 1,275 | 50.0% |
| async | 2,698 | 1,362 | 50.5% |

The baseline runtime Wasm R is byte-identical to current R, but this 0c2c census is not a current cache-hit/miss count. Its third direct self-call site is preserved as unmeasured; the recorded self-call sites have no prototype-versus-proxy attribution.

The earlier [PMU and caller profiles](react-attribution-current.json) retain source `0c2c10dac9c0` and their [own evidence archive](scriptw-performance-predicate-transport-evidence-2026-11-08.tar.xz); their time shares have not been remeasured on this source.

[Architecture measurements and raw-evidence identities](react-architecture-current.json).

## Focused performance situations

Measured **2026-11-08** on source `453a9074fcac`. All 21 programs use two balanced blocks and eight fresh processes per actor. Each program compares Fast, Speed and V8.

| Program · retained call | Fast · ms (median; min–max) | Speed · ms (median; min–max) | V8 · ms (median; min–max) |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.005135 (0.005049–0.00519) | 0.0050795 (0.004969–0.00511) | 0.000351 (0.000281–0.000361) |
| `global-shallow.js` | 2.15892 (2.15098–2.18126) | 1.75727 (1.74812–2.04566) | 0.970546 (0.940678–1.03143) |
| `global-deep8.js` | 2.74284 (2.72885–2.92174) | 1.94218 (1.9237–2.01476) | 1.00055 (0.970535–1.0487) |
| `global-bound-deep.js` | 0.650219 (0.64179–0.950012) | 0.209287 (0.208817–0.275623) | 0.004238 (0.002334–0.004268) |
| `global-getter-deep.js` | 2.93196 (2.89787–3.02188) | 2.14701 (2.12884–2.22942) | 0.958326 (0.945037–0.984638) |
| `frame-compact.js` | 1.91435 (1.89905–1.98996) | 0.946457 (0.942151–0.987764) | 0.057239 (0.057008–0.07401) |
| `frame-branches32.js` | 7.87876 (7.78232–8.0048) | 5.78901 (5.77248–5.84046) | 0.0819705 (0.078679–0.087987) |
| `frame-branches128.js` | 8.15025 (8.06496–8.39231) | 5.72321 (5.70344–5.82869) | 0.112624 (0.111522–0.114667) |
| `frame-branches512.js` | 10.3622 (9.78221–11.2168) | 5.94032 (5.87943–5.98716) | 0.180257 (0.171055–0.184701) |
| `args-unused.js` | 0.0998545 (0.099239–0.107864) | 0.0552095 (0.055064–0.056267) | 0.003116 (0.003066–0.003416) |
| `args-observed.js` | 1.04514 (1.03264–1.11379) | 0.737263 (0.729382–0.768438) | 0.003051 (0.003005–0.047811) |
| `property-stable.js` | 0.141032 (0.140727–0.141939) | 0.0677185 (0.067518–0.06837) | 0.0711295 (0.06827–0.071556) |
| `property-mutated.js` | 0.140146 (0.139124–0.140868) | 0.0699925 (0.069662–0.070403) | 0.062052 (0.061667–0.062749) |
| `metadata-ordinary.js` | 0.19525 (0.19484–0.197044) | 0.080022 (0.079551–0.080523) | 0.012123 (0.012062–0.014177) |
| `metadata-function.js` | 0.193001 (0.191653–0.196132) | 0.0802325 (0.079501–0.081545) | 0.204027 (0.202523–0.213895) |
| `metadata-intrinsic.js` | 0.19756 (0.192776–0.206421) | 0.0809185 (0.080433–0.082066) | 0.228404 (0.223133–0.288137) |
| `typeof-dispatch.js` | 0.883671 (0.879608–0.893415) | 0.371901 (0.364902–0.38966) | 0.0137865 (0.013606–0.024857) |
| `singleton-dispatch.js` | 0.851495 (0.850242–0.865823) | 0.356111 (0.352128–0.376815) | 0.010465 (0.006302–0.01615) |
| `strict-equality-numbers.js` | 0.082521 (0.082096–0.082587) | 0.030197 (0.030047–0.030368) | 0.014838 (0.011091–0.014919) |
| `strict-equality-mixed.js` | 0.087501 (0.087316–0.087727) | 0.0334085 (0.033253–0.033624) | 0.033624 (0.033243–0.034986) |
| `for-in-wide.js` | 0.635239 (0.631468–0.754462) | 0.54836 (0.545264–0.626258) | 0.037556 (0.03709–0.039946) |

All scores time the public retained call after preparation; each program keeps its own workload and exact-output checks. No workload ratio or blended score is reported. [Full identities and observations](situations-current.json).

![Current focused situation timings](assets/situations-current/global-lookup.svg)

### Native program image sizes

| Retained program | Fast native P · KiB | Speed native P · KiB | Portable Wasm P · KiB |
| --- | ---: | ---: | ---: |
| `frame-compact.js` | 2240.2 | 964.1 | 192.3 |
| `frame-branches32.js` | 2167.2 | 963.2 | 207.3 |
| `frame-branches128.js` | 2758.1 | 1221.4 | 302.6 |
| `frame-branches512.js` | 5130.8 | 2278.2 | 689.8 |

Native serialized sizes include code metadata and data; they do not measure peak RSS or runtime heap.


### Global lookup

Six constructor values are read over 1,024 passes. Deep lookup retains eight
padding captures; bound aliases sit beside the worker. Shallow/deep also differ
in padding arithmetic, and V8 may hoist aliases or global reads. Getter controls
include calls and counter increments. These differences are not isolated lookup costs.

![Global lookup](assets/situations-current/global-lookup.svg)

### Control-flow and call-site scaling

These programs execute 8,192 logical source applications across selected sites with matching checksums. Fast and Speed may fuse admitted calls into the loop body, so logical applications are not a direct-call count. Compact dispatch uses one polymorphic site and a dynamic key; switch arms use constant keys and separate sites. This does not isolate stack-probe cost.
The current 512-site native P image is **5.01 MiB Fast / 2.22 MiB Speed**; portable P is **0.67 MiB**. Native serialized sizes include metadata and data.

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

The full 21-situation cohort has 504 process observations and 60,984 exact calls. The RSC cohort has 40 processes and 800 exact Flight calls.

Scoped verification (**2026-11-08**, source `453a9074fcac`): 2,832 broad checks, 380 source guards, and 9 CLI smokes pass. Full CI, Test262, and private units were not rerun. Existing formatting and Intl issues remain owned; four AOT unit failures remain a historical owned gate.

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
