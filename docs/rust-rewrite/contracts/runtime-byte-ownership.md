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
