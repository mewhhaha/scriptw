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

The current private runtime ABI is layout 42. Stored values put the 64-bit scalar
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

An Array owns its indexed-storage fields directly. Its ordinary object header
remains separate, while logical length, occupied count, storage mode and backing
references share one 64-byte Array record under the pinned copying collector.
This replaces the former 48-byte Array and 64-byte indexed-storage records,
removing 48 bytes and one allocation per Array. Numeric presence bits, descriptor
promotion and inactive-backing retirement retain their existing rules. Frozen
template arrays use the same complete constructor. Snapshot readers use the
concrete Array witness for both ordinary and indexed state, without fabricating
a second object identity. These layout savings do not predict process RSS.

Eager Realm constructors allocate their initial dictionary metadata directly,
then bootstrap adds prototype and family properties. This avoids temporary
compact-storage conversion. Ordinary native methods retain compact metadata.

Temporal, Intl and binary-data graphs initialize on their first observable or internal use.
Each defining Realm retains the complete canonical graph after initialization,
independently of public namespace deletion or replacement. Own-key enumeration
can inspect unresolved property recipes without creating those graphs; reads
that expose property values resolve them through the defining Realm.
The callable shared by Array and TypedArray `toString` remains core-owned.

Date's ordinary global data descriptor holds one immutable defining-Realm recipe.
First value observation builds the canonical constructor/prototype graph through
`RealmEnsureDate`; no public global supplies a parent or receives a replacement
publication. Its two existing intrinsic slots retain the shared nonnull undefined
record while dormant. A closed state and private build capability prevent partial
observation and keep every normal slot read behind the demand gate. The graph
retains its Realm after the requesting program is dropped. This changes schema,
helper and package ownership in ABI 42, with no collector/stack/default change.

The deferred Date method slice contains 203 records / 7,312 gross bytes before
constructor, prototype, backing and String costs. Actual collector layout checks
confirm a 32-byte global recipe and a 96-byte Realm record. First use still
performs the original installation; all 51 Date native roots remain available.

The [accepted Date checkpoint](../runtime-date-demand-20261031.json) is a scoped
fresh-Realm startup CPU improvement. In its eight-source normal-cache follow-up,
startup improved 7.27% median / 11.59% mean at the original boundary and 7.70%
median / 9.38% mean at the ordinary boundary. First-demand and retained
repeated-use costs remain separate tradeoffs. Selected-case RSS increased about 0.15–0.46 MiB;
the gross dormant slice establishes neither a net memory gain nor an improvement
for every workload.

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

Both Cargo native images are linked raw, unchanged from the checked producer,
and aligned to 64 KiB so each starts on a page boundary of the executable
file. On Linux the loader maps an image's own range of the running executable
through the vendored Wasmtime's `Module::deserialize_open_file_range`
([patch](../../../vendor/wasmtime-47.0.0/PATCHES.md#file-range-deserialization)).
Nothing is decoded, hashed, copied or written at load, and no file is created
in the cache directory. Wasmtime owns each private copy-on-write mapping; its
clean pages are the executable's page-cache pages, shared with every process
running the same file, so untouched code stays unfaulted and touched code is
shared rather than private. Only the proved range is retained, at most once
per two fixed Cargo source profiles; no Engine, Module, Store or JavaScript
Realm is retained by this owner. Modules compiled at run time are a separate
owner with a different trust rule: see
[native module cache](#native-module-cache).

Admission is by origin and pairing, not by re-hashing at every start:

- **Origin.** `scriptw-runtime-image` constructs a `LinkedNativeImage` only
  from its two linked statics, and an `ExecutableImageRange` only after it
  proves that the static's exact address range is a read-only private mapping
  of the file behind `/proc/self/exe`. The static's pointer and length select
  an exact file-backed VMA in bounded `/proc/self/maps` data with matching
  device and inode, or the kernel's own main-ELF program headers place it in a
  read-only `PT_LOAD`. Renaming or unlinking the executable cannot redirect
  its kernel handle. Maps pathnames, external files, the cache directory and
  digest-selected blobs are never native-code authority.
- **Pairing.** The Cargo packaging step hashes the exact file it links and
  links that SHA-256 beside the image. Every load requires it to equal the
  native digest in the producer's independent manifest before any mapping or
  copy.

The earlier loader decoded a zstd container and compared the decoded SHA-256
with the manifest at every start. The image, the manifest, the digest and the
code comparing them are one file, so that hash could not detect a deliberate
change its own writer could not also hide. It did catch accidents: the decode
and write steps, which no longer exist, and accidental corruption of the file
(a truncated copy, bit rot, or an in-place relink of a running executable),
which now executes as it would in the process's own `.text`. Linux normally
refuses to open a running executable for writing (ETXTBSY), but not on every
kernel (6.11 dropped it; 6.13 restored it) or filesystem (NFS, FUSE, overlay
lower layers), and Cargo's linkers replace the inode rather than rewrite it. The check that remains meaningful at
load is the pairing above. A test in `scriptw-runtime-image` hashes the linked
bytes against the attested digest and reads them back through the proved file
range. Evidence of the bytes on disk is `CompilerIdentity`, whose SHA-256
covers the whole executable, images included.

Missing procfs, an image linked outside the main executable, a host page size
that does not divide the image's file offset, and non-Linux hosts keep the
same paired image through `Module::deserialize`, which copies the linked bytes
into anonymous Wasmtime memory. The load trace names the path taken
(`backing: executable-mapping` or `backing: copied`). Record it with the image
identities in footprint comparisons: the copy is private to each process and
the mapping is not.

The raw images make the executable larger than the compressed containers did;
the pages cost memory only when touched. RSS counts a shared mapped page once
per process and PSS splits it, so compare PSS or USS across processes.

RSS, anonymous memory and reclaimable file-cache observations must remain
separate. Native perf-map text hashing touches every code page, so map captures
with that hashing enabled cannot establish ordinary lazy-page residency.


## Native module cache

A module compiled at run time (a program P, or a runtime that is not the Cargo
image) is stored in `native-modules-v1` in the cache directory as the bytes
`Module::serialize` produced, one file per key, and a later process maps that
file with `Module::deserialize_open_file` instead of compiling
(`cache::NativeModuleCache`, called only from `compile_wasm_module`). Product
Engines configure no Wasmtime module cache. Wasmtime's zstd-compressed cache
(`wasmtime-modules-v1`) is now opened only by the Cargo runtime-image producer.

- **Key.** SHA-256 over a domain string, the SHA-256 of the exact Wasm bytes
  and Wasmtime's precompile compatibility hash of the Engine: target, ISA and
  compiler flags, tunables, features and the module version, which carries the
  native compiler fingerprint. The key selects a file. It admits nothing.
- **Admission.** Wasmtime checks the image's recorded target, flags, tunables,
  features and module version against the loading Engine before using it. An
  entry that is absent, empty, truncated, not an image or another Engine's
  image is a miss: the module is compiled and the entry replaced. The bytes
  after that header are not verified. Wasmtime's cache did not verify them
  either, but a damaged zstd frame usually failed to decode, and a damaged
  mapped image has no such accident to rely on.
- **Trust.** The cache directory was already trusted native-code storage and
  still is. What is new is that the file is the code's backing store while a
  process runs it: the mapping is private and copy-on-write, and Linux shows
  later writes to a file through the untouched pages of such a mapping. So an
  entry is never written in place. A writer fills an exclusively created
  temporary file and renames it over the key; pruning unlinks. Both leave
  every existing mapping on the inode it opened, and a concurrent reader sees
  the old complete file, the new complete file or no file. Anything else that
  truncates or rewrites an entry in place can crash or corrupt processes
  running it; `scriptw cache prune` only unlinks.
- **Durability.** An entry's bytes are synced before the rename that names
  it, so a power loss leaves the old name, no name or a whole entry, never a
  torn one. The image is mapped and executed without a digest: a full-length
  entry whose middle was damaged after publication (by anything other than
  Scriptw) is not detected, and every later run of that program fails until
  `scriptw cache prune` removes it. An empty, short or foreign file is a miss
  and is rewritten.
- **Bounds.** The tier keeps the module budget
  (`SCRIPTW_MODULE_CACHE_LIMIT_BYTES`, by default half the total) and the
  prune-to-70% policy, ordered by last use: a hit refreshes the entry's
  modification time. Opening the cache and loading from it never list the
  directory. A process lists it, and prunes it if it is over the limit, on its
  first insert and again when its own inserts cross the limit, so a process
  whose lookups all hit never pays for the directory's size and a directory
  only read from is never pruned. The function and program tiers follow the
  same rule.
- **Sharing.** Clean pages of an entry are page-cache pages shared by every
  process that maps it, and untouched code is never read. On a cache directory
  mounted `noexec` the mapping cannot be made executable; the entry is then
  read into private memory, as every hit was before.

Unix file semantics are assumed. On a host where a mapped file cannot be
renamed over or unlinked, replacing or pruning an entry that some process maps
fails and the entry stays.

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

The [combined ownership](../runtime-ownership-20261031.json),
[full resolver fold](../runtime-fold-20261031.json) and
[string-only fold](../runtime-strings-20261031.json) failed performance gates.
Their static graph/native-byte reductions do not establish acceptable runtime
costs. Production retains the accepted Array fusion and original resolver.

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

The [accepted fusion comparison](../array-owner-fusion-20261031.json) records
retained-object Scriptw/Scriptc RSS of 21.46/17.43 MiB and private dirty memory of
7.69/8.89 MiB on the same host. Higher RSS alone does not prove larger live
JavaScript ownership. The fixture's four-number child Arrays already use raw
number bits and a presence bitmap, with no StoredValue allocation per number;
immediate references would require a separate backing-representation change.
