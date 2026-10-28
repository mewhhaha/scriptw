# Local Wasmtime versus V8 baseline

## Cached string-hash checkpoint — published 2026-10-23

Source `97036e5a8` closes **P023 and P112** against the frozen accepted
`5c63c6597` baseline. StringValue lazily stores the raw UTF-16 hash once, and
property/Map/Set users reuse it before their existing tag mixing. Pooled literals
and dynamically created strings share this path. First-use hashing still occurs;
equality still checks identity/content independently of the hash. A legitimate
zero hash is recomputed without changing the result. This is runtime metadata,
not an exposed property or a second string representation.

Layout **16** appends an i64 field to every StringValue, including strings never
used as keys. Compiler-owned publication and all three host syntax constructors
initialize it to zero; host admission checks its type/mutability and rejects
layout 15. Code-unit writes remain confined to private construction; publication
consumes that construction object. The Wasm array type itself remains mutable.

[Portable metadata](performance-string-hash-20261023.json) and the
[byte-verified raw archive](performance-string-hash-20261023.tar.gz) preserve
all screens, paired observations, exact product/source identities, native and
Test262 receipts, memory observations and storage histories. The benchmark code
is checked in under `v8_repeated_keys`, `v8_string_retention`,
`v8_property_definition` and `v8_baseline` in the CLI fixtures.

The idle-machine comparison pins both products to one CPU. Each ABBA round
uses seven measured invocations after ten warmups in fresh Stores/Realms;
Node checks the workload stdout. The table is the median of two round medians
per side, in milliseconds, and includes setup and checksums. It is descriptive,
not a statistical significance test or an isolated estimate of hash-loop cost.
The separate three-sample screen, including its slower observations, is retained.

| Workload | Before | After | Median change |
| --- | ---: | ---: | ---: |
| `map-get-short.js` | 29.084 | 23.670 | -18.6% |
| `map-get-long.js` | 97.455 | 45.718 | -53.1% |
| `map-get-numeric.js` | 22.695 | 22.254 | -1.9% |
| `set-has-short.js` | 23.923 | 22.883 | -4.3% |
| `set-has-long.js` | 89.234 | 41.188 | -53.8% |
| `set-has-numeric.js` | 22.122 | 22.553 | +1.9% |
| `property-get-short.js` | 10.104 | 9.938 | -1.6% |
| `property-get-long.js` | 57.372 | 9.845 | -82.8% |
| `property-get-numeric.js` | 13.356 | 12.536 | -6.1% |
| `retained-unhashed.js` | 2,830.208 | 2,994.417 | +5.8% |
| `retained-first-hash.js` | 3,014.591 | 3,111.050 | +3.2% |
| `define-long-missing.js` | 55.783 | 30.346 | -45.6% |
| `define-long-present.js` | 99.565 | 33.127 | -66.7% |
| `integer.js` | 6.011 | 5.501 | -8.5% |
| `objects.js` | 15.648 | 13.880 | -11.3% |
| `retained-objects.js` | 1,331.412 | 1,365.153 | +2.5% |

Repeated long property reads improve about 83%; long Map/Set lookups about
53–54%. Long-key define-own also benefits. Short/numeric controls have overlapping
rounds, so this does not establish a universal speedup. The retained-unhashed and
first-hash controls have **5.8% / 3.2% higher medians**, respectively, with overlapping
rounds; retained object rows have a **2.5% higher median**, also with overlapping rounds. These costs are retained
in the acceptance decision. Every string pays the added field cost even where
no speed benefit is possible.

The two 32,768-string controls grow reported GC capacity from 16 to 32 MiB on
both products. That coarse capacity is not live heap or allocation bytes, and
cannot establish zero memory overhead. The raw records include sampled invocation
RSS and whole-child peak RSS; they also include runtime/code/allocator overhead.
Wasmtime exposes no live-GC/allocation counter through this profiling boundary.

The CLI grows by **169,104 bytes (0.16 MiB)**. The separate 56-fresh-process
startup ABBA gives cold **1,082.815 → 1,045.699 ms**
and cached **788.814 → 761.327 ms**.
Cold rounds overlap; do not attribute those whole-process differences entirely
to hashing. Cold RSS medians are 384.11 → 384.13 MiB,
cached 351.18 → 351.25 MiB.
Application caches are empty/prewarmed as labelled; OS caches are not flushed.
The ordinary V8 comparison above remains the separately measured source50 cohort.

The release product builds in **6m23s**. All **18 compiled semantic families**
pass, including exact UTF-16, equal-but-distinct strings, cross-property/Map/Set
use, collisions, GC, deletion/reinsertion and descriptor semantics. All
**4,083 native tests / zero failures / zero ignored** pass across the twelve
recorded affected release scopes, including GC entries, native program templates
and runtime-cache controls. The independent runtime producer matches all payload
bytes; its generated include wrapper differs only in the producer directory.
The original run caught eight stale test expectations: six RegExp fixture
constructors, one completion fixture and one runtime-cache trace assertion.
Their failed receipt remains in the archive. Complete engine-lib and runtime-cache
reruns at `7efa13482` repair only those tests; the other ten scopes
retain their source `97036e5a8` results. Each run has its own source/linker fence,
and the exact three-file patch is retained. The mixed production/test file's
production prefix is byte-identical; its conservative compiler fingerprint does
change, so these are separately identified sources. The frozen measured product
and original Test262 replay remain `97036e5a8`. Both repaired runtime packages
have byte-identical full Wasm payloads/runtime keys and serialized native code;
only their source fields and dependent package/manifest checksums change. The 277 source-only guards pass
as a fast diagnostic.

The original **13,104-execution Test262 replay** covers complete Map, Set, String,
Object, Reflect, object-expression and string-literal families, plus all four
tracked failures: **13,100 Success / four known Bug / zero Crash /
zero NotImplemented**. It changes no case IDs, worker counts, deadlines or outcomes.
This is affected-family acceptance, not a new complete Test262 run; the README
full chart remains source `b9ecfe64b`.

One storage preparation attempt stopped before native compilation because its
inventory expected four old executables, but one old product contained a single
larger CLI; the exact three-file inventory corrected that bookkeeping assumption.
The failed launch/log and successful continuation remain in the raw archive.
The first publication check also stopped on changed native staging hashes.
The test-only repair had refreshed source metadata through two shared mutable
package aliases. Both repaired outputs were archived and verified; the original
product aliases were restored from their verified pre-build archive. Frozen
executables and all completed tests were unaffected. The source-transition
receipt validates complete payload identity and the precise header/checksum
differences. Atomic producer publication is the next tooling repair before any
new Cargo build; cached diagnostic linking runs no producer.

Closed products, runtime payloads, private measurement caches and diagnostic
executables are losslessly archived and all members verified before retirement.
Dependency libraries remain available through verified storage aliases, protected
against dirty rustc writers; the two-GiB Cargo reserve and 256-MiB-per-process
scratch requirements are unchanged. Twenty older shared-memory archives remain
unavailable as previously documented; none of these new archives uses that storage.

The ledger now has **25 terminal decisions, seven partial and 128 unresolved**.
Broader release/all-feature, retention and full-suite acceptance remain open.
Before another P067 implementation, profile resolved environment/cell loads:
zero-hop and exact-hop traversal is already statically emitted. Hoisting a cached
handle would need lexical-instance, iteration and resume lifetime proofs. Its
earlier inline-payload experiment and P024's redundant equality guard remain rejected.

## Prepared property-definition checkpoint — published 2026-10-23

Product **`5c63c6597383ba4d6f56e0780daac922798b1033`** carries the
ordinary property's prepared key hash from descriptor lookup into successful
insertion. Missing-key definitions avoid a second table probe; existing-key
definitions retain their descriptor.
An empty table still avoids hashing when descriptor validation rejects the
definition. Key/descriptor conversion finishes before lookup, and the rooted
descriptor stays live through validation and publication. Proxies and exotic
objects keep their existing dispatch. Layout15 and the helper ABI are unchanged.
[Portable measurements, failed attempts and exact verification evidence](performance-property-definition-20261023.json)
include an independently verified archive of the raw records and tools.

**P020 is complete against its original lookup/insertion scope.** The earlier
collection and callback work plus this ordinary define-own change remove its
remaining duplicate hash. Broader Set/growth work and cached string hashes are
separate opportunities. The ledger is **22 terminal decisions / seven partial /
131 unresolved**; the full performance backlog remains open.

The affected release checkpoint passes **3,893 native tests / zero failures /
zero ignored** across nine complete targets. The language structural target
initially fails an obsolete exact call-count assertion left behind by earlier
string optimizations. Its repaired test checks that live APIs still have a
definition and caller while preserving the removed-API absence checks. All
151 tests in that target then pass; the other eight targets retain their
unchanged-source outcomes. The original failure and storage-deferred attempts
remain in the archive. Seventeen compiled semantic families pass, including
descriptor-conversion mutation, empty/rejected definitions, equal-content keys,
symbols, accessors, deletion/reinsertion, GC, proxies, sealing and freezing.
The existing 517-key fixture contains 51 colliding buckets at capacity 2048:
keys 207 and 220 share bucket 478, while only key 207 is deleted and reinserted.
This identifies collisions in the executed fixture; it is not an extra runtime
test or an isolated measurement of hashing.

The **7,174 original mode-specific Test262 executions** give **7,170 Success /
4 Bug / 0 Crash / 0 NotImplemented**. They cover complete Object definition,
descriptor, key, assignment and integrity families, Reflect/Proxy definition,
object expressions and the four known upstream disagreements. Exactly those
four remain failures. Four case workers, isolation and original 60-second
budgets stay unchanged. This affected replay does not replace the separately
published complete source50 campaign or its README graph.

Confirmation uses the accepted `b9ecfe64b` product as baseline, ABBA order,
seven measured invocations after ten warmups per observation, and two
observations per side. Both products run on one CPU, serially after builds.
Values are medians of round medians; they do not establish statistical
significance or an isolated contribution to unrelated workloads.

| Whole execution workload | Before | After |
| --- | ---: | ---: |
| 2,048 missing long-key definitions | 60.357 ms | 56.084 ms |
| 2,048 missing short-key definitions | 28.359 ms | 26.802 ms |
| Existing long-key definitions | 102.400 ms | 102.405 ms |
| Rejected definitions on empty objects | 19.456 ms | 18.141 ms |
| Long-key delete/reinsert | 133.696 ms | 124.830 ms |
| Ordinary long-key assignment | 79.033 ms | 69.660 ms |
| Integer baseline | 6.524 ms | 5.850 ms |
| Object baseline | 15.644 ms | 15.420 ms |
| Retained-row baseline | 1,320.507 ms | 1,307.657 ms |

The definition fixtures include key/descriptor construction, object setup and
checked output, rather than measuring only a hash loop. Long-key insertion is
about 7.1% faster and short-key insertion 5.5% faster in this confirmation.
The initial three-sample screen is retained: short-key definition rises
27.380→28.442 ms and the object control 15.677→16.528 ms. Existing-key updates
are flat in confirmation; object and retained-row rounds overlap. The integer
screen is roughly flat (5.793→5.753 ms), so the later integer drop is not
attributed to property hashing.

A separate 56-process startup ABBA gives **cold 1,035.635→1,062.143 ms**
(26.508 ms / 2.56% slower) and **cached 745.673→748.012 ms** (overlapping rounds).
The actual CLI grows **893,104 bytes**, to **291,453,000 bytes**; process peak
RSS rises roughly **1–1.5 MiB**. These are explicit costs of the accepted
execution improvement. OS caches are not flushed, and RSS includes the host,
compiler, runtime and warmups; it is not live JavaScript heap usage.

Independent product and native builders produce identical runtime packages,
native images, manifests, bindings and static archive. Only the generated Rust
wrapper's absolute producer directory differs; its source is kept separately.
The nine binary/data files are compared byte for byte before completed copies
share storage. The full ten-file product archive remains immutable and verified.
The runtime-image target resumes after a storage deferral without lowering
the two-GiB build reserve or 256-MiB per-process scratch allowance.
Completed private caches and diagnostic executables are archived with sizes,
hashes and restoration metadata before their originals are retired.
The initial native run samples a whole-cgroup maximum of 34,356,191,232 bytes
and records 4,041 memory-limit events with no OOM or OOM-kill event. This includes
temporary files and shared file pages, not just process RSS or JavaScript memory.
The fresh runtime-image scope and structural repair complete without limit
events. A rejected continuation launch ran no test bodies and remains recorded.

Both structural targets now participate in the fast source-only diagnostic;
the typed modules remain mandatory in Cargo. Nine tooling controls verify
target selection, complete inventories and failure propagation. This catches
stale language guards before an expensive native build. The restored Cargo
configuration remains required; this checkpoint explicitly retains the prior
empty-encoded-Rustflags dependency cohort. The older missing shared-memory
archives remain unavailable and are not claimed restorable.

The README's V8 table stays bound to its separate ordinary source50 report.
Paired values above are not substituted into that comparison. Broader release,
all-feature, retention and remaining performance acceptance are still open.


## Hot-read checkpoint — published 2026-10-22

Measured product source **`b9ecfe64bdac63e0553b8c38a178229056e38f73`**
replaces floating-point ArrayIndex recognition with bounded decimal recognition,
tests concrete Array indices before named-property headers, constructs one-unit
charAt/at results directly, and bulk-copies string iterator outputs. Typed-array
numeric-key classification stays separate. Layout15 and the existing helper ABI remain.
[Raw results, initial failures, tools and identities](performance-hot-reads-20261022.json)
are losslessly embedded in the portable checkpoint.

Seven complete runtime targets pass **3,576 native tests**. The newly included
structural target first gives 165 passes and one stale trim-visibility assertion:
the accepted bulk-trim implementation shares one closed crate-internal mode,
while the older guard still required file-private visibility. After updating
the guard and its contract, the entire target passes **166/166**. This disjoint
union is **3,742 passes / zero failures / zero ignored** across eight targets;
production compiler inputs did not change for the repair. Seventeen real
compiled semantic families pass, including canonical/noncanonical keys, String
exotic descriptors, coercion/throws, UTF-16, iterator freshness and dynamic-call
controls. Seven independently generated product/test runtime payloads match.
The subsequent [fresh full Test262 run](test262-current.md), completed
2026-10-23 with this exact product, gives **102,990 Success / 4 known Bug /
0 Crash / 0 NotImplemented** across all 102,994 original executions. No new
failures appeared. Wider default/all-feature native acceptance remains separate.

Paired confirmation uses source48 as the direct baseline, two observations per
side in ABBA order, seven measured invocations after ten warmups per observation.
Values below are medians of invocation medians; rounds overlap, and the combined
batch does not establish isolated contributions or statistical significance.

| Execution workload | Source48 | Source50 |
| --- | ---: | ---: |
| Concrete indexed reads | 173.015 ms | 152.961 ms |
| Named Array reads | 75.750 ms | 75.683 ms |
| Nonindex Array reads | 76.902 ms | 75.847 ms |
| charAt / at | 43.131 ms | 41.011 ms |
| Single-character String reads | 77.580 ms | 74.219 ms |
| ASCII String iteration | 15.740 ms | 14.845 ms |
| Unicode String iteration | 15.379 ms | 15.746 ms |
| Integer baseline | 5.707 ms | 5.957 ms |
| Object baseline | 18.854 ms | 15.579 ms |
| Retained-row baseline | 1,443.129 ms | 1,327.993 ms |

The original three-sample screen's slower integer, character and Unicode
observations remain. A subsequent eight-round ABBAABBA recheck gives integer
6.199→6.070 ms and Unicode 14.149→13.768 ms, both with overlapping rounds.
Keep the initially slower confirmation values above; these rechecks establish
no reliable integer/Unicode regression or gain. A separate 56-process startup
ABBA gives application-cold 1,038.276→1,036.388 ms and cached
745.742→761.709 ms, with overlapping rounds and roughly unchanged process RSS.
All measured program outputs match stock Node or explicit proposal controls.

The README's **separate ordinary report** uses the same source50 product, seven
samples and ten warmups. Wasmtime/V8 medians are cold 1,084.901/32.679 ms,
cached 801.288/37.437 ms, integer 6.505/2.493 ms, objects 15.040/0.064 ms,
and retained rows 1,287.305/3.357 ms. Cold/cached Wasmtime child peak RSS is
383.04/349.78 MiB; retained rows peak at 392.80 MiB across the whole child.
The CLI is **290,559,896 bytes (277.10 MiB)**. V8 remains faster. OS caches
are not flushed, and RSS includes compilation, runtime and warmups rather than
isolated JavaScript heap memory. Separate ordinary cohorts are not causal
comparisons against the old README table.

The product build takes **9m37s**, including runtime-image regeneration; native
runtime builders spend about 70.4s/72.4s inside their distinct profiles.
Two AOT Rust units take 94.0s/67.7s and overlap other work; do not add these
durations to total wall time. The first native attempt is storage-deferred;
the second completes all runtime bodies. Its sampled whole-cgroup maximum is
34,343,419,904 bytes, with zero OOM/OOM-kill events; it includes shared files and
temporary storage and is neither process RSS nor JavaScript heap usage.

The new [source-only diagnostic](../../CONTRIBUTING.md#verification)
passes 143 guards in 3.471s compilation plus 1.46s execution, avoiding the Cargo
dependency graph. Six runner controls pass, and the full166 Cargo target remains
mandatory. Restoring missing tracked Cargo config prevents accidental Rustflags
fingerprint forks. Native recheck intentionally retains the previous empty-flags
cohort; future normal builds use the restored config.

Twenty historical compressed archives relocated to `/dev/shm` are now absent
in this execution environment. Their inventories and previous receipts remain,
but missing historical binaries are not claimed restorable. Current products
and runtime payloads are verified. Persistent evidence must not rely on shared
memory; the committed bundle retains all selected raw observations and tools.
Hill counts stay **21 terminal / eight partial / 131 unresolved**: the full
conformance run validates these changes but does not complete the broader
performance hypotheses or all-feature native acceptance.

## Rejected NativeHost storage experiment — 2026-10-22

Source `f9f4efa9ee0b284ca0b26106d16fa718e586d75f` used the same bounded
compressed-native codec and independent transient leases for both profiles.
[Source-bound raw observations, tools and checks](performance-nativehost-storage-rejected-20261022.json)
are retained. **This experiment is removed. Existing Portable compression stays.**

The actual static CLI shrank from **290,533,336 to 231,237,264 bytes**; the runner
from **283,225,848 to 223,929,712 bytes**, saving **59,296,072 and 59,296,136 bytes respectively**.
The product build finished in **1m01s**, reusing both complete runtime images.
All 21 measured program modules, runtime keys and outputs matched accepted48;
seven prior runtime payload/manifests retain code with validated source framing,
and the eighth packed image decodes to the same original native bytes.
Both actual NativeHost startup and Portable program execution were exercised.

A pinned 56-process ABBA uses seven application-cold and seven application-cached
invocations in each of four rounds, with separate traced priming excluded.
Each value below is the median of two round medians; OS caches are not flushed.

| Whole-process measure | Accepted48 | Rejected49 |
| --- | ---: | ---: |
| Application-cold startup | 1,059.862 ms | 1,129.665 ms |
| Application-cached startup | 775.355 ms | 830.447 ms |
| Cold child peak RSS | 382.73 MiB | 391.49 MiB |
| Cached child peak RSS | 349.54 MiB | 365.28 MiB |

Cold startup is **6.6% slower**, cached startup **7.1% slower**, and peak child
RSS increases about **9–16 MiB**. This is process memory, not JavaScript heap.
Traces put NativeHost decode/digest admission near 136–137ms in the new path,
beside package hashing; they are diagnostic observations outside score spans.
An optimized cached-dependency microbenchmark of bulk versus streaming decode
has overlapping costs and does not recover the startup tradeoff. That diagnostic
is not a product implementation or a decoder-boundary acceptance result.

The size saving does not justify these startup and memory regressions. No broad
native or Test262 campaign ran for this rejected source. The four upstream Bugs,
full-suite chart, accepted48 checkpoint and source47 README timings are unchanged.
This rejection adds no terminal hill: broader P132/P133/P149 work remains open.

## Numeric and collection checkpoint — published 2026-10-22

**Measured source:** `bf681edcbbb59ef6f2d70a40df99758ccd66b98a` (candidate47).
**Verified source:** `37ab9fa440c947468f82139a4fc97d0e0fc3c159` (candidate48).
[4924 portable evidence files](performance-prepared-numeric-20261022.json) bind each observation to its actual
compiler, executable, workload and raw output. Source 48 removes one optional
Rust `must_use` annotation from source 47. All 21 emitted program Wasm modules
match byte for byte; native runtime images and runtime keys match. Both runtime
packages differ only in authenticated compiler-source framing, and their v4
manifests have matching validated source/package digests. **Timing samples stay
attributed to source 47; native and Test262 acceptance belong to source 48.**

**P081: a verified numeric Math.imul extension.** For exactly two ordinary
Number arguments, provisional static target knowledge selects a candidate.
After evaluating callee, receiver and arguments once in order, runtime checks
prove the concrete FunctionObject, immutable StandardBuiltin/MathImul code and
current Realm. The existing exact modulo conversion feeds Wasm `i32.mul` and
the signed Number result, avoiding generic argument storage and dispatch.
Patched/wrapped/proxied/foreign/coercing calls, other arities, spread/captured
arguments and tail calls retain generic behavior. The source regression checks
the actual exported script entry for direct/alias multiplication, excluding
bootstrap helper operations. No GC layout or helper ABI changes.

**P020 remains partial.** A prepared collection key owns its rooted value and
raw hash. Computed insertion carries it across callbacks but probes the current
table again after mutation, growth or GC. GroupBy, Set intersection/symmetric
difference and Map/Set deletion reuse this stable hash across their internal
operations. Object/Symbol identity hashes remain stable across revocation and
GC; string equality still compares contents. Ordinary define-own descriptor
lookup followed by absent-key append still computes the hash twice. Broader
multi-pass Set/growth work is separate; this checkpoint does not close P020.

The 21-case ABBA uses seven samples after ten warmups per process against
accepted44 (`e78c9e072`). Each cell is the median of two process medians.
The invocation includes setup, checks and fresh realm/bootstrap, not isolated
hash cost or a per-change causal effect. All outputs and module-cache hits
are checked. A separate three-sample/three-warmup screen covers the same cases.

| Whole invocation | Before44 | Measured47 |
| --- | ---: | ---: |
| map-direct-long-fresh.js | 49.023 ms | 45.888 ms |
| map-direct-long-hit.js | 61.129 ms | 74.778 ms |
| map-direct-numeric-fresh.js | 9.809 ms | 10.138 ms |
| map-computed-long-fresh.js | 64.387 ms | 56.893 ms |
| map-long-fresh.js | 45.233 ms | 52.510 ms |
| map-numeric-fresh.js | 14.581 ms | 13.997 ms |
| set-long-fresh.js | 46.821 ms | 45.152 ms |
| property-long-fresh.js | 84.151 ms | 85.180 ms |
| integer.js | 8.037 ms | 6.371 ms |
| objects.js | 18.602 ms | 21.750 ms |
| retained-objects.js | 1,482.765 ms | 1,476.889 ms |
| imul-direct.js | 33.127 ms | 25.144 ms |
| imul-alias.js | 19.231 ms | 8.877 ms |
| imul-patched.js | 108.277 ms | 104.735 ms |
| imul-coercion.js | 17.156 ms | 16.557 ms |
| map-groupby-long.js | 99.925 ms | 91.774 ms |
| object-groupby-long.js | 121.245 ms | 121.309 ms |
| set-intersection-long.js | 38.055 ms | 34.587 ms |
| set-symmetric-difference-long.js | 75.856 ms | 64.163 ms |
| map-delete-long.js | 57.150 ms | 49.815 ms |
| set-delete-long.js | 56.868 ms | 45.422 ms |

Direct and aliased imul medians fall **24.1% and 53.8%**, with integer **20.7%
lower**. Several collection cases improve; ordinary object-grouping is flat.
The original cohort also records **22.3% slower Map hit, 16.1% slower Map put
and 16.9% slower object controls**. These observations are retained.
Targeted eight-process ABBAABBA checks give Map-hit median 63.043→64.169ms with
overlapping rounds; Map put 50.623→50.615ms, essentially flat; objects
18.967→17.259ms and integer 8.152→6.419ms. The first slowdowns were not repeated
at their original magnitude. This establishes neither a universal gain nor
statistical significance. Both original and follow-up observations remain raw.

The separate ordinary V8 comparison supplies the README: cold/cached startup
**1,166.159 / 799.041ms**, integer **6.004ms**, objects **16.802ms**, retained rows
**1,495.307ms**. Compared with diagnostic45's separate ordinary report,
cold startup and retained rows are slower; cached startup, integer and object
medians are lower. Different cohorts are not a
paired causal comparison. RSS remains host/process memory, not live JS heap.

**Correctness:** all twelve compiled semantic families and the dynamic-result
controls pass. They cover 361 numeric pairs, evaluation/throw order, mutation,
accessors, actual foreign realms, bound/proxy/revoked functions, BigInt/Symbol,
GC, mixed collection keys, callback clear/growth and duplicate/churn behavior.
Stock Node supplies compatible oracles; unavailable proposal/HTMLDDA APIs keep
their explicit existing repository assertions, without substituting polyfills.
All **3,575 native tests pass / 0 failed / 0 ignored** across seven complete
release targets, with exact inventories and unchanged source/linker fences.
Seven independently produced product/test runtime files match before storage
sharing. Source 47's earlier Intl preflight rejection ran no test bodies and
remains retained; all fifteen provenance recipes pass unchanged on source 48.

After native drain, **3,235 exact original Test262 executions** give **3,231
Success / 4 Bug / 0 Crash / 0 NotImplemented**. These cover complete Math, Map,
Set, Object.groupBy, Function.prototype, Proxy and call-expression families plus
the four original tracked failures. Four reused workers keep original budgets;
raw exit1 preserves the failures. Earlier broader acceptance remains attached
to its original source. The full-suite graph and wider default/all-feature
acceptance are unchanged.

Native verification samples reach **34,308,009,984 bytes** for the inherited group,
including tmpfs/file pages; there are zero OOM kills. Two live libtests,
256MiB scratch per process, the two-GiB Cargo reserve and retained-module cache
ceilings remain. Current dependencies and held native images stay available.
Completed artifact archives preserve exact SHA, sizes, modes and mtimes;
deduplication verifies compressed and raw bytes before immutable aliases.

**Iteration lessons:** initial45 attached the hook to an unused CallMethod path;
normal direct/alias P was unchanged. Activated46 then caught a missing Realm
local load during Wasm validation. Both attempts remain diagnostic, with no
transferred timing/native acceptance. The standalone unoptimized emitter validates
real parser/IR/Wasm using cached optimized dependencies; measured linking takes
0.500s and eight-fixture validation 4.436s. These are diagnostic checks, not CLI
execution or a complete library-build timing. The actual product48 build takes
**3m42s**, with both full runtime images reused from the build-module cache.
Source 47's optional annotation changed a globally pinned provenance input;
removing it avoids needless Intl regeneration. The first48 equivalence checker
also expected unchanged manifest provenance; its corrected continuation validates
the changed fields explicitly and reuses the already-passing 21 saved P checks.

The backlog now has **21 terminal decisions, eight partial/open outcomes and
131 proposed/unresolved hills** of 160. P081 is the narrow verified extension;
P020 and wider startup/string/hash work remain open. Rejected experiments are
decisions, not speedups.

## Map insertion checkpoint — published 2026-10-22

Measured compiler: `e78c9e0723414fd8af48de51034b71d3cce958d9`.
[1840 verified portable evidence files](performance-map-insert-20261022.json) retain actual static release
producers, frozen executable hashes, emitted Wasm, ordinary and paired raw
observations, semantic controls, native receipts and exact Test262 IDs. This
compiler's ordinary seven-sample/ten-warmup report supplies the README table.

**P020 remains partial; the value-only gap is closed.** `Map.getOrInsert` prepares
one hash with the rooted normalized key, then shares it through lookup and put.
Insertion still probes the owner's current buckets and preserves equality checks.
`getOrInsertComputed` retains its fresh post-callback path: callbacks can clear,
grow, delete/reinsert or insert the same key, and the computed value still wins.
Retaining a proven-stable hash across that callback boundary remains separate work,
as do groupBy, cross-owner Set operations and lookup/unlink deletion paths.
There is no GC layout, global layout or Wasm helper ABI change.

The ten-case ABBA uses seven samples after ten warmups per process, comparing
accepted `2cb9fc53d` directly with this compiler. Each cell is the median of two
process medians. Whole invocation includes key/container construction, checks
and fresh Wasm realm/builtin initialization; it is not isolated hash timing or
per-change causal attribution. All outcomes and overlapping rounds remain raw.

| Whole invocation | Before | After |
| --- | ---: | ---: |
| map-direct-long-fresh.js | 58.516 ms | 49.966 ms |
| map-direct-long-hit.js | 64.618 ms | 61.643 ms |
| map-direct-numeric-fresh.js | 9.362 ms | 9.315 ms |
| map-computed-long-fresh.js | 76.235 ms | 60.800 ms |
| map-long-fresh.js | 49.867 ms | 47.767 ms |
| map-long-hit.js | 37.199 ms | 35.540 ms |
| map-numeric-fresh.js | 14.853 ms | 14.396 ms |
| set-long-fresh.js | 46.123 ms | 46.912 ms |
| integer.js | 8.701 ms | 8.283 ms |
| objects.js | 18.678 ms | 16.931 ms |

Value-only long-key misses are descriptively **14.6% lower**; numeric misses are
roughly flat, while the Set control is 1.7% higher with overlapping observations.
The computed-path timing changes despite no new hash-sharing there: these
combined-product observations do not establish a hash-only or universal gain.
A separate three-sample/three-warmup screen passes every output check.

**Immutable harness sources are shared within one bounded worker session.**
The prelude store loads lazily once; every request still executes its selected
harness in a fresh Engine/Realm/Store, with its own exact durable snapshot.
The changed strict/include control checks an expected failure followed by fresh
globals, prototypes, Promise jobs and weak state; all three replies and identities
are checked both in the actual product protocol and the native integration test.

A separate ABBA uses the same 64 distinct pinned parse-negative IDs per native
run-list process, one worker, original 60s case budgets and the existing
64-request lifetime. Before process times are 2.618058 /
2.952172s; after 2.567852 /
2.633097s. Median pairs are
2.785115→2.600474s,
with overlap. This does not isolate harness-load latency or measure JavaScript
execution speed. Repetition adds no distinct conformance coverage. The original
diagnostic rejected an invalid ordinary-CLI argument before any case ran; its
logs remain, followed by a corrected diagnostic without rebuilding Rust or
repeating already-passed performance cohorts.

**Keep default Fast compilation.** The actual release policy probe admits the
matching optimized R into its destination Engine and compares Fast with
SizeOptimized in ABBA. Both modes use identical emitted program bytes, checked
outputs and seven measured invocations after ten warmups. Separate traced
controls prove image admission; tracing observations are excluded from scores.

| Whole invocation | Fast | SizeOptimized |
| --- | ---: | ---: |
| integer.js | 7.604 ms | 9.680 ms |
| objects.js | 17.485 ms | 16.826 ms |
| retained-objects.js | 1,462.288 ms | 1,409.441 ms |

The integer control regresses, and object/retained rounds overlap. First-invocation
module/total spans are retained but combine R admission and P compilation; no
break-even isolation or universal policy winner is established. The policy also
combines optimization and register allocation; it does not close P155 separately.
P154 stays diagnostic/open and the product default remains Fast.

The ordinary V8 comparison records cold/cached startup **1,375.013 / 872.484ms**,
integer **8.441ms**, objects **19.892ms**, retained rows **1,398.560ms**.
Cold/integer/object observations are higher than the preceding ordinary report;
retained rows are lower. Both reports remain source-bound, separate cohorts;
these movements are not attributed to prepared hashes. The actual CLI is
**290,592,424 bytes / 277.13MiB**, DWARF stripped with symbol table retained.

**Correctness: 4,035 native passed / 0 failed / 0 ignored**, across ten complete
release targets in an exact disjoint one-plus-nine union. The first 454-test
Test262 library scope passed; other targets were deferred by the two-GiB disk
reserve. The runner rejects feature-root retry mode before execution, so that
attempt is retained and nine fresh same-source scopes complete the union.
No passed body was rerun, discarded or converted to a different verdict.
All ten compiled semantic families pass. Seven independently produced runtime
payloads/manifests match before working files are shared; immutable originals
have verified round-trip archives. Source and linker fences remain unchanged.

After native drain, **1,173 exact original Test262 executions** return
**1,169 Success / 4 Bug / 0 Crash / 0 NotImplemented**. These include all selected
original Map/Set IDs and exactly the four previously tracked failures, using the
frozen product and four reused workers at original budgets. Raw process exit 1
remains those failures. The preceding wider 15,899-ID replay remains bound to
`2cb9fc53d`; neither is a fresh full-suite/all-feature campaign. The chart stays.

Whole inherited-group memory samples reach **34,346,147,840 bytes**, including tmpfs,
file pages and other processes; both native receipts record zero OOM/OOM kills.
Two libtest processes, 256MiB scratch per active process, the two-GiB build reserve
and 512-entry/2GiB retained-module ceilings stay intact. Current/held native images
and fresh dependencies stay available. Historical archival preserves all bytes;
lower-layer archived sizes need not equal physical free space, so admission uses
actual filesystem free bytes. Correctness runs are not product memory benchmarks.

Iteration: the product build takes **6m12s**, with runtime production **148.41s**,
opt1/opt3 AOT Rust units **73.61 / 94.85s**, Test262 **24.98s**, Engine **14.93s**
and CLI **9.87s**. Native compilation still reports **1,019 hits / 1,165 misses**
for NativeHost and **1,270 / 914** for Portable, exactly the preceding counts,
despite a small R byte change. The function tier is shared by builds and test P
modules with a 1GiB ceiling and a 70% prune target. Contention/eviction is a
hypothesis; stable dependency identity does not establish identical Wasmtime
stencil keys. Measure actual key/prune events before changing cache budgets.
Different Rust profiles/features still prevent assuming a crate split shares
compiled units. These are changed-source observations, not an isolated speedup.

The backlog remains **20 terminal decisions, eight partial/open outcomes and
132 proposed/unresolved hills**. Rejected experiments are decisions, not speedups.

## Key and argument checkpoint — published 2026-10-22

Measured compiler: `2cb9fc53d514669a1f47eb37feb96add24d2912b`.
[693 verified portable evidence files](performance-key-arguments-20261022.json)
retain frozen static release producers, raw emitted Wasm and observations,
semantic controls, native transcripts, source fences and preservation receipts.
This compiler's ordinary report supplied the README table at that checkpoint. The wider
[**15,899-execution Test262 replay**](performance-key-arguments-replay-20261022.json)
returns **15,895 Success / 4 Bug / 0 Crash / 0 NotImplemented**, preserving exactly
the original four tracked failure IDs. Four reused case workers ran after native
tests drained, with original per-case deadlines and frozen executable identity.
The earlier 693-file measurement snapshot records replay pending; this separate
final publication closes that gate. No fresh full-suite/all-feature result is claimed.

**P020 is partial, with measured execution gains.** Map/Set insertion and ordinary
property append retain a hash tied to the rooted canonical key through internal
lookup and insertion. Buckets use current capacity after growth; hash matches
still require equality. No callback lies inside that reuse interval. The separate
non-callback `Map.getOrInsert` outer lookup still repeats hash work in its inner
put; completing that path needs its own evidence. The computed-callback route
keeps fresh lookup/hash work after callback reentrancy.

**P078 extends its existing empty-storage route.** Runtime-empty internal argument
vectors use the canonical immutable empty ValueArray. Iterator and length-coercion
effects still occur. Mutable JS arguments, rest arrays, bound-call results and
constructor/generator state remain independent. This is not another completed hill.
No GC layout/global-layout or Wasm helper ABI changes.

The direct fifteen-case ABBA uses seven samples after ten warmups per process.
Each cell is the median of its two process medians, against accepted `1e2978361`.
All key/array/container construction and checks remain inside timed invocation,
along with Wasm realm/builtin initialization. These combined-change comparisons
do not isolate hash-only or argument-allocation cost:

| Whole invocation | Before | After |
| --- | ---: | ---: |
| map-long-fresh.js | 65.135 ms | 55.338 ms |
| map-long-hit.js | 42.757 ms | 37.532 ms |
| map-numeric-fresh.js | 16.058 ms | 16.126 ms |
| set-long-fresh.js | 65.953 ms | 51.507 ms |
| set-long-hit.js | 44.072 ms | 35.427 ms |
| set-numeric-fresh.js | 14.537 ms | 13.516 ms |
| property-long-fresh.js | 112.415 ms | 95.054 ms |
| property-long-hit.js | 89.599 ms | 85.413 ms |
| property-numeric-fresh.js | 32.610 ms | 30.354 ms |
| call-runtime-empty-spread.js | 31.822 ms | 29.663 ms |
| call-runtime-empty-apply.js | 18.947 ms | 16.204 ms |
| call-runtime-two-apply.js | 19.998 ms | 19.447 ms |
| call-syntax-empty.js | 15.125 ms | 12.918 ms |
| integer.js | 7.802 ms | 7.602 ms |
| objects.js | 18.124 ms | 18.421 ms |

A separate three-sample/three-warmup screen retains all fifteen original outcomes.
Numeric Map and object controls are 0.4% and 1.6% higher in ABBA, with overlapping
observations; no universal or statistically significant improvement is claimed.
The new ordinary retained-object score, **1,604.245 ms**, is slower than the prior
ordinary **1,418.171 ms**. A direct retained-object ABBA gives **1,689.952→
1,579.330 ms** instead; both observations remain visible rather than assigning
an unproved cause. That separate cohort is not multiplied into other ratios.

Ordinary cold/cached starts are **1,178.550 / 881.710 ms**. A separate balanced
56-process startup comparison gives cold **1,237.046→1,305.263 ms** and cached
**899.148→932.300 ms**. Startup is descriptively slower there; no startup win is
claimed. Full loaded-executable hashing and R/package/native admission stay intact.
The actual static release CLI is **290,603,536 bytes / 277.14 MiB**, with DWARF
stripped and its symbol table retained. Child process RSS is distinct from
sampled invocation RSS, GC capacity and live JavaScript heap bytes.

Correctness: **3,572 native passed / 0 failed / 0 ignored** across seven complete
release targets, with unchanged source/linker. The three new compiled semantic
families cover long/equal-distinct/mixed keys, growth, ordering, symbols, coercion,
accessor descriptors, actual GC, empty iterators, abrupt completion, converted
array-like lengths, rest/arguments freshness and captures across GC. Six preceding
families also pass. Seven independently produced image payloads/manifests match;
their test hashes were checked before sharing mutable working copies. Immutable
product-image archives were verified before those writes could occur.

Native memory samples reach **34,356,269,056 group bytes**, with 3,446 max-pressure
events and **zero OOM / OOM kills**. Group usage includes tmpfs and file pages,
not just the tested JavaScript. Two libtest processes, original deadlines and
512-entry / 2 GiB retained-module ceilings remain unchanged. Verified archival
of old-source units and historical test images keeps the two-GiB Cargo reserve;
fresh dependencies are retained. These correctness runs are not memory benchmarks.

Iteration: the product build takes **6m54s** in this changed-source observation.
Cargo records **169.36s** in runtime-image production and **185.02s** across two
AOT Rust units: opt1 build-side **76.78s**, opt3 product **108.24s**. Their codegen
portions are 48.2s and 72.5s. They have different profiles and real graph roles;
matching one flag would not make them a shared unit. An isolated warm incremental
AOT experiment is a candidate, with its own artifact/disk costs; crate splitting
alone is not a measured improvement. The runtime producer spends about 164.6s
compiling changed NativeHost/Portable native functions; R emission/admission is
about 2.2s. Reuse must retain exact source/stencil identities.
NativeHost reuses 1,019 stencils and compiles 1,165; Portable reuses 1,270 and
compiles 914. Runtime argument-vector emission changes many shared bodies, so
this has fewer cache hits than the preceding three-function replacement edit.
It is not an isolated build-speed comparison or evidence that the cache is absent.
The backlog has **20 terminal decisions, eight partial/open outcomes and
132 proposed/unresolved hills**; rejected decisions are not shipped speedups.

## Replacement assembly checkpoint — published 2026-10-22

Measured compiler: `1e2978361aeb7b7a5ed5fbac2d246307693593fd`.
[933 verified portable evidence files](performance-replacement-20261022.json)
retain actual static release Cargo producers, frozen executable hashes, emitted
Wasm, raw observations, semantic controls, native receipts and exact Test262 IDs.
This previously supplied the README ordinary seven-sample/ten-warmup report.

**P107 is accepted with measured tradeoffs.** Literal and RegExp replacement
retain eight completed Strings in typed GC locals, spill additional pieces to
the existing list, check aggregate UTF-16 length and copy once into final backing.
Required coercions, exec/lastIndex effects, callbacks and named-capture Gets keep
their order. Empty/no-match handling avoids unnecessary assembly. No object
layout, global layout or helper ABI changes. Forced-GC inline/spill controls,
lone surrogates and exact catchable-u32-overflow controls pass.

Three comparisons remain separate; no ratios are multiplied across them:

1. Accepted `b59303ebe` → final: 15-case screen, three samples/three warmups.
2. Intermediate `9a971c238` → final: 15-case case-wise ABBA, seven samples/ten warmups.
3. Accepted `b59303ebe` → final: six short cases, eight rounds each, seven samples/ten warmups.

| Whole-invocation screen | Accepted original | Final |
| --- | ---: | ---: |
| Literal callback | 456.100 ms | 7.507 ms |
| Literal plain template | 510.698 ms | 9.272 ms |
| Literal `$` template | 533.704 ms | 137.577 ms |
| RegExp callback | 2,410.068 ms | 101.373 ms |
| RegExp plain template | 2,507.185 ms | 117.525 ms |
| RegExp `$` template | 2,857.313 ms | 425.046 ms |

The direct short check's two-match medians of round medians are
**45.926→45.548 ms**; this removes the observed short-control obstacle to the
first experiment. Rounds overlap and vary: no-match RegExp is 6.4% higher and
integer 0.9% higher in that cohort. No significance or universal-speedup claim.
Against the intermediate collector, ABBA RegExp `$`-template medians rise
**244.873→337.369 ms (37.8%)**, and literal plain-template medians rise 28.8%.
The inline roots are a tradeoff; the original long-output gains remain in their
separate screen. Input construction, callbacks, formatting and checksum work
are included, rather than timing only the final copy.

The ordinary report records cold/cached process medians **1,203.063 / 807.470 ms**
and integer/object/retained-row execution **8.014 / 18.082 / 1,418.171 ms**.
Cold start is slower than the preceding ordinary report. A new balanced ABBA
startup diagnostic, 14 fresh processes per image/mode, gives cold
**1,095.598→1,084.711 ms** and cached **763.558→810.802 ms**, with overlapping
ranges. That separate diagnostic neither replaces the ordinary table nor
establishes an isolated cause for either difference. Full executable hashing
and independent runtime/package/native admission remain mandatory.

**P004's fixed-overhead controls are complete.** All four print checked output;
empty and loop controls retain the same heap-backed IIFE framing, and the
scalar-return control calls a function returning 364. Two runtime orders use
seven samples after ten warmups:

| Wasmtime control | Wasmtime-first report | V8-first report |
| --- | ---: | ---: |
| Empty body plus checked print | 1.165 ms | 1.556 ms |
| Empty 10,000-step loop plus print | 2.473 ms | 1.876 ms |
| Scalar expression plus print | 2.245 ms | 1.311 ms |
| Scalar-return function plus print | 1.968 ms | 1.798 ms |

These are minimally observable programs, not a pure bootstrap span. Their
variation illustrates why small deltas need repeated controls. Timed Wasm main
includes a fresh GC Realm, intrinsics and global-object initialization; host
Store/linker setup and R/P instantiation are separate. No control value is
subtracted from published raw scores. The original scalar-expression workload
is retained; the deterministic corpus gains a distinct scalar-return entry,
with exact measured source bytes and all three corpus checks passing.

The frozen compiler passes **3,569 native tests / 0 failed / 0 ignored** across
seven targets, six compiled fixture families and the exact Scriptw overflow
boundary. A two-GiB disk-reserve deferral stopped three unexecuted scopes; four
passed scopes remain in their original receipt. Their same-source, same-linker
continuation covers exactly the three deferred scopes, with a fresh ordinary
feature-graph proof. No passing body was rerun or replaced. Seven product/test
image payloads/manifests are byte-identical. Immutable original images remain
round-trip verified in archives, separate from Cargo's writable working files.

The **410-execution Test262 replay returns 406 Success / 4 Bug / 0 Crash /
0 NotImplemented**. It covers every pinned execution in String replace,
replaceAll and RegExp `@@replace`, related staging/Annex B cases and the four
original failures. All failures are exactly those original tracked IDs. This
is not a fresh full 102,994-case or all-feature native campaign; the chart is
unchanged.

Native verification keeps two simultaneous processes and original deadlines
and cache limits. Sampled inherited-cgroup usage reaches 32 GiB; kernel pressure
reclaims pages, with **zero OOM or OOM-kill events**. These samples include tmp
files, caches, archive/staging chores and unrelated work; per-PID RSS counts
shared mappings. They are not live JavaScript heap measurements. The long-lived
CLI test processes reach several GiB; investigate more sequential chunks at the
same two-process ceiling before increasing parallelism.

The actual product build takes **4m36s**: emitter opt1/opt3 units 72.08/102.13 s,
runtime-image production 42.63 s. Each image reuses **2,181 function stencils**
and compiles three; NativeHost/Portable native compilation takes 19.17/18.81 s.
This identifies duplicated Rust emitter compilation as the larger remaining
iteration cost. Source/cache state differs from earlier builds, so no isolated
same-source build-speed gain is claimed.

The [initial collector](performance-replacement-probe-20261022.json) and
[64-unit-prefix revision](performance-replacement-prefix-probe-20261022.json)
remain rejected diagnostic history. Both passed their compiled controls but
failed to remove short-match overhead. Historical `regexp-one-match.js` actually
matched twice; its bytes/results remain intact, and the current corpus keeps
separate true one-match/two-match cases. No broad acceptance is assigned to
those rejected products.

## Release packaging checkpoint — published 2026-10-22

Measured compiler: `b59303ebe953e5f07ad7aebba467211dd2ac88ff`, source fingerprint
`db6e85fc67799d1f95ea938c63631cfa3e89299db30c8533d2db9ee7c36c9d9a`.
It previously supplied the README table with its actual release
Cargo CLI and runner, seven samples and ten execution warmups. The
[portable evidence](performance-kept-packaging-20261022.json) retains ordinary
reports, source-frozen native acceptance, compiled semantic controls, paired
execution controls, Cargo producers/timings and reproduction harnesses. Each
embedded gzip+base64 asset is independently length/digest verified.

The CLI is **290,489,840 bytes (277.03 MiB)**, **202.07 MiB smaller** than the
preceding accepted release. Rust debug information is stripped from the CLI and
examples; symbols and compiled panic locations remain. Full loaded-executable
SHA-256 and independently validated compiler/profile-bound cache identities are
preserved. The product remains statically linked to its runtime image archive;
the test-only shared linker is used only for libtest executables.

Ordinary cold/cached medians are **1,059.284 / 818.785 ms**, versus V8
**33.660 / 35.440 ms**. These are separate ordinary reports, not paired effects.
The earlier allocated-section-equal strip experiment below isolates the startup
benefit. One 1,645.712 ms cached-start outlier remains in the raw samples.
Process memory is essentially unchanged: cold/cached medians **382.83 / 349.91
MiB**, retained-row whole-child peak **392.82 MiB** and median invocation-sampled
RSS **373.34 MiB**. V8's retained whole-child peak is **52.88 MiB** and its
invocation endpoint median is **49.13 MiB**. GC capacity is still 64 MiB;
none of these measures is live JavaScript heap usage.

The three ordinary execution medians are **8.363 / 18.966 / 1,470.230 ms**.
Short-run variation prompted separate seven-sample/ten-warmup case-wise ABBA
controls against the prior frozen runner. Integer cohort medians are
**10.038→7.933 ms** and object medians **19.654→18.174 ms**. These controls do
not show a consistent execution regression; they do not establish an isolated
algorithm gain or statistical significance. The README retains the actual
ordinary scores rather than substituting favorable paired observations.

Native acceptance is **3,568 passed / 0 failed / 0 ignored**: AOT 444, engine
974, builtins 541, language 324, realm/modules 428, CLI 854 and runtime-image 3.
Source and linker remained unchanged throughout; all ignored controls were
explicitly enabled. Seven product/test image payloads and manifests are verified
byte-identical. Six actual compiled CLI semantic fixture families pass. The
**9,411-execution Test262 replay returns 9,407 Success / 4 Bug / 0 Crash /
0 NotImplemented**, with exactly the original four upstream-tracked failures.
[Final replay evidence](performance-kept-packaging-replay-20261022.json) supplements
the earlier immutable native/measurement snapshot, which records replay pending.
Four workers ran after native verification drained, with existing per-case
deadlines retained. This adds 204 array-expression executions to the preceding
9,207-case selection. This affected replay and scoped native checkpoint do not
refresh full-suite counts; all four tracked failures remain open.

The product build took **8m44s**. Cargo timings identify repeated AOT LLVM work
and two Intl feature/profile variants as major costs; runtime-image production
was only **5.04s** with its exact-input native cache warm. Overeager detachment
of unchanged preserved Intl outputs caused about **143s** of avoidable rebuild
work. A next-batch transparent rustc wrapper has passed a tiny real-Cargo
fresh/rebuild/fresh probe: it preserves unrelated fresh dependencies and detaches
only an actually rebuilding unit's verified output aliases. Bounded four-slot
LLVM compilation is also available. Neither prototype has yet been timed on a
Scriptw product build. CLI Intl tests independently repeat SDK/CLI P emission;
existing exact raw-R caches can be shared only within matching profile/selection
identities. Slow test spans have not been attributed to startup alone.

The next replacement experiment has twelve source fixtures in
`crates/scriptw-cli/tests/fixtures/v8_replacements`. They use the existing
`benchmark-v8.py --fixtures … --skip-startup` interface and exact-output checks;
whole-workload timings include receiver/template construction and output
checksum. Callback and plain/dollar-template cohorts separate whole-result
assembly from substitution work; deletion/no-match/single-match controls check
the small cases. No measurements or acceptance are claimed for that source yet.

The opt-in `scripts/test-shared-runtime-images/preserve-output-rustc` wrapper
composes the existing quota-bounded LLVM jobserver without altering rustc
arguments. `SCRIPTW_PRESERVED_OUTPUT_INVENTORY` names a JSON object with absolute
`cargo_target_dir` and `files` entries (`original`, `storage`, `sha256`);
`SCRIPTW_OUTPUT_PROTECTION_LOG` names an append-only intent journal. Every owned
output alias for the rebuilding rustc unit is validated before any is detached.
The preserved payload stays intact, unrelated dependency aliases stay attached,
and queries remain literal. The inventory must contain only task-owned aliases.
Build-script-produced runtime image outputs still require separate protection;
this wrapper only intercepts rustc. `SCRIPTW_TEST_CODEGEN_JOBS=auto` requires
serial Cargo and a finite inherited cloud budget. Twelve ownership/argument controls and
a tiny real-Cargo probe pass; an actual Scriptw build has not yet measured the
iteration-time benefit.
The existing native `test-rustc` wrapper also admits that optional inventory;
its existing test/incremental policy stays intact. This avoids protecting product
outputs at the cost of stale test-graph dependencies or the reverse. Metadata-only
compilation leaves the unit's unwritten linked outputs attached.

## Second execution batch — published 2026-10-22

Measured compiler: `86518905134e401bd9533697e1d93007558e54cf`, product source
fingerprint `7e96c784036365a6f8f7da1a84bcbdb841e17feb847ceda76800714cc8e93266`.
This source's seven-sample, ten-warmup Wasmtime/V8 report is retained as the
preceding baseline. The [portable evidence](performance-second-wave-20261022.json)
retains complete
ordinary reports, the 61-case corpus, paired controls, rejected variants,
Cargo producers, native acceptance and host profiles. Assets retain the verified
gzip+base64 format described below; executable images, archives and private
caches are excluded.

Native acceptance is **3,567 passed / 0 failed / 0 ignored** across AOT library
444, engine library 974, builtins 541, language 324, realm/modules 428, CLI 853
and runtime-image library 3. All ignored controls were explicitly enabled.
The two checkpoint receipts have identical source maps and unchanged source;
product/test raw, native, compressed and manifest image bytes match. This is
scoped default-feature verification, not full-workspace/all-feature acceptance.
Ten compiled semantic fixture families and all 61 ordinary corpus cases pass.
The final-source **9,207-execution Test262 replay returns 9,203 Success / 4 Bug /
0 Crash / 0 NotImplemented**, with exactly the original four upstream-tracked
failures and no new regressions in this campaign.
[Final replay evidence](performance-second-wave-replay-20261022.json) supplements
the earlier native/measurement snapshot. The intermediate F replay remains
historical. The README graph still describes its existing reconciled full-suite
checkpoints; this affected-case campaign does not refresh full-suite counts.

The kept batch shares immutable empty private storage, shortens concrete array
length and normal-call completion paths, bulk-copies repeat/pad/partial slices,
reuses full Strings, and shares identical-operand descriptor snapshots. Binding
payload flattening, signed-range conversion and runtime Number-tag branches
were rejected and removed. Layout remains 15. Paired short controls overlap:
F→final cohort medians are integer 7.775→7.667 ms, objects 19.384→17.732 ms and
retained rows 1,506.528→1,469.976 ms. These descriptive diagnostics use a separate
cohort from the README; they do not establish isolated per-change contributions.

Here C denotes `621ba89ca` and F denotes `a5b4a6c5d`.
The preceding C→F ABBA string controls establish the larger changes: repeat
169.226→19.993 ms, padEnd 4.460→1.498 ms, trimStart 24.120→1.617 ms, trimEnd
24.129→1.390 ms, full slice 6.691→1.475 ms and partial slice 6.860→2.618 ms.
The final ordinary corpus records 23.834 / 1.847 / 1.472 / 1.600 / 1.384 / 2.352 ms
for those cases. These are separate source-bound cohorts; do not multiply ratios
or call them a direct C→final paired effect. Final repeat/partial-slice observations
are slower than the matching F ordinary report. No universal speedup is claimed.

Portable native storage now occupies **21,713,669 bytes**, decoding to
**80,992,360 bytes**. NativeHost/raw package bytes and independent manifest,
digest and actual destination-Engine checks remain required. A weak lease shares
concurrent loads and releases decoded storage after Wasmtime copies it into its
own Module mapping. Final CLI size is **502,376,224 bytes**, **57.12 MiB smaller**
than the preceding accepted batch's 562,274,800 bytes.

The retained-row run records median invocation process RSS **373.68 MiB** and a
separate whole-child high-water peak **392.80 MiB** (one observation). The preceding
accepted report recorded 439.91 / 439.73 MiB; F's retained decoded buffer recorded
454.86 / 454.66 MiB. These are combined-batch process measurements with different
boundaries, including runtime/caches and, for the whole child, compilation and
warmups. They are not live JS heap sizes. GC capacity remains 64 MiB. V8's final
invocation RSS endpoint is 49.38 MiB; its whole-child peak is 53.12 MiB. Those
boundaries must not be merged into one heap comparison.

Four separately instrumented cached CLI profiles attribute **64.38–74.32%** of
65–74 user CPU samples per child to SHA-256, with no lost samples. Full loaded
executable binding currently hashes approximately 500 MB once per fresh process.
This is whole-child host attribution, not ordinary startup timing or a JS-span
profile. A separately labelled debug-stripped copy is prepared, with allocated
ELF code/data equality and distinct full executable identity verified. The
[completed startup diagnostic](performance-startup-strip-20261022.json) records
14 fresh processes per variant/mode in ABBA order (seven per round):

| Boundary | Full parent | Derived debug-stripped copy |
| --- | ---: | ---: |
| Identity binding | 376.835 ms | 221.256 ms |
| Cold application start | 1,171.645 ms | 1,018.480 ms |
| Cached application start | 923.743 ms | 739.889 ms |

Both variants retain full loaded-executable hashing, separate independently
primed application caches and embedded-native admission. All allocated ELF
sections match. Size falls 502,376,224→289,166,424 bytes (203.33 MiB); process RSS
does not materially change. These derived-copy diagnostics are separate from
ordinary V8 scores. Rust DWARF line tables disappear; global symbols, compiled
panic locations and Wasm code/data remain. Ordinary producer/hash checks and
compiler-bound cache identity remain intact.

The next source batch keeps release CLI/example debug stripping. Its combined
P017/P038 literal experiments were built and measured, then removed because they
did not establish a throughput gain. [Exact decisions and 216 verified assets](performance-literal-decisions-20261022.json)
retain the 14-case screen, ten-case ABBA confirmation, six compiled semantic
families, actual Cargo producers and 24 unique Wasm programs. Object nine-key and
33-key medians rise 17.823→19.393 / 47.447→50.464 ms; four-element arrays are
9.917→10.109 ms and retained rows 1,369.244→1,395.373 ms. These descriptive
Wasmtime-only cohorts are separate from ordinary V8 scores and do not isolate
each combined change. The packaging-only product and scoped native acceptance
and its completed expanded Test262 replay are now published above.


## First execution batch — published 2026-10-22

Measured compiler: `621ba89ca04a3522192b97afdd2c9fae617009a7`, product source
fingerprint `31caadf8de6aa83eb3581a6ba0a49e9d8147d4f2cc43c490abb72b0dc306bf76`.
The historical seven-sample, ten-warmup table is retained in the evidence.
[Portable evidence](performance-first-wave-20261022.json)
retains the ordinary reports, checked comparisons, rejected experiments, exact
Cargo producers, native checkpoint, original-case replay and instrumented profiles.
Assets use the same verified gzip+base64 format described below; ELF images and
private cache payloads are excluded. Report sources remain their actual tested
commits, independently of this later documentation commit.

NativeHost and Portable now have **separate** matching optimized runtime R
images, SpeedAndSize/Backtracking, with profile-specific raw keys and v4 manifests.
Generated P keeps Fast/None/SinglePass. Admission is lazy per profile and always
uses the actual destination Engine; no mutable Store or realm is shared.
The startup prime receipt proves NativeHost admission. The separate Portable
trace and runtime-key proof bind the execution corpus to actual Portable image
admission; the startup flag alone does not prove that path.

In the matched seven-sample confirmation against `3b1a3427f`, integer execution
fell from 17.481 to 8.109 ms, 500 objects from 44.053 to 18.275 ms, and retained
rows from 1,904.600 to 1,385.741 ms. These describe the combined concat/runtime
batch, with no general JS score or significance claim. A separate focused ABBA
concat study retains the earlier compiler's 3.17× and 13.47× gains on large-part
inputs. The numeric Array own-data experiment was rejected under the preceding
unoptimized Portable runtime policy and removed; its raw failed attempt remains.

The image tradeoff is explicit: the CLI grew from 440,628,904 to 562,274,800 bytes
(**116.01 MiB**). In a same-session before/current/current/before startup check,
14 observations per image/mode gave medians 1,137.973 → 1,195.838 ms cold and
904.560 → 926.627 ms cached. This accepts a measured startup/image cost in return
for execution gains; it does not call startup improved. Samples retain ranges
and round order. The main table is the separate paired Wasmtime/V8 run.

Retained-row whole-child peak RSS was **439.73 MiB** versus V8 **53.21 MiB**,
one observation per process. The invocation's median sampled process maximum
was **439.91 MiB**; V8's median endpoint was **48.96 MiB**. Different measurement
boundaries and kernel/proc sampling need not agree exactly. GC capacity remains
64 MiB and is not live JS memory. No allocation, live-byte, collection or pause
counters are claimed.

The separate loaded-code profile collected **670 user CPU samples, zero lost**.
About **59%** of self samples name copying-GC functions; ordinary property
read/find helpers account for about 3.58% each. Actual R/P function names now
resolve through the opt-in map, bound to loaded text and Wasm hashes.
This is whole-child attribution including compile/warmup/hash work, not a claim
that 59% of the execution span is GC. Four map-off/on/on/off observations retain
instrumentation controls; their overlap does not establish zero profiler cost.

The ordinary native checkpoint passes **2,687 tests**: engine library 973,
builtins 541, language 322 and CLI 851, including their ignored controls.
The source-bound Test262 replay is **182 Success / 4 Bug** across 186 original
executions, with the same four upstream-tracked failures and no new failures.
The replay compiler is `4fe934c4b`, with the same product fingerprint as `621ba89ca`;
the latter changes a Rust test only. This is neither a fresh full Test262 run
nor full-workspace/all-feature acceptance. The separate 85-workload Portable
correctness run competed with native verification: its timings are excluded.

## Performance and memory baseline — 2026-10-21

Measured source: `cbbd9b17039c595d470b5bc140b4ab7a1f2beea9`. Ordinary default-feature opt3 release;
seven measured observations per timing cell after ten execution warmups.
Both runtimes used one pinned CPU on the AMD EPYC 9V74 cloud machine, with a
32 GiB memory limit and four-CPU quota. Linux 6.18.55 x86_64 / glibc 2.41;
Wasmtime 47.0.0; Node 24.19.0 / V8 13.6.233.17-node.51.

| Measurement | scriptw / Wasmtime median | Node / V8 median |
| --- | ---: | ---: |
| Cold start | 1,117.891 ms | 31.094 ms |
| Cached start | 938.986 ms | 36.474 ms |
| Execute: 10,000 integer steps | 19.393 ms | 2.584 ms |
| Execute: 500 objects | 51.342 ms | 0.061 ms |
| Execute: 8,192 retained objects | 2,118.118 ms | 3.575 ms |

| Whole-child high-water RSS | scriptw / Wasmtime | Node / V8 |
| --- | ---: | ---: |
| Cold start (median) | 384.16 MiB | 27.12 MiB |
| Cached start (median) | 350.95 MiB | 27.12 MiB |
| Retained-object run (one process) | 558.05 MiB | 53.12 MiB |

Every checksum matched. Every measured Wasmtime execution hit the native module
cache, and startup priming confirmed admission of the embedded precompiled
runtime. Wasmtime uses a fresh Store; V8 uses a fresh context. Source compilation
and Store/context setup are outside the execution timer. The execution cache
was conditioned before this run; cold application startup caches remain unique
and empty. OS caches are not flushed.

Startup measures the CLI's NativeHost profile, whose embedded runtime is already
optimized with SpeedAndSize and Backtracking register allocation. The execution
driver emits Portable artifacts through `profile_script_compilation`; that
different source-compilation profile does not use the embedded NativeHost image.
Its runtime follows ordinary native compilation/cache admission for its profile.
Keep these boundaries distinct when attributing execution costs or comparing
compiler policies; the [performance backlog](../performance-hills.md) records
the matching-image experiment as P153.

[Source-bound evidence](performance-memory-baseline-20261021.json) contains the
full report, every warmup and measured observation, all child logs, workload and
emitted Wasm bytes, Cargo producers/build log, harness bytes, machine limits and
validation logs. Assets use deterministic gzip+base64 with original and compressed
SHA-256. Executable ELF images and private caches are excluded; actual executable
hashes and producer records remain in the reports.

Whole-child peaks include compilation, warmups, caches and host work. During the
retained-object invocations, Wasmtime's median sampled process RSS maximum was
**441.69 MiB**; V8's median process RSS endpoint was
**48.88 MiB**. A sampled maximum and an endpoint have different boundaries.
Wasmtime GC heap capacity was **64 MiB**, while V8 process-wide used heap was
**12.87 MiB**; those are different metrics, not comparable live JS heap sizes.
The runner explicitly reports unavailable Wasmtime live-byte, allocation and
collection/pause counters rather than inventing them.

The complete run took **59.6 seconds**. Two matched three-sample/three-warmup
integer runs took **3.0 and 3.2 seconds** after the initial **39.1-second**
execution-cache conditioning report. Offline comparison and publication
round-trip validation pass. The matched pair uses the same compiler source:
it demonstrates the short iteration workflow and observed variation, not an
optimization gain. The 8,192-object workload, at about 2.12 seconds per invocation,
is the largest absolute execution-time target in this small corpus.

Validation includes the completed release build, 13 standalone Rust fingerprint
controls, final module-boundary checks and three source-guard mutation controls.
The full 703-test tooling checkpoint predates the build-coherence guard; its exact
source and logs are retained alongside the subsequent focused controls. These
measurements do not establish full native, Test262 or calibrated T25 acceptance.

The initial `target/performance-memory-baseline-20261021` run remains a diagnostic,
not this healthy baseline. A shared Cargo target had reused an old custom-build
executable with a stale compiler-input inventory, rejecting the embedded runtime
and recompiling it at cold startup. Rebuilding only that custom-build executable
restored source/image identity. Current build scripts attest their fingerprint
implementation against the checkout. Earlier process RSS accounting could also
inherit the Python launcher's high-water floor; the current runner uses a small
GNU time supervisor to measure the actual runtime child.

To unpack and verify the published assets for offline report comparison:

```sh
python3 - <<'PY'
import base64, gzip, hashlib, json
from pathlib import Path
bundle = json.loads(Path('docs/rust-rewrite/performance-memory-baseline-20261021.json').read_text())
root = Path('target/performance-evidence-20261021')
root.mkdir(parents=True, exist_ok=False)
for asset in bundle['assets']:
    packed = base64.b64decode(asset['data'], validate=True)
    assert hashlib.sha256(packed).hexdigest() == asset['gzip_sha256']
    raw = gzip.decompress(packed)
    assert len(raw) == asset['bytes'] and hashlib.sha256(raw).hexdigest() == asset['sha256']
    path = root / asset['path']
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)
PY
python3 -B scripts/benchmark-v8.py compare \
  target/performance-evidence-20261021/runs/iteration-before/report.json \
  target/performance-evidence-20261021/runs/iteration-after/report.json
```

## Measured cloud baseline — 2026-10-20

Measured source: `c5053c9d4ed1ce961f66eff8cd0d721b39108ffd`. Ordinary default-feature opt3 release,
with no product oracle dependency; seven measured samples in every cell.
AMD EPYC 9V74 80-Core Processor; 32 GiB cloud memory limit; 4-CPU quota.
Both runtimes used the same one pinned CPU.
Node 24.19.0 / V8 13.6.233.17-node.51; Wasmtime 47.0.0.
Kernel/platform: `Linux-6.18.55-x86_64-with-glibc2.41`. Architecture: `x86_64`.

| Measurement | scriptw median [min, max], ms | V8 median [min, max], ms |
| --- | ---: | ---: |
| Cold start | 1221.570 [1124.999, 1449.517] | 32.789 [32.268, 64.987] |
| Hot start (disk cache) | 920.607 [869.768, 1024.726] | 35.565 [32.464, 74.314] |
| Execute: 10,000 integer steps | 17.020 [16.040, 17.523] | 2.400 [1.684, 2.552] |
| Execute: 500 objects | 44.490 [42.013, 62.516] | 0.044 [0.041, 0.067] |

All checksums matched. Integer output: `730532261`; object output: `149600`;
startup output: `scriptw-startup-ok`. Every measured Wasmtime execution hit
the in-process native module cache. [Source-bound evidence](v8-baseline-20261020.json)
contains the complete measurement report, all nanosecond samples and warmups,
process logs, Cargo producers, harness/fixture bytes and machine limits.
Assets use deterministic gzip+base64 with original and compressed SHA-256.
Executables are hash-bound to their actual Cargo producers; ELF images are
not embedded. The first rejected build and its fixed-width wire-format repair
are retained separately, along with the subsequent Intl identity repair and
sparse-fixture admission failure. The main baseline completed without failed samples.

The shared runtime image is compiled during the Rust build and reused even
for a cold application cache. The generated program modules use Wasmtime's
ordinary Fast/Cranelift `OptLevel::None` policy. The CLI endpoint includes its
outcome diagnostic, while Node only prints the checked program output.
Startup spans include the driver's receipt/log setup and child-process wall time.
Execution includes Scriptw realm/bootstrap/completion work; fresh Store and
V8 context setup sit outside the measured span. Warmups on fresh V8 contexts
do not guarantee the same JIT tier in every measured context.
These boundaries make this an explicit product baseline, not isolated
instruction-throughput or calibrated T25 acceptance.

## Current performance and memory tooling

The [performance backlog](../performance-hills.md) owns proposed optimizations,
their priorities, semantic controls and the first measurement batches.

The v2 runner extends the historical timing harness with memory observations,
phase timings, retained emitted Wasm, workload selection and checked comparisons.
The 2026-10-20 table is the historical v1 result. The 2026-10-21 results above
use v2 instrumentation; these schemas cannot serve as each other's performance
baseline.

Requires Linux, GNU `time`, Node and the Rust toolchain. Use `--time PATH` for
a GNU time executable outside PATH. Use the release/default-feature product CLI
and the companion runtime runner.
Commit the coherent source batch before building: both executables must identify
that exact commit. Keep the Cargo JSON build log. Run builds and tests before
benchmarking, then measure on an otherwise idle machine. The cloud launcher uses
the inherited machine limits; the benchmark pins both runtimes to one allowed CPU.

```sh
mkdir -p target/performance-build
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  cargo build --locked --offline --release -p scriptw-cli \
  --bin scriptw --example runtime_baseline --jobs 1 --message-format=json \
  > target/performance-build/cargo.jsonl
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  python3 -B scripts/benchmark-v8.py \
  --scriptw target/release/scriptw \
  --runtime-runner target/release/examples/runtime_baseline \
  --build-log target/performance-build/cargo.jsonl \
  --output-dir target/performance-before --samples 7 --warmups 10 --idle-machine
```

If `CARGO_TARGET_DIR` is set, use its actual executable paths in these commands.
The output directory must be new. Default startup priming enables compilation
trace outside the measured samples and requires a confirmed embedded-runtime hit.
A stale compiler/image source stamp fails here with retained diagnostics, before
spending time on cold samples. Both build-script producers also verify that their
compiled fingerprint implementation matches the current checkout. If that check
rejects a shared target directory, remove only the affected release build artifacts
with `cargo clean --release -p scriptw-engine -p scriptw-runtime-image`, then rebuild;
there is no need to clean third-party compiler dependencies. The runner keeps an incomplete report and logs
on failure; a failed checksum, trap or timeout fails the benchmark. It records
machine/cgroup limits, one-CPU affinity, runtime versions, source and executable
identities, exact workload bytes and output checksums. JavaScript still follows
the ordinary Rust compiler to generated Wasm and the product Wasmtime policy.

For a shorter optimization loop, select an existing manifest entry and omit
startup. Use the same options for before/after comparisons:

```sh
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  python3 -B scripts/benchmark-v8.py \
  --scriptw target/release/scriptw \
  --runtime-runner target/release/examples/runtime_baseline \
  --build-log target/performance-build/cargo.jsonl \
  --output-dir target/performance-objects-before \
  --case retained-objects.js --skip-startup --samples 3 --warmups 3 \
  --execution-cache target/performance-execution-cache --idle-machine
```

The explicit execution cache reuses the ordinary checked runtime/function/module
caches across runs. It does not affect isolated cold/cached startup measurements.
Compilation and warmup observations remain outside the execution summaries.

Generate the attribution corpus to vary retained row counts, widths and read
passes, and cover call/property/string/collection/JSON families:

```sh
python3 -B scripts/performance_corpus.py --output-dir target/performance-corpus \
  --rows 256,1024,4096 --widths 4 --passes 0,1,4
```

Pass `--fixtures target/performance-corpus` to the benchmark, then select entries
such as `--case rows-n1024-w4-p0.js --case rows-n1024-w4-p4.js`.
These are whole-invocation measurements: construction remains included in every
case. Increasing read passes supports marginal-cost attribution; it is not a
separately timed traversal-only span. Generated checksums are independently
validated in normal V8. Use `--first-runtime v8` to reverse the recorded runtime
order in paired repeatability runs without changing workload or sample plans.
After a complete report, derive checked phase attribution with
`python3 -B scripts/performance_study.py REPORT --output NEW_STUDY.json`.
The runtime driver now records the Portable profile and linked runtime key/hash
explicitly, alongside emitted-program and compiler identities.

After a compiler change, commit and rebuild with a new build log. Repeat the
same measurement with a fresh output directory and optionally
`--baseline target/performance-objects-before/report.json`. Existing reports
can also be compared without rerunning JavaScript:

```sh
python3 -B scripts/benchmark-v8.py compare \
  target/performance-objects-before/report.json \
  target/performance-objects-after/report.json
```

The runner writes a readable `summary.md`, raw `report.json`, child stdout/stderr
under `logs/`, exact sources and filtered manifest under `workload/`, and compiled
artifacts under `wasm/`. Preserve the entire directory when moving a baseline;
comparison verifies retained bytes and recomputes summaries from raw observations.
Different hardware/affinity, sample plans, workload bytes, runtime versions,
measurement policy, release profile or harness bytes reject comparison. Compiler
commits, compiler fingerprints and emitted Wasm may differ as part of optimization.
Ratios are observations, not a statistical-significance verdict. V1 reports remain
historical and are rejected as v2 baselines.

For the separately recorded frozen-product ABBA diagnostics, analyze preserved
round variation without rerunning the programs:

```sh
python3 -B scripts/paired_repeatability.py PATH/receipt.json \
  --output target/paired-repeatability.json
```

The input must be a complete `scriptw-frozen-paired-diagnostic-v1` receipt
(the historical `scriptw-frozen-string-hash-diagnostic-v1` spelling also works),
with its original raw child JSON files beside it. The analyzer checks their
digests, ABBA order, source/runner/Wasm identities, warmups and measured cache
hits. It preserves all samples and reports each source's two-round median
range plus the two adjacent before/after effects. Cross-round stdout consistency
is checked; workload/oracle correctness remains bound to the original producer's
evidence. These are descriptive observed ranges, not confidence intervals or an
automatic acceptance threshold. For a small future gain within the observed
variation, collect further balanced or same-source controls before accepting it.
This analysis does not retroactively validate earlier decisions.

## What is measured

### Retained-row phase diagnostics

An opt-in runner mode observes construction and traversal inside the ordinary
JS main invocation, using the existing synchronous host print callback. It
keeps the same fresh Store/Realm and compiled artifact policy. Generate the
marked versions of the existing row corpus and its empty marker control:

```sh
python3 -B scripts/performance_corpus.py --row-phases \
  --output-dir target/row-phase-corpus --rows 1024,8192 --widths 4 --passes 0,1,4
mkdir -p target/row-phase-artifacts
target/release/examples/runtime_baseline target/row-phase-corpus 7 10 \
  rows-n8192-w4-p1-phases.js target/row-phase-artifacts --row-phases
```

The example accepts the same five ordinary arguments followed by `--row-phases`.
This mode emits `scriptw-wasmtime-row-phases-v1`, separate from the ordinary v2
baseline schema. Each observation keeps the complete execution/completion score
and adds construction, traversal, combined and middle-callback wall durations.
Construction includes the checksum seed and captures of retained row/array
identities. Traversal reads those same objects in the same invocation; final
identity checks and checksums remain observable after the timing markers.

Individual phases exclude their endpoint host callbacks. Guest work to enter
the next print callback remains in the preceding phase, and the combined span
includes the middle callback explicitly. Empty markers and hook-off/on controls
are required before interpreting small phase differences. Do not subtract these
diagnostic boundaries from the ordinary README scores or call wall time CPU time.

For optional Linux sampling, launch the runner under `perf record -D -1` with
`--control fifo:CONTROL,ACK`, using fresh control/ack FIFOs. Supply all three
environment variables: `SCRIPTW_ROW_PERF_CONTROL`, `SCRIPTW_ROW_PERF_ACK` and
`SCRIPTW_ROW_PERF_REGION` (`construction`, `traversal` or `combined`). The host
callback waits at most five seconds for each acknowledgement and records its
duration. Only measured invocations issue enable/disable commands; warmups and
compilation stay disabled. Sampling includes a small amount of boundary-control
work, which needs its own control measurement. Preserve exact loaded-code maps,
runtime/Wasm/executable identities, raw profiles and profiler overhead alongside
any attribution claim. This diagnostic introduces no new engine import.

`scripts/measure_row_phases.py` automates the serial measurement: a Node checksum
oracle, case-wise observer off/on/on/off controls, profiler off/on/on/off controls
for the empty and selected row cases, then two profiles each for construction
and traversal. It retains complete raw outputs, emitted Wasm, loaded-code maps,
profile data and incremental receipts, including a failed child. On an otherwise
idle cloud machine with the runner already built:

```sh
python3 -B scripts/limited_verification.py --cloud -- \
  python3 -B scripts/measure_row_phases.py \
  --runner target/release/examples/runtime_baseline \
  --corpus target/row-phase-corpus --output target/row-phase-measurement \
  --cache target/row-phase-cache --perf /usr/bin/perf
```

The default is seven samples after ten warmups, 499-Hz user-CPU sampling and one
pinned CPU. `--perf-library-path` can supply a private profiler's libraries;
they are removed from the runtime child's environment. Output directories must
be new. Validate captured sample PIDs against the saved map before attribution;
the producer checks fresh map, artifact and byte identities. CPU analysis and
any backlog decision require a separate review of the completed evidence.

### Ordinary baseline boundaries

- **Cold start:** a fresh child process with a unique empty Scriptw application
  cache. The shared runtime is already compiled into the release build. Node
  starts normally with its persistent compile cache disabled. OS caches stay warm.
- **Cached start:** a fresh child process after priming the Scriptw disk cache.
  Node remains a new process without an equivalent persistent code cache. This
  does not measure a running server.
- **Execution:** each case has its own process. Source compiles once, then both
  runtimes use the same explicit warmup count and measured count. Wasmtime uses
  a fresh Store and V8 a fresh vm.Context per invocation. Context/Store setup and
  source compilation sit outside the execution span; output/completion processing
  is inside. Scriptw initializes its fresh Wasm GC Realm and builtins inside
  timed main; V8 context creation is outside its source-execution span. V8 tiering and natural GC stay enabled; equal warmup counts do not
  prove equal steady-state/JIT tiers. Measured Wasmtime module-cache hits are
  required. Warmup observations remain in raw data and are excluded from summaries.
- **Phases and bytes:** Wasmtime reports native-module/cache work, Store/linker,
  instantiation, export lookup, execution and total. Preparation/lowering/emission/
  validation spans are single compilation diagnostics, not repeated benchmarks.
  The exact emitted Wasm and body/data footprint identify code-size changes.
- **CPU and process memory:** GNU `time` gives the runtime child's kernel high-water RSS,
  excluding the Python launcher's inherited pre-exec footprint. Linux `wait4`
  gives CPU time for the small native supervisor and its waited child. Startup has repeated child observations;
  execution has one whole-child observation per runtime/case, including compilation,
  warmups and retained runtime caches. Blocking wait avoids timeout-polling rounding
  in short startup spans. Driver receipt/log writes sit outside the wall timer;
  the GNU time supervisor, process launch, watchdog setup and scheduling stay inside. A watchdog kills the
  owned process group at the explicit timeout.
- **Invocation memory:** Wasmtime's existing profiler samples process RSS every
  10 ms plus endpoints around runtime admission through completion. Short peaks
  can be missed; it includes process-wide caches, host work and sampler overhead.
  V8 reports process memory endpoints before context creation, before execution
  and after execution. Earlier warmups and contexts can contribute to those process
  totals until normal GC runs. These are not per-workload live bytes or peak samples.
- **Heap observations:** Wasmtime exposes GC heap *capacity* before/after execution.
  V8 exposes process-wide used/reserved heap, external and ArrayBuffer endpoints.
  They have different meanings. Do not subtract process RSS to invent a JS heap,
  equate Wasmtime capacity with V8 used heap, or interpret natural GC deltas as
  allocation totals. Wasmtime live bytes, allocation totals, collection counts,
  GC pauses and peak GC heap stay explicitly unavailable. No forced GC is added.

The default corpus retains the original 10,000-step integer and 500-object
workloads and adds 8,192 retained objects, strings and four-element arrays with
32,768 indexed reads. Each prints an independently specified checksum. These
small kernels help find costs; they do not establish application-wide performance.
Use `--fixtures PATH` for a directory with the same manifest schema, a checked
startup entry and nonempty execution entries (`file`, `stdout`, optional `work`).
Case files must have distinct `.js` basenames. An entry may name any self-contained
script supported by both runtimes; `print` is the common output interface. The
benchmark rejects incorrect output instead of accepting a faster wrong result.

For the existing fixed twenty-case Wasmtime compiler/runtime reports, see the
[compiler profile](contracts/compiler-performance-profile.md),
[runtime profile](contracts/runtime-performance-profile.md) and
[whole-CLI reporter](contracts/performance-reporting.md). Those reports use their
own fixed Test262 host corpus and policies; do not mix their timing spans with
this normal-product/V8 comparison. Wider calibrated T25 acceptance remains open.
