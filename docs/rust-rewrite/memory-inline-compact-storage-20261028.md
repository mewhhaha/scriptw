# Inline compact property storage

Verified source `fb41daeafb83ce25f7c1842f2e943eea54f9b347` stores one- and two-key compact ordinary property records in inline GC fields; other compact extents keep the existing arrays. Runtime layout 31 requires matching runtime artifacts. The [current storage contract](contracts/compact-ordinary-data-storage.md) records representation, mutation, snapshot and ownership invariants.

**Verification:** 4,912 fresh native passes across 19 complete affected targets, including 517 AOT library tests, 1,056 engine library tests and 234 GC tests. The six focused controls count once. All 4,906 predecessor IDs remain present with exactly six additions. No inherited passes or original Test262 executions are credited.

R0 failed during test compilation and ran zero tests. R1 recorded three AOT and two GC passing rows, then failed a snapshot assertion; those five rows receive no final coverage credit. Two test-only repairs use public snapshot indices and distinguish a local Symbol from a legitimate well-known prototype Symbol. The retained ten-row proof binds equal compiler inputs across feature, both earlier local sources, final source and probe.

**Matched comparison:** [CI run 40640282249](https://github.com/mewhhaha/scriptw/actions/runs/40640282249), measured source `a19ba7395edfa93327ec01fbe53765ce50020c82`, passed 68/68 attempts and all 17 workload checksums. Six observer native controls passed separately. Current Scriptw/Scriptc RSS ratios range from 1.135458167 to 1.565176280. These observations do not establish causal savings or overall Scriptc parity.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS MiB | Scriptc RSS MiB | RSS ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.074434 | 0.134246 | 19.039062 | 14.292969 | 1.332058 |
| sort-reverse-512.js | 3.460040 | 0.251532 | 21.533203 | 14.335938 | 1.502044 |
| sort-sorted-512.js | 1.701260 | 0.252989 | 19.291016 | 14.478516 | 1.332389 |
| to-sorted-512.js | 4.200825 | 0.253155 | 22.630859 | 14.458984 | 1.565176 |
| join-1024.js | 1.820396 | 0.320010 | 19.498047 | 14.433594 | 1.350880 |
| join-4096.js | 5.959233 | 0.923996 | 20.927734 | 14.787109 | 1.415269 |
| join-mixed-1024.js | 1.939240 | 0.188678 | 18.960938 | 14.335938 | 1.322616 |
| numeric-loop-10000.js | 0.979550 | 0.085368 | 17.537109 | 13.591797 | 1.290272 |
| numeric-loop-50000.js | 3.286552 | 0.194938 | 17.339844 | 13.568359 | 1.277962 |
| folded-string-4096.js | 1.330003 | 0.446842 | 17.925781 | 13.535156 | 1.324387 |
| dynamic-string-4096.js | 1.631015 | 0.188363 | 17.919922 | 13.605469 | 1.317112 |
| integer.js | 2.254481 | 0.079509 | 17.830078 | 13.580078 | 1.312958 |
| objects.js | 1.357990 | 0.428414 | 18.699219 | 14.343750 | 1.303649 |
| retained-objects.js | 33.152960 | 21.438869 | 22.822266 | 20.099609 | 1.135458 |
| join-empty-1024.js | 2.539840 | 0.200025 | 19.812500 | 14.187500 | 1.396476 |
| join-singleton-1024.js | 2.658923 | 0.100396 | 20.082031 | 13.677734 | 1.468228 |
| join-small-1024.js | 2.801584 | 0.146415 | 19.925781 | 13.494141 | 1.476625 |

Measurements use two rounds with ten checked warmups per round and fourteen measured samples in total per runtime and workload. Both compiler-free consumers share the same Wasmtime executable, optimization and runtime settings; each Store remains alive at the RSS boundary. RSS includes code, runtime state and allocator history. The Rust toolchain is pinned to 1.99.1; cross-runner history remains unsuitable for causal subtraction.

**Allocation model:** the copying-collector layout for each two-reference, two-key ordinary record changes from 144 to 96 bytes including its header. Across 8,192 records, that calculates to 384 KiB less allocated storage and 16,384 fewer GC objects. This is a source-layout calculation, not measured RSS or a guaranteed win; dispatch and native code can offset it.

Four separate unscored v2 captures cover integer and retained-object ownership for each compiler. Warm observation 16 follows sixteen earlier calls. Page and section overlaps are nonadditive; mapped presence does not prove execution. Layout 31 and its current runtime key are recorded independently of prior keys and cross-runner native bytes.

The [JSON checkpoint](memory-inline-compact-storage-20261028.json) and [bounded inert archive](memory-inline-compact-storage-20261028-inert.tar.xz) retain exact sources, inventories, both failure prefixes, streams, controls, resource receipts and measurement metadata. Native executables and runtime packages are excluded.

Native verification used CPU0, a 4 GiB process-tree cap, no swap and bounded module retention, with no OOM event. Evidence processing used a separate 1 GiB cap. The separate seven-test toolchain/shell receipt records no kernel counters; it does not inherit either resource scope.

Refresh using the [precompiled comparison workflow](contracts/precompiled-comparison.md). The paired canonical graphs describe this admitted run. The V8 comparison and pinned conformance status are unchanged.
