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

This removes a second raw runtime allocation when the identical bytes already
exist in the executable. It changes Rust byte ownership, without changing the
R/P Wasm layout, foreign ABI, JavaScript object representation or GC policy.
Native code admission remains independent and retains its checked Wasmtime
mapping; borrowing raw Wasm does not borrow executable native memory.

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
The first actual Intl host operation resolves the shared provider once and
checks its complete identity before publication. This currently defers the
whole provider; first use still loads the complete selected Minimal data.
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
this Cargo authority. Portable decompression remains transient; first-load
peaks include its decoded buffer and the new backing file's page cache.
Both NativeHost and Portable native images use bounded compressed Cargo
storage and weak decoded leases. Each load independently checks the actual
decoded native digest against its manifest before constructing backing storage.
Subsequent mappings can reuse the already admitted private file without
retaining the decompressed buffer or paging an uncompressed executable array.

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
