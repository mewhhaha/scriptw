# Runtime import compaction memory checkpoint

Measured and verified product source: `0bd2c2ea7a4a98146d3dac5734afa0f3812de5c3`.
Date: 2026-10-27. This checkpoint does **not** establish Scriptc memory parity.

The final program-module pass removes unused reserved-runtime function/global
imports and remaps all semantic references and diagnostic indices. It preserves
defined bodies, types, segment identity, native-template ordinal witnesses and
all host/foreign imports. Instantiation publishes only exports requested by the
actual program. Later native Function/eval and specialized RegExp compilation
resolve additional imports from the same retained runtime instance.

## Matched measurements

The [complete paired comparison](precompiled-comparison.md) retains the original
17 workloads, checksums, shared compiler-disabled consumer, two rounds, ten
warmups per round and seven samples per round. Both runtimes passed all cases.
Scriptw's median post-execution process RSS is **1.3–1.7× Scriptc's**.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 19.5 MiB | 13.5 MiB |
| Objects | 20.5 MiB | 14.3 MiB |
| Retained objects | 26.8 MiB | 20.0 MiB |

These are medians of 14 samples, with each Store alive. RSS includes host code,
runtime mappings and allocator history; it is neither live JS heap nor peak
memory. Separate runner cohorts cannot establish a causal percentage saving.
The ordinary V8 comparison remains a separate, explicitly dated measurement.
The [measured CI run](https://github.com/mewhhaha/scriptw/actions/runs/40478463964)
uses clean Git compiler fingerprint
`5cb6815ef01444155471c19cb984a20d637efe853c22135c4df6123b5e1ac439`.

An inert, checksum-bound census of all 17 actual program artifacts finds 40–47
runtime function imports and 18–22 runtime global imports, compared with
1,058 and 72 respectively in the previous cohort. The complete runtime remains;
this import-count change alone is not an RSS attribution.

## Verification

The release workspace/all-target check passed. All **4,858 native cases across
19 disjoint target inventories** and **2,365 exact original pinned Test262
executions** passed. Focused controls overlap those inventories and are not
added again. This is not a full pinned-suite refresh; the README's existing
102,990/4 full-suite status remains separate.

An original structural test failed its global source-reference census after a
new private native-compilation control added a legitimate Structured-mode use.
The recorded repair explicitly inventories that named, cfg(test)-gated consumer;
it preserves the private domain, production consumer checks and all 37 test IDs.
The rebuilt target passes all 37 cases; the other 18 completed target streams
are retained unchanged. Original failed streams remain in the evidence.

Verification used the same product compiler inputs. The integration-test census
repair is outside the compiler-input closure. Local fingerprint
`44b4da1e5c566933f3e650a9014869509a0b0ec17664c593f6c96ae1f04ae033`
additionally includes the retained ignored standalone regress dependency lock.
Both exact input closures and that lock are archived; the difference is explicit.

All work used one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and a bounded
one-entry/64-MiB retained-module cache. The long original conformance process was
OOM-killed at that cap. Its checkpoint records 2,100 successes and no failures.
The remaining 265 original IDs completed in fresh processes of at most 16 IDs;
the union is exactly the original 2,365-ID selection without duplicates or skips.
Original interruption evidence and incomplete reports are preserved. Successful
retry resource counters belong to the fresh scope; no missing original after
counters are synthesized.

## What remains resident

A separate local diagnostic used one unchanged integer workload, one freshly
owned native producer image and the same compiler-disabled public runtime
configuration. It is not a warmed benchmark or a CI attribution. At Store drop,
requested Rust allocations returned exactly to the pre-Store level, while RSS
remained 17.71 MiB. Runtime native mappings retained 3.75 MiB of resident pages;
host executable pages also remained resident. Dropping the runtime removed its
native mappings. Mapping permissions, exact API endpoints and raw smaps are
archived. These observations do not identify which functions executed or prove
that particular pages can safely be removed.

Separate exact owned-image probes measured about 0.26 MiB of decoded native
metadata and 0.50 MiB of retained Rust allocations after module admission/type
registration. These quantities exclude mmap residency and allocator rounding;
they do not explain the whole multi-MiB execution gap. The next implementation
batch targets compact value backing and repeated intrinsic publication code.

Unscored CI mapping traces are also preserved separately. Their asynchronous
sample maxima are not phase-aligned and can miss execution entirely; they are
not used as canonical peaks, savings or live-heap measurements. Downloaded native
artifacts were never loaded or executed locally.

[Checked verification record](memory-runtime-imports-20261027.json) ·
[Lossless raw verification archive](memory-runtime-imports-20261027-verification.tar.xz).
The record enumerates member hashes and bounded expansion sizes. Native images,
executables and the private next implementation batch are excluded. Regenerate
the paired figures with the commands in the comparison report.
