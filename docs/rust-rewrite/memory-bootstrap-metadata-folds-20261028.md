# Native bootstrap metadata and accessor folds

Verified source `bd564df792204753db1636c787f450e7dd083a03` omits native metadata publication only when the existing compact header already supplies all observable metadata. Fifteen Intl getters now use the existing checked shared accessor allocator. ThrowTypeError, JavaScript functions, automatic prototypes, callable source identity and Realm ownership retain their existing paths. Runtime layout remains 30.

**Verification:** 4,906 fresh native passes across 19 full affected targets, including 514 AOT library tests and 1,056 engine library tests. The four focused emission and Intl Realm/GC controls count once. No inherited passes or original Test262 executions are credited.

The initial feature attempt failed during test compilation and ran zero tests. A test-only repair uses the actual builtin registry iterator. The retained source proof binds identical compiler inputs for the feature, repaired local source and measured probe.

**Matched comparison:** [CI run 40630427913](https://github.com/mewhhaha/scriptw/actions/runs/40630427913), measured source `7f9bacff03c454dd505a0047c9439bee253028db`, passed 68/68 attempts and all 17 workload checksums. Six diagnostic native controls passed separately. Scriptw/Scriptc RSS ratios range from 1.421894605 to 1.979259487. Cross-runner observations do not establish causal savings; Scriptc parity remains open.

Compared with the [previous checkpoint](memory-calendar-rows-20261028.md), Rust changed from 1.99.0 to 1.99.1 and the runner kernel changed from 6.17 to 7.0. The shared Wasmtime configuration remained identical. Separate unscored execution captures show Scriptw's increase mainly in clean host-file residency: +2,472 KiB for integer work and +1,992 KiB for retained objects. Anonymous residency changed by −4/0 KiB, while module VMAs added 64/128 KiB. Scriptc's anonymous residency at those endpoints also fell by about 2 MiB. These phase captures follow sixteen earlier calls and do not replace the scored samples or isolate the cause of their changes.

| Workload | Scriptw ms | Scriptc ms | Scriptw RSS MiB | Scriptc RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| sort-reverse-128.js | 1.519257 | 0.116309 | 21.343750 | 12.865234 |
| sort-reverse-512.js | 5.066455 | 0.346156 | 23.818359 | 12.931641 |
| sort-sorted-512.js | 2.644014 | 0.346716 | 21.300781 | 12.837891 |
| to-sorted-512.js | 6.275285 | 0.393034 | 25.162109 | 12.712891 |
| join-1024.js | 2.860456 | 0.454340 | 21.566406 | 12.935547 |
| join-4096.js | 9.918948 | 1.528320 | 23.375000 | 13.392578 |
| join-mixed-1024.js | 3.101226 | 0.268068 | 21.361328 | 12.429688 |
| numeric-loop-10000.js | 1.521708 | 0.081747 | 20.062500 | 12.099609 |
| numeric-loop-50000.js | 5.149738 | 0.227761 | 19.851562 | 12.115234 |
| folded-string-4096.js | 2.338151 | 0.791000 | 19.263672 | 12.279297 |
| dynamic-string-4096.js | 2.592681 | 0.260732 | 20.494141 | 12.353516 |
| integer.js | 4.110071 | 0.051116 | 20.125000 | 12.181641 |
| objects.js | 2.234585 | 0.730669 | 21.005859 | 12.908203 |
| retained-objects.js | 58.606922 | 39.543491 | 26.560547 | 18.679688 |
| join-empty-1024.js | 3.842767 | 0.276104 | 22.019531 | 12.332031 |
| join-singleton-1024.js | 4.016846 | 0.111627 | 22.349609 | 11.935547 |
| join-small-1024.js | 4.229533 | 0.161266 | 22.458984 | 12.113281 |

Measurements use two rounds, ten checked warmups and fourteen measured samples per runtime and workload. Both compiler-free consumers run in the same Wasmtime executable with identical optimization and runtime settings; each Store stays alive at the RSS boundary. RSS includes code, runtime state and allocator history.

Four separate unscored captures record integer and retained-object module ownership and section residency for each compiler. Page and section overlaps are nonadditive; mapped presence does not prove execution. The [JSON checkpoint](memory-bootstrap-metadata-folds-20261028.json) and [bounded inert archive](memory-bootstrap-metadata-folds-20261028-inert.tar.xz) retain exact source, inventories, streams, controls, resource receipts and measurement metadata. Native executables and runtime packages are excluded.

Local native verification used one CPU, a 4 GiB process-tree cap, no swap and bounded module caches. No OOM occurred. Inert evidence processing used a separate 1 GiB cap. Earlier checkpoints retain their own conformance evidence.

Refresh the matched observations with the [precompiled comparison workflow](contracts/precompiled-comparison.md). The paired README graphs describe this run; V8 comparison and pinned conformance status are unchanged.
