# Typed Array lookup and Set data checkpoint

Measured and verified source `eb9ff7b0552e2b57a8c6b393ee99540b664eb615`; refreshed 2026-10-27.

The [matched comparison](precompiled-comparison.md) records Scriptw at **1.4–1.7 times Scriptc’s post-execution RSS** across all 17 workloads. Scriptc parity remains open.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 20.1 MiB | 13.5 MiB |
| Objects | 21.0 MiB | 14.2 MiB |
| Retained objects | 27.4 MiB | 20.0 MiB |

Two Sol reviews checked allocation/algorithm boundaries and GC/control-flow ownership. An actual Array own-descriptor query now uses only its ordinary header and Array indexed/virtual-length storage. The indexed/length tail is shared with public non-Proxy lookup. This factors the existing algorithm without emitting namespace, Arguments, boxed-string and typed-array branches beneath an actual Array proof. Internal queries retain fresh mutable descriptor snapshots and immutable value edges.

The two OrdinarySet data-receiver definitions accept a closed Update/Create enum. Its factory constructs only data fields and validates the existing descriptor lattice. Updating carries only a present value; creating carries all three true attributes. An actual Array consumes that witness through the existing Array DefineOwnProperty and ArraySetLength owner. Other receivers retain the private carrier/native bridge. General accessors and conversion callers gain no shortcut or new callability authority. Definition failures remain Boolean false for the outer strict Set or throwing builtin to convert. See the [receiver contract](contracts/array-set-receiver-data-definition.md).

No second object model, helper ABI, runtime layout or GC type-index space was added. A local Cargo-check output records the packed Portable runtime image at 20,347,044 bytes, compared with 20,760,937 bytes in the preceding frozen build: about 0.39 MiB smaller. These are artifact sizes, not resident or physical-memory savings. Runtime keys and source fingerprints correctly change with the new compiler source.

Workspace release/all-targets type checking passed. Fifteen complete affected native inventories passed **4,535 cases**, with zero failures or ignored cases. All 213 GC-entry cases passed first, including four new controls in both strict and sloppy modes. They cover two length conversions before current-state reads, reentrant writability/storage changes, mismatched conversions and executing foreign RangeError, original throws, distinct receivers, partial flags, nonextensible existing/new indices, named/Symbol data, shrink rollback, strict failure ownership and uint32 boundary publication. The 1,047-case engine library ran in fresh groups of at most eight.

The pinned selection passed **1,200 original Test262 executions**, including the preceding Array push/sort/join, Proxy and descriptor selection plus Array length and Proxy defineProperty. This is selected verification, not a full pinned-suite refresh. The full-suite README status remains unchanged. The unchanged standalone runtime-image, Intl, output-publication and vendor collector scopes retain their separately dated [earlier evidence](memory-array-native-input-20261027.md); this batch does not claim fresh executions of them.

Source-only guards, identities and formatting passed. Those cheap checks ran against the exact committed source in an isolated worktree before a Git fast-forward brought the same commit into the main build worktree. Native and original Test262 execution then used the frozen main revision. Verification kept one CPU, 4 GiB, no swap, grouped OOM and bounded module retention. Clean CI compiler inputs and local inputs with the retained ignored standalone regress Cargo.lock have separately recorded fingerprints; local binaries are not relabelled as CI binaries.

The [matched CI run](https://github.com/mewhhaha/scriptw/actions/runs/40456976322) uses the original 17 workloads and checksums, compiler-disabled consumers in the same executable, identical Wasmtime settings, two rounds, ten checked warmups and seven samples per round. Both RSS and execution-time graphs are regenerated from this run. RSS includes host/runtime, code, data and warmup history; it is neither peak memory nor live JS heap. Differences from earlier runner campaigns do not establish causal gains or regressions.

Both this CI run and the preceding run 40455594371, whose product inputs match the earlier a9ad49c8e checkpoint, retain separate post-measurement mapping observations for integer and retained-object workloads in both adapters. Every original checksum, executable/source identity, raw smaps hash and summary was revalidated. Asynchronous sampled maxima can miss execution entirely: the new Scriptc integer sample maximum is below its independently recorded execution endpoint. Therefore mapping maxima are not aligned phase comparisons or process peaks. Native-file, anonymous and host-code categories describe mapping storage, not which functions executed.

A read-only local symbol review binds present-page observations to the exact frozen local executable, without applying its addresses to CI. Compiler/parser and Intl symbols can share pages made resident by fault-around; symbol overlap is not evidence those functions ran. A private consumer-command-root experiment is separately gated after product verification, uses unchanged execution/configuration modules, and cannot enter the canonical scores. Its presence or preparation alone is not a memory-savings claim.

[Verification record](memory-array-set-kernel-20261027.json) · [Lossless raw archive](memory-array-set-kernel-20261027-verification.tar.xz) · [Matched measurements](precompiled-comparison.md).

The separate local command-root probe passed all 24 consumers (three executable variants, two adapters, two workloads and two reversed rounds), each with ten warmups and seven samples. Reconstructed full versus consumer-only roots measured Scriptw integer at 21.04 → 20.72 MiB and retained objects at 28.34 → 28.04 MiB; Scriptc measured 13.81 → 13.58 and 20.26 → 20.15 MiB respectively. The original Cargo executable’s identity and observations are also retained as a reconstruction control in the raw record. These modest command-root/link-layout observations do not explain the remaining gap or justify replacing the canonical host. They are not a 17-workload result and do not enter either graph.

The archive contains 1,139 regular files and 279,838,325 expanded bytes, with a SHA-256 per member. Streaming round-trip checks bound expansion to 2 GiB, members to 128 MiB and compressed size to 32 MiB. Executables and serialized native images are excluded.
