# Loaded executable identity and startup experiments

Three identities describe the running compiler, and they are different types
with no conversion between them:

| Identity | Type | What it binds | Cost | Consumers |
| --- | --- | --- | --- | --- |
| Evidence | `CompilerIdentity` | build source and SHA-256 of the whole loaded executable | one full read, about 0.2 s for 200 MiB | `compiler-identity`, Test262 execution identity and publication, differential workers, benchmark reports (a benchmark host report under `SCRIPTW_BENCHMARK_REPORT_IDENTITY=build-source` carries a null digest instead, and its driver hashes the host file) |
| Build source | `CompilerBuildIdentity` | the embedded source fingerprint and revision only | none | precompiled-image and portable-bundle admission, which also check their own digests |
| Cache | `CompilerCacheIdentity` | build source and one image binding below | none by default | program-Wasm and runtime-artifact caches; execution-profile program keys |

`CompilerIdentity::current()` captures SHA-256 of the actual loaded
executable, opened through `/proc/self/exe` on Linux. It validates unchanged
length/modification metadata around the read and rejects missing, unreadable,
changing or empty images. Reports retain this full SHA-256 evidence whichever
binding keys the caches. Only evidence producers call it: compilation, cache
lookup and precompiled-image admission do not. A source fingerprint alone
never authorizes a compiler-bound cache.

`SCRIPTW_COMPILER_CACHE_IMAGE_HASH=build-id|blake3|sha256` selects a closed
compiler-cache image binding, parsed once per process. Each has its own
fingerprint domain, so entries of one binding are never read by another:

- `build-id` (`scriptw-program-cache-compiler-v6-build-id`): the
  `NT_GNU_BUILD_ID` note of the loaded image. The linker computes it over the
  complete linked output, so two different links have different IDs. The
  process reads it from its own kernel-loaded program headers (`AT_PHDR`,
  through `/proc/self/mem`), not from a path, and admits it only from a
  non-writable, file-backed `PT_LOAD`. An ID shorter than 16 bytes (lld's
  `fast` style), a zeroed or filled one and a repeated note are refused. It
  reads no executable bytes beyond the headers and the note.
- `blake3` (`scriptw-program-cache-compiler-v5-blake3`) and `sha256`
  (`scriptw-program-cache-compiler-v4`, the evidence digest): a serial hash of
  the whole opened executable under the same metadata checks.

With the variable unset the binding is `build-id` when the loaded image has an
admissible ID and `blake3` otherwise: no build ID, not a 64-bit little-endian
Linux image, procfs unavailable, no `PT_PHDR` (a static non-PIE link), an ID
shorter than 16 bytes or constant-filled, a `PT_NOTE` over 64 KiB, or any
malformed note. The unset default therefore depends on the environment as well
as the binary: the same executable can bind `build-id` where `/proc/self/mem`
is readable and `blake3` where it is not. Caches only miss across the two, but
an execution profile's program key includes the fingerprint and is refused in
the other cohort, so pin the variable when profiles move between environments.
An explicit selection never falls back: an explicit `build-id`
without an admissible ID, an unknown value and an image read failure disable
compiler-bound caches, and the diagnostic accessor returns the concrete error.
The outer cache fingerprint remains SHA-256 over the domain, the validated
build-source digest and the image identity; it never labels a BLAKE3 digest or
a build ID as SHA-256.

The build ID assumes an honest build whose executable is the linker's output.
It does not detect bytes edited in the file after linking while the note is
kept, and a link given a constant `--build-id=0x...` has no per-build ID.
Select `blake3` or `sha256` for such executables. The cache directory holds
native code and is trusted to the same degree under every binding.

The experiment pins the maintained official [`blake3` crate, version1.8.7](https://github.com/BLAKE3-team/BLAKE3/blob/1.8.7/Cargo.toml).
Only its standard single-threaded API is used; the optional Rayon and mmap
features are disabled. The official [implementation repository](https://github.com/BLAKE3-team/BLAKE3)
describes its runtime SIMD detection and feature choices. No speed improvement
is accepted until paired startup evidence is retained.

Private validated build metadata is shared with portable-bundle source-only
admission. It supplies source identity without hashing the executable; it does
not supply executable evidence. External bundle checksums, source/profile/Intl
and full raw-Wasm/layout validation are unchanged.

`compiler_cache_identity()` exposes the actual admitted binding, image identity
and domain. New boundary examples report those separately from the mandatory
`executable_sha256`. The ordinary six benchmark harness files remain unchanged;
nondefault cache-image cohorts must not be merged with their default reports.

The native-module tier is not keyed by `CompilerCacheIdentity`. Its key is the
exact Wasm and the Engine's precompile compatibility hash, whose module version
carries the native compiler fingerprint
([layout and trust](runtime-byte-ownership.md#native-module-cache)), so a
Scriptw source edit that emits the same Wasm for the same native compiler
reuses the entry. The tiers under the cache root are
`cranelift-functions-v1`, `native-modules-v1`, `program-wasm-v1` and the
build-only `wasmtime-modules-v1`. None of them is listed when a process opens
or reads it; each is listed and bounded by the processes that insert.

Runtime profiles additionally report R and P module loading separately. A P
cache hit is not evidence of an R hit. Optional embedded-image spans cover
manifest/digest admission, the once-per-profile proof of the image's range of
the executable, and native deserialization. They are nested within R loading and the overall
module phase. Matched diagnostic images retain their existing separate access
label; their production cost remains in R loading. Cache-owner observations
report actual entry/image-byte limits and retained native image accounting,
independent of GC capacity or process RSS.

The `scripts/startup_retention_cohort.py` experiment uses the same frozen
executable in A/B/B/A order:1entry/64MiB versus2entries/128MiB. It changes only
each measured payload's positive cache-limit environment inside the existing
4GiB/one-CPU cgroup, retaining all code in the existing accounted cache and
creating fresh Stores/Realms. It never pools mutable JS state or changes global
verification budgets. The old30fc executable cannot report R cache outcomes or
owner occupancy: receipts mark those unavailable and preserve its P-only label.

The completed observations are recorded in the
[measured checkpoint](../performance-six-workstreams-20261024.md). Reproduction
requires the matching frozen executable, rather than the current rebuilt runner:

```sh
python3 scripts/limited_verification.py -- python3 scripts/startup_retention_cohort.py \
  --runner /path/to/frozen-30fc/runtime_baseline \
  --fixtures target/performance-work-20261024/after/baseline/workload \
  --source-commit 30fc1931471646253e339100a6d392b2337a373b \
  --time /path/to/gnu-time \
  --output-dir NEW_DIRECTORY
```

The source-commit and loaded-executable checks reject a subsequently rebuilt
binary at the same path. The completed checkpoint includes the Rust SDK,
cache identity/read-failure and loader regressions, driver cohort checks, and
serial measurements under the unchanged resource budget.
