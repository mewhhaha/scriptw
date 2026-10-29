# Runtime byte ownership

`RuntimeArtifact` owns an immutable Wasm byte sequence. Its outer `Arc` shares
the artifact, layout and data boundary across programs; `bytes()` exposes a
borrowed `&[u8]` tied to that owner.

Emitted Wasm transfers its existing `Vec<u8>` into the artifact. Safe external
and disk-package admission copies the admitted payload into owned storage, so
changing or dropping the caller's package cannot invalidate the runtime.

Cargo-owned packages use their existing immutable-build authority. Its
constructor requires a static package and the producer's source identity.
After envelope, checksum, profile, Intl, section, data and layout admission,
the runtime borrows the package's static Wasm payload. The lifetime is retained
through admission; a temporary input cannot become a static borrowed artifact.
The producer's complete function-body validation is reused only through this
existing private build-output boundary. Ordinary external packages retain
per-use body validation and cannot acquire that authority.

Cargo also publishes a bounded runtime receipt after complete raw admission.
It records the checked function/global layout, pool and Intl frame spans,
source/profile identities, raw hashes and largest body extent. An opaque token
created only by the unsafe immutable-build constructor binds that receipt to
the exact static package and compilation metadata. Ordinary compilation and
native loading reuse those facts without hashing or parsing the complete raw R
payload again. Pool reconstruction still hashes its canonical generated data
and matches the producer's two admitted data digests. Raw Wasm remains available
for output, explicit diagnostics and native compilation when required.

The safe package/cache paths continue to validate actual bytes and reconstruct
their layouts. A parsed receipt or static lifetime cannot mint build authority.
The native manifest's package digest can be reused only for the exact package
pointer/length retained by the admitted owner; source, target, configuration,
runtime key and decoded native-image digest checks still precede deserialization.

Canonical Intl identity bytes use compile-time name order and direct byte
emission. Fixed-width receipt, manifest and private binary execution-facts readers
establish unique encodings while parsing. Precompiled consumers use the existing
validated foreign-declaration constructors; public JSON declarations retain
their canonicality check. Consumers do not re-serialize validated values.

This removes a second raw runtime allocation when the identical bytes already
exist in the executable. It changes Rust byte ownership, without changing the
R/P Wasm layout, foreign ABI, JavaScript object representation or GC policy.
Native code admission remains independent and retains its checked Wasmtime
mapping; borrowing raw Wasm does not borrow executable native memory.

## Compact GC records

The current private runtime ABI is layout 39. Stored values put the 64-bit scalar
before the 32-bit tag and GC reference. Under the pinned copying collector's
16-byte object header and alignment, this reduces each record from 48 to 32
bytes while retaining the complete scalar and reference payload. The separate
call ABI still passes tag, scalar and reference in its original order. Host
argument construction uses the schema's field ordinals when storing a value.

Property descriptors retain separate closed types for legal stored attributes
and query masks. Their four flag bits use 32-bit storage, reducing a complete
descriptor from 48 to 32 bytes under the same collector layout. Field accesses
and snapshot projections derive from the common schema. Old runtime packages,
native caches and Portable bundles must match that current ABI.

These are per-record layout sizes, not measured process RSS reductions. GC
headers, backing arrays, native code, compiler metadata and allocator retention
remain separate costs.

Eager Realm constructors allocate their initial dictionary metadata directly,
then bootstrap adds prototype and family properties. This avoids temporary
compact-storage conversion. Ordinary native methods retain compact metadata.

Temporal, Intl and binary-data graphs initialize on their first observable or internal use.
Each defining Realm retains the complete canonical graph after initialization,
independently of public namespace deletion or replacement. Own-key enumeration
can inspect unresolved property recipes without creating those graphs; reads
that expose property values resolve them through the defining Realm.
The callable shared by Array and TypedArray `toString` remains core-owned.

## Execution stacks

Embedded calls reuse one 64-MiB execution worker per calling host thread.
Borrowed work joins before returning; the worker retains no Store or Realm.
Nested entry and overlapping jobs from an external Rayon caller use independent
scoped stacks. Compilation keeps its temporary workers. Touched execution-stack
pages can remain resident until the owning caller thread exits.

## Compiler scratch ownership

The pinned Cranelift compiler retains at most two small idle compilation
contexts. Successful returns clear final IR immediately. Contexts exceeding
explicit instruction, value, block, local or native-body reuse limits are
dropped with their translator and codegen scratch storage. Module assembly
also checks idle owners; active compilation jobs retain exclusive ownership.
Compiled Modules, native code and GC/type registrations keep their own owners.
The limits bound retained work sizes rather than claiming an allocator census.
The [vendored patch](../../../vendor/wasmtime-internal-cranelift-47.0.0/PATCHES.md)
records the exact limits and upstream provenance.

## Deferred Intl providers

The Cargo runtime producer admits all twelve Minimal component consumers and
derives all six supported-value catalogues. It publishes a small metadata
snapshot bound to the compiler source, complete canonical identity and every
physical frame hash. Compilation borrows the frames in the existing static R
package; the snapshot contains no second copy of those frames.
The dependency-free `scriptw-intl-build-data` crate owns the sole unsafe trust
constructor. Its opaque token is consumed by `scriptw-intl`'s entirely safe
metadata reader; Intl retains its workspace `unsafe_code = "forbid"` rule.

The runtime artifact retains that admitted compilation owner, including across
native Function/eval/created-Realm compilation. Compilation metadata and UTC or
fixed-offset compatibility checks do not construct the operation providers.
Locale canonicalization/maximize/minimize and NumberFormat/PluralRules use
separate typed kernels that admit only the Locale and Number consumers they
need. Their full identities come from the admitted compilation owner; a Number
kernel cannot request a DateTime operation. These consumers share the canonical
image owners with later complete-provider initialization. Other operations
resolve the complete selected provider and check its identity before publication.
Custom/external admission remains complete rather than acquiring build authority.
Hostless standalone artifacts without Intl metadata have an explicit absent
provider. Custom/external component admission retains its checked consumers;
Portable packages still receive full Wasm-body/layout and exact-frame checks.

## Native backing storage

On Linux, admitted Cargo native images can use private disk-backed mappings.
The loader creates an unnamed `O_TMPFILE|O_EXCL` inode under the configured
module-cache directory, writes only the independently admitted image, syncs it
and publishes an owner that exposes no mutation path. Wasmtime owns each
private executable mapping and its file lifetime. Only immutable backing
storage is shared, at most once per two fixed Cargo source profiles; no Engine,
Module, Store or JavaScript Realm is retained by this owner. The normal bounded
module cache retains its existing policy.

This permits unused code pages to remain unfaulted and clean file pages to be
reclaimed. Tmpfs/ramfs are rejected for this optimization: moving bytes to
unreclaimable memory would merely move them outside process RSS. Unsupported
or read-only filesystems retain ordinary slice loading of the same admitted
native image. Files, caller bytes and native disk-cache entries cannot enter
this Cargo authority. Both source profiles stream decompression and SHA-256
through a 64-KiB output buffer with an 8-MiB zstd history limit. Exact decoded
length, single-frame EOF and the independent native manifest must agree before
the private file is published. This path allocates no decoded-image-sized buffer.
Unsupported backing filesystems retain bounded-source decoding into the existing
weak byte lease; concurrent fallback loads can share that transient owner.
Subsequent mappings reuse the admitted private file without retaining decoded
storage, an Engine, or a Module.

The packed Cargo array remains part of the executable. On Linux, its pointer
and length select an exact read-only file-backed VMA in bounded
`/proc/self/maps` data. The loader opens only `/proc/self/exe`, requires matching
device and inode, checks the complete file range and reads exactly that range
through a bounded reader. Taking the linked array's address does not read its
payload through the mapping. Renaming or unlinking the executable cannot
redirect its kernel handle. Maps pathnames, external files and digest-selected
cache blobs are never native-code authority. Missing procfs, a shared-library
image, or an unprovable range uses the original immutable static source.

Reading through the executable file avoids faulting the complete packed array
into that process's read-only mapping. The input and output still occupy
reclaimable kernel file cache; a smaller RSS is not a claim that those physical
cache pages disappeared. No eviction or collection is added to benchmark runs.

RSS, anonymous memory and reclaimable file-cache observations must remain
separate. Native perf-map text hashing touches every code page, so map captures
with that hashing enabled cannot establish ordinary lazy-page residency.

## Runtime string bootstrap

R's constant strings are initialized by one loop over checked static descriptor
rows. Each row retains the original string ordinal and exact passive UTF-16
span, in the original allocation order. Rows are appended to the existing
private static segment before its pool boundary is sealed; no extra data
segment or program-owned literal can change that runtime table. This replaces
unrolled initialization code while preserving GC object/string identities.

Absent accessor snapshots and initial native-function home/public-prototype
snapshots reuse the Realm's canonical immutable stored `undefined` value.
Descriptors, functions, contexts and mutable edges remain distinct; source
functions still retain their actual home object for `super` semantics.

Native functions store their own `length` and `name` in the existing compact
property arrays, with shared immutable shapes carrying primitive keys and the
correct data attributes. Values and mutable storage belong to each function.
Ordinary reads use those arrays directly. Reflection creates descriptor
snapshots when needed; structural or attribute changes promote the object to
the ordinary dictionary representation. Insertion order, readonly and
non-enumerable attributes, configurability and deletion remain observable.
This defers metadata descriptors; constructor and prototype graphs retain their
normal initialization and per-Realm identities.

## Static regexp construction

Static regexp descriptors remain in P metadata. Literal and optimized
constructor publication uses a checked P-relative span and the same bounded
copy as dynamic candidates, rather than emitting one Wasm operand per byte.
Descriptors already present in R receive one deduplicated P copy. Their wire
bytes and matching semantics stay identical; construction publishes the GC
program only after the copy is complete.

## Observe resident memory

On Linux, capture a serial diagnostic using a built `runtime_baseline` example:

```sh
python3 -B scripts/limited_verification.py -- \
  python3 -B scripts/profile_memory_maps.py \
  --runner target/release/examples/runtime_baseline \
  --output target/memory-maps
```

The output directory must be new. The runner compiles the checked print,
integer and retained-object controls to real Wasm. Three measured invocations
after one warmup pause at synchronous guest-start and guest-body-finished
callbacks. The parent records bounded `/proc/PID/smaps` snapshots and admits
the actual R/P native-text map identities before acknowledging each callback.
These instrumented runs attribute residency, not ordinary execution latency.

Raw snapshots retain RSS, proportional RSS, private/shared residency and
mapping permissions. Anonymous ranges and the kernel's `Anonymous` counter do
not identify a JavaScript heap. Mappings intersecting native text are recorded
whole; no proportional allocation of their RSS to individual functions is
invented. GC capacity, virtual reservations and process RSS remain distinct.
