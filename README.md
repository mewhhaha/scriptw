# scriptw

Scriptw is an experimental JavaScript-to-WebAssembly compiler written in Rust.
Compile JavaScript and its builtins to Wasm, run it through Wasmtime, or embed
it in a Rust application. WebAssembly is the sole output target.

Rust embedders can declare numeric, lossless UTF-16 string and byte foreign
functions, retain and call JavaScript exports, and settle async host operations
through their own Wasmtime `Store` and `Linker`. ABI requirements persist in
source-free Portable bundles. See the [embedding contract and runnable examples](docs/rust-rewrite/contracts/foreign-modules.md).
`initialize_profiled` and `evaluate_profiled` report initialization-worker CPU
separately from caller-side work.

Trusted Wasmtime native program images support fresh-process execution with
the native compiler disabled. Their format is host-specific and requires
explicit admission of native code. Process RSS includes the host, runtime,
code and execution history; it is not a measurement of the live JavaScript heap.
See [runtime memory ownership](docs/rust-rewrite/contracts/runtime-byte-ownership.md)
and [compact object storage](docs/rust-rewrite/contracts/compact-ordinary-data-storage.md).

An experimental [precompiled consumer](docs/rust-rewrite/contracts/precompiled-comparison.md#consumer-capability-experiment)
can exclude dynamic-source compilation callbacks from its host. The existing
Portable embedding API remains available; static compiler dependencies remain.

Math, Reflect, JSON, Atomics and eager intrinsic constructors and prototypes use
complete immutable property layouts with per-Realm value identity. Constructor
name, length and static properties share the layout from allocation. Reads
materialize individual values; mutation converts the object to ordinary storage.
Data-property reads retain the resolved value after first observation.
Runtime initialization uses compact intrinsic-installation tables and defers
the Error, Date, RegExp, Temporal, Intl and binary-data intrinsic graphs until
their first observable or internal use.
Each Realm retains its own canonical constructors, prototypes and descriptors.
Immutable runtime String descriptors carry exact UTF-16 key hashes; separately
created equal Strings retain the ordinary lazy hash path. Linked-module caching
keeps an admitted runtime resident when its program cannot share the configured
entry or image-byte budget. Programs still execute in fresh Stores and Realms.
Runtime admission avoids re-serializing validated receipts. Embedded calls reuse
host execution stacks; callers own Store and Realm lifetimes. Runtime ABI 46
requires rebuilding older Portable bundles and native images. Native images use
binary execution facts (`SCRWEP03`).
Abstract equality, BigInt equality and for-in advancement share compiled Wasm
kernels. Missing identifier errors retain the exact name, such as
`TextEncoder is not defined`.
Actual Number tags admit direct Wasm addition, comparisons and numeric updates;
other values retain the full coercion path. Closed scalar loops retain proven
32-bit integers in integer locals, with bounded unrolling and exact integer
recurrence reduction while preserving Number values at their exits. Fixed positive
literal trip counts can use one guarded affine transform after the existing
Number and native-method checks; dirty bindings and the last body completion
share the ordinary loop exit.
Ordinary effectful loops also retain body-only unsigned counter proofs for
remainder by positive integer literals. Private bindings, constant bounds and
unchanged counters admit masks or unsigned remainder; calls and loop control
still execute normally. Parameters, captures and dynamic environments decline.
Read-only loops over dense Number Arrays can borrow one guarded backing and
keep their counter and arithmetic in primitive locals. Closed row reads resolve
key slots on the first actual row, then validate each row's shape and Number leaves
before using direct slots.
Other layouts, effects and index forms retain ordinary property access and coercion.
Bounded integer sums can reduce a complete trip when every sequential Number
operation stays exact. Pure loops that observe only the length of a native
Array join on Strings can compute it once behind live method and layout guards.
Strict ordinary module functions keep proven uncaptured `var` bindings and simple
parameters in local storage; exports, captures and Realm-relative names retain
their live environment lookup. Actual eval/arguments use keeps the conservative
environment plan.
Guarded Number loops and packed Array
push/join kernels avoid repeated conversion, dispatch and allocation when their
semantic checks pass. Guarded fixed-arity push calls avoid temporary argument
lists; own-data reads retain immutable lookup metadata within an invocation and
check live storage and shape before loading a slot. Static named Array reads
check the live receiver and immediate prototype before loading an already-resolved
intrinsic DATA value; cold caches, shadows, mutations and proxies use ordinary Get.
Validated reads of immutable globals `undefined`, `NaN` and `Infinity` emit
constants. Local, `with` and eval bindings retain their own resolution and TDZ checks.
Property reads reject impossible length keys before
resolving the canonical name; literal String keys reuse their evaluated value after
receiver boxing. Guarded join calls reuse explicit String separators.
Exact static `.length` reads use actual String or Array storage; computed keys
and other receivers retain ordinary lookup. Guarded constant-return
calls eliminate temporary objects and call setup. Pure numeric sort callbacks
have typed entries; packed Number sort snapshots stay unboxed, admit the callback
once and recognize ordered runs. Writeback retains ordinary mutation checks. The benchmark sections below
identify each measurement's source and configuration.
Linux profiling records execution-worker CPU alongside wall time, including
synchronous host work. Unsupported clocks report an explicit reason.
Wasm emission requests the executable's cache identity only when it needs a
persistent entry. Embedded runtime admission and compilation without a cache
avoid that work; persisted entries remain bound to the actual compiler executable.
Persistent cache identity defaults to serial BLAKE3 over the opened executable;
`compiler-identity` and execution evidence retain full SHA-256. Explicit
`SCRIPTW_COMPILER_CACHE_IMAGE_HASH=sha256` uses its separate cache domain.
Unicode property catalog construction reuses pinned ICU Script ranges across
aliases; each exact spelling still receives its own Unicode 17 delta.
Runtime-free primitive programs omit compiler-owned pools and linear memory.
String literals retain the same WasmGC String and UTF-16 array representation.
Their UTF-16 bytes use passive program data and copy into fresh GC arrays when
evaluated, without linear memory or compiler-owned tables.

The [2026-11-01 local scalar-region comparison](docs/rust-rewrite/scalar-regions-20261101.json)
records **4–14% lower sorting medians**, **10–35% lower numeric-loop medians**
and **63–71% lower small-join medians**, in both balanced orders.
All eight control programs remain byte-identical; slower control means remain in
the record. This and the
[earlier checkpoint](docs/rust-rewrite/typed-regions-20261030.json) retain their
own sources and settings; neither refreshes the published CI cohorts below.

The [entry checkpoint](docs/rust-rewrite/entry-bootstrap-20261101.json) adds
**5–16% lower sorting medians**, **15–16% lower numeric-loop medians** and
**15–16% lower small-join medians** in both balanced orders. All 17 program
Wasm files remain byte-identical; R caches exact String hashes and stays resident
within the existing module budget. Its separate hot-R control passes. The direct
reference has median and mean wins on **6/17 Scriptc** and **2/17 V8** cases;
overall parity remains open. Slower controls and unequal setup boundaries remain
explicit. These local results do not refresh the published CI cohorts.

The [compact-row checkpoint](docs/rust-rewrite/compact-rows-20261101.json) records
about 21% lower object medians and 28% lower retained-object medians. Its
[first-row setup follow-up](docs/rust-rewrite/compact-row-entry-20261101.json)
reduces those medians another **2.1–2.8%** and **11.1–11.4%**, respectively,
against that checkpoint in both blocks. The separate precompiled control records
0.8–1.0% and 4.35–4.54% lower medians, without a repeated veto. Both R/native
profiles and the other 15 original programs stay byte-identical. That checkpoint
retains its slower sort, join and retained-object reference processes.
The [local gap diagnosis](docs/rust-rewrite/performance-gaps-20261101.json) records
median and mean wins on 2/17 V8 and 5/17 Scriptc cases in ordinary execution;
a separate precompiled comparison records 4/17 Scriptc wins. That cohort's
480-process identical-binary control failed its worker-CPU gate, so the
primitive-append prototype remains unbuilt. Precompiled profiling measures
initialization-worker CPU separately from caller CPU. Failed controls and all
observations remain retained; overall parity remains open. These local cohorts
do not refresh published CI results. The five-case LLVM/final-Wasm inspection
finds guarded whole-loop shortcuts where Scriptc still executes the loops.
It motivates work on fresh intrinsic dictionaries and property lookup. Literal-key
cleanup reduces worker instruction counts about 0.9% on two join cases; timing
remains unqualified, including slower large-join means.

The [Array/global checkpoint](docs/rust-rewrite/performance-gaps-20261101.json)
reuses observed intrinsic DATA values after live guards and folds validated
immutable global reads. The three remaining join cases take **35–43%** of their
previous execution time in the 30-case precompiled screen. The 480-process
identical-binary calibration passes; the screen has no repeated wall/worker-CPU
regression flags. **Peak RSS rises 4.25–5.28 MiB across all 30 cases**, despite
byte-identical native R images. A zero-trip mapping probe locates about 4.4 MiB
of added clean host-executable residency; anonymous memory differs by 2 KiB.
The kernel mechanism and memory parity remain open.

**1,844 selected native checks** and **128 affected pinned Test262 variants**
pass. These selected scopes do not refresh full Test262 status. The earlier
remainder subset's two native-frame crashes remain
[owned](docs/rust-rewrite/current-inventory.md). Historical failed controls and
source-bound measurements remain in the evidence.

Removing an unnecessary durability flush from unnamed native backing lowers
precompiled native-load medians **8.4–10.5%** across 16 fresh processes in two
balanced blocks. Supervised whole-process wall medians fall **3.1–4.5%**.
All 30 program bodies and both raw/native runtime profiles remain identical;
live snapshots show transient deferred writeback.

[Cold compilation](docs/rust-rewrite/cold-compiler-20261101.json) records
**42.7–42.9% lower wall** and **43.1–43.5% lower CPU** geometric means of
process medians across 19 sources in two balanced blocks. The boundary includes
startup, requested P/R file writes and teardown. All program Wasm, both runtime
Wasm profiles and complete native runtime images remain byte-identical.
Persisted-program execution controls retain executable identity and pass the
predeclared regression gates; slower retained-object and join observations remain
in the record.

[RegExp catalog construction](docs/rust-rewrite/regexp-catalog-20261101.json)
lowers cold-build median geomeans another **34.5–35.9% wall** and
**35.6–35.8% CPU**. Three cold-execution controls improve **9.4–10.1% wall**
and **11.2–11.6% CPU** in aggregate. Program Wasm and both raw/native runtime
profiles remain byte-identical. Hello's peak RSS rises about 3.4 MiB in one
block; all observations and the passing repeated-regression gates are retained.

The [runtime-free primitive checkpoint](docs/rust-rewrite/standalone-pool-20261101.json)
reduces cold-build median geomeans **88.3% wall** and **88.7–88.8% CPU** across
19 admitted expression programs. Cold `run` wall time falls **28.7–28.9%**;
median P size drops from **599,381 to 7,592 bytes**. These programs keep the
existing WasmGC representation and omit linear memory/data. All 18 heap-backed
control P/R outputs and both native runtime profiles stay byte-identical.
Slower control observations and a one-block 3.9-MiB RSS rise remain recorded.

[Executable cache identity](docs/rust-rewrite/cache-image-20261101.json) lowers
cold-process median geomeans **17–19% wall** and **19% CPU**, and warm-process
geomeans **30–31% wall** and **32–33% CPU**, across six sources in both blocks.
All 37 P/R outputs and both native runtime profiles stay byte-identical. A mixed
CLI/example build failed the RSS gate and remains rejected; the separately built
ordinary CLI passes. Its retained-object RSS rises and slower observations remain
recorded. These startup results do not refresh V8/Scriptc execution comparisons.

[Standalone literal emission](docs/rust-rewrite/literal-admission-20261101.json)
lowers cold-run wall and CPU median geomeans **about 68%** across seven literal
programs in both balanced blocks. Passive UTF-16 data replaces per-unit Wasm
stores; admission collects the exact literal image once. All 22 scalar/heap P/R
controls and both raw/native R profiles stay byte-identical. Two rejected
experiments, slower controls and the unchanged gates remain recorded. This
compilation result does not refresh V8/Scriptc execution comparisons.

[Current work](docs/rust-rewrite/current-inventory.md) tracks unresolved goals.
[CONTRIBUTING.md](CONTRIBUTING.md) covers the
build, debug and verification loop.

<!-- application-benchmarks:start -->
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
<td><img src="docs/rust-rewrite/assets/applications-current/edge-json-api.svg" width="440" alt="edge-json-api.js; Scriptw: 3.999 ms, 26.4 MiB RSS; Scriptc: 0.793 ms, 17.7 MiB RSS; V8: 0.340 ms, 55.3 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/applications-current/edge-router.svg" width="440" alt="edge-router.js; Scriptw: 2.637 ms, 25.2 MiB RSS; Scriptc: 0.694 ms, 17.5 MiB RSS; V8: 0.204 ms, 54.6 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/applications-current/edge-query.svg" width="440" alt="edge-query.js; Scriptw: 6.298 ms, 27.5 MiB RSS; Scriptc: 1.268 ms, 17.6 MiB RSS; V8: 0.653 ms, 55.0 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/applications-current/edge-middleware.svg" width="440" alt="edge-middleware.js; Scriptw: 2.426 ms, 25.6 MiB RSS; Scriptc: 0.622 ms, 17.8 MiB RSS; V8: 0.170 ms, 54.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/applications-current/edge-html.svg" width="440" alt="edge-html.js; Scriptw: 10.564 ms, 30.8 MiB RSS; Scriptc: 1.142 ms, 17.8 MiB RSS; V8: 1.004 ms, 56.6 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/applications-current/edge-nested-result.svg" width="440" alt="edge-nested-result.js; Scriptw: 7.307 ms, 29.7 MiB RSS; Scriptc: failed (execution); V8: 0.390 ms, 56.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/applications-current/react-ssr.svg" width="440" alt="react-ssr.js; Scriptw: failed (timed out); Scriptc: failed (build); V8: 0.762 ms, 56.7 MiB RSS"></td>
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

![Node Flight and Web API references](docs/rust-rewrite/assets/applications-current/react-server.svg)
![Chromium React DOM references](docs/rust-rewrite/assets/applications-current/react-browser.svg)

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
The current Scriptw host also lacks DOM, encoding, Web Streams,
Fetch and URL APIs, as recorded by the separate capability probe.

The [failure diagnosis](docs/rust-rewrite/applications-current.json) identifies
missing `TextEncoder` during RSC initialization. Corrected Scriptc Web probes
reach `SC3002` (WASI API admission), and its nested-result case reports `SC2020`
(dynamic `Array.slice` end unsupported). React bundles retain type-check errors.
Scriptw's SSR output has a 2-MB function, triggering size-optimized native
compilation; a debugger sample finds Cranelift optimization active at 20 seconds.

[Data, identities and diagnostics](docs/rust-rewrite/applications-current.json) ·
[Inert raw evidence](docs/rust-rewrite/application-benchmarks-20261102-inert.tar.xz) ·
[Workflow](CONTRIBUTING.md#react-and-edge-benchmarks).
<!-- application-benchmarks:end -->

The [shared-kernel replay](docs/rust-rewrite/react-codegen-20261102.json) reduces
SSR's largest function from 2,014,440 to 1,110,482 Wasm bytes. All three ordinary
runs render correctly in 41.2–41.9 seconds; benchmark preparation finishes in
56.6 seconds under its original 60-second limit, followed by ten checked renders.
All 79 pinned modulus variants pass, including the previously crashing BigInt case.
RSC still needs `TextEncoder` and related Web APIs; Request-based handlers need
`Request`. The client case still times out. These replays do not replace the
dated cross-engine benchmark results above.
Two [native compiler experiments](docs/rust-rewrite/native-compiler-probes-20261103.json)
left compilation near 50 seconds with identical machine code; both were reverted.

## Compilation time

**JavaScript → portable Wasm**, measured 2026-11-02 with the same compiler
fingerprint as the runtime charts. Scriptw is faster in **17/17 workloads**,
taking **0.524x Scriptc's time** by geometric mean of workload medians.

![JavaScript to portable Wasm compilation time](docs/rust-rewrite/assets/compilation-current.svg)

Four fresh CLI builds per compiler/workload across two balanced blocks
(**136 processes**). Includes startup and output I/O: Scriptw P plus its shared R
sidecar; Scriptc standalone WASI. Application caches are fresh; OS and Zig toolchain
caches remain. Runtime production, Wasm-to-native compilation and execution are
excluded. [Data and tool identities](docs/rust-rewrite/compilation-current.json) ·
[Reproduce](CONTRIBUTING.md#benchmark-charts).

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
<td><img src="docs/rust-rewrite/assets/benchmarks-current/sort-reverse-128.svg" width="440" alt="sort-reverse-128.js; Scriptw: 0.091 ms, 21.6 MiB RSS; Scriptc: 0.133 ms, 17.2 MiB RSS; V8: 0.023 ms, 59.3 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/sort-reverse-512.svg" width="440" alt="sort-reverse-512.js; Scriptw: 0.133 ms, 21.7 MiB RSS; Scriptc: 0.340 ms, 17.5 MiB RSS; V8: 0.069 ms, 59.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/sort-sorted-512.svg" width="440" alt="sort-sorted-512.js; Scriptw: 0.133 ms, 21.7 MiB RSS; Scriptc: 0.338 ms, 17.2 MiB RSS; V8: 0.070 ms, 59.8 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/to-sorted-512.svg" width="440" alt="to-sorted-512.js; Scriptw: 0.143 ms, 21.9 MiB RSS; Scriptc: 0.375 ms, 17.4 MiB RSS; V8: 0.069 ms, 59.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/join-1024.svg" width="440" alt="join-1024.js; Scriptw: 0.235 ms, 22.2 MiB RSS; Scriptc: 0.410 ms, 17.5 MiB RSS; V8: 0.055 ms, 59.5 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/join-4096.svg" width="440" alt="join-4096.js; Scriptw: 0.699 ms, 22.1 MiB RSS; Scriptc: 1.547 ms, 17.8 MiB RSS; V8: 0.233 ms, 67.7 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/join-mixed-1024.svg" width="440" alt="join-mixed-1024.js; Scriptw: 0.269 ms, 21.4 MiB RSS; Scriptc: 0.271 ms, 17.2 MiB RSS; V8: 0.096 ms, 60.5 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/numeric-loop-10000.svg" width="440" alt="numeric-loop-10000.js; Scriptw: 0.068 ms, 20.9 MiB RSS; Scriptc: 0.095 ms, 16.6 MiB RSS; V8: 0.142 ms, 64.8 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/numeric-loop-50000.svg" width="440" alt="numeric-loop-50000.js; Scriptw: 0.069 ms, 20.7 MiB RSS; Scriptc: 0.184 ms, 16.9 MiB RSS; V8: 0.698 ms, 66.7 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/folded-string-4096.svg" width="440" alt="folded-string-4096.js; Scriptw: 0.170 ms, 21.1 MiB RSS; Scriptc: 0.808 ms, 16.4 MiB RSS; V8: 0.126 ms, 60.4 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/dynamic-string-4096.svg" width="440" alt="dynamic-string-4096.js; Scriptw: 0.189 ms, 20.6 MiB RSS; Scriptc: 0.285 ms, 16.7 MiB RSS; V8: 0.193 ms, 65.7 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/integer.svg" width="440" alt="integer.js; Scriptw: 0.070 ms, 21.1 MiB RSS; Scriptc: 0.074 ms, 16.6 MiB RSS; V8: 2.546 ms, 66.0 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/objects.svg" width="440" alt="objects.js; Scriptw: 0.242 ms, 21.6 MiB RSS; Scriptc: 0.679 ms, 17.5 MiB RSS; V8: 0.067 ms, 59.6 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/retained-objects.svg" width="440" alt="retained-objects.js; Scriptw: 6.235 ms, 25.8 MiB RSS; Scriptc: 34.484 ms, 23.4 MiB RSS; V8: 3.584 ms, 84.6 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/join-empty-1024.svg" width="440" alt="join-empty-1024.js; Scriptw: 0.077 ms, 21.4 MiB RSS; Scriptc: 0.283 ms, 17.5 MiB RSS; V8: 0.062 ms, 59.4 MiB RSS"></td>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/join-singleton-1024.svg" width="440" alt="join-singleton-1024.js; Scriptw: 0.078 ms, 21.3 MiB RSS; Scriptc: 0.130 ms, 16.6 MiB RSS; V8: 0.098 ms, 60.1 MiB RSS"></td>
</tr>
<tr>
<td><img src="docs/rust-rewrite/assets/benchmarks-current/join-small-1024.svg" width="440" alt="join-small-1024.js; Scriptw: 0.077 ms, 21.4 MiB RSS; Scriptc: 0.178 ms, 16.8 MiB RSS; V8: 0.168 ms, 60.1 MiB RSS"></td>
<td></td>
</tr>
</table>

[Chart data and runtime identities](docs/rust-rewrite/execution-current.json) ·
[Complete evidence](docs/rust-rewrite/performance-gaps-20261101.json) ·
[Workflow](docs/rust-rewrite/contracts/precompiled-comparison.md).

[Regenerate or verify the charts](CONTRIBUTING.md#benchmark-charts).

## Benchmarks

The historical three-runtime CI comparison was measured **2026-10-30**,
compiler `aedaa6e4e`.
It uses the ordinary Fast configuration and post-execution RSS endpoints.
[Dated CI charts and evidence](docs/rust-rewrite/compiler-comparison.md) ·
[Historical precompiled CI comparison](docs/rust-rewrite/precompiled-comparison.md).

The [module-local diagnostic](docs/rust-rewrite/module-local-performance-20261030.json)
records 24–99.6% lower retained-call medians for derived strict module workloads.
A later [packed-join experiment](docs/rust-rewrite/join-dense-values-20261030.json)
failed its repeatability gate and was reverted: ordinary small joins slowed
7–9% in both balanced orders. These local diagnostics retain their own compiler
identities and measurement boundaries.

The [array-storage experiments](docs/rust-rewrite/array-storage-experiments-20261030.json)
also retain two rejected candidates: deferred numeric bitmaps and direct
general-value publication. Repeated retained-call controls slowed, so both
production changes were reverted. Their mutation and snapshot regressions remain.

Arrays now keep indexed-storage metadata in their Array record, removing one
allocation and 48 bytes per Array under the pinned copying collector. In two
[local balanced cohorts](docs/rust-rewrite/array-owner-fusion-20261031.json),
retained objects ran 10–17% faster across ordinary/precompiled execution and used
0.58–0.77 MiB less post-execution RSS. All 2,648 selected native checks and 1,326
original Test262 executions passed across the recorded continuation. Shorter
controls and separate retained-call results remain in the evidence. The published
comparisons above retain their original sources; every-case parity remains open.

Moving the existing numeric Array probe before `ToObject` did not demonstrate a
repeatable target benefit. The [two balanced cohorts](docs/rust-rewrite/numeric-getv-probe-20261031.json)
retain all 17 workloads: ordinary retained objects slowed 8.3% and 2.8%, while
compiler-free results changed direction. The production change was reverted;
the branch-aware read regression and exact experimental products remain.

The [combined runtime ownership experiment](docs/rust-rewrite/runtime-ownership-20261031.json),
[full construction fold](docs/rust-rewrite/runtime-fold-20261031.json) and
[string-only fold](docs/rust-rewrite/runtime-strings-20261031.json) passed their
selected correctness checks but failed performance controls and were reverted.
The string-only fold saved 96,040 native bytes, yet pooled eight-round retained
mixed/large joins slowed by 12.18%/7.73%. Exact products and slower observations
remain in their separate source-bound records.

[Guarded whole-trip affine composition](docs/rust-rewrite/whole-trip-affine-20261031.json)
improves the unchanged integer workload by 2.4–6.8% in compiler-free execution
and 6.1–7.0% in ordinary invocation across two balanced groups. Million-step
signed/unsigned controls improve about 70%/84% at those separate boundaries.
The runtime and the other 16 original programs retain identical Wasm. Validation
covers 35 selected native checks through one documented fixture correction and
78 original Test262 executions. Singleton joins and derived retained integer
calls still vary; small-join ordinary results include a 2.95% median and 38.07%
mean slowdown in the final control. Every observation is retained; this is a
scoped CPU improvement, with no RSS or all-workload parity claim.

[Date graph deferral](docs/rust-rewrite/runtime-date-demand-20261031.json)
improves normal-cache startup by 7.3% in compiler-free execution and 7.7% in
ordinary invocation across two balanced groups. Validation covers 1,312 selected
native checks and 106 variants from 53 original Test262 files. The normal-cache
follow-up preserves eight sources at all three boundaries; the full 21-source
cohort used the separate anonymous native-backing fallback. First-epoch original
execution slows 3.46% median / 4.91% mean, repeated Date retained calls slow
1.58% / 4.69%, and join-4096 original has a 3.94% median slowdown despite a lower
mean. Selected normal-cache RSS rises 0.15–0.46 MiB. This is a startup CPU
improvement; memory parity and those control costs remain open.

The [eager](docs/rust-rewrite/append-key-summary-20261031.json) and
[lazy](docs/rust-rewrite/append-key-lazy-summary-20261031.json) prototype-key
summaries are rejected and reverted. Both improved array-building workloads,
but repeated controls failed. The lazy candidate reduced large-join time by
18–22% and retained-object calls by about 25%; its final ordinary small-join
control still slowed 8.00% median / 13.25% mean, and retained empty joins slowed
4.67% / 6.86%. Its 680 native checks, 72 selected original Test262 variants and
both balanced cohorts remain available. Neither candidate changes production.

The [i31 presence experiment](docs/rust-rewrite/numeric-presence-i31-20261031.json)
also failed its control gate and is reverted. It removed the small numeric
bitmap allocation and improved retained-object calls by 8.8% in the full cohort
and 19.2% in the separate follow-up, with effectively unchanged RSS. Ordinary
`toSorted` repeated a 7.16% median / 6.99% mean slowdown; retained empty joins
repeated 3.83% / 5.58%. All 660 selected native checks and 48 original Test262
variants passed. Production retains nominal bitmap storage and ABI 42.

The [scalar-count alternative](docs/rust-rewrite/numeric-presence-packed-count-20261031.json)
is also rejected and reverted. Retained objects improved 7.2% in fresh execution
and 18.4% in retained calls, but ordinary invocation slowed 4.4% and RSS rose
0.08 MiB. Both balanced groups slowed ordinary `toSorted` and singleton joins;
pooled medians rose 11.13% and 17.99%. Native R grew 133,368 bytes and its
ObjectRead frame grew from 800 to 912 bytes. The report preserves 1,043
source-bound native passes, test-witness repairs and their evidence limits.

The [fresh sorted-buffer experiments](docs/rust-rewrite/fresh-sorted-buffer-20261031.json)
are also rejected. Inline transfer improved sorting but slowed retained empty
joins about 5%; outlining restored the caller frame but slowed fresh precompiled
execution of the retained-object workload about 7%. Production retains
per-element publication and ABI 42.
The evidence separates both products, their checks and all slower controls.

In the accepted fusion baseline's same-host retained-object comparison, Scriptw's
RSS was 21.46 MiB versus Scriptc's 17.43 MiB, while private dirty memory was
7.69 MiB versus 8.89 MiB. These process counters do not measure live JavaScript
ownership; the remaining residency gap needs code, stack and host attribution.

## Tests

**Full verification, 2026-10-23:** **102,990 passed / 4 failed / 0 crashed /
0 unimplemented** across all 102,994 pinned Test262 executions, using compiler
`b9ecfe64b`. The 12 repaired failures stay fixed; no new failures appeared.
These four failures remain open (upstream status checked 2026-10-21):

| Remaining case | Failed executions | Why it fails | Upstream tracking |
| --- | ---: | --- | --- |
| Implicit `arguments` binding | 1 | The test expects the arguments object to survive; the current specification replaces it with the block function. | [Test262 #5113](https://github.com/tc39/test262/issues/5113), [fix PR #5112](https://github.com/tc39/test262/pull/5112): open, unmerged |
| Johnston/Honolulu timezone identity | 2 | The test expects Johnston (UM) and Honolulu (US) to share an identity; country-preserving rules and historical-zone handling disagree. | [ECMA-402 #806](https://github.com/tc39/ecma402/issues/806): open, blocked; related Johnston discussion |
| Labelled block functions | 1 | Function visibility under block labels differs between the pinned specification and deployed engines. | No exact issue found; related [Annex B tracker #1595](https://github.com/tc39/ecma262/issues/1595) is open |

**All four remain counted as failures.** Upstream tracking does not change their results.
[Exact cases and verification](docs/rust-rewrite/failed-case-replay-20261021.md#remaining-four-recheck)
· [Upstream status and related issues](test262/errata/proposals/upstream-status-20261021.md).
[Latest development verification](docs/rust-rewrite/current-inventory.md) is separate
from this published full-suite result.

<!-- scriptw-test-graph:start -->
![Test262: 102,561 passing in scope, 4 failing, 0 not implemented, 429 out of standalone Wasm scope](docs/rust-rewrite/assets/test262-status.svg)

**Fresh full run, 2026-10-23:** 102,994 mode-specific
executions at pin `6a4268a93`, compiler `b9ecfe64b`.
All executions belong to this compiler; no earlier failure replay is mixed in.

Blue is the unchanged audited **429-execution subset** focused
on dynamic `eval` / Function source compilation outside the standalone Wasm promise.
These cases passed through the native host in this run. Ordinary API properties
and `call` / `apply` / `bind` stay in the measured categories.
The four bars partition all 102,994 executions; scope cannot hide a failure.
[Portable native snapshots and exact scope inventory](docs/rust-rewrite/test262-chart-full-20261022.json) ·
[Complete published result](test262/snapshots/published-status-wasm-aot-857165c90f0fb4ac3ec0493a3b4470977007808c5b1c10ecf5512840edc80f46.json) ·
[Scope policy](docs/rust-rewrite/runtime-requirements.md#out-of-scope-tests).
<!-- scriptw-test-graph:end -->

## Caveats

- **Wasm is the sole compilation target.** C/native output and a REPL are outside scope.
- **Conformance is incomplete.** The chart identifies its measured compiler
  and complete pinned suite. [Current verification and remaining work](docs/rust-rewrite/current-inventory.md).
  [Verified repairs and original-test conflicts](docs/rust-rewrite/failed-case-replay-20261021.md).
- **Dynamic code generation is limited.** Prepared `eval` / `new Function`
  sources can run; the native host can compile additional supported sources.
  Standalone Wasm does not contain a parser or provide general runtime `eval`.
  Cases requiring a parser or VM inside emitted Wasm are outside product scope.
- **Host APIs are limited.** Scriptw is not a Node.js or browser replacement.
  Three synchronous React SSR cases work; the public browser/streaming entry
  fails. [React probe and limitations](docs/rust-rewrite/react-ssr.md).

## Wasm requirements and support

The backend requires:

- Wasm GC structs and arrays;
- reference types and typed function references;
- exception handling and tail calls;
- multi-memory and threads/shared memory.

The host must also provide the matching Scriptw runtime, `scriptw_host` imports
and Intl data. The native integration selects Wasmtime's copying collector.

| Runtime / host | Scriptw support |
| --- | --- |
| Vendored Wasmtime 47.0.0 through the Scriptw CLI or Rust API | Tested execution path |
| Browsers, Node's Wasm API and other engines | Integration and compatibility unverified; feature support alone is insufficient |
| A bare WASI host | Does not supply Scriptw's JavaScript runtime or host bindings |

[Runtime requirements and dynamic-source details](docs/rust-rewrite/runtime-requirements.md).

## Try it

Requires Rust 1.94 or newer. From a checkout:

```sh
cargo build --locked --release -p scriptw-cli --bin scriptw
printf 'print("Hello Wasm");\n' > hello.js
./target/release/scriptw run hello.js
SCRIPTW_WASM_DUMP=hello.wasm ./target/release/scriptw build wasm hello.js
```

<details>
<summary>Detailed published conformance record</summary>

## Current Status
<!-- scriptw-status:start -->
Rust rewrite status must be read in layers, not one vanity number:
- Fake wasm-safe Test262 subset: `187/187` green
- Fake full Rust rewrite suite: `191/191` green
- Pinned real Test262 baseline (`wasm-aot`, refreshed `2026-10-23`): `102990/102994` not green (`100.0%`)
- Real Test262 goal: Success=102990/102994 (100.0%); burn down NotImplemented=0, Crash=0, Bug=4 to zero
- Pinned revisions: `ecma262=5345883164f463e87f8b40aca4956157ecba8783` `test262=6a4268a9354a545d41c3b62efebc478ed8c521f4`
- Compiler build fingerprint: `7337ccbc2834d0d17902382431789227b6dddf8fda40842bf19c4243161d56f9`; executing image SHA-256: `32ee09a9a33daaf7d27fcb1b649db7223fcbd3ce00f0943cf19cce5ef886101b`
- Current real outcomes: `Success=102990`, `NotImplemented=0`, `Crash=0`, `Bug=4`
- Biggest current real failing kinds: `Runtime=4`
- Biggest current real failing origins: `unknown=4`
- Worst current real matrix targets: `intl402/Temporal/ZonedDateTime: 248/250 passed`, `annexB: 1376/1377 passed`, `staging/sm/lexical-environment: 38/39 passed`
- Published status artifacts: `/workspace/scriptw-full-suite-20261022/test262/snapshots/scriptw-hot-reads-b9ecfe64b-20261022-aggregate-13327628081868727304.json` and `/workspace/scriptw-full-suite-20261022/test262/snapshots/scriptw-hot-reads-b9ecfe64b-20261022-aggregate-13327628081868727304.txt`

As of `2026-10-23`, Rust Wasm-AOT path is at 100% of repo fake coverage, not 100% ECMAScript. Project is still off literal 100% until full pinned real Test262 run is green for Rust path.

Status refresh commands:
- `cargo test -p scriptw-engine --quiet`
- `cargo test -p scriptw-cli --quiet`
- `./target/debug/scriptw test262 run language/wasm/pass --suite-root crates/scriptw-test262/tests/fixtures/fake_test262/vendor/test262 --execution-backend wasm`
- `./target/debug/scriptw test262 run --suite-root crates/scriptw-test262/tests/fixtures/fake_test262/vendor/test262`
- `./scripts/publish-real-status-low-ram.sh wasm-aot codex-published-real`

When counts move, update this block in same change. Do not claim full Test262 `100%` from fake-suite numbers.
<!-- scriptw-status:end -->

</details>

## Documentation

[Current work](docs/rust-rewrite/current-inventory.md) ·
[Development and verification](CONTRIBUTING.md) ·
[Runtime memory ownership and profiling](docs/rust-rewrite/contracts/runtime-byte-ownership.md) ·
[Conformance commands](CONTRIBUTING.md#test262) ·
[Compiler contracts](docs/rust-rewrite/contracts/README.md).
