# Lazy runtime allocation checkpoint

Source `99a4ff2e962b6efa8066a48447f3c077981c59ec`, verified 2026-10-27.
The [matched precompiled comparison](precompiled-comparison.md) measures
Scriptw at **1.8–2.8 times Scriptc's post-execution RSS** across 17 workloads.
Scriptc parity remains open.

| Workload | Scriptw RSS | Scriptc RSS |
| --- | ---: | ---: |
| Integer | 25.3 MiB | 13.4 MiB |
| Objects | 30.2 MiB | 14.2 MiB |
| Retained objects | 40.9 MiB | 20.0 MiB |

Three Sol reviews guided this batch. Canonical runtime strings now materialize
from their sealed UTF-16 rows on demand. Noncanonical intrinsic method data
properties retain their keys, attributes, aliases and defining Realm while
deferring the function allocation until a value is needed. Canonical slots,
constructors and accessors remain eager. Packed Array HasProperty probes avoid
string keys for present own data while retaining live fallback semantics.
The [allocation contract](contracts/lazy-runtime-values.md) records the
representation, snapshot handling and incompatible-layout rejection.

The workspace/all-targets type check passed. Fifteen source-identity recipes,
the source-only structural guards, the runtime-source profile guard and the
retired-JavaScript guard passed. Fifteen complete native scopes cover
**4,503 distinct cases**, with zero failures or ignored cases in the completed
inventory; focused checks overlap these totals. Both complete typed structural
targets are included (166 builtin and 152 language cases).

The exact original pinned Test262 selection covers Array map/slice, Set aliases,
Math.abs, Object/Reflect descriptors, freeze/seal and string normalization.
**1,589 executions passed**, with zero failed, crashed or unsupported executions.
This is selected source-bound verification, not a full pinned-suite refresh.

Verification uses one CPU, a 4-GiB process-tree cap, no swap, grouped OOM and
bounded module retention. The original engine-library process was killed at
that cap after 237 individually recorded successes. The interrupted case passed
in a fresh process; the remaining 802 cases then passed in groups of at most
eight against the same frozen Cargo-produced executable. The union covers its
full 1,040-case inventory exactly. The failed command, kernel OOM journal and
all fresh-process results are retained. No source case was removed and the
limit was not raised. This establishes the completed inventory; it does not
establish that the original full-process invocation passes.

The [CI run](https://github.com/mewhhaha/scriptw/actions/runs/40411932915)
is the authoritative same-runner Scriptw/Scriptc measurement: compiler-disabled
consumers, two rounds, ten checked warmups and seven samples per round.
The README graphs show this latest matched cohort. Other runner revisions are
not controlled before/after evidence. RSS includes host/runtime, code and
warmup history; it is neither live JavaScript heap size nor peak memory.

[Verification record](memory-lazy-runtime-20261027.json) ·
[Lossless raw verification archive](memory-lazy-runtime-20261027-verification.tar.xz) ·
[Matched measurements and raw evidence](precompiled-comparison.md).

The verification archive contains 390 regular files and
842,718,412 expanded bytes. Its recorded SHA-256, member hashes and
bounds permit checking without running native code. The 64-MiB xz dictionary
compresses repeated Cargo diagnostics losslessly; expansion is bounded at 2 GiB,
with at most 128 MiB per member and 32 MiB compressed. Executables and serialized
native images are excluded. Failed preparatory checks remain in the archive.
