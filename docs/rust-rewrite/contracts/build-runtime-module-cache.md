# Build runtime module-cache admission

This deliberately extends the 2026-10-11 function-stencil reuse contract. Native
Cargo builds may now reuse Wasmtime's existing bounded native-module cache.
The earlier producer always called `precompile_module`; pinned Wasmtime 47
explicitly bypasses module caching in that API, even with a configured cache.

Every build still emits and fully admits its current `RuntimeBuildArtifact`
before native compilation. The package and manifest bind the complete current
compiler fingerprint, target, configuration, compilation modes, exact raw R
key, package digest and native-image digest. No narrower source fingerprint,
saved package, caller-supplied native image or custom native-file loader is used.

When Cargo's `HOST` equals `TARGET` and the existing module cache opens,
`RuntimeImageCompiler` configures that cache on the product build Engine and
calls `Module::new` on the freshly admitted raw R, then serializes the temporary
Module. Wasmtime's cache key includes exact Wasm, target, ISA/compiler flags,
tunables, features and module version. Cache deserialization and incompatible
entry handling remain Wasmtime's responsibility. The temporary mapping and
build Engine do not reach any product Engine or retained-module owner.

The package-version-only native cache had a real invalidation gap: its vendored
47.0.0 namespace omitted edited native compiler source and lockfile changes.
Both Cargo producers now derive `scriptw-native-compiler-v1` over Cargo.toml,
Cargo.lock, all vendored source and the Engine/runtime-image feature manifests.
Registry dependencies are checksum-bound by the lockfile; patched dependencies
are content-bound. This is native compiler identity, not a replacement or subset
of Scriptw's full compiler identity. Scriptw source edits still invalidate program
Wasm and produce a fresh current-source package, but do not churn native cache
keys when exact R and native dependencies/configuration stay equal.

Build and product Engines use the same custom Wasmtime module version containing
that identity. Configuration schema is4. A mismatching native dependency version
misses the ordinary module-cache key and fails native deserialization. Build
stencils and product stencils both use `NativeFunctionCache`, which hashes the
same identity and Cranelift's opaque key into the existing flat bounded store.
Its budget, accounting and pruning remain unchanged. Unversioned legacy stencil
entries are not accepted; the first new namespace starts cold. Program-Wasm
cache storage and its full compiler key remain unchanged. Unrelated vendored
edits may cause conservative native misses.

Cross-target builds never open the module cache. Cross-target and unavailable
cache paths retain `precompile_module` on the same checked bytes. Incremental
function caching remains enabled independently and warms native-module misses.
The existing module-cache directory, budget and prune/status ownership remain
unchanged. Cargo now watches `SCRIPTW_MODULE_CACHE_LIMIT_BYTES` as well as the
existing cache inputs. Explicit target selection and serial native compilation
remain unchanged.

Since 2026-11-24 the producer is the only user of that Wasmtime cache
directory (`wasmtime-modules-v1`): product Engines map whole modules from
`native-modules-v1`
([contract](runtime-byte-ownership.md#native-module-cache)). The producer's
directory keeps the module budget for itself, is listed and pruned when the
producer opens it, is cleared by `scriptw cache prune` and is reported by
`scriptw cache status` as `build-module-cache`. Entries that product
processes wrote there before that date are never read again. Only the producer
lists or prunes that directory, so on a machine that never builds Scriptw they
stay, within that budget, until `scriptw cache prune`.

Meaningful controls compare direct precompile with cold and warm cached images,
check actual cache counters, execute both in a Fast product Engine, and require
misses plus correct execution after raw Wasm, allocator or optimization changes.
Dependency-version controls cover module misses/rejected foreign native images,
distinct stencil generations sharing one budget, actual stencil recompilation,
lockfile/feature/profile/vendor changes and stable native identity across Scriptw
edits. Missing dependency input rejects fingerprinting.
Existing complete embedded-manifest/raw-package parity and compatibility
controls remain. Source fingerprint controls include both new helper files.

Actual `781feba52` verifies all 23 focused controls, all 4,825 tests in the eleven
complete optimized native scopes and all 27 structure controls. All 4,846 earlier
IDs remain passing and six controls are added. The staged CLI retains all 30
Promise.try IDs, four earlier repaired RegExp/ArrayBuffer IDs and both Syriac
witness IDs. Two direct actual-Cargo-producer replays preserve the package,
native image and manifest byte-for-byte and each records one module-cache hit.
Native compilation is 1.986/2.032 seconds after the 676.217-second module miss;
whole warm producers are 41.410/42.037 seconds. Fresh emission/admission,
packaging and relinking remain separate costs. The actual Cargo custom-build
profile is opt-level0; its release target does not prove producer optimization.
These observations have concurrent full-suite cloud load, not an idle or
general build-speedup claim. See the
[source-bound checkpoint](../native-runtime-cache-checkpoint-20261013.json).
Full pinned conformance and task acceptance remain pending.
