# Benchmark details

These are the latest published cohorts for each workload family. They use
separate recorded compiler builds and timing boundaries. Reproduction commands
live in [CONTRIBUTING](../../CONTRIBUTING.md#benchmark-charts);
older implementation notes live in the [README archive](benchmark-history.md).

## React Server Components

Measured **2026-11-05**, React **19.3.1**, compiler `971be2adbe2f`
(base `086fe17fa` plus the archived source patch).
Both sync and async Flight outputs match Node exactly.

| Engine | Sync · ms | Async · ms |
| --- | ---: | ---: |
| Scriptw | 6.693 | 7.089 |
| Node 24.12.0 / V8 | 0.579 | 0.442 |

![Retained RSC request timings](assets/rsc-current/runtime.svg)

Each engine uses eight fresh processes across two balanced blocks, with three
checked warmups and seven measured requests per mode. A process retains one
Module/Realm and runs sync then async. Requests include full Flight serialization
and consumption; JSON encoding and exact-output checks follow timing. Values
are medians of process medians. The frozen prior-build control stays in the
receipts; chart bars show the latest compiler and V8. Scriptc's React build
remains unqualified; client React/DOM is a separate open scope.
Program and runtime Wasm are byte-identical to the control; this checkpoint establishes no React execution improvement.

Cold Scriptw setup is **3.719 s JavaScript → Wasm**,
**6.509 s native preparation**, and
**2.533 ms module initialization**.
Program Wasm is **4,231,662 B** (3,683,242 B code; 490,228 B data);
shared runtime Wasm is **38,816,202 B**. All setup precedes request timing.
Child peak RSS includes compilation and process history; it is not live guest heap.

![RSC cold compilation phases](assets/rsc-current/compilation.svg)

Scriptw compiles P with Wasmtime 47 **Fast / OptLevel::None / SinglePass**.
Shared R is built with **SpeedAndSize / Backtracking**. Its successful native
bundle admission is silent; absence of a hit trace does not establish fallback. The cohort uses
a generic target, one CPU, 16-MiB copying GC and the recorded stack/deadline/cache settings.
The [hardware-counter diagnostic](react-attribution-current.json) records
100 checked requests after three warmups in each of three fresh processes per
engine/mode. Scriptw retires **50.19 million sync /
54.08 million async user instructions per request**;
Node retires **2.53 / 2.72 million**.
Counters run without multiplexing. They count Scriptw's execution worker and
Node's main thread, including Node barrier/Promise-return edges; background
compilation/GC threads are excluded. These are unscored counts, not causal shares.

An exact-artifact allocation diagnostic uses one host binary in off/on/on/off
order, three warmups and seven measured requests per mode: **80 exact Flight
checks**, with **40 valid counting epochs**. Measured
requests allocate a median **49,794 GC objects / 2,016,016 B** synchronously and
**53,305 / 2,209,728 B** asynchronously. Physical `StoredValue` wrappers account
for **19,935 / 637,920 B** and **21,064 / 674,048 B**, respectively. These are
aligned reservations, including dead objects and host-bridge work; copied
survivors are excluded. They do not identify source allocation sites or retained
memory. Both program/runtime Wasm match the scored reference byte-for-byte;
all four required heap controls pass. Timings stay unscored.
[Counts, source identities and limits](react-allocation-current.json).

Thirteen physical types are authenticated from actual helper signatures and
field graphs. These are medians of process medians for the measured request
boundary; labels describe heap types, not allocation sites. Per-type medians
need not sum to the total. In particular, ValueArray is not an argument-origin
count, and descriptor allocations do not establish why they occurred.

| Physical type | Sync · objects / B | Async · objects / B |
| --- | ---: | ---: |
| StoredValue | 19,935 / 637,920 | 21,064 / 674,048 |
| CodeUnitArray | 3,371 / 282,416 | 3,925 / 358,576 |
| PropertyDescriptor | 8,412 / 269,184 | 8,790 / 281,280 |
| ValueArray | 3,150 / 120,112 | 3,428 / 135,616 |
| StringValue | 3,284 / 105,088 | 3,827 / 122,464 |
| PropertyKeyTable | 1,916 / 97,648 | 1,956 / 99,600 |
| PropertyEntry | 1,904 / 91,392 | 1,993 / 95,664 |
| OrdinaryObject | 1,327 / 63,696 | 1,413 / 67,824 |
| OrdinaryPropertyIndex | 820 / 52,832 | 864 / 55,648 |
| PropertyTable | 820 / 39,536 | 864 / 41,648 |
| OrdinaryPropertyStorage | 938 / 30,016 | 1,000 / 32,000 |
| BindingCell | 346 / 27,680 | 359 / 28,720 |
| ForInEnumerationRecord | 99 / 3,168 | 101 / 3,232 |

A separate hardware-entry diagnostic passes eight exact known-count controls
and 20 Flight checks. One process per mode counts seven request windows without
changing Wasm or JIT bytes: `ft`/`vn` enter **242 / 276** times per synchronous
request and **254 / 291** asynchronously. Five times their combined entries is
at most **2,590 / 2,725** first-five argument wrappers, roughly 13% of the counted
StoredValue totals, before caller, actual-arity and vector-reuse restrictions.
This is a conditional upper bound, not measured boxing or removable time.
The entry traps perturb execution; these timings stay unscored.
Raw maps, entry bytes and enabled/running counter intervals remain in the archive.

The emitted program shares immutable birth layouts and cached-read code in the
existing runtime. Strict arguments elision walks complete ordinary operands;
eval, capture and resumable refusals remain. Literal typeof and strict
null/undefined comparisons use runtime predicates while preserving operand work
and Throws. Visited-key extension copies existing immutable snapshots and boxes only the new key.
Closed numeric row loops can borrow authenticated captured arrays, use masked
indices and read own Number fields and primitive String lengths. Complete live
guards retain the original loop and completed-prefix fallback; no calls or
allocations enter the admitted loop.
Function own-data probes use the current ordinary dictionary without the
former storage gate. The seven infallible Boolean operations publish directly.
Retained private operands use initialized compiler storage; source lexical TDZ
and suspended reference owners retain their protocols.
Cached literal reads retain their original twelve-input/five-result boundary.
Closed row loops can borrow current dictionary data after checking live
extent, current key/descriptor/value and complete field guards. For repeated
low-bit masked rows with Number/string-length leaves, canonical own-data
reads validate a complete read-only period before any source body and copy its
leaves to one existing numeric array. A second authenticated dense array may
supply primitive String keys; both masks define one combined period. Unsupported
keys, holes, accessors and Proxies decline before source work. A miss resumes the whole original loop; a successful preflight uses
numeric-only loads with the original arithmetic and row identity. Other admitted
rows retain compact/dictionary guarded backedges and completed-prefix fallback.
Recipes, accessors, Proxies and incomplete regions keep their original semantics.
Scratch roots clear before the generic continuation. Projection allocates one
numeric array. Canonical recipe reads can also allocate or normalize intrinsic metadata without invoking JavaScript; rejected preflight
work remains an explicit cost.
Closed strict comparisons can precompute a pure repeating input period from
two current dense arrays through the canonical equality kernel. One private
numeric buffer and a bounded integer induction retain the original branches,
arithmetic order and publication. Strings, BigInts and references keep exact
semantics. Unsupported values and effects retain the original loop.
Local argument entries retain their two-formal bound. Runtime value,
environment, descriptor and call-transport costs remain; V8 parity is open.

Fresh user-cycle sampling attributes **25.30% sync /
23.97% async** to six property/enumeration helpers' disjoint
own samples. The `vn`/`ft` page-touch loops account for
**2.17% /
2.27%**. Each mode has one
sampled process and 200 checked requests after three warmups. Frame-pointer
chains can truncate; inclusive caller shares overlap. These observations do not
partition wall time or predict a change's gain. The linked diagnostic retains
raw samples, source identities and same-process prefixes.

Older allocator controls, source screens and rejected experiments retain their
own source/binary identities in the inert archive. The allocator census found
most SinglePass spill capacity in cross-block/edge homes; strictly block-local
reuse was rejected as the main hill. Entry stack budgets are distinct from spill
capacity, executed spill traffic and allocation counts. GC allocation
reservations are recorded above; their source origins remain unmeasured.

Runtime ABI 51 supplies bounded UTF-8 codecs, byte streams with default readers,
events, abort signals and tasks for these fixtures. Fetch/Request/URL, DOM nodes
and full WHATWG conformance remain unsupported or unverified.

[Current report and process receipts](rsc-current.json) ·
[Size accounting](rsc-wasm-size-current.json) ·
[Inert evidence](situations-evidence-2026-11-05.tar.xz).

## Focused performance situations

Measured **2026-11-05**: **21** standalone Module programs, compiler `971be2adbe2f`,
Node **24.12.0 / V8 13.6.233.17-node.37**, Rust **1.97.1**.
Each program has its own score; no aggregate combines arbitrary operation counts.

| Program · retained call | Scriptw Fast · ms | Scriptw Speed · ms | V8 · ms |
| --- | ---: | ---: | ---: |
| `control-empty.js` | 0.009 | 0.009 | 0.000 |
| `global-shallow.js` | 2.224 | 1.709 | 0.949 |
| `global-deep8.js` | 2.727 | 1.978 | 1.077 |
| `global-bound-deep.js` | 0.700 | 0.240 | 0.004 |
| `global-getter-deep.js` | 3.071 | 2.243 | 0.982 |
| `frame-compact.js` | 8.268 | 5.851 | 0.057 |
| `frame-branches32.js` | 8.272 | 6.026 | 0.081 |
| `frame-branches128.js` | 8.690 | 5.953 | 0.113 |
| `frame-branches512.js` | 13.821 | 6.202 | 0.178 |
| `args-unused.js` | 0.732 | 0.510 | 0.003 |
| `args-observed.js` | 2.434 | 1.999 | 0.003 |
| `property-stable.js` | 0.154 | 0.074 | 0.071 |
| `property-mutated.js` | 0.155 | 0.078 | 0.062 |
| `metadata-ordinary.js` | 0.209 | 0.085 | 0.012 |
| `metadata-function.js` | 0.204 | 0.085 | 0.206 |
| `metadata-intrinsic.js` | 0.206 | 0.086 | 0.219 |
| `typeof-dispatch.js` | 0.943 | 0.383 | 0.014 |
| `singleton-dispatch.js` | 0.906 | 0.357 | 0.010 |
| `strict-equality-numbers.js` | 0.109 | 0.048 | 0.012 |
| `strict-equality-mixed.js` | 0.104 | 0.048 | 0.034 |
| `for-in-wide.js` | 2.052 | 1.959 | 0.038 |

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
The empty control is shown without subtraction. Benchmark scratch and prepared
images used tmpfs after Btrfs exhausted metadata reservations; completed raw files
were copied byte-for-byte into the evidence tree. File preparation remains untimed.

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
The 512-site native P image is **5.43 MiB Fast /
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

A separate warmup sensitivity check uses four fresh processes per engine/case,
100 checked warmups and 21 measured calls with the same frozen host and workloads.
Number equality measures **0.051 ms Speed /
0.012 ms V8**; mixed equality measures
**0.052 /
0.036 ms**. Normal V8 JIT and natural GC
remain enabled; 100 warmups do not prove optimized steady state. These separate
observations stay outside the chart cohort; raw receipts remain in the archive.

### Enumeration

Sixteen for-in loops visit all 64 own enumerable properties of a prepared ordinary
record, checking 1,024 yielded keys and numeric value reads. Inherited nonenumerable
processing is included. Immutable visited-key snapshots are copied without
reboxing; prefix reference copying and membership comparisons remain quadratic.
Mutation, Proxy and suspension controls run separately as semantic regressions.

![For-in enumeration](assets/situations-current/enumeration.svg)

### Public-call control and verification

![Public-call control](assets/situations-current/control.svg)

All **504** situation processes and **60,984** calls passed exact checks.
The balanced RSC campaign adds 24 processes and 480 exact Flight checks.
Fresh scoped verification passes **1,770 Engine**, **65 emission**,
**637 structural** and **380 source-guard** checks. The **769 IR** cases
reuse their original passing binary/receipts with unchanged IR/front-end inputs.
AOT units pass **653** cases; four known bootstrap assertions remain failures.
Full workspace CI and pinned Test262 were not rerun. Eleven unchanged workspace
formatting failures and fourteen stale Intl recipes remain [owned](current-inventory.md).

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
