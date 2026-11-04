# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-04** with pinned React 19.3.1 and compiler `e07895027937`.
Both sync and async Flight outputs match Node exactly.

| Engine | Sync · ms | Async · ms |
| --- | ---: | ---: |
| Scriptw | 13.646 | 14.515 |
| Node 24.12.0 / V8 | 0.487 | 0.400 |

![Matched retained RSC request timings](assets/rsc-current/runtime.svg)

Each engine uses eight fresh processes across two balanced blocks, with three
checked warmups and seven measured requests per mode. Each process retains one
Module/Realm and runs sync then async. Requests include full Flight serialization
and consumption; JSON encoding and exact-output checks follow timing. Values are
medians of process medians. Scriptc's React build failures remain unqualified;
client React/DOM is a separate open scope.

Cold Scriptw setup is **3.279 s JavaScript → Wasm**,
**11.860 s native preparation** and
**2.673 ms module initialization**.
Program Wasm is **6,086,295 B** (5,540,898 B code; 490,228 B data);
shared runtime Wasm is **39,224,975 B**. Native preparation, initialization and
input preparation precede request timing. Child peak RSS includes compilation
and process history; it is not the live guest heap.

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

Scriptw uses Wasmtime 47 **Fast / OptLevel::None / SinglePass**, a generic target,
one CPU, 16-MiB copying GC and the recorded stack/deadline/cache settings.
The [fresh hardware-counter diagnosis](react-attribution-current.json) captures
100 checked requests after three warmups in each of three fresh processes per
engine/mode. Scriptw retires **94.83 million sync /
100.86 million async user instructions per request**;
Node retires **2.49 / 2.81 million**.
Counters run for their complete windows without multiplexing. This counts the
Scriptw execution worker and Node main thread; Node barrier/Promise-return edges
remain included, while background compilation/GC threads are excluded. These are
unscored instruction counts, not timing ratios or an allocation breakdown.
Current sampled caller paths and exact native prefixes are retained in the
evidence archive, separately from these hardware counts. Inclusive sample
owners overlap; no sampled share predicts a wall-time gain.

Current user-cycle sampling places **8.70%** of samples in the stack-probe
regions of `vn` and `ft`. Their entry stack-check budgets are **963,920 B /
362,480 B**. An allocator-only diagnostic keeps the same Wasm and
`OptLevel::None`: Backtracking reduces these budgets to **17,728 B / 7,088 B**
and probe samples to **0.022%**. In four fresh processes per arm, sync/async
requests take **10.643 /
11.327 ms**, versus
**13.615 /
14.538 ms** with SinglePass;
native preparation rises from **11.91 s**
to **20.07 s**.
This diagnostic is separate from the scored chart and changes no default.
Allocator choice also changes spill traffic and native code size; probe sample
shares do not account for the complete timing difference.

One typed runtime helper now owns both `instanceof` entry algorithms. Source
operators evaluate their operands once and hold them across the call; the native
method supplies its actual execution Realm. Hooks, bound-target transitions,
prototype walking and whole Throws retain the existing state machine. The
scored comparison keeps Fast's optimization and allocator settings unchanged.
Total executed allocation and each mechanism's causal share remain unmeasured.

Eligible strict ordinary functions pass up to two arguments in Wasm locals to
one canonical source body. Saved actual callable and defining-program guards
permit direct calls; bounded constant/numeric leaves can inline at source level.
Canonical Module root captures use validated direct cells with live import
forwarding and TDZ checks. Internal OwnKeys consumers share a typed key table;
public Reflect.ownKeys still creates the required JavaScript Array.
Arguments observers and unsupported protocols retain ordinary dispatch. Fast and
Speed both disable general native inlining; larger functions still pay for
runtime value tags, environments and dynamic property semantics.

The current read path remains the next source target: private descriptor reads
for exotics still construct a native GPD callable and JavaScript descriptor,
then reconstruct a private record. A shared private `[[GetOwnProperty]]` owner
could remove that transport. Ordinary/Array reads already bypass it. Preserve
Proxy validation and snapshot ownership; sampled inclusive shares motivate this
experiment without predicting a speedup.

Runtime ABI 50 supplies bounded UTF-8 codecs, byte streams with default readers,
events, abort signals and tasks for these fixtures. Fetch/Request/URL integration,
DOM nodes and full WHATWG conformance remain unsupported or unverified.

[Current report and process receipts](rsc-current.json) ·
[Size attribution](rsc-wasm-size-current.json) ·
[Inert evidence archive](situations-evidence-2026-11-04.tar.xz).

## Focused performance situations

Measured **2026-11-04**: 13 standalone Module programs, compiler `e07895027937`
(base `19b59f953` plus the archived performance patch),
Node **24.12.0 / V8 13.6.233.17-node.37**, Rust **1.97.1**.
Each program has its own score, separate from RSC and fresh-entry kernels.
No aggregate score combines their arbitrary operation counts.

| Program · retained call | Scriptw Fast · ms | Scriptw Speed · ms | V8 · ms |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.010 | 0.009 | 0.001 |
| `global-shallow.js` | 2.366 | 1.909 | 0.954 |
| `global-deep8.js` | 2.923 | 2.156 | 1.000 |
| `global-bound-deep.js` | 0.734 | 0.249 | 0.010 |
| `global-getter-deep.js` | 3.208 | 2.384 | 0.963 |
| `frame-compact.js` | 10.343 | 5.942 | 0.080 |
| `frame-branches32.js` | 10.459 | 6.175 | 0.099 |
| `frame-branches128.js` | 11.182 | 6.269 | 0.139 |
| `frame-branches512.js` | 18.940 | 6.451 | 0.213 |
| `args-unused.js` | 0.926 | 0.541 | 0.102 |
| `args-observed.js` | 6.529 | 5.735 | 0.139 |
| `property-stable.js` | 3.647 | 1.699 | 0.076 |
| `property-mutated.js` | 3.165 | 1.604 | 0.067 |

Each value is the median of eight fresh-process medians across two balanced
blocks. Every process initializes one Module/Realm, then runs three checked
warmups and seven measured calls. Four inputs rotate identically in every actor;
every call checks an independently computed integer checksum. Bars show current
configurations; whiskers span the eight process medians. Block medians and
process summaries remain in the [data](situations-current.json).

Scriptw uses Wasmtime 47: **Fast = None / SinglePass**, **Speed = Speed /
Backtracking**, generic target, 16-MiB copying GC, one CPU, 4-GiB cap and no swap.
JavaScript/Wasm/native compilation, module initialization and `prepare(input)`
precede the public `run()` clock. Every Wasmtime consumer has its compiler disabled.
Node uses the same source bytes in `vm.SourceTextModule`, with normal JIT and
natural GC; three warmups do not establish optimized steady state. Timings include
each runtime's public-call bridge and `print` conversion/capture; output draining
and validation follow timing. The empty control is shown without subtraction.
Excluded setup can influence timed GC, feedback and JIT invalidation.

The balanced RSC control compares the frozen prior compiler and outlined
candidate in both blocks. Additional allocator diagnostics stay separate from
the V8/Fast/Speed chart cohort. Older and current situation cohorts are separate
runs; their process ranges and compiler identities must be retained when
assessing small differences.

### Global lookup

The four programs read six constructor values across 1,024 passes. Deep lookup
retains eight nested padding captures; the bound control captures constructor
references beside the worker. Proven unbound references in eligible ordinary
Module functions now start at the defining Realm's Global Environment Record,
retaining live lexicals, mutable properties and getters.

![Global lookup situations](assets/situations-current/global-lookup.svg)

Shallow/deep also differ in padding reads and arithmetic; their difference is not
pure lexical-depth cost. Bound aliases may fold and V8 may hoist global reads.
The V8 bound control varies with JIT history; retain process ranges and both
block medians rather than treating one ratio as stable. The getter score includes
getter calls and counter increments. Source counts are not executed lookup counts.

### Control-flow and call-site scaling

All four programs make **8,192** entries into a function with one, 32, 128 or 512
selected call sites, exercising every site with identical checksums. Eligible
dense integer switches use guarded Wasm `br_table` dispatch. Fast ranges from
**10.343 to 18.940 ms**;
Speed from **5.942 to 6.451 ms**.

![Control-flow and call-site scaling](assets/situations-current/frame-size.svg)

![Native program image growth](assets/situations-current/frame-native-image.svg)

The 512-site native P image is **6.67 MiB Fast /
2.52 MiB Speed**; portable P is
**0.84 MiB**. Serialized native images include
metadata and data. Compact dispatch uses a dynamic key and one polymorphic site;
switch arms use constant keys and separate sites. This does not isolate stack
frame growth or stack-probe cost. Eligible call sites now use local argument transport and guarded direct calls.
Compact polymorphic dispatch still requires runtime selection.

### Arguments observation

Both programs make 1,024 strict ordinary calls with captured state, reassign a
formal parameter and compute the same result. The observed program also reads
the original `arguments[0]` and `arguments.length`. Eligible strict functions
omit unobservable arguments objects; Module planning removes unused synthetic
cells before capture hops finalize. Real captures and observers retain storage.
The observed case also loses unused `this`/`new.target` cells, so it is not an
untouched control for that change. Its arguments object remains observable.
The extra observation is timed work; the difference is not an allocation count.

![Arguments observation situations](assets/situations-current/arguments-observation.svg)

### Property mutation history

Both programs perform 8,192 variable-key reads over 64 records. Mutation setup
adds an accessor, deletes properties and restores the values before timing.
Current guarded Number array hits and computed-String own-data hits bypass
object/key conversion and generic lookup. Misses preserve full conversion,
accessors, prototype traversal and proxy dispatch. Each read still inspects live
storage; no new heap or property cache is introduced. These timings combine
indexing, key production and record reads and do not isolate one operation's
cost or allocation count.

![Property mutation history situations](assets/situations-current/property-mutation-history.svg)

### Public-call control and verification

![Retained public-call control](assets/situations-current/control.svg)

[Generate/run selected programs](../../CONTRIBUTING.md#focused-performance-situations)
with `--case <file.js>`; choose `--policies fast` or `speed`.
Scriptc has no matched retained-Module consumer here. These programs expose
separate hills, not each mechanism's share of RSC time.

All **312** situation processes and **3,120** calls passed exact-output checks.
The balanced RSC campaign adds 24 processes, including a frozen prior-build
control; chart bars contain only the latest compiler and V8. The recorded compiler shares the existing HasInstance state machine through one typed
runtime boundary. Encoded-Wasm and semantic controls cover both entry modes,
held operand order, reentrant hooks, bound targets, whole thrown values and
actual execution Realms. Public OwnKeys acceptance checks use the external
emission target; all original canonical/helper-context copy bodies remain
checked.

Full workspace CI and pinned Test262 were not rerun. Detailed native/source
verification receipts and the four reproduced pre-existing AOT bootstrap failures
remain in the evidence. Workspace formatting fails on 12 unchanged files;
14 stale Intl identity recipes remain [owned](current-inventory.md).

[Report, identities and process summaries](situations-current.json) ·
[Checksummed UTF-8 evidence](situations-evidence-2026-11-04.tar.xz).

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
