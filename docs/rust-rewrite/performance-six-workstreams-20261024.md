# Six-workstream performance checkpoint, 2026-10-24

Measured compiler: `31c5958d2dc60dcd3e20fdb8bd00a6821d63fb03`.
V8 parity has not been reached. The changes improve several workloads and
establish separate startup, fresh-program and existing-Realm scoreboards.
They also introduce measured regressions on some small workloads.

The frozen compiler control is `d89fd40689f60d94f14fa7800c180be89cfdd66a`:
the shared measurement/cache infrastructure with the original layout-16 AOT
code. The separate storage comparison uses the first layout-17 checkpoint,
`30fc1931471646253e339100a6d392b2337a373b`. These comparisons have different
baselines and must remain separate. The historical cloud measurements in
[the baseline report](v8-baseline.md) retain their own identities.

## What changed

- Measurement now distinguishes source to checked result, precompiled raw
  P+R startup, and calls within one existing Realm. It retains exact outputs,
  artifact identities, phase timings, code size and process memory. React SSR
  runs the pinned, unmodified synchronous production renderer and checks three
  exact HTML outputs.
- Explicit GC and native-compiler controls report the actual Engine policy.
  Cold P compilation is measured separately from R production and execution.
- Guarded static-string reads use live storage/shape checks and current values.
  The cache retains no receiver, user value, prototype or Symbol. Misses retain
  ordinary Get, receiver, proxy, accessor and error behavior.
- Layout 18 adds compact ordinary data slots with shared source shapes and
  packed Number arrays with separate presence bits. Dictionary descriptors and
  dense/sparse Array descriptors preserve the complete mutation semantics.
- Proven Number expression regions avoid intermediate boxing. Fixed argument
  evaluation reuses scratch while retaining immutable argument snapshots.
  Validated leaf activation plans omit unobservable arguments objects and
  mapping-only environments; strict parameters keep cells when needed.
- Engine-owned immutable host registration can be reused with fresh Stores and
  Realms. Startup reports separate R/P loading and cache occupancy. The explicit
  BLAKE3 cache-image cohort preserves full-SHA provenance and a separate cache
  identity domain.

Contracts: [compact storage](contracts/compact-ordinary-data-storage.md),
[packed arrays](contracts/packed-array-storage.md),
[default-data publication](contracts/array-default-data-publication.md),
[activation/inlining boundaries](contracts/direct-call-inlining-boundary.md),
[startup boundaries](contracts/portable-performance-boundaries.md), and
[native-policy repayment](contracts/native-policy-break-even.md).

## Balanced compiler comparison

Each case ran before/after/after/before in four separate processes, with seven
checked execution samples after ten warmups per process. The table pools the
14 samples on each side. Both adjacent comparisons and all raw samples remain
in the evidence; these are descriptive observations, not confidence intervals.
Execution includes a fresh Store/Realm and completion, as in the original
baseline harness. It is not isolated existing-Realm call time.

| Workload | Control median | Current median | Change |
| --- | ---: | ---: | ---: |
| 10,000 integer steps | 4.639 ms | 4.572 ms | −1.4%; paired rounds disagree |
| 500 objects | 6.646 ms | 5.545 ms | −16.6% |
| 8,192 retained objects/arrays/strings | 855.983 ms | 426.124 ms | −50.2% |
| Sustained calls | 3.234 ms | 3.138 ms | −3.0% |
| Sustained closures | 1.851 ms | 1.797 ms | −3.0% |
| Sustained objects | 12.134 ms | 9.986 ms | −17.7% |
| Sustained arrays | 32.105 ms | 33.088 ms | +3.1% |
| Sustained strings | 1.169 ms | 1.188 ms | +1.5% |
| Sustained Map | 3.596 ms | 3.593 ms | −0.1%; paired rounds disagree |
| Sustained JSON | 11.067 ms | 10.564 ms | −4.5% |

The layout-18 scaling comparison against layout 17 ran another 60 processes.
For 16-field objects retained across 2,048 rows, the median fell from
113.032 to 34.110 ms (−69.8%, with both pairs agreeing). Several smaller
controls regress: one-field objects by about 5–6%, 32-field/128-row objects
by 7.9%, and numeric arrays by about 4–9%. The current storage design therefore
does not establish a universal speed improvement.

Compact storage retains six records per admitted object plus shared shape/site
metadata. Packed payload accounting is `8*C + 8*ceil(C/64)` bytes before headers
and alignment. These are layout facts. Neither semantic snapshots nor process
RSS establish dynamic allocation counts or live JavaScript bytes. The current
metadata registry also scans executed sites linearly; many-site scaling and a
cheaper lookup remain performance work.

The initial P-side Array guards regressed the first-batch array comparison.
Their removal had mixed adjacent effects in its direct ablation, so no clean
isolated speedup is attributed to that removal. The historical source-bound
observations remain in the evidence.

## Three execution boundaries

The ordinary current baseline below uses seven samples and ten execution
warmups. Startup is whole-child CLI time; execution uses compiled code and a
fresh Store/Realm or V8 context. Source and raw-bundle READY timings belong to
the separate portable-boundary cohort.

| Ordinary baseline | Scriptw / Wasmtime | Node / V8 |
| --- | ---: | ---: |
| Cold CLI start | 698.400 ms | 17.521 ms |
| Cached CLI start | 517.923 ms | 17.595 ms |
| 10,000 integer steps | 4.799 ms | 2.025 ms |
| 500 objects | 5.515 ms | 0.052 ms |
| 8,192 retained objects/arrays/strings | 422.117 ms | 2.614 ms |

The portable cohort ran all eleven selected workloads through source, fresh
bundle and existing-Realm call boundaries. Seven measured fresh children follow
one child warmup. Calls retain two separately initialized Realms per runtime,
each with ten guest warmups and seven measured batches. Callback exit/entry
timestamps delimit the call intervals; there is no empty-program subtraction.

Existing-Realm session ratios range from about 17× to 859× Scriptw/V8 on these
fixtures. The two React SSR sessions are 134.6× and 132.6×. Source and precompiled
startup also retain large gaps. Two Realm sessions do not establish statistical
confidence, and these timings cannot be substituted for the amortized
fresh-Realm corpus above.

The supported React scope is React/ReactDOM 19.3.1 and its synchronous production
renderer alias, with exact upstream source/license hashes and three HTML
oracles. Streaming and public browser entrypoints are not covered. Previously
opened corpus holdouts are labelled validation; they are not unseen holdouts
for this later compiler batch.

## GC, native compilation and startup decisions

The controlled GC16/64/64/16 comparison uses the existing 8,192-retained-object
fixture, seven samples and ten warmups. Within this matched-image cohort,
execution medians are 440.172 and 98.051 ms, about 4.49× apart, with both
adjacent pairs favoring 64 MiB. The 16-MiB request grows to 32 MiB; the 64-MiB
request remains at 64 MiB. Capacity is reserved space, not live bytes or a
collection count. Whole-child RSS includes the cold matched-image producer.

The native-policy matrix has 17 controls and four policies, one policy per
child. Speed/SinglePass improves execution in 15 controls, raises cold P
compilation cost, and reduces native text in all 17. Where execution savings
are positive, the observed P-only repayment is 24–1,459 workload invocations.
That estimate excludes R production and cannot establish whole-program
repayment. Other controls have no positive execution repayment.

Ordinary embedded R uses its own producer/ISA/storage path. Diagnostic matched
R uses the host ISA and a bounded private image cache. Capacity comparisons
within that diagnostic cohort are controlled; comparing its absolute RSS or
speed with the ordinary baseline does not isolate GC capacity alone.

A separate bounded native-inlining trial passed its disabled control, then
was OOM-killed at the unchanged 4-GiB limit. The last compiler phase was not
retained. Tiny bounded-module tests passed independently, but successful
whole-runtime admission, speed and repayment were not established. The raw
incomplete trial and its kernel rejection are retained.

The same old layout-17 executable was used for the cache-retention experiment.
Changing only its payload limits from one entry/64 MiB to two entries/128 MiB
reduced warm total-invocation medians from 113.436 to 5.736 ms. That SDK does
not report R hits or cache-owner occupancy, so neither is inferred. This is
within-process reuse, not fresh-process CLI startup.

The SHA256/BLAKE3/BLAKE3/SHA256 source experiment reached valid checked-result
READY 95.209 and 45.543 ms sooner in the two BLAKE3 comparisons. Complete
diagnostic child lifetime was instead 56.294 and 116.658 ms longer because
the separate post-READY full-SHA proof remains mandatory. This does not prove
a production CLI end-to-end latency win. The BLAKE3 option is
`SCRIPTW_COMPILER_CACHE_IMAGE_HASH=blake3`; it has a separate cache domain.

Defaults remain Fast, 16 MiB, SinglePass and native inlining disabled, with
SHA256 cache-image identity. GC64 and Speed have useful measured candidates
for selected workloads, rather than evidence for a universal default change.
Source-body inlining and small-arity entries still require validated target,
Realm, source-depth, legacy activation and capture authority.

## Verification and resource budget

The final source passed the complete 5,633-check affected native checkpoint
and 93 Python tooling tests. The 70 focused checks and complete 28-test emitted
target overlap those native scopes; they are not additional totals. Inventories
were nonempty, terminal counts matched, and no tests were ignored.

The selected original 5,587-ID Test262 replay completed with 5,583 successes,
the same four known failures, and zero crashes, unsupported cases or timeouts.
The verifier checked the exact execution-ID set and retained the real nonzero
suite exit. The historical full 102,994-execution README chart remains bound
to `b9ecfe64b`; this checkpoint does not refresh that chart. These results bind
to performance source `31c5958d2dc60dcd3e20fdb8bd00a6821d63fb03`;
the later foreign-module work has a separate verification checkpoint.

Every substantial build, test and benchmark used a kernel-enforced 4-GiB
process-tree cap, zero swap, grouped OOM termination, one CPU and serial worker
defaults. Benchmarks ran separately from builds and tests. The final native
checkpoint and ordinary measurement pipeline recorded zero OOM kills.

The cap caught an earlier code-growth regression: small Array elements repeated
the packed growth/copy/fallback CFG in P. One existing RegExp fixture produced
a 2,630,970-byte function and hit the limit. Publication now crosses one typed
R helper; the fixture's largest strict body is 1,407,419 bytes, and both modes
pass. The separate bounded-inlining rejection did not cause a cap increase.

Hardware: AMD Ryzen 7 7800X3D, 16 logical CPUs and about 96 GiB physical RAM;
the payload used one CPU and 4 GiB. Node 24.12.0, V8
13.6.233.17-node.37, Wasmtime 47.0.0. The local release CLI is 264.30 MiB with
line-table debug information. These package sizes and process RSS are not
JavaScript heap measurements. Experimental Wasmtime GC/reference/EH support
remains the required backend capability set.

## Evidence and refresh

[The final JSON](performance-six-workstreams-20261024.json) and
[compressed evidence archive](performance-six-workstreams-20261024.tar.gz)
retain exact compiler/executable/harness/P/R/source
identities, all balanced rounds and Realm sessions, native inventories, rejected
experiments, original execution IDs and a bundle of the frozen source commits
above base `8d24976c6`. The frozen candidate patch is also retained. Executables,
native cache images, raw P/R and V8 cachedData payloads are excluded from the
archive; their identities are retained for regeneration.

The archive's `portable-evidence-manifest.json` records every included file's
SHA-256. Its `frozen-performance-sources.bundle` contains the exact source refs;
in a checkout containing base `8d24976c64432a5edd712e4fb91e58cac78db7ed`, run
`git bundle verify /path/to/frozen-performance-sources.bundle`, then
`git fetch /path/to/frozen-performance-sources.bundle 'refs/codex/performance-20261024/*:refs/codex/performance-20261024/*'`.
Use a detached worktree at the desired frozen ref for reproduction. The JSON
records the archive checksum and frozen patch checksum.

Refresh commands and the complete scoped command lists are retained in the
archive. The repository entrypoints are:

```sh
python3 scripts/limited_verification.py -- cargo build --locked --release --jobs 1 -p scriptw-cli --bin scriptw --examples --message-format=json
python3 scripts/limited_verification.py -- python3 scripts/benchmark_boundaries.py --runner target/release/examples/runtime_boundaries --time /path/to/gnu-time --output-dir NEW_DIRECTORY --samples 7 --call-sessions 2
python3 scripts/limited_verification.py -- python3 scripts/policy_break_even.py --runner target/release/examples/runtime_policy_break_even --time /path/to/gnu-time --output-dir NEW_DIRECTORY --idle-machine
```

Use a clean frozen checkout and retain Cargo JSON separately from stderr for
the ordinary baseline's build admission. Use
[the development workflow](../../CONTRIBUTING.md#verification) for bounded native verification.
Only the native Test262 publisher refreshes the pinned full-suite README block.
