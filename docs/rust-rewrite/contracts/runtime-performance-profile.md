# Product runtime performance observations

## Explicit compiler and GC capacity cohorts

`Engine::profile_wasm_execution_with_options` accepts closed, immutable
`WasmRuntimeProfileOptions`: Fast/SizeOptimized/Speed program compilation,
8/16/32/64/128 MiB initial GC capacity, and SinglePass/Backtracking program
register allocation. These are diagnostic configurations. Ordinary execution
retains automatic native-mode selection, a 16 MiB initial heap and SinglePass.
All cohorts retain the required copying collector, GC/reference/exception
features, Store limits, deadlines and a fresh Store/Realm for every invocation.

The runtime-baseline example accepts trailing `--native-mode
fast|size-optimized|speed`, `--gc-initial-mib 8|16|32|64|128` and `--regalloc
single-pass|backtracking` options (each flag and value are separate arguments).
Any explicit policy flag activates a separate diagnostic cohort; omitted fields
then use fast/16/single-pass. Ordinary invocations keep their v2 schema.
Explicit invocations emit `scriptw-wasmtime-policy-baseline-v1`, their complete
`requested_native_policy`, and each observation's `actual_native_policy`.
The benchmark runner admits these only with `--skip-startup`; policy settings
cannot be compared to ordinary scores or to a different policy through the
ordinary baseline comparison.

Two diagnostic Engine configurations may remain hot. The existing module-image
LRU checks actual Engine ownership as well as Wasm/native-mode identity, so an
evicted and recreated configuration cannot reuse another Engine's Module.
The independent v5 runtime manifest explicitly admits Speed destinations.
Ordinary embedded R retains its SpeedAndSize/Backtracking build producer.

Pinned Wasmtime includes initial GC capacity in native-image compatibility.
Every explicit policy cohort, including the 16 MiB control, therefore uses
the same matching optimized-R producer and serialized-image cache through the
checked native module/function disk caches. Runtime-module and host-template
keys distinguish this explicit image authority from ordinary embedded-image
admission. This preserves runtime optimization while changing the heap parameter
and prevents a retained ordinary R from bypassing the diagnostic treatment. Only privately serialized producer bytes
enter this diagnostic image cache. It retains at most two images and 256 MiB in
total; oversized images execute transiently and are not retained. Actual
destination-Engine native-version, feature and tunable admission remains
mandatory. The first warmup includes matching-R production; each observation
records `matched_runtime_image_cache` as NotAccessed/Hit/Miss/Oversized.
Explicit-cohort RSS includes the additional matching serialized image and
producer/allocator high-water effects. It is diagnostic packaging RSS, not
ordinary product RSS or live JavaScript heap usage.

The ordinary and explicit producers also select different native CPU features.
The [Cargo runtime producer](../../../crates/scriptw-engine/src/embedded_runtime/build.rs)
explicitly calls `Config::target` with the build target triple. Pinned
[Wasmtime47 target handling](../../../vendor/wasmtime-47.0.0/src/config.rs)
disables host CPU feature inference when a target is supplied, including when
that target equals the host triple. The explicit
[matching-image producer](../../../crates/scriptw-engine/src/embedded_runtime/matched_capacity.rs)
uses the product configuration without supplying a target, so its Cranelift
builder infers the current host's CPU features. Matching the optimization level
and allocator therefore does not make ordinary and explicit R identical.

All explicit variants on the same measured host share that inferred target and
the same serialized-image treatment, including the 16 MiB control. Comparisons
within that matrix can isolate the selected P compiler or initial-capacity
variable when all other controls match. An ordinary-versus-explicit comparison
changes R's native target and image storage too; it cannot isolate a P-mode,
allocator or capacity effect. Such a comparison remains a labelled diagnostic,
even when the requested policy says Fast/16 MiB/SinglePass.

Store-independent host function registration uses at most two linker templates,
keyed by host Wasm/native-mode identity and checked against the actual Engine.
Templates retain immutable callbacks and canonical types, without Modules,
artifact bytes, Stores, Realms, memory imports or runtime instances. Each run
clones its template before defining fresh private/shared memory and linking its
own R instance. `host_import_cache` reports the actual Hit/Miss result.

Runtime timings additionally expose artifact/Intl `admission` and
`host_import_registration`. Host import registration is a subspan of
`store_and_linker`, so additive attribution must not count it twice. These
diagnostics and cache paths require the focused native and semantic checkpoints;
their presence alone makes no speedup or new full-suite conformance claim.

## Historical fixed-corpus observations

The fixed twenty-case conformance corpus explicitly compiles with the Test262
host surface, matching the original ignored CLI chunk gate. It executes real
emitted Wasm through the product runtime. Policy v3 records this host authority
and rejects product-surface profiles as comparable baselines. The first idle
optimized owner, `target/optimized-default-performance-20261013/runtime`, failed
at typed-array warmup because policy v2 omitted that authority; its original
ReferenceError observation and prefix are retained. Corrected `743c810c7` native
controls and all twenty seven-sample profiles now pass. All140 measured module
cache outcomes are hits; mean module/execution spans are 24.685/2.522 ms. See
[the retained owners](../default-optimized-performance-20261013.json).

Status: complete all-feature/default native checkpoints cover the sampler and
runtime-profile controls. The corrected optimized CLI controls and fixed-corpus
idle benchmarks pass. No full-pin status refresh is claimed here; these results
do not establish calibrated T25 budgets or complete task acceptance.

After the coherent source batch is verified, the opt-in command is:

```sh
python3 scripts/limited_verification.py -- \
  ./target/release/scriptw performance runtime \
  --output-dir target/performance/runtime-candidate --samples 7 \
  --machine-label dedicated-benchmark-host --idle-machine
```

The original fixed twenty-case corpus supplies the exact ordered filenames and
embedded bytes. Every selected fixture must match those bytes. The reporter
uses the existing identity readers, one compilation worker and serial execution.
The output directory must be fresh. `--fixture-root PATH` may select another
copy of the same corpus; its canonical filename root participates in identity.
There are no new generated workloads, input ceilings or timing budgets.
`--gate` belongs to the original whole-CLI report and is rejected here.

Each case compiles once through the product parse, IR, lowering, Wasm emission
and validation pipeline, outside the runtime samples. Its actual Wasm bytes,
SHA-256 and validated section footprint are retained. One unmeasured warmup
precedes 3..100 measured repetitions of those same bytes. Each repetition calls
`Engine::profile_wasm_execution` and constructs a fresh original Wasmtime Store.
The runtime's normal native-module retention and disk/function caches remain
enabled, and each sample records the actual module-memory-cache outcome. A
warmup does not promise a cache hit; capacity limits or ambient configuration
can cause misses. No reporter cache or alternate runtime is introduced.

Function-stencil and program-Wasm disk hits refresh recency through the same
`CacheStore::get` owner, including on `noatime` filesystems. Timestamp updates
are best effort: failure cannot discard successfully read bytes. Cranelift's
stencil/target/version identity and the configured cache limits remain intact.
All sixteen cache checks pass in `tasks-dry-closure-foundation2`, including
pruning after both actual entry points read a file whose access time stays old.

Pooled string slots now retain their checked insertion indices. Initializers,
ordinary literal reads and collected source diagnostics use that same stored
index; duplicate interning adds no slot, and literal lookup uses the existing
ordered map directly. An unrelated source string can no longer renumber the
builtin literals already collected before it. Collection order, UTF-16 data,
static offsets and native cache identity stay owned by their existing paths.
This does not promise stable late-collected literals or static table offsets
across arbitrary programs.

The retained native4 primitive artifacts expose the predecessor cost: their
type/import/function/export/element sections match, but 203 builtin bodies and
24 shared helpers differ. The 888,484-byte Duration round body differs at 13
pooled-string-index bytes. The existing two-emission literal-data control now
also checks byte-identical shared coercion code across source strings that sort
on opposite sides of builtin spellings. Slot/data correspondence, duplicates,
source-diagnostic resolution and the u32 table-length boundary have three unit
controls. All three and the artifact control pass with the joined workspace
type check on 2026-10-08. The actual shared helper bytes match across changed
source literals. Measured native cache/timing gains remain unverified, and the
original five-second campaign remains open.

Global identifier reads now share four typed Completion helpers for ordinary
Get/typeof and sloppy/strict source contexts. The caller selects its existing
source Global Environment and execution Realm, then transports those roots with
the source name and caller Environment. Only the registered helper parameter
can establish this execution-Realm authority. The private helper reuses the
original ResolveBinding/GetBindingValue/Reference-release owners: both object
HasProperty checks, live lexical delegate refresh, TDZ behavior and exact Throw
values remain ordered. References retained across RHS evaluation keep their
existing separate lifecycle; the helper does not own a source Environment or
manufacture a callable context.

The retained native5 expanded Switch artifact measured a 1,638,171-byte sloppy
main and a 1,201,087-byte strict main before this extraction. The sloppy native
case passed; strict compilation was deliberately stopped. The existing captured
Switch artifact control now emits that exact combined sloppy source and checks
one declaration of every read helper, actual Value/typeof call edges from main,
and the unchanged 1-MiB body threshold. The existing native global-read cohort
also covers typeof TDZ and four foreign-Realm read/error cases. These extraction
checks and any resulting compile-time improvement are pending verification.

Realm intrinsic initialization now has one emitted
`helper::realm_initialize_intrinsics` body, shared by entry bootstrap and created
Realms. The existing ordered initialization algorithm remains its sole producer.
Its typed arguments carry the selected Realm and caller Environment; native
function construction retains its explicit selected-Realm context and null
lexical/private captures. Six nullable OrdinaryObject results carry Reflect,
Math, JSON, Atomics, Temporal and Intl in that order. The helper places all six
references on the Wasm result stack before clearing its local owners; the caller
roots all six before reloading the same Realm's Object/Function prototypes.
The shared bootstrap plan determines absent namespaces on both sides.

Global publication stays at the caller: entry declarations and the created
Realm's canonical-only globals keep their original separate paths and ordering.
Agent-wide literal/symbol roots still initialize before entry bootstrap. The
zero-length Array prototype uses a fixed-zero entry to the same private array
allocation body, avoiding a JavaScript completion return inside the reference
helper; dynamic array lengths retain their existing range check and throw.
No GC layout, backend, cache identity, resource cap or native optimization
threshold changes. The exact optional-alias fixture supplies an authored artifact
guard for one shared helper, both direct call paths, and the existing 1-MiB body
threshold. The capped serial `tasks-realm-bootstrap1` checkpoint passes the module
guard, workspace all-feature/all-target type check, six runtime-helper controls
and ten selected AOT controls, with no failures or ignored tests. Its validated
artifact has one 662,558-byte initializer helper; main falls from 1,196,893 to
534,595 bytes and the ShadowRealm constructor from 686,818 to 24,632 bytes. Every
body is below 1 MiB, and both direct call edges pass. `native3` passes expanded
namespace isolation and the exact optional-alias fixture in both modes. The
optional-alias test takes 194.27 seconds versus the prior 364.18; native
compilation takes 75.157 / 72.615 seconds versus 160.616 / 140.921 for sloppy /
strict modes. The emitted sloppy module is 41,458,110 bytes. These are single
observed runs with cache differences, not calibrated performance acceptance.
The wider native queue and original five-second campaign remain open.

The earlier retained CrossRealm seed 1270 program module was 558,363 bytes,
with a 475,586-byte main body. Opcode attribution found 38 tagged-addition
branches totaling 239,225 bytes (50.30% of main). Reusing the existing
ValueToString and ValueToNumber helpers reduced P to 341,040 bytes and main to
279,145 bytes, with the native arithmetic controls passing. The unchanged
five-second campaign still failed after native R reuse was established: the
worker reached native compilation of P but did not complete it before its
deadline.

Complete coercive addition now has one `helper::coercive_add` body in R. P
evaluates and roots both operands before the call; the typed helper also receives
the actual execution Realm and caller Environment. R performs the existing
left-then-right Default ToPrimitive conversions, string/numeric branch and
BigInt checks. It returns the whole Completion for P's original active handler.
The statically proven Number-plus-Number fast path remains inline. There is one
algorithm owner, with no recursive helper call or change in evaluation order.

The encoded-body regression follows actual R exports, P imports and Call
operators. It checks one addition call per dynamic site, the helper's conversion
and BigInt edges, and bounded growth of P. Existing native cohorts additionally
cover Symbol/string errors and mixed BigInt errors in both defining Realms,
original thrown-object identity, both operand evaluations before coercion,
early abrupt completion, finally, and the saved left-hand compound-assignment
value. The capped serial `production-addition-helper3` checkpoint passes the
workspace all-feature/all-target type check, seven helper ABI controls, three
encoded-body controls, six raw-runtime codec controls and all fifteen focused
native tests. The dynamic fixture adds 1,257 bytes per site, below its 1,536-byte
bound and the preceding 2,324-byte implementation; both existing static Number
bitwise bounds also pass. Module boundaries, all fourteen Intl identity recipes
and the host-ABI, task, legacy and shortcut audits pass. These are focused
results, with the broad workspace sweep still pending.

That checkpoint emits the original CrossRealm seed 1270 as a 287,878-byte P
with a 231,442-byte main and a 41,199,837-byte R. Its first worker has cold raw
and native R caches and reaches the unchanged five-second deadline while loading
R. After a general native arithmetic producer warms the native R cache, the
original baseline completes within five seconds with the same primitive
completion and eight print events as SpecExec. The enclosing program-cache
lookup takes 1.751 seconds, including 1.187 seconds of Intl data admission and
0.563 seconds of validated raw R loading. Native R loading takes 0.383 seconds,
native P compilation takes 1.950 seconds and execution takes 0.042 seconds.

The transformed worker still misses the same five-second deadline while
compiling its fresh P, so the campaign remains red. Its P is 288,798 bytes with
a 232,425-byte main. Its enclosing emission takes 2.858 seconds, including
1.197 seconds of Intl data admission and a 0.503-second raw R hit; native R
loading then takes 0.370 seconds. These are individual observed runs with the
stated cache conditions, not calibrated T25 acceptance. The deadline and native
resource limits are unchanged. Optional trace output separates Intl runtime-data
admission, raw R loading, and native R/P loading so a partial timeout cannot be
mistaken for a successful cache hit. Admission and raw-cache spans are nested
inside emission or program-cache lookup and must not be added to them again.

Bitwise operations with two IR-proven Number operands evaluate both operands
in order and call the same private Number emitter used by dynamic dispatch.
They no longer emit unreachable ToNumeric and BigInt branches. The extracted
Number instruction sequence retains the original truncation, modulo and signed
or unsigned shift behavior. Dynamic operands keep ordered coercion and mixed
type errors. Existing static-site byte limits remain unchanged; native controls
cover wrapping, negative and oversized shift counts, nested expressions,
evaluation effects and abrupt completion, mutable storage and large BigInts.

Named-zone admission now shares immutable transition data within each
constructor. The 598 pinned names contain 341 distinct TZif payloads;
an exact-byte map lets identical payloads reuse the original complete transition,
offset, POSIX-cycle and gap-topology admission. Each row still passes its spelling
and digest checks before reuse, and each name retains its own primary identity
and selected-profile availability. The map borrows input bytes and drops at
constructor return. Separate image constructions retain separate owners; no
global cache or admission bypass is introduced.

Lowercase and uppercase mapping images now come from the original ordered Rust
Unicode scalar walks at build time. Emission copies their unchanged 16-byte
little-endian rows and retains the existing alignment and ICU Cased and
CaseIgnorable ranges. Build-time payload-width checks and compile-time row extent,
count and host/target Unicode-version checks guard the image boundary. An
exhaustive control compares every target mapping and complete pool bytes across
all eight alignment prefixes. Named-zone controls check exact-payload sharing,
per-name errors, distinct-payload rejection, constructor ownership and omitted
geographic primaries. The capped `production-startup-tables1` checkpoint passes
the workspace type check and all 40 focused controls: one exhaustive mapping,
29 named-zone admission, six image/projection and four native Date/Unicode
tests. Formatting, module boundaries, all fourteen generated identities and
repository source audits also pass. The checkpoint takes 330 watched seconds;
its all-feature/all-target type check takes 64.886 seconds.

The first campaign attempt still times out during cold native R loading. After
the native controls, both CrossRealm baseline and transformed programs complete
under the original five-second deadline and match SpecExec, including all eight
actions. The transformed P still requires native compilation: emission takes
2.432 seconds (including 0.939 seconds of Intl admission and 0.490 seconds of
raw R loading), native R loads in 0.339 seconds, P compiles in 1.452 seconds and
execution takes 0.024 seconds. These are single observations with a warm native
R cache, not cold-start acceptance.

The campaign then reaches Temporal seed 3215. Its 350,335-byte P has a
266,566-byte main; lowering takes 0.312 seconds, emission 2.523 seconds and
native R loading 0.341 seconds. Its fresh P does not finish compiling before
five seconds. Temporal's transformed program remains unrun, and the campaign
is still red. Deadlines, cache budgets and verification resource limits remain
unchanged. Retained failure evidence is
`target/verification-tmp/scriptw-generated-campaign-759906-1791488252109251786-1`.

The pending normalization startup change moves the original complete ICU scalar
walk into the compiler build. Its versioned little-endian image contains the same
canonical and compatibility sequences, sequence-relative mapping rows, combining
classes and compositions. The consumer checks the complete extent before
allocating, then admits scalar domains, sparse row order and complete sequence
partitions once per process. It retains the existing table owner and unchanged
pool serializer, including alignment, sequence-pointer relocation and emitted
row padding. The build uses the same pinned ICU dependency and checks its image
against the constructed rows; the independent full-scalar oracle and actual pool
bytes across all eight prefix alignments provide exhaustive parity controls.
Malformed extents and row domains have focused rejection controls. This batch
has no measured performance result yet and does not claim the remaining Temporal
or cold native-R acceptance paths are complete.

The cold-start batch builds one exact Minimal runtime with Cargo. Its native
image uses the existing SizeOptimized setting; ordinary P retains its existing
Fast compiler plan and settings.
The AOT build-package owner emits raw R, checks it through the same complete
Wasm/Intl/layout decoder used by the raw disk cache, and exposes the paired
Wasm and package only through read-only getters. Its separate envelope binds
compiler source, runtime ABI, admitted physical Intl sections, pool extents and
the actual R key. Ordinary source/executable cache identities are unchanged.
Target-side Intl admission still precedes package lookup; malformed or mismatched
packages use the existing raw cache/emitter. Scalar-only programs leave the
candidate unresolved. Ordinary, rooted and cached-P paths share this resolver.

A shared configuration owner supplies the existing feature, copying-collector,
epoch, stack and memory settings to both Cargo and the product Engine.
Cargo selects SpeedAndSize and the Backtracking register allocator for immutable
R. Ordinary Fast P retains OptLevel::None and SinglePass register allocation;
its compilation deadline and execution policy are unchanged. The ordinary size threshold, retry and fallback
configuration are unchanged. Cargo explicitly selects TARGET and baseline ISA,
uses serial native precompilation on the existing compiler stack, and disables
ambient backtrace-detail selection. It produces a bundle only for supported
little-endian x86_64, aarch64 and riscv64 targets. Other targets retain ordinary
runtime compilation and capability checks. Separate bundles cover exact
Minimal NativeHost and Portable R for Fast and SizeOptimized execution. Each
profile has its own raw key, package and version-four manifest, admitted lazily;
selecting one does not initialize the other. Other raw identities use the
original path. R is deserialized into the actual P Engine, not returned from a
separate optimized Engine.

Cargo publishes bundle data, compressed Portable storage and generated bindings
by writing a new sibling file and renaming it over the destination. An existing
symlink or hardlink is replaced rather than written through, preserving shared
completed build outputs. This changes output-file ownership, not package bytes,
source fingerprints, admission rules or the per-profile publication sequence.
The bundle is still admitted only after the producer finishes all artifacts.

The Engine's private native authority accepts only immutable Cargo-produced
linked arrays and their private verified decoder output. No file, cache blob or caller-provided native
slice can enter its unsafe deserializer. A manifest binds source, target,
configuration, the R image mode and closed set of execution modes, the closed
R allocator identity, and both image digests to the actual admitted R key.
The version-four manifest requires SizeOptimized/Backtracking R and explicitly
admits both Fast and SizeOptimized execution. Version-three manifests retain no
authority under this format. Each field is decoded and admitted independently.
Wasmtime still checks compiler version, proposals, collector, tunables and ISA;
an incompatible known-valid image is an optimization miss. The pinned compiler
[treats optimization level and register allocation as compatibility-neutral for deserialization](https://github.com/bytecodealliance/wasmtime/blob/v47.0.0/crates/wasmtime/src/engine.rs),
while product admission still requires the exact supported mode set. The shared
module-cache key remains the execution Engine's mode and raw Wasm digest, so
an image compiled in a different mode cannot cross Engine ownership.
Native deserialization runs only inside the existing shared R/P cache factory.

Portable native storage may use a private Cargo-produced compressed container.
The producer first publishes its checked raw native image and unchanged v4
manifest, then verifies an exact storage roundtrip. Runtime decoding admits the
bounded output length, frame window and raw SHA-256 before returning a private
immutable `NativeRuntimeBytes`. Its fields have no public constructor or mutable
projection. A mutex/weak lease shares simultaneous decodes without retaining a
strong process-lifetime owner. Portable metadata/package admission is cached;
every actual load independently compares the derived digest with the v4 native
manifest before `Module::deserialize` into the actual Engine. NativeHost keeps
its once-per-process package/native digest admission. Decoder failure remains an
optimization miss and ordinary checked native compilation.

Wasmtime 47's `load_code_bytes` calls `MmapVec::from_slice_with_alignment`, copying
the caller slice into Module-owned `CodeMemory`. The final lease drops immediately
after that call; `deserialize_raw`, whose borrowing contract differs, is unused.
Cache misses, bypass and a separate execution mode can decode again. Concurrent
loads share an immutable Arc only while deserializing; no Store or Module moves
into this lease cache. Measure transient and retained RSS and repeated-load cost
separately; freeing the Box does not itself prove an RSS reduction. Raw package
authority, NativeHost arrays and public artifact/cache formats are unchanged.
Package-only selection does not decode the native payload. Compressed pages and
decoded ownership must both count toward RSS; storage compression is not a
claim of reduced live JS memory or reduced deserialization cost.

Runtime layout 15 introduced four immutable non-null zero-length GC array globals
(values, property entries, property indexes and private elements) to the shared
R/P prefix. Their hidden storage identity has no mutable element. Ordinary
objects still receive fresh property-storage records and prototype records;
JS-visible arrays, arguments/rest objects and proxy argument arrays stay fresh.
Every mutation allocates or replaces backing before writing. Syntax-empty
internal argument vectors may use the shared value array; spread/dynamic lists
retain their existing construction path. Global ownership is per runtime
instance/Store, and the layout version participates in package/cache identity.

Runtime layout 16 adds a mutable i64 raw UTF-16 key hash to StringValue. Strings
initialize it to zero, compute it on first hashed lookup, and reuse it before
the caller applies property/collection tag mixing. A real zero hash remains
correct and is recomputed. Equality still compares content. Published code units
remain read-only under the consuming StringConstruction API; host-created syntax
strings initialize the hash with an actual i64 zero. Cache/package admission
rejects layout 15. Every string carries the field even if it is never hashed;
retained-string measurements must account for that cost.

Binding cells retain immutable StoredValue snapshots. This records the historical
layout-16 checkpoint; the current private runtime ABI is layout 32.
The earlier, unrelated inline-payload experiment also used draft layout 16; it
did not establish a closure benefit and is removed. Its raw measurements remain
historical evidence. Descriptor
allocation may share one immutable getter/setter snapshot when both inputs are
the same compiler operand. The descriptor's two reference fields remain
independently replaceable; no mutable StoredValue or JS-visible identity is
shared by that allocation shortcut.

The profiling SDK can request Fast or SizeOptimized through
`profile_wasm_execution_with_native_mode`; each result exposes its actual mode.
The ordinary method/runner retains automatic dispatch and CodeTooLarge retry.
Forced Fast does not silently retry. The `native_policy_probe` example emits
separately labelled source/Wasm/executable-bound cohorts, with equal warmups,
fresh realms, checksums and actual modes. These are not ordinary V8 reports;
existing report compatibility checks are unchanged.

Actual mapped-image accounting, bounded cache retention, oversized-module
behavior, explicit bypass and agent bypass all retain their existing owner.
The bundle retains metadata, never a native Module or mutable Store state.

Authored controls compare the host-produced package with fresh target emission,
reject identity/framing/Wasm/pool corruption, and exercise native compatibility
rejection using valid embedded machine code. Fresh-process controls cover
independent empty caches, an unavailable cache directory, linked P cache hits
and rooted observation with exact Wasm-AOT completions. The prior disk-cache
process controls use a valid unbundled custom projection and retain their exact
raw reuse and native 0/2 then 1/1 assertions. Native controls retain incompatible
execution-mode rejection and separately reject the wrong R image mode. Actual
Fast Engine ownership and fresh-process Fast P / SizeOptimized R traces are
checked with the same exact completions and cache counts. Source changes still
invalidate the ordinary compiler fingerprint.

The initial Fast-R build produced a 41,151,160-byte raw package and a
292,798,624-byte native image. Its ELF text section accounts for 265,093,120
bytes (90.5%); the size problem is mainly native instructions. Those numbers are
from the initial build, not a measurement of the SizeOptimized-R candidate.
The candidate still requires build-time/RSS, image-size and startup acceptance
under the unchanged limits; smaller output is not yet established.

This batch has no measured acceptance result yet. Native precompilation adds
build work whenever watched compiler inputs change; raw and native embedding
also enlarge executables and their first whole-file fingerprint pass. Cold build,
rebuild, executable size, startup RSS, admission and unchanged one/five-second
gates require the joined capped verification. No previous warmed-cache result
establishes cold-start acceptance, and no deadline or resource cap changes.

The SDK shares the actual product execution method, required copying collector,
imports, limits, epoch timeout mechanism, host work and structured completion
decoder. Only a requested runtime profile starts the memory sampler; ordinary
execution has no sampling thread or procfs reads. The default report supplies
no new timeout or acceptance budget.
Runtime phase spans measure engine lookup/setup, native-module compilation or
cache lookup, Store/linker setup, instantiation, export lookup and execution
through host work and completion decoding. Total starts at runtime artifact
admission and ends after completion roots are dropped and the sampler is joined.
Its sampler setup/retirement overhead is included. It excludes source
compilation, worker-thread creation, Store destruction, evidence writes and
summary/checkpoint work. Module start code, if present, belongs to instantiation.
These exact spans are retained separately; they are not general throughput,
operation-count or statistical-significance claims.

Pinned Wasmtime 47 exposes `Store::gc_heap_capacity()`. The SDK samples that
actual byte capacity after instantiation/before the main call and after
completion decoding/host-root release. The report names it **capacity**, in byte
units, and retains both snapshots for every warmup and sample. Capacity can be
zero when the Store has not initialized a GC heap. It can differ between
repetitions as the real allocator/collector behaves. It is neither allocated
bytes, live bytes, resident process memory nor a peak.

The selected runtime has no public per-Store allocation-count, allocated-byte,
live-byte, collection-count or pause-time counter. Those fields explicitly say
`wasmtime-has-no-public-counter`. Peak GC heap still says
`no-per-invocation-peak-sampler`. No collection is forced before or after a sample.
Explicit program `gc()` calls and automatic collection keep their original
product semantics; this profile does not turn observed explicit calls into a
total collector count.

On Linux, a private invocation owner reads current `VmRSS` from
`/proc/self/status` at entry, at a nominal 10-ms interval, and at retirement after
completion decoding/root release. Each bounded read admits only the current
RSS field in kernel kB units and converts it to bytes with checked arithmetic.
The first and final reads count even when an invocation finishes before the
first interval. The report retains initial/final RSS, maximum observed RSS,
sample count, nominal interval, actual sampling span and the closed
`runtime-invocation-process-rss` scope. The periodic loop waits at least one
interval between reads; scheduling and read time can lengthen that interval.
The observed maximum is a sampled lower bound on the true peak, not an exact
peak. Short-lived allocations can occur between samples.

This is whole-process residency during one invocation. Retained native modules,
other threads, the Rust host and the sampler itself contribute; it does not
attribute RSS growth to JavaScript, one Store or the collector. The sampler has
one 256-KiB-stack thread and bounded 64-KiB procfs records with a one-byte
oversize probe, retains only aggregate
observations, and owns its stop/join through RAII on success, error and unwind.
No lifetime `VmHWM`/`ru_maxrss`, cgroup-wide peak, reserved GC capacity or capacity
difference is substituted for an invocation observation. Unsupported platforms,
unreadable/malformed procfs, thread start/panic and sample-counter failures have
typed unavailable results; failed sampling never becomes numeric zero or a
partial successful maximum.

Linear-memory capacity is not added in this slice: the profiling API accepts
admitted artifacts beyond the normal product memory layout, and one exported
memory is not a complete owner of every private/shared memory. GC capacity
continues to use the two original snapshots described above.

Every successful invocation retains the original typed completion's category
and debug representation plus verbatim captured print events. Object/Symbol
observations remain category-only; no user coercion or backend address is used.
Different normal observations are retained independently because real programs
may observe time. An ECMAScript throw, trap, host failure or compiler failure
marks the report Failed and retains all completed prefixes. Interruptions leave
Running. Neither is admitted as a baseline or receives complete summaries.
An ECMAScript throw also retains its current completion and print observation
before failure; the failure reason names that file. Trap/host errors retain the
original product error and already completed measurements rather than inventing
a completion for an execution that never reached the observation boundary.

To compare a later compiler, add `--baseline PATH/report.json`. Baseline
admission precedes candidate compilation/execution. It requires the same exact
ordered workload, filename root, machine, toolchain/build, environment/resource
configuration, sample count, runtime capability report and measurement policy.
The compiler revision/image may differ and both identities are retained.
Admission rechecks retained source, Wasm and observation digests, validates Wasm
under the product runtime, recomputes its footprint and all summaries from raw
samples, and rejects unsupported metric claims or totals smaller than the sum
of their disjoint phases. Original baseline evidence remains in its directory;
candidate `baseline.json` retains the admitted report bytes.

The runtime report is version two and binds this sampling policy and scope.
Version-one reports have no equivalent memory observation or sampler overhead
and are refused as compatible baselines. Each retained invocation observation
includes its RSS result; admission checks its equality with the aggregate sample,
endpoint/max ordering, kernel byte units, interval/count bound and sampling span
within total invocation time. Existing source/Wasm/output evidence checks remain.
RSS summaries exclude warmup and exist only when every measured invocation in
that case has valid sampling. Otherwise typed reasons remain and no numeric RSS
budget metric or delta is published; successful raw samples are still retained.

Timing summaries use the existing min/max, median and nearest-rank p90/p95
rules. Capacity summaries use the same statistics in explicitly named byte
fields. Comparisons show signed timing and capacity changes and original/candidate
Wasm footprints. Available sampled-RSS maxima use byte medians/p95 and the
`sampled-process-rss` budget metric under an explicit policy. This metric is not
an allocation or live-GC budget. Timing ratios preserve explicit null for zero baselines. No
green/red performance limit is inferred from completing the planned samples.
The existing one-second/five-second whole-CLI gates and wider T25 runtime,
allocation, GC, fuzz and CI obligations remain separate acceptance work.

Run this command alone under the existing one-CPU/4096-MiB/no-swap outer
launcher. `--idle-machine` records operator acknowledgment, not proof of
isolation or absence of other activity. There is no nested launcher or uncapped
fallback. The report checkpoint and workload/artifact/observation files use the
same fresh-write and retained-evidence rules as the compiler/whole-CLI owners.
