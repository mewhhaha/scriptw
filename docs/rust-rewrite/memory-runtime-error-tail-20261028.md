# Shared runtime error preparation

Verified source `0658e6ae61981becc6dd338bcf1b1abd4b39feb3` shares messageful compiler error preparation through a typed runtime helper. Runtime layout 32 requires matching runtime artifacts. The [error-tail contract](contracts/runtime-error-for-throw.md) records operand, Realm, GC and completion invariants.

**Verification:** 4,917 fresh native passes across 19 complete affected targets, including 520 AOT library tests, 1,056 engine library tests, 353 language tests and 234 GC tests. Seven focused controls count once, including two existing cancellation controls. All 4,912 predecessor IDs remain present with exactly five additions. No inherited passes or original Test262 executions are credited.

The feature native run recorded three AOT passes before both new language fixtures failed on an incorrect expected Syntax diagnostic. The separate instrumented diagnostic also failed both fixtures and recorded the actual static message. These runs receive zero final coverage credit. The final correction touches only the language-test file. A retained six-row proof binds equal compiler inputs across feature, verified final and measured probe under both lockfile states.

**Matched comparison:** [CI run 40655566391](https://github.com/mewhhaha/scriptw/actions/runs/40655566391), measured source `68dae6ed898a5d31aed30f713df2c7b5add4c1e0`, passed 68/68 attempts and all 17 workload checksums. Six observer native controls passed separately. Current Scriptw/Scriptc RSS ratios range from 1.350485027 to 1.952793685. These observations do not establish causal savings or overall Scriptc parity.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS MiB | Scriptc RSS MiB | RSS ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.596504 | 0.134893 | 20.314453 | 12.710938 | 1.598187 |
| sort-reverse-512.js | 5.124734 | 0.362809 | 22.974609 | 12.759766 | 1.800551 |
| sort-sorted-512.js | 2.679942 | 0.368890 | 21.166016 | 12.716797 | 1.664414 |
| to-sorted-512.js | 6.286289 | 0.398295 | 24.642578 | 12.619141 | 1.952794 |
| join-1024.js | 2.803924 | 0.509539 | 21.396484 | 12.744141 | 1.678927 |
| join-4096.js | 9.376369 | 1.710341 | 23.103516 | 13.169922 | 1.754264 |
| join-mixed-1024.js | 3.037351 | 0.312389 | 20.984375 | 12.646484 | 1.659305 |
| numeric-loop-10000.js | 1.588312 | 0.089909 | 19.320312 | 12.056641 | 1.602462 |
| numeric-loop-50000.js | 5.387957 | 0.204979 | 19.671875 | 12.080078 | 1.628456 |
| folded-string-4096.js | 2.312003 | 0.735151 | 19.781250 | 12.003906 | 1.647901 |
| dynamic-string-4096.js | 2.621557 | 0.270461 | 19.785156 | 11.986328 | 1.650644 |
| integer.js | 4.335662 | 0.062572 | 19.970703 | 12.050781 | 1.657212 |
| objects.js | 2.203375 | 0.733403 | 20.748047 | 12.890625 | 1.609545 |
| retained-objects.js | 56.380347 | 38.452680 | 25.015625 | 18.523438 | 1.350485 |
| join-empty-1024.js | 3.817516 | 0.273632 | 21.792969 | 12.566406 | 1.734224 |
| join-singleton-1024.js | 4.020867 | 0.128921 | 22.023438 | 11.921875 | 1.847313 |
| join-small-1024.js | 4.215936 | 0.192590 | 21.970703 | 11.992188 | 1.832085 |

Measurements use two rounds with ten checked warmups per round and fourteen measured samples in total per runtime and workload. Both compiler-free consumers share the same Wasmtime executable, optimization and runtime settings; each Store remains alive at the RSS boundary. RSS includes code, runtime state and allocator history. Rust is pinned to 1.99.1; cross-runner history is unsuitable for causal subtraction.

The caller retains prototype selection, message resolution and throw completion. The shared runtime tail allocates a fresh error before canonical runtime-name lookup and diagnostic publication. Allocation-only normal cancellation, message-less TypeErrors, native-parser Syntax errors and ConstructorName diagnostics remain unchanged. No static-byte or RSS-saving estimate is inferred from source call counts.

Four separate unscored v2 captures cover integer and retained-object ownership for each compiler. Warm observation 16 follows sixteen earlier calls. Page and section overlaps are nonadditive; mapped presence does not prove execution. Layout 32 and its current runtime key are recorded independently of prior keys and cross-runner native bytes.

The [JSON checkpoint](memory-runtime-error-tail-20261028.json) and [bounded inert archive](memory-runtime-error-tail-20261028-inert.tar.xz) retain exact sources, inventories, failed runs, streams, controls, resource receipts and measurement metadata. Native executables and runtime packages are excluded.

Native verification used CPU0, a 4 GiB process-tree cap, no swap and bounded module retention, with no OOM event. Evidence processing used a separate 1 GiB cap. The older syntax diagnostic omitted an affinity field; its pinned driver enforces CPU0, and the receipt omission remains explicit.

Refresh using the [precompiled comparison workflow](contracts/precompiled-comparison.md). The paired canonical graphs describe this admitted run. The V8 comparison and pinned conformance status are unchanged.
