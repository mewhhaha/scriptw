# Initial GC capacity experiment

The 16 MiB product default remains unchanged. A same-host 16/8/8/16 MiB
diagnostic on source `623f722a1d1be62bdf603f8d1f791f522f68efc4` found that
starting smaller did not justify a default change.

Each cohort used the existing matching-R diagnostic path, size optimization,
backtracking allocation, a fresh Store/Realm per invocation, ten checked warmups
and seven measured samples. All three input files and expected outputs match
the published 17-workload corpus. All 204 invocations passed their original
checksums. No collection was added to the workloads or measurement endpoint.

| Workload | First 16 MiB | First 8 MiB | Second 8 MiB | Last 16 MiB |
| --- | ---: | ---: | ---: | ---: |
| Integer | 2.508 ms | 2.560 ms | 2.522 ms | 2.522 ms |
| Retained objects | 38.295 ms | 79.587 ms | 79.529 ms | 38.198 ms |
| Copying sort, 512 entries | 4.477 ms | 5.107 ms | 4.978 ms | 4.491 ms |

These are per-process execution medians, including Realm initialization and
completion handling. The retained-object workload grew from 8 to 16 MiB in
every measured smaller-capacity observation. Its 16 MiB controls stayed at
16 MiB. Integer and copying-sort observations retained their requested capacity.
Capacity snapshots are not live-byte or collection counters.

Diagnostic process RSS included matching-image, compiler and allocator history:
per-process median endpoints ranged from roughly 212 to 527 MiB. That variation
prevents a useful RSS saving claim from this run. These values are separate from
the compiler-disabled Scriptw/Scriptc comparison in the README. Only comparisons
within this explicit diagnostic share its matching-image and host-ISA treatment.

Verification passed all 18 runtime-profile native controls, all seven diagnostic
example tests and all 20 Python measurement controls. The native controls include
independent Engine observation of 8 MiB and use of the matching runtime image.
No broad native-suite or original Test262 run is claimed for this diagnostic-only
extension. The earlier layout-31 checkpoint retains that separate evidence.

The native check and experiment each used one CPU, a 4 GiB process-tree cap,
no swap and bounded module caches. Neither had an OOM. The experiment's cgroup
peak was 810,455,040 bytes, including producer/cache activity.

The [JSON receipt](memory-gc-capacity-20261028.json) retains all measured samples,
capacity observations and source identities. The
[inert archive](memory-gc-capacity-20261028-inert.tar.xz) retains raw command
streams, emitted Wasm and the diagnostic drivers; native executables are excluded.

The diagnostic SDK and runner now accept an explicit 8 MiB initial capacity.
Ordinary execution remains at 16 MiB. The next compiler candidate is sharing
repeated native error-construction code while keeping Realm selection and
abrupt-completion handling at their original call sites.
