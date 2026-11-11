# Local Wasmtime versus V8 baseline

Dated checkpoints and their evidence live in Git history; this page keeps the
measurement tooling and boundaries that still apply.

## Current performance and memory tooling

The [performance backlog](../performance-hills.md) owns proposed optimizations,
their priorities, semantic controls and the first measurement batches.

The v2 runner extends the historical timing harness with memory observations,
phase timings, retained emitted Wasm, workload selection and checked comparisons.
The 2026-10-20 table is the historical v1 result. The 2026-10-21 results above
use v2 instrumentation; these schemas cannot serve as each other's performance
baseline.

Requires Linux, GNU `time`, Node and the Rust toolchain. Use `--time PATH` for
a GNU time executable outside PATH. Use the release/default-feature product CLI
and the companion runtime runner.
Commit the coherent source batch before building: both executables must identify
that exact commit. Keep the Cargo JSON build log. Run builds and tests before
benchmarking, then measure on an otherwise idle machine. The cloud launcher uses
the inherited machine limits; the benchmark pins both runtimes to one allowed CPU.

```sh
mkdir -p target/performance-build
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  cargo build --locked --offline --release -p scriptw-cli \
  --bin scriptw --example runtime_baseline --jobs 1 --message-format=json \
  > target/performance-build/cargo.jsonl
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  python3 -B scripts/benchmark-v8.py \
  --scriptw target/release/scriptw \
  --runtime-runner target/release/examples/runtime_baseline \
  --build-log target/performance-build/cargo.jsonl \
  --output-dir target/performance-before --samples 7 --warmups 10 --idle-machine
```

If `CARGO_TARGET_DIR` is set, use its actual executable paths in these commands.
The output directory must be new. Default startup priming enables compilation
trace outside the measured samples and requires a confirmed embedded-runtime hit.
A stale compiler/image source stamp fails here with retained diagnostics, before
spending time on cold samples. Both build-script producers also verify that their
compiled fingerprint implementation matches the current checkout. If that check
rejects a shared target directory, remove only the affected release build artifacts
with `cargo clean --release -p scriptw-engine -p scriptw-runtime-image`, then rebuild;
there is no need to clean third-party compiler dependencies. The runner keeps an incomplete report and logs
on failure; a failed checksum, trap or timeout fails the benchmark. It records
machine/cgroup limits, one-CPU affinity, runtime versions, source and executable
identities, exact workload bytes and output checksums. JavaScript still follows
the ordinary Rust compiler to generated Wasm and the product Wasmtime policy.

For a shorter optimization loop, select an existing manifest entry and omit
startup. Use the same options for before/after comparisons:

```sh
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  python3 -B scripts/benchmark-v8.py \
  --scriptw target/release/scriptw \
  --runtime-runner target/release/examples/runtime_baseline \
  --build-log target/performance-build/cargo.jsonl \
  --output-dir target/performance-objects-before \
  --case retained-objects.js --skip-startup --samples 3 --warmups 3 \
  --execution-cache target/performance-execution-cache --idle-machine
```

The explicit execution cache reuses the ordinary checked runtime/function/module
caches across runs. It does not affect isolated cold/cached startup measurements.
Compilation and warmup observations remain outside the execution summaries.

Generate the attribution corpus to vary retained row counts, widths and read
passes, and cover call/property/string/collection/JSON families:

```sh
python3 -B scripts/performance_corpus.py --output-dir target/performance-corpus \
  --rows 256,1024,4096 --widths 4 --passes 0,1,4
```

Pass `--fixtures target/performance-corpus` to the benchmark, then select entries
such as `--case rows-n1024-w4-p0.js --case rows-n1024-w4-p4.js`.
These are whole-invocation measurements: construction remains included in every
case. Increasing read passes supports marginal-cost attribution; it is not a
separately timed traversal-only span. Generated checksums are independently
validated in normal V8. Use `--first-runtime v8` to reverse the recorded runtime
order in paired repeatability runs without changing workload or sample plans.
After a complete report, derive checked phase attribution with
`python3 -B scripts/performance_study.py REPORT --output NEW_STUDY.json`.
The runtime driver now records the Portable profile and linked runtime key/hash
explicitly, alongside emitted-program and compiler identities.

After a compiler change, commit and rebuild with a new build log. Repeat the
same measurement with a fresh output directory and optionally
`--baseline target/performance-objects-before/report.json`. Existing reports
can also be compared without rerunning JavaScript:

```sh
python3 -B scripts/benchmark-v8.py compare \
  target/performance-objects-before/report.json \
  target/performance-objects-after/report.json
```

The runner writes a readable `summary.md`, raw `report.json`, child stdout/stderr
under `logs/`, exact sources and filtered manifest under `workload/`, and compiled
artifacts under `wasm/`. Preserve the entire directory when moving a baseline;
comparison verifies retained bytes and recomputes summaries from raw observations.
Different hardware/affinity, sample plans, workload bytes, runtime versions,
measurement policy, release profile or harness bytes reject comparison. Compiler
commits, compiler fingerprints and emitted Wasm may differ as part of optimization.
Ratios are observations, not a statistical-significance verdict. V1 reports remain
historical and are rejected as v2 baselines.

For the separately recorded frozen-product ABBA diagnostics, analyze preserved
round variation without rerunning the programs:

```sh
python3 -B scripts/paired_repeatability.py PATH/receipt.json \
  --output target/paired-repeatability.json
```

The input must be a complete `scriptw-frozen-paired-diagnostic-v1` receipt
(the historical `scriptw-frozen-string-hash-diagnostic-v1` spelling also works),
with its original raw child JSON files beside it. The analyzer checks their
digests, ABBA order, source/runner/Wasm identities, warmups and measured cache
hits. It preserves all samples and reports each source's two-round median
range plus the two adjacent before/after effects. Cross-round stdout consistency
is checked; workload/oracle correctness remains bound to the original producer's
evidence. These are descriptive observed ranges, not confidence intervals or an
automatic acceptance threshold. For a small future gain within the observed
variation, collect further balanced or same-source controls before accepting it.
This analysis does not retroactively validate earlier decisions.

## What is measured

### Retained-row phase diagnostics

An opt-in runner mode observes construction and traversal inside the ordinary
JS main invocation, using the existing synchronous host print callback. It
keeps the same fresh Store/Realm and compiled artifact policy. Generate the
marked versions of the existing row corpus and its empty marker control:

```sh
python3 -B scripts/performance_corpus.py --row-phases \
  --output-dir target/row-phase-corpus --rows 1024,8192 --widths 4 --passes 0,1,4
mkdir -p target/row-phase-artifacts
target/release/examples/runtime_baseline target/row-phase-corpus 7 10 \
  rows-n8192-w4-p1-phases.js target/row-phase-artifacts --row-phases
```

The example accepts the same five ordinary arguments followed by `--row-phases`.
This mode emits `scriptw-wasmtime-row-phases-v1`, separate from the ordinary v2
baseline schema. Each observation keeps the complete execution/completion score
and adds construction, traversal, combined and middle-callback wall durations.
Construction includes the checksum seed and captures of retained row/array
identities. Traversal reads those same objects in the same invocation; final
identity checks and checksums remain observable after the timing markers.

Individual phases exclude their endpoint host callbacks. Guest work to enter
the next print callback remains in the preceding phase, and the combined span
includes the middle callback explicitly. Empty markers and hook-off/on controls
are required before interpreting small phase differences. Do not subtract these
diagnostic boundaries from the ordinary README scores or call wall time CPU time.

For optional Linux sampling, launch the runner under `perf record -D -1` with
`--control fifo:CONTROL,ACK`, using fresh control/ack FIFOs. Supply all three
environment variables: `SCRIPTW_ROW_PERF_CONTROL`, `SCRIPTW_ROW_PERF_ACK` and
`SCRIPTW_ROW_PERF_REGION` (`construction`, `traversal` or `combined`). The host
callback waits at most five seconds for each acknowledgement and records its
duration. Only measured invocations issue enable/disable commands; warmups and
compilation stay disabled. Sampling includes a small amount of boundary-control
work, which needs its own control measurement. Preserve exact loaded-code maps,
runtime/Wasm/executable identities, raw profiles and profiler overhead alongside
any attribution claim. This diagnostic introduces no new engine import.

`scripts/measure_row_phases.py` automates the serial measurement: a Node checksum
oracle, case-wise observer off/on/on/off controls, profiler off/on/on/off controls
for the empty and selected row cases, then two profiles each for construction
and traversal. It retains complete raw outputs, emitted Wasm, loaded-code maps,
profile data and incremental receipts, including a failed child. On an otherwise
idle cloud machine with the runner already built:

```sh
python3 -B scripts/limited_verification.py --cloud -- \
  python3 -B scripts/measure_row_phases.py \
  --runner target/release/examples/runtime_baseline \
  --corpus target/row-phase-corpus --output target/row-phase-measurement \
  --cache target/row-phase-cache --perf /usr/bin/perf
```

The default is seven samples after ten warmups, 499-Hz user-CPU sampling and one
pinned CPU. `--perf-library-path` can supply a private profiler's libraries;
they are removed from the runtime child's environment. Output directories must
be new. Validate captured sample PIDs against the saved map before attribution;
the producer checks fresh map, artifact and byte identities. CPU analysis and
any backlog decision require a separate review of the completed evidence.

### Ordinary baseline boundaries

- **Cold start:** a fresh child process with a unique empty Scriptw application
  cache. The shared runtime is already compiled into the release build. Node
  starts normally with its persistent compile cache disabled. OS caches stay warm.
- **Cached start:** a fresh child process after priming the Scriptw disk cache.
  Node remains a new process without an equivalent persistent code cache. This
  does not measure a running server.
- **Execution:** each case has its own process. Source compiles once, then both
  runtimes use the same explicit warmup count and measured count. Wasmtime uses
  a fresh Store and V8 a fresh vm.Context per invocation. Context/Store setup and
  source compilation sit outside the execution span; output/completion processing
  is inside. Scriptw initializes its fresh Wasm GC Realm and builtins inside
  timed main; V8 context creation is outside its source-execution span. V8 tiering and natural GC stay enabled; equal warmup counts do not
  prove equal steady-state/JIT tiers. Measured Wasmtime module-cache hits are
  required. Warmup observations remain in raw data and are excluded from summaries.
- **Phases and bytes:** Wasmtime reports native-module/cache work, Store/linker,
  instantiation, export lookup, execution and total. Preparation/lowering/emission/
  validation spans are single compilation diagnostics, not repeated benchmarks.
  The exact emitted Wasm and body/data footprint identify code-size changes.
- **CPU and process memory:** GNU `time` gives the runtime child's kernel high-water RSS,
  excluding the Python launcher's inherited pre-exec footprint. Linux `wait4`
  gives CPU time for the small native supervisor and its waited child. Startup has repeated child observations;
  execution has one whole-child observation per runtime/case, including compilation,
  warmups and retained runtime caches. Blocking wait avoids timeout-polling rounding
  in short startup spans. Driver receipt/log writes sit outside the wall timer;
  the GNU time supervisor, process launch, watchdog setup and scheduling stay inside. A watchdog kills the
  owned process group at the explicit timeout.
- **Invocation memory:** Wasmtime's existing profiler samples process RSS every
  10 ms plus endpoints around runtime admission through completion. Short peaks
  can be missed; it includes process-wide caches, host work and sampler overhead.
  V8 reports process memory endpoints before context creation, before execution
  and after execution. Earlier warmups and contexts can contribute to those process
  totals until normal GC runs. These are not per-workload live bytes or peak samples.
- **Heap observations:** Wasmtime exposes GC heap *capacity* before/after execution.
  V8 exposes process-wide used/reserved heap, external and ArrayBuffer endpoints.
  They have different meanings. Do not subtract process RSS to invent a JS heap,
  equate Wasmtime capacity with V8 used heap, or interpret natural GC deltas as
  allocation totals. Wasmtime live bytes, allocation totals, collection counts,
  GC pauses and peak GC heap stay explicitly unavailable. No forced GC is added.

The default corpus retains the original 10,000-step integer and 500-object
workloads and adds 8,192 retained objects, strings and four-element arrays with
32,768 indexed reads. Each prints an independently specified checksum. These
small kernels help find costs; they do not establish application-wide performance.
Use `--fixtures PATH` for a directory with the same manifest schema, a checked
startup entry and nonempty execution entries (`file`, `stdout`, optional `work`).
Case files must have distinct `.js` basenames. An entry may name any self-contained
script supported by both runtimes; `print` is the common output interface. The
benchmark rejects incorrect output instead of accepting a faster wrong result.

For the existing fixed twenty-case Wasmtime compiler/runtime reports, see the
[compiler profile](contracts/compiler-performance-profile.md),
[runtime profile](contracts/runtime-performance-profile.md) and
[whole-CLI reporter](contracts/performance-reporting.md). Those reports use their
own fixed Test262 host corpus and policies; do not mix their timing spans with
this normal-product/V8 comparison. Wider calibrated T25 acceptance remains open.
