# Separate performance boundaries

The V8 comparison now derives three named scoreboards from the existing
identity-bound v2 raw reports. It preserves their original samples, workload
bytes, checksums, Wasm artifacts and source identities. The historical README
table and conformance chart retain their original measured compiler. This
tooling does not establish a new performance result.

## Measurements and limits

| Boundary | Recorded work | Remaining limit |
| --- | --- | --- |
| Source startup | Existing cold/hot fresh-process spawn-to-reap samples, including normal source preparation and execution | Application cache state is labelled; OS caches are not flushed |
| Precompiled invocation | Wasmtime's observed invocation total, including module/cache admission, Store/linker, instantiation, execution and completion; V8's sum of observed context creation and execution spans | Source compilation is outside both scores; the V8 spans are summed, with memory observations between them, rather than a single contiguous timer |
| Same-realm batches | An explicit number of checked batches within one guest invocation, with the complete execution span divided by batch count | Realm bootstrap, setup, construction and final checksum remain amortized in the score; this is not an isolated steady-state timer |

The scoreboard explicitly reports **fresh-process artifact-only startup** and
**isolated persistent-instance steady state** as unavailable. The current runtime
driver compiles once in its process, then creates a fresh Store for each
invocation. A same-realm guest loop does not turn that driver into a persistent
host session. No empty-control subtraction estimates a missing timing span.

Each scoreboard retains the individual workload ratios. Geometric means and
worst-case ratios are descriptive and remain separate by boundary and by
`tuning`, `holdout` or legacy `unassigned` partition. A missing metric stays
unavailable, including older V8 observations without context-creation spans.
No aggregate may hide a missing case or mix the tuning and holdout partitions.
The proposed first competitive target is at most 2x V8 for every agreed case;
this reporter supplies no automatic performance-acceptance verdict. All raw
samples and slower observations remain in the source report. Seven-sample
nearest-rank p95 is descriptive, not an established tail-latency bound.

The same artifact includes existing process RSS, Wasmtime GC capacity, V8 heap
endpoints, and emitted Wasm bytes. These keep their original, different meanings.
It does not infer live JavaScript heap bytes from process RSS.

## Bounded workload generation

The same-realm corpus contains seven tuning families: direct calls, a persistent
mutable closure, ordinary object allocation/property reads, dense arrays,
strings, Map, and JSON round trips. Three separately declared holdout workloads
cover polymorphic calls, holey arrays, and deletion/reinsertion. Each source
observes the total checksum of every batch. Python computes expected results
independently; the focused corpus control executes all generated families with
normal V8. Neither source generator nor reference harness implements the
Scriptw compiler/runtime.

Array index and length reads retain the shared runtime's compiled Wasm Get
dispatch. Layout 18 introduced packed Number backing alongside complete
dense/sparse descriptors and ordinary static-string cache guards in R.
Layout 19 replaces linked site metadata with indexed paged tables. The
[efficiency checkpoint](../compiler-efficiency-20261024.md) separates its native
verification from the historical layout-18 timings. The corpus and
scoreboard do not imply an array or cache speedup; native-policy defaults remain
unchanged. See [compact ordinary storage](compact-ordinary-data-storage.md).

Generate a fresh corpus with 1..256 batches per case:

```sh
python3 scripts/limited_verification.py -- \
  python3 -B scripts/performance_corpus.py \
  --output-dir target/performance-same-realm --sustained-batches 32
```

This option creates only the ten batched cases. It cannot be combined with
row-phase instrumentation. The existing default construction/traversal corpus
and its manifest schema remain unchanged.

After an identity-bound release build, use the ordinary benchmark invocation
from [the V8 baseline workflow](../v8-baseline.md#current-performance-and-memory-tooling)
with `--fixtures target/performance-same-realm --partition tuning --skip-startup`.
Keep sample count, warmups, CPU, cache state, source and native policy explicit.
Use `--partition holdout` for a separate measurement report after selecting an
implementation. `--case` may narrow a partition; selecting a case from the
wrong partition fails if it leaves no workload. Legacy corpora without declared
partitions remain `unassigned`; they are never silently relabelled tuning.

All builds, tests and benchmarks use the existing resource limiter. On this
local machine it enforces a process-tree 4096-MiB limit, zero swap and one CPU.
Do not overlap benchmarks with builds/tests. The small source corpus is bounded;
the ordinary benchmark's child timeout, sample limits and raw failure retention
remain in force. A bounded generator is not a substitute for the process-tree
resource limit.

## Derive a scoreboard without running JavaScript

```sh
python3 scripts/limited_verification.py -- \
  python3 -B scripts/benchmark-v8.py scoreboard \
  target/performance-candidate/report.json \
  --output target/performance-candidate/boundaries.json
```

The command first admits the complete v2 report through its existing loader:
raw stdout/stderr, workload bytes, emitted Wasm, exact command/sample inventory,
checksums and recomputed summaries must all match. Derived JSON binds the
source report path and SHA-256. Existing output files are never overwritten.
The normal human-readable benchmark summary also shows the boundary table.
Old v2 evidence remains readable; comparison still rejects changed harness,
workload, machine, native policy or measurement configuration.

## Explicit native-policy experiments

The runner accepts `--native-mode fast|size-optimized|speed`,
`--gc-initial-mib 8|16|32|64|128`, and
`--regalloc single-pass|backtracking`. Any explicit option selects a separately
labelled policy cohort; omitted fields become Fast/16 MiB/SinglePass. These
options require `--skip-startup`, because the source-startup CLI still uses its
ordinary policy. Default invocations retain automatic product mode selection
and the original v2 report schema.

Native R is a separate experimental boundary. Ordinary execution admits the
Cargo-produced image, whose explicit target triple disables host CPU feature
inference. Every explicit cohort uses a matching optimized-R producer with
host-inferred CPU features and the same bounded serialized-image cache,
including its 16 MiB control. This follows pinned47's `Config::target` and
`build_compiler` behavior; the complete source ownership is recorded in
[runtime profiling](runtime-performance-profile.md#explicit-compiler-and-gc-capacity-cohorts).
The explicit matrix therefore shares a native target and storage treatment on
one measured host. Comparing it with ordinary execution also changes R's target
and image storage, so the difference cannot establish an isolated P compiler,
register-allocation or GC-capacity speedup. Policy RSS additionally includes
serialized-image storage and producer/allocator effects; it is not ordinary
product RSS or live JavaScript heap usage.

The 8 MiB option permits a smaller-capacity control without changing the ordinary
16 MiB default. Both explicit capacities produce compatible R and P images for
their executing Engine. Initial capacity is demand-paged and may grow; it is
neither committed RSS nor a live-byte limit. A 16/8/8/16 sequence on one host can
test capacity and timing effects within this diagnostic boundary. Its absolute
RSS does not replace the compiler-disabled Scriptw/Scriptc comparison.

Explicit reports use `scriptw-v8-machine-policy-baseline-v1` and require the
runner's `scriptw-wasmtime-policy-baseline-v1` wire schema. Every observation's
actual mode, initial GC capacity, and register allocator must match the recorded
requested policy, including warmups. Raw flags alone cannot establish which
policy executed. Baseline comparison rejects ordinary/policy mixing and any
changed policy; compare those labelled experiments as distinct cohorts rather
than presenting a compiler-source speedup. The derived boundary scoreboard
admits either report schema through these checks.

A separate next experiment may compare an explicit build-target R with a
host-inferred R while holding initial capacity, optimization, register allocation,
source, image storage, sample/warmup counts and CPU fixed. Record actual native
ISA settings and images, and measure cold production cost, cached admission,
execution, size and memory independently. No default target or production policy
change follows from the current ordinary-versus-explicit screens.

Focused verification commands are:

```sh
python3 scripts/limited_verification.py -- \
  python3 -B -m unittest discover -s scripts/tests -p 'test_performance_corpus.py'
python3 scripts/limited_verification.py -- \
  python3 -B -m unittest discover -s scripts/tests -p 'test_benchmark_metrics.py'
python3 scripts/limited_verification.py -- \
  python3 -B -m unittest discover -s scripts/tests -p 'test_performance_study.py'
```

These tooling/reference controls do not establish Scriptw execution speed or
conformance. Native/compiler changes require their own affected semantic tests
and performance measurements using the same retained contracts.
