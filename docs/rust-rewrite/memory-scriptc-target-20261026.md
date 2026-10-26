# Scriptc execution-memory target

Source `b1ffc3ffd9f62d990c6f88058c4684018ba3a776`, verified 2026-10-26.
The matched Scriptc process footprint is the target. Parity remains open:
the [current comparison](precompiled-comparison.md) measures Scriptw at
1.9–2.8 times Scriptc's post-execution RSS across the 17 workloads.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 25.9 MiB | 13.4 MiB |
| Objects | 30.5 MiB | 14.2 MiB |
| Retained objects | 41.4 MiB | 19.9 MiB |

Three Sol agents reviewed native residency, GC layout and allocation algorithms.
The implemented batch streams exact Cargo native images through a 64-KiB output
buffer into private disk backing. Linux can read the packed input through the
actual executable inode and symbol range, avoiding faults across its whole
read-only mapping. The decoder retains exact length, digest, window and EOF
checks; no external native cache gains authority. The decoded-image-sized
temporary allocation is absent on the private-backing path.

Sort comparator calls reuse the private sort's immutable value snapshots while
retaining fresh argument vectors. Completed string construction reuses storage
when capacity equals length. Tests cover retained and mutated arguments,
reentrancy, proxies, abrupt completion, and exact/spare UTF-16 storage. One
proposed descriptor optimization was withdrawn after source inspection confirmed
the canonical-undefined fast path already existed.

The full workspace/all-targets type check passed. Nine disjoint native scopes
passed **2,631 cases**, with zero failures or ignored tests; focused embedding,
profiling and array-buffer checks also passed and overlap those scopes.
The candidate harness discovered and ran **463 exact original pinned Test262
executions** for Array/TypedArray sorting, joining and locale conversion:
463 successes, zero bugs, crashes, unsupported cases or not-implemented results.
All 53 selected tooling controls and all 15 source-identity recipes passed.
Integration checked 323 observations across 19 fresh consumers and one deliberate
output rejection; these integration observations contribute no benchmark scores.

Verification and measurements used one CPU, a 4-GiB process-tree cap, no swap,
grouped OOM and bounded caches. No OOM events occurred. This is a selected
source-bound checkpoint; the full pinned Test262 publication was not refreshed.

The local before/after attempt stopped after its frozen baseline executables
disappeared. Candidate and CLI executables subsequently disappeared too. Its
partial observations remain archived and contribute **zero performance samples**.
The completed [CI run](https://github.com/mewhhaha/scriptw/actions/runs/40401356113)
is the authoritative same-runner Scriptw/Scriptc comparison. Earlier measurements
from other runs are not a controlled before/after experiment.

RSS includes code, host/runtime and warmup history; it is not live JavaScript
heap size. Packed input and the native backing still occupy reclaimable kernel
file cache. The [ownership contract](contracts/runtime-byte-ownership.md) explains
these limits and the fallback behavior.

[Verification record](memory-scriptc-target-20261026.json) ·
[Lossless raw verification archive](memory-scriptc-target-20261026-verification.tar.xz) ·
[Matched measurements and raw evidence](precompiled-comparison.md).

The verification archive contains 266 regular files, including complete original
command streams and the failed local attempt. Its recorded SHA-256, member
hashes and bounds permit checking without running native code. The 64-MiB xz
dictionary compresses repeated Cargo diagnostics losslessly; expansion is bounded
at 512 MiB, with at most 64 MiB per member. Executables and serialized native
images are excluded.
