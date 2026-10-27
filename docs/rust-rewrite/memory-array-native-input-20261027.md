# Array and native-input memory checkpoint

Measured and verified source `a9ad49c8e73191c4e309240811b7c30d88d15650`; refreshed 2026-10-27.

The [matched comparison](precompiled-comparison.md) records Scriptw at **1.4–1.8 times Scriptc’s post-execution RSS** across all 17 workloads. Scriptc parity remains open.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 19.9 MiB | 13.5 MiB |
| Objects | 22.3 MiB | 14.3 MiB |
| Retained objects | 27.9 MiB | 20.0 MiB |

Three Sol reviews guided source provenance, GC layout and transient allocation work. Internal own-descriptor queries for actual Array objects now use the existing non-Proxy storage kernel and copy a fresh mutable descriptor record. Immutable value/getter/setter edges remain shared. Proxies and other exotic targets retain their existing dispatch. No public builtin function, argument array or JavaScript descriptor transport object is allocated for these Array queries. See the [descriptor contract](contracts/proxy-target-descriptor-completion.md).

The native-image reader retains strict mapped device/inode admission first. When device aliases prevent that proof, a private file-owning kernel-main-ELF constructor can prove the exact immutable static range against /proc/self/exe, bounded kernel auxiliary-vector facts, the loaded program-header table, entry/load bias, inode and exact file offsets. Header and payload spans must each have exactly one read-only file-backed load; all overlapping load types reject. Proc denial, unsupported layouts and ambiguity keep the embedded source. Existing decoded digest, manifest and destination-Engine admission remain mandatory. See the [packed-input contract](contracts/immutable-runtime-build-package.md#packed-native-image-source).

Workspace release/all-targets type checking passed. Sixteen complete affected native inventories passed **4,549 cases**, with zero failures or ignored cases. The 1,047-case engine library ran in fresh groups of at most eight. Runtime-image controls include malformed ELF/auxv facts, overlapping RO/RW/RX headers, offset/range failures and an independently gated actual-source test. The Array controls cover both packed-number and mutable descriptor storage, reentry after descriptor capture, inherited setters, accessor and Symbol properties, length/index boundaries, Proxy invariants, foreign realms and public-intrinsic poisoning.

The pinned selection passed **1,092 original Test262 executions**, covering Array push/sort/join, Proxy descriptor/get/set/has/delete invariants, Reflect.set and Object/Reflect own-descriptors. This is selected verification, not a full pinned-suite refresh; the full-suite README status is unchanged. The preceding [fixed-floor checkpoint](memory-fixed-floor-runtime-20261027.md) retains its separate 662-case Intl library, four output-publication cases and seven collector controls; those unchanged scopes were not repeated in this batch.

Source-only guards and focused Python tooling controls passed. Verification used one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and bounded retained-module caches. The clean Git-tree fingerprint matches CI. Local verification also fingerprints the retained ignored standalone regress Cargo.lock; both exact identities and its bytes are retained. Local binaries are not relabelled as CI binaries.

The [matched CI run](https://github.com/mewhhaha/scriptw/actions/runs/40449574274) uses compiler-disabled consumers, identical host settings, two rounds, ten checked warmups and seven samples per round. Every original checksum passed. Both RSS and execution-time graphs are regenerated from this exact run. RSS includes code, data, runtime and warmup history; it is neither peak memory nor live JavaScript heap. Differences from earlier runner campaigns do not establish causal gains or regressions.

Fresh local mapping observations use kernel-main-elf and retain source-kind traces, raw smaps and present bits without PFNs. The current Portable packed range has at most 49 of 5,070 pages present across samples, while the preceding source had all 4,912 pages present. Local integer post-execution median RSS is 24.34 MiB (earlier run: 41.50 MiB), and retained objects 32.12 MiB (earlier: 49.31 MiB). These are separate unscored local runs, not a controlled before/after campaign or physical-RAM savings. Asynchronous sampling competes on the same CPU and may miss transients. This source-selection evidence does not explain the CI gap or enter scored outputs; exact observations and boundaries are retained in mapping-summary.json in the raw archive. The packed Portable native payload also grew by about 0.6 MiB between these local builds. Factoring the Array-only lookup and pruning unused program imports are follow-up candidates, with no RSS saving established yet.

[Verification record](memory-array-native-input-20261027.json) · [Lossless raw archive](memory-array-native-input-20261027-verification.tar.xz) · [Matched measurements](precompiled-comparison.md).

The archive contains 509 regular files and 167,613,455 expanded bytes, with a SHA-256 per member. Streaming round-trip verification bounds expansion to 2 GiB, members to 128 MiB and compressed size to 32 MiB. Executables and serialized native images are excluded.
