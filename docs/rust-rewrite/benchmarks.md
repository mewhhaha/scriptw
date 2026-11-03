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
| Scriptw prior build | 23.802 | 26.590 |
| Scriptw current build | 19.898 | 21.012 |
| Node 24.12.0 / V8 | 0.487 | 0.392 |

![Matched retained RSC request timings](assets/rsc-current/runtime.svg)

Each engine uses eight fresh processes across two balanced blocks, with three
checked warmups and seven measured requests per mode. Scriptw's request clock
covers the retained call through full Flight consumption; Node uses the same
bundle and wrapper in one `vm.SourceTextModule`/Realm for sync then async. JSON
encoding and exact-output checks follow the clock. Current medians are **16.4%**
and **21.0%** below the prior build; both modes improve in both blocks. Process
medians are the comparison units, with no significance or engine-parity claim.
V8 remains about **41× / 54× faster** on this fixture. Scriptc's existing React
build failures remain unqualified; client React/DOM is a separate open scope.

Strict ordinary module functions now keep private, uncaptured variables and
simple parameters in Wasm locals even when the bundle uses arguments. Real
captures, synthetic bindings and arguments retain their storage and snapshots.
Proven reads of retained cells also avoid generic name resolution while keeping
TDZ and mutation behavior. Immutable program UTF-16 slices share physical data;
all 987 literal contents remain. This batch changes those mechanisms together,
so the matched cohort does not isolate each mechanism's contribution.

| Cold setup / raw size | Prior build | Current build |
| --- | ---: | ---: |
| JavaScript → Wasm | 3.737 s | 3.216 s |
| Native preparation | 12.779 s | 12.400 s |
| Module initialization | 2.635 ms | 2.620 ms |
| Program Wasm | 7,653,675 B | 6,372,954 B |
| Code section | 6,540,463 B | 5,830,771 B |
| Data section | 1,016,420 B | 490,228 B |
| Whole-child peak RSS | 538.5 MiB | 513.5 MiB |

The program shrinks **16.7%**, including a **10.9%** code reduction. R is byte
identical at 39,380,016 bytes. An excluded loading trace confirms R loads from
the bundled native image; P still compiles from Wasm before request timing.
RSS includes compilation and all process history; it does not measure live guest heap. The first rejected intermediate trial grew
code 4.7% despite a smaller total program; its separate diagnostic is retained
in the evidence. Large bodies and native compilation remain open costs.

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

The main cohort keeps Wasmtime 47 **Fast / OptLevel::None / SinglePass**, one
CPU, 16-MiB copying GC and the same stack/deadline settings. A separate
**one-process** Speed/Backtracking diagnostic reached 16.00 / 16.69 ms, with
18.45 s native preparation; it is excluded from the comparison and changes no
default. The ordinary CLI chooses its own automatic native policy: the nine
exact-output controls include cold RSC at about 17.8 s and React HTML SSR at
42.4 s. Those Script-mode controls do not establish an SSR performance gain.

Excluded request profiles still place about 8.5% of sampled user cycles in
string equality, 4.4–4.8% in environment resolution and 4.4–5.1% in property
search. React's `vn` body contributes 11.4–12.1%. These are instruction-location
samples, not wall-time or allocation percentages; zero-IP and unresolved
samples stay in the denominator. The next work is lookup representation and
remaining call/prologue machinery, guided by caller and allocation evidence.

Runtime ABI 47 supplies bounded UTF-8 codecs, byte streams with default readers,
events, abort signals and tasks for these fixtures. Fetch/Request/URL integration,
DOM nodes and full WHATWG conformance remain unsupported or unverified.

[Current report](rsc-current.json) · [Size attribution](rsc-wasm-size-current.json) ·
[Inert evidence archive](rsc-final-evidence-2026-11-03.tar.xz) ·
[Prior report](rsc-initial-20261103.json).

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
