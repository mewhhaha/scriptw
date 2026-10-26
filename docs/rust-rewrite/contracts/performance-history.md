# README performance graphic and retained measurements

The README shows two images from the [compiler benchmark workflow](compiler-comparison.md):
[execution time](../assets/execution-performance.svg) and
[post-execution process memory](../assets/execution-memory.svg), comparing
Scriptw, Scriptc/WASI and V8. Both use the checked `compiler-comparison.json`
report and preserve its measured compiler identity and date. The renderer's
default and `--check` commands process both images.

The dated two-runtime checkpoint below remains historical evidence. Use its
explicit `--report` and a separate `--output` path to render it without changing
the README images. Historical startup measurements remain in their own manifest.

## Historical execution checkpoint graphic

The separate [2026-10-26 local packed-record checkpoint](../execution-memory-packed-records-20261026.md)
retains its [combined two-runtime graph](../assets/execution-memory-packed-records-20261026.svg).
It shows all 17 fresh-Store/Realm workloads as paired bars: V8 in orange and
that measured Scriptw revision in blue. There is no baseline series. Both panels
use zero-based linear scales shared within each section, with absolute values
at the bar ends: elapsed milliseconds and process RSS in MiB.

Each timing bar is the median of 14 measured observations, pooled from the two
candidate rounds of seven measurements after ten warmups. V8 comes from the
same candidate rounds. The RSS bars are medians of post-execution process
snapshots: Wasmtime's final sampler read while the Store remains alive, and
V8's `memoryUsage().after` while its context remains alive. They include
host/runtime, setup and retained warmup effects. They are not synchronized
heap measurements, live JavaScript bytes or whole-child high-water RSS.

Render that historical graph offline, without running JavaScript or compiling:

```sh
python3 scripts/render-execution-performance.py \
  --report docs/rust-rewrite/execution-memory-packed-records-20261026.json \
  --output docs/rust-rewrite/assets/execution-memory-packed-records-20261026.svg
python3 scripts/render-execution-performance.py \
  --report docs/rust-rewrite/execution-memory-packed-records-20261026.json \
  --output docs/rust-rewrite/assets/execution-memory-packed-records-20261026.svg --check
```

The renderer requires matplotlib and uses its bundled DejaVu Sans font.
`--preview PATH.png` also writes a local raster preview. The data reader checks
the committed report, evidence archive and manifest identities, binds the
archived analysis to the report, and recomputes timing/RSS medians from raw
observations. It rejects missing cases, altered summaries and mismatched
compiler identities. Measurement dates and revisions come from the evidence.

Shared-host observations remain descriptive, with no significance or V8-parity
claim. Earlier Scriptw comparisons and existing-Realm calls retain their own
tables in the checkpoint. This corpus and boundary do not enter the older
four-workload v2 history as though they were compatible.

## Retained startup history

Publish a complete ordinary startup report from the workspace root:

```sh
python3 scripts/publish-performance.py record PATH/report.json
```

This records its checkpoint in
[`performance-history.json`](../performance-history.json), retains the checked
evidence, and updates the separate historical
[`performance-memory.svg`](../assets/performance-memory.svg). It does not change
README contents or the current execution graph.

Regenerate or check that historical graphic without timing any code:

```sh
python3 scripts/publish-performance.py render
python3 scripts/publish-performance.py render --check
```

Check mode is read-only and reports stale output. Rendering works offline from
committed evidence, without the original `target` directory. `--root PATH`
selects another repository root for isolated checks and tests.

## Admission and retained evidence

The historical publisher accepts complete `scriptw-v8-machine-baseline-v2` reports with
startup enabled and exactly the ordinary `startup.js`, `integer.js`,
`objects.js` and `retained-objects.js` workload inventory. It rejects partial,
selected-workload, startup-disabled and explicit native-policy reports for
the retained startup history. Separate diagnostic scoreboards retain their own scope.

Admission uses `benchmark_metrics.load_report`: retained logs, source snapshots
and emitted P Wasm must match their hashes; child commands must have completed
successfully; measured outputs and warmups must match the plan; and summaries
are recomputed from the raw observations. Additional checks bind the report's
source and executable identities to the compiler-identity and Wasmtime child
outputs. A new label cannot turn old measurements into evidence for new code.

Each admitted report has a deterministic archive at
`docs/rust-rewrite/performance-history/REPORT_SHA256.tar.gz`. It contains the
exact `report.json` and every retained log, workload file and P Wasm file
(80 retained files for each seeded seven-sample report).
R is not added: it was not part of the original report's retained-file plan.
The manifest pins both report and archive SHA-256 values. Every render/check
safely extracts the archive to a temporary directory and revalidates the full
report, including the actual P bytes. Missing or changed evidence fails refresh.

Records retain measurement timestamps and measured compiler commits. The
publication date and checkout HEAD do not replace those identities. The initial
2026-10-24 series contains compatible checkpoints `d89fd4068`, `30fc19314`
and `31c5958d2`; its latest measurement is `31c5958d2`, not the later FFI commit.

## Historical startup comparison boundaries

Both sections show V8 in orange and Scriptw in blue, with actual values at each
bar's end. Lower values are better. There is no ratio column or blended overall
score.

The timing section shows paired absolute median elapsed times in milliseconds.
All five measurements share one zero-based linear time scale: cold start,
cached start, 10,000 integer steps, 500 objects and 8,192 retained objects.

The memory section shows paired absolute peak process-RSS values in MiB. All
three rows share one zero-based linear RSS scale, separate from the time scale
because the units differ. Cold and cached startup use median peaks;
retained-object execution uses the whole-child peak, including compilation and
warmups. Process RSS includes host/runtime overhead and is not live JavaScript
heap usage.

Cold and cached startup measure fresh processes. Execution compiles once per
case/process, then uses a fresh Wasmtime Store/Realm or V8 Context per invocation
after equal warmups. Scriptw's execution span includes realm/builtin work and
completion. These scores are not isolated calls within an existing Realm.

Comparison cohorts group only reports admitted by the existing `compare()`
compatibility fields: schema, machine, configuration, measurement policy,
release profile, Node versions, GNU time version/hash, Wasmtime version, complete workload
metadata, harness hashes and sample count. Compiler source and image identities
may change. Different hardware, runtime versions, harnesses, corpora or policies
form separate cohorts. All cohorts and their immutable raw evidence remain in
the manifest; its separate SVG renders the latest admitted startup checkpoint.

These are descriptive observations; retain both absolute scores and raw samples
when assessing a compiler change. The graphic establishes no confidence
interval, statistical significance, V8 parity or conformance result.

## Measure a later compiler

Use a clean detached worktree at the committed compiler revision. This avoids
the benchmark's clean-tree check being blocked by unrelated local files. Build
and verify first, then measure separately on an otherwise idle machine. Keep
the local one-CPU, 4-GiB, no-swap process-tree policy:

```sh
mkdir -p target/performance-build
python3 scripts/limited_verification.py -- \
  cargo build --locked --offline --release --jobs 1 -p scriptw-cli \
  --bin scriptw --example runtime_baseline --message-format=json \
  > target/performance-build/cargo.jsonl
python3 scripts/limited_verification.py -- \
  python3 scripts/benchmark-v8.py \
  --scriptw target/release/scriptw \
  --runtime-runner target/release/examples/runtime_baseline \
  --build-log target/performance-build/cargo.jsonl \
  --output-dir target/performance-next \
  --samples 7 --warmups 10 --idle-machine
```

Linux, Node and GNU time are required. Use `--time PATH` for a GNU time binary
outside PATH; if `CARGO_TARGET_DIR` is set, substitute its actual executable
paths. The output directory must be new. Retain the entire successful report
directory, then run `record` in the publishing checkout with that report path.

Publication stays outside `benchmark-v8.py`, whose bytes participate in harness
identity. Rendering a graph must not change the measurement harness or silently
create a new timing cohort. See the
[baseline methodology](../v8-baseline.md#current-performance-and-memory-tooling)
for the full measurement policy and diagnostic commands.
