# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-03** with pinned React 19.3.1 and one retained module per
process. Both sync and async Flight outputs match Node exactly.

| Engine | Sync · ms | Async · ms |
| --- | ---: | ---: |
| Scriptw | 19.898 | 21.012 |
| Node 24.12.0 / V8 | 0.487 | 0.392 |

![Matched retained RSC request timings](assets/rsc-current/runtime.svg)

Each engine uses eight fresh processes across two balanced blocks, with three
checked warmups and seven measured requests per mode. Scriptw's request clock
covers the retained call through full Flight consumption; Node uses the same
bundle and wrapper in one `vm.SourceTextModule`/Realm for sync then async. JSON
encoding and exact-output checks follow the clock. Values are process medians;
V8 is about **41× / 54× faster** on this fixture. Scriptc's React build failures
remain unqualified; client React/DOM is a separate open scope.

Current Scriptw cold setup is **3.216 s JavaScript → Wasm**, **12.400 s native
preparation**, and **2.620 ms module initialization**. Program Wasm is
**6,372,954 B** (5,830,771 B code; 490,228 B data); shared runtime Wasm is
**39,380,016 B**. Whole-child peak RSS is 513.5 MiB and includes compilation
and process history, not just the live guest heap. The size attribution and
frozen evidence archive retain byte-level details.

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

The cohort uses Wasmtime 47 **Fast / OptLevel::None / SinglePass**, one CPU,
16-MiB copying GC and fixed stack/deadline settings. The Wasm size attribution
and evidence archive include the matching frozen compiler and process receipts.

Excluded diagnostics use the same compiler and source, with 100 checked requests
after three warmups in each engine/mode process:

| Retired instructions per request · median | Scriptw | Node / V8 |
| --- | ---: | ---: |
| Sync | 158.67 million | 2.53 million |
| Async | 167.07 million | 2.64 million |

All counter windows ran without multiplexing. Scriptw has higher aggregate
instructions per cycle and fewer branch misses per branch; excessive instruction
volume is the main observed gap. These are execution-thread counters, excluding
background threads. Node's barrier edges are included; an empty-window calibration
measures 0.33 million instructions, without subtraction. Natural JIT/GC history
across 100 calls differs from the timing cohort; these counts establish no new
timing ratio.

The sync caller profile identifies repeated global-name resolution: **92.1%** of
StringEquality leaf sample weight comes directly from the environment resolver.
Most originates in 22 React `vn` sites reading `Map`, `Number`, typed arrays,
`Date`, `ReadableStream`, `Blob` and `FormData`. These unbound references start
at the current environment and scan irrelevant lexical binding tables. The next
compiler cut is a proof that selects the existing global-environment start while
preserving live global lexical bindings, mutable properties, getters and Realms.

Fast native code is also excessive: P contains 85.70 MB of machine code, and
`vn` reserves a 1,011,584-byte stack frame. One checked sync render executes
88,258 repeated stack-page writes in `vn`/`ft`; their probe loops account for
5.3–5.8% of retained sampled locations. Speed/Backtracking shrinks P to 29.19 MB
and those frames to 7,424/4,912 bytes, yet still executes 150–158 million
instructions per request. Both optimization and allocator change together;
policy alone leaves the large semantic-work gap. Samples are not wall-time or
allocation percentages. Total heap and arguments-object allocations remain
unmeasured.

[Diagnostic report](react-attribution-current.json) ·
[Diagnostic evidence](react-attribution-evidence-2026-11-03.tar.xz).

Runtime ABI 47 supplies bounded UTF-8 codecs, byte streams with default readers,
events, abort signals and tasks for these fixtures. Fetch/Request/URL integration,
DOM nodes and full WHATWG conformance remain unsupported or unverified.

[Current report and process receipts](rsc-current.json) ·
[Size attribution](rsc-wasm-size-current.json) ·
[Inert evidence archive](rsc-final-evidence-2026-11-03.tar.xz) ·
[Historical raw cohort](rsc-initial-20261103.json).

## Focused performance situations

Measured **2026-11-03**: 13 standalone Module programs, compiled Scriptw build
`083d9dd79f51` (base `ee46c7cb6` plus the archived benchmark-host patch),
Node **24.12.0 / V8 13.6.233.17-node.37**, Rust **1.97.1**.
Each program has its own score; these are separate from the RSC and fresh-entry
kernel cohorts. No aggregate score combines their arbitrary operation counts.

| Program · retained call | Scriptw Fast · ms | Scriptw Speed · ms | V8 · ms |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.012 | 0.010 | 0.001 |
| `global-shallow.js` | 3.769 | 3.469 | 0.973 |
| `global-deep8.js` | 5.277 | 4.800 | 0.990 |
| `global-bound-deep.js` | 0.781 | 0.278 | 0.006 |
| `global-getter-deep.js` | 6.932 | 6.239 | 0.966 |
| `frame-compact.js` | 31.885 | 23.402 | 0.062 |
| `frame-branches32.js` | 34.658 | 24.738 | 0.095 |
| `frame-branches128.js` | 44.998 | 27.850 | 0.140 |
| `frame-branches512.js` | 117.189 | 43.185 | 0.215 |
| `args-unused.js` | 2.350 | 1.663 | 0.088 |
| `args-observed.js` | 6.703 | 5.877 | 0.135 |
| `property-stable.js` | 4.736 | 3.843 | 0.075 |
| `property-mutated.js` | 4.645 | 3.749 | 0.066 |

Each value is the median of eight fresh-process medians across two balanced
blocks. Every process initializes one Module/Realm, then runs three checked
warmups and seven measured calls. Four inputs rotate identically in every actor;
every call checks an independently computed integer checksum. Bars show current
configurations only; whiskers span the eight process medians. Block medians and
process summaries remain in the [data](situations-current.json); every call is
retained in the evidence archive.

Scriptw uses Wasmtime 47: **Fast = None / SinglePass**, **Speed = Speed /
Backtracking**, generic target, 16-MiB copying GC, one CPU, 4-GiB cap and no swap.
JavaScript/Wasm/native compilation, module initialization and `prepare(input)`
precede the public `run()` clock. Every consumer has its compiler disabled.
Node uses the exact same source bytes in `vm.SourceTextModule`, with normal JIT
and natural GC; three warmups do not establish optimized steady state. Timings
include each runtime's public-call bridge and `print` conversion/capture; output
draining and validation follow timing. The empty control is shown without
subtraction. Excluded preparation can still influence timed GC, feedback and JIT
invalidation. Child peak RSS and preparation observations are retained separately.

### Global lookup

The four cases read six constructor values across 1,024 passes. Deep lookup
retains eight syntactically nested padding captures; the bound control captures
constructor references beside the worker. Fast takes **5.277 ms** for deep global
reads versus **0.781 ms** for bound references; Speed takes **4.800 / 0.278 ms**.
This gives the global-resolution hill a small reproducible program. The getter
control changes the returned constructor with the input and checks all getter
calls, preserving a live global observation.

![Global lookup situations](assets/situations-current/global-lookup.svg)

Shallow/deep also differ in padding reads and arithmetic; their difference is not
pure lexical-depth cost. Bound aliases may fold, and V8 may hoist global reads.
Operation counts describe logical source references, not guaranteed executed
lookups. The getter score includes getter calls and counter increments.

### Control-flow and call-site scaling

All four cases make **8,192** entries into a function with one, 32, 128 or 512
selected call sites, exercising every site and producing identical checksums.
Fast rises from **31.885 to 117.189 ms**; Speed from **23.402 to 43.185 ms**.

![Control-flow and call-site scaling](assets/situations-current/frame-size.svg)

![Native program image growth](assets/situations-current/frame-native-image.svg)

The 512-site native P image is **6.71 MiB Fast / 2.55 MiB Speed**; portable Wasm P
is **0.83 MiB**. Native serialized images include metadata and data, not just code.
This measures source/control-flow and call-site scaling: compact dispatch uses a
dynamic key and one polymorphic site; switch arms use constant keys and separate
sites. It does not isolate individual stack frame growth or stack-probe cost.
V8's 512-site block medians differ (**0.290 / 0.215 ms**); retain that variation
when evaluating future changes.

### Arguments observation

Both cases make 1,024 strict ordinary calls with captured state, reassign a formal
parameter and compute the same result. The observed case reads the original
`arguments[0]` and `arguments.length`. Fast takes **2.350 / 6.703 ms**; Speed
**1.663 / 5.877 ms**. The extra observation is timed work; the difference is not
an allocation count or an isolated arguments-object construction cost.

![Arguments observation situations](assets/situations-current/arguments-observation.svg)

### Property mutation history

Each case performs 8,192 variable-key reads over 64 records. Mutation setup adds
an accessor, deletes properties and restores the same values before timing.
Fast takes **4.736 / 4.645 ms** and Speed **3.843 / 3.749 ms**: this history adds
little cost here, while both cases remain much slower than V8. No compact or
dictionary representation is assumed, and equal string recipes do not establish
distinct string backing storage.

![Property mutation history situations](assets/situations-current/property-mutation-history.svg)

### Public-call control and reproduction

![Retained public-call control](assets/situations-current/control.svg)

[Generate/run selected programs](../../CONTRIBUTING.md#focused-performance-situations)
with `--case <file.js>`; choose native policy with `--policies fast` or `speed`.
Scriptc has no compatible retained-Module `prepare`/`run` consumer in this
workflow, so it has no substituted fresh-WASI-entry score. These programs expose
optimization targets; they do not estimate each mechanism's share of RSC time.

The release example's 16 tests, seven generator/V8 checks and 11 runner checks
passed; all 312 process results passed exact output checks. No product compiler
or runtime source changed. Full workspace CI and pinned Test262 were not rerun.
Changed Rust files pass formatting and conformance ownership passes. Workspace
formatting still fails on 13 unchanged files; 14 stale Intl identity recipes
remain [owned](current-inventory.md).
The aborted first harness attempt and its configuration fix remain in the inert
evidence, excluded from scored calls.

[Report, source/tool identities and process summaries](situations-current.json) ·
[Checksummed UTF-8 evidence](situations-evidence-2026-11-03.tar.xz).

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
