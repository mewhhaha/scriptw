# Lazy retained calendar rows

Verified source `0a29835741bb9b1477f5946ec4ba1812c902959b` has 1763 passes across nine affected full native scopes plus three selected engine controls: 1766 fresh executions and 0 byte/source-bound retained passes. The full engine library and original JS/Test262 executions are not claimed.

The whole changed AOT library passed 512 freshly, including the five focused controls. Earlier Rust test compilation and two stale physical-layout assertions were repaired only in excluded test files; their raw failures/counters remain preserved, and the old 505 broad passes receive no credit. Cheap validation records 278 Rust source controls, 15 guard-mutation tests and 15 current Intl identities.

CI40621647185 (`7b81ba6c9a6a30189acf75fed72f3f8a434d0992`) passes 68/68 attempts across 17 workload checksums. Current Scriptw/Scriptc RSS ratios are 1.204774845–1.599479666. Integer is 17.988/13.646 MiB and retained objects 24.246/20.125 MiB. No overall causal RSS saving is established. V8/status remain separate.

The change defers 133712 exact calendar-row bytes: cold active data accounting changes 366448→232736 bytes. A readiness bit belongs to each R instance; retained readers initialize once after their range guard. Outside-range calculations keep their existing path. This copied-byte reduction does not establish a heap or RSS reduction.

| Current retained warm case | R image pages | Text | Nontext image pages | .rodata.wasm overlaps/full |
|---|---:|---:|---:|---:|
| integer.js | 633 | 462 | 171 | 107/106 |
| retained-objects.js | 921 | 558 | 363 | 107/106 |

Warm observation16 follows 16 prior calls. Section/page overlaps are nonadditive; presence does not prove execution. Layout30 has a new runtime key, with no prior-runtime or cross-runner native equality claim.

The bounded inert archive preserves native inventories/results, both failure prefixes, cheap controls, current scored/unscored observations, exact parsers, source-equivalence proofs and drivers. Native executables and runtime packages are excluded. README, graphs and main are not changed by candidate generation.

The same batch prefilters actual native-source compiler imports before entering three binding routines. NativeHost ABI validation and late arming remain unchanged; the three selected engine controls exercise import domains, malformed imports and all three dynamic calls. See the [binding contract](contracts/native-source-import-prefilter.md).
