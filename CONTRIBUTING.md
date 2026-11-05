# Contributing to Scriptw

Read [AGENTS.md](AGENTS.md) for the compiler contract and
[conformance ownership](docs/rust-rewrite/conformance-ownership.md) for the owning
area. [Current work](docs/rust-rewrite/current-inventory.md) tracks open gates;
[acceptance criteria](docs/rust-rewrite/conformance-acceptance.md) define completion.

## Checkout and build

Use the stable toolchain selected by `rust-toolchain.toml`. Keep the full checkout,
including `.cargo/config.toml`, generated data, vendor sources and test fixtures.
For an existing cone-mode sparse checkout:

```sh
git sparse-checkout add .cargo .github crates scripts vendor test262 docs snapshots
cargo metadata --locked --offline --no-deps --format-version 1
```

Offline metadata requires installed dependencies. Linux defaults require `lld`;
`.cargo/config.toml` documents the override. `scripts/dev.sh` and ordinary Cargo
share flags and `target/`. Avoid changing Rustflags when reusing build artifacts:
a different fingerprint can rebuild the dependency graph.

Freeze production measurements from a CLI-only build (`cargo build --release
--locked -p scriptw-cli --bin scriptw`). Building the CLI and examples together
activates dev-dependency features in the CLI. Retain Cargo JSON and compare its
target dependency features with the baseline; runtime-image host features are a
separate graph.

Situation campaigns hash host/Node/time at admission and termination, including
failed runs, and check file metadata before each use.

Run native builds and tests through the resource wrapper:

```sh
python3 -B scripts/limited_verification.py -- ./scripts/dev.sh build
./target/debug/scriptw run crates/scriptw-cli/tests/fixtures/hello.js
```

The local wrapper enforces one CPU, at most 4 GiB, no swap, grouped OOM termination,
serial Cargo/libtest and bounded retained-module caches. On a managed cloud
machine, explicit `--cloud` uses its finite inherited cgroup budget; serial
defaults remain. Optional cloud CPU settings never authorize changing a campaign's
workers, deadlines or cache policy. Missing resource controls must fail clearly.

Prefer persistent Cargo output directories. If filesystem pressure requires
temporary tmpfs outputs, restore verified persistent copies before runtime tests.
Tmpfs pages consume the producing cgroup's memory budget after compilation ends.

## Development workflow

1. Start from a reproducible failure or missing invariant and choose one coherent
   batch. Give shared files one owner; implement independent chunks concurrently
   when that helps. Prefer existing domain boundaries over speculative abstractions.
2. Finish the code, appropriate regressions, types and documentation before costly
   verification. Run cheap source and structural checks before a broad native
   build; compile early only to resolve a blocking uncertainty or risky foundation.
3. Compile once, run focused and adjacent checks, then the required broader
   checkpoint in sequence so Cargo reuses artifacts. After failures or test-only
   edits, retain passing unaffected scopes with their recorded source and artifact
   identities, rerun affected checks, then continue unfinished scopes. Report
   commands, actual inventories, results and unverified work. Batch source changes
   and commits around verification and measurement checkpoints: changing HEAD
   invalidates compiler identity. A test-only correction is not a reason to
   rebuild unchanged production code or repeat its passing scopes.

Prefer compiler-enforced invariants and focused consumer contracts. Keep tests
that protect behavior; discard exploratory harnesses and duplicate prose checks.
Never branch on Test262 paths, source text or assertion messages to manufacture
conformance. Update README for user-visible changes; publish status numbers only
through the verified status workflow.

## Verification

Start with formatting, ownership and affected source identities:

```sh
cargo fmt --all -- --check
python3 -B scripts/conformance_owners.py
python3 -B scripts/intl_source_identities.py
```

Before a changed backend's dependency build, source guards provide a fast diagnostic:

```sh
python3 -B scripts/limited_verification.py -- python3 -B scripts/check-source-guards.py
```

They compile source-only Rust harnesses, including runtime-link routing and host
authority checks (`--target runtime_link`). Typed IR/Wasm modules remain in the full
Cargo structural targets; guards do not replace native or semantic acceptance.
Then run a focused target and the batch's broader checks under the same wrapper:

```sh
python3 -B scripts/limited_verification.py -- cargo check --locked --workspace --all-targets
python3 -B scripts/limited_verification.py -- cargo test --locked -p scriptw-aot-wasm --test structure_language -- --test-threads=1
python3 -B scripts/limited_verification.py -- ./scripts/check-no-interpreter-in-product-graph.sh
```

Encoded R/P ABI and code-size acceptance checks belong in the external
`emission` target when the public emission API is sufficient. It links the
ordinary emitter library that Engine also uses; private unit checks remain in
`--lib` qualification. For the shared internal operations:

```sh
python3 -B scripts/limited_verification.py -- cargo test --release --locked -p scriptw-aot-wasm --test emission -- has_instance_helper:: --test-threads=1
python3 -B scripts/limited_verification.py -- cargo test --release --locked -p scriptw-aot-wasm --test emission -- own_property_helper:: --test-threads=1
python3 -B scripts/limited_verification.py -- cargo test --release --locked -p scriptw-aot-wasm --test emission -- own_keys_helper:: --test-threads=1
```

Select the owning target rather than copying the example for every change.
A zero-test filter is not a passing regression check. Use
`scripts/run_workspace_tests.py --plan-only --output <fresh-directory>` to inspect
Cargo-selected scopes; use `--test-processes 1` for local execution. The driver
supports source-bound receipts, focused scopes, debug/release profiles and bounded
CLI partitions. `scripts/run_workspace_cohorts.py` bounds linking across larger
inventories; explicitly select one test process locally. CI command groups live in
`scripts/ci-targets.json`; `run-ci-target.py --print-command` shows their argv.

For the resumable per-module CLI baseline, run `scripts/rung1c-chunks.sh` under the
resource wrapper with fresh `RUNG1C_DONE`, `RUNG1C_COUNTS` and `RUNG1C_RESULTS` paths
per source cohort. Preserve them on an exact-source resume. Its count sidecar
cannot detect same-count test edits or compiler changes: invalidate affected
chunks after either. Incomplete, killed and zero-test chunks cannot establish a
verdict. The disjoint chunk union must cover the complete selected inventory;
chunking does not test whole-suite ordering effects.

```sh
RUNG1C_DONE=target/watched/fresh-cli-done \
RUNG1C_COUNTS=target/watched/fresh-cli-counts \
RUNG1C_RESULTS=target/lane-notes/fresh-cli.md \
python3 -B scripts/limited_verification.py -- ./scripts/rung1c-chunks.sh
```

Long checks use `scripts/run-watched.sh` with the scope's existing stall allowance.
Silence alone does not establish a hang. Keep original failures and executable
identities. Final release acceptance still requires the full default/all-feature,
ignored, documentation, differential, robustness and conformance gates listed in
[acceptance criteria](docs/rust-rewrite/conformance-acceptance.md).

## Test262

Use the built CLI, original pinned suite and an exact case or subtree first:

```sh
python3 -B scripts/limited_verification.py -- ./target/debug/scriptw --jobs 1 test262 run built-ins/Array/prototype/find --execution-backend wasm-aot --suite-root test262/vendor/test262 --threads 1 --timeout-ms 60000
```

For a full baseline without publication, use a new snapshot family:

```sh
python3 -B scripts/limited_verification.py -- ./target/debug/scriptw --jobs 1 test262 report-all --execution-backend wasm-aot --suite-root test262/vendor/test262 --snapshot-dir target/test262-baseline --snapshot-name fresh-baseline --threads 1 --timeout-ms 60000
```

For explicit sharding use `test262 shard --shard INDEX/TOTAL [filter]`, with
one-based indices. A positional `1/2` is a literal filter. Preserve compiler,
suite pins, workers, original deadlines and snapshot identity before adding
`--resume`; never run two writers for one family. `progress-status` reads saved
progress, which does not prove a process is alive.

To publish a complete verified matrix, use:

```sh
python3 -B scripts/limited_verification.py -- ./scripts/publish-real-status-low-ram.sh wasm-aot <fresh-snapshot-name>
```

The publisher owns generated README counts. The chart can then be refreshed with
`python3 -B scripts/generate-test-graph.py --refresh-evidence --full-evidence`
and checked with `python3 -B scripts/generate-test-graph.py --check`.
For presentation-only changes, `python3 -B scripts/status_publication.py
--check-committed --render-readme` prints the concise block from the existing
verified generation. This changes no counts or dates and requires no new test run.
Fake suites and selected replays are separate evidence. `Unsupported`, timeout,
crash and bug outcomes are non-passing; retain all four documented upstream
disagreements as ordinary failures. A new complete publication is required to
replace the [published full-suite result](docs/rust-rewrite/test262-current.md).

## Benchmark charts

The benchmark cards in [benchmark details](docs/rust-rewrite/benchmarks.md) read
the verified inert performance archive; the README retains concise summaries. Rendering
executes no JavaScript, Wasm or native program images. Use the recorded plotting
version to reproduce the committed SVGs:

```sh
python3 -m venv target/chart-env
target/chart-env/bin/python -m pip install matplotlib==3.11.2
python3 -B scripts/limited_verification.py -- target/chart-env/bin/python scripts/render-runtime-comparison.py --check
```

Omit `--check` to regenerate; add `--preview-dir target/chart-previews` for PNGs.
The renderer reads `execution_three_runtime` in
`docs/rust-rewrite/performance-gaps-20261101.json`, validates the three-runtime
schedule and process samples, and writes `execution-current.json`, the
`assets/benchmarks-current/` cards, the detailed comparison and its README summary. The data
retains the exact Wasmtime host and Node/V8 identities and timing policy.
The older CI comparison retains its dated evidence and ordinary runtime
configuration in `docs/rust-rewrite/compiler-comparison.md`.

For a fresh three-runtime reference, reuse a locally built, qualified frozen
cohort containing its producer receipts and matching native images. The driver
checks source bytes and images before running the unchanged 17 workloads:

```sh
python3 -B scripts/limited_verification.py -- python3 scripts/benchmark-runtime-comparison.py --frozen target/qualified-cohort --output target/new-runtime-reference --time /usr/bin/time --qualification-key array_global_constants
```

Select the installed GNU time binary with `--time`; `--node` selects Node and its
exact executable/version are recorded. `--qualification-key` selects the source-bound
authority entry (default `private_numeric_facts`). Each actor runs in eight fresh processes
per case, with 21 measurements and ten checked warmups per process. V8 context
creation is captured separately from its public-entry timer.

For JavaScript-to-portable-Wasm compilation, select a frozen ordinary CLI
(`scriptw`, not a precompiled execution host), Scriptc 0.4.5 and Zig 0.16.0:

```sh
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-compilation.py run \
  --scriptw target/qualified-cohort/candidate-ordinary/scriptw \
  --scriptc target/performance-compact-row-entry/tools/scriptc/node_modules/.bin/scriptc \
  --runtime-pack target/performance-compact-row-entry/tools/scriptc/node_modules/@scriptc/runtime-wasm32-wasi \
  --zig target/performance-compact-row-entry/tools/zig-x86_64-linux-0.16.0/zig \
  --time target/performance-worker-cpu/tools/time \
  --package-lock target/performance-compact-row-entry/tools/scriptc/package-lock.json \
  --output target/new-compilation-reference
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-compilation.py publish \
  target/new-compilation-reference/report.json --output target/new-compilation-publication
python3 -B scripts/limited_verification.py -- target/readme-chart-env/bin/python -B scripts/benchmark-compilation.py render \
  target/new-compilation-publication/compilation-comparison.json \
  --svg target/new-compilation-publication/compilation.svg --png target/new-compilation-publication/compilation.png
python3 -B scripts/limited_verification.py -- target/readme-chart-env/bin/python -B scripts/benchmark-compilation.py render \
  target/new-compilation-publication/compilation-comparison.json \
  --svg target/new-compilation-publication/compilation.svg --check
```

Use Matplotlib 3.11.2 in `target/readme-chart-env`. Measurement and publication
directories must be fresh. The fixed plan measures 136 actual CLI processes:
four builds per compiler for each original workload, across two balanced blocks. Wall time includes
compiler startup and output I/O; Wasm-to-native compilation and execution are
excluded. Scriptw emits P plus its shared R sidecar; Scriptc emits standalone
WASI Wasm with its prebuilt runtime linked in. Runtime production is excluded;
Scriptw's R sidecar write is included on every build and retained once by digest.

Each process gets fresh application caches and scratch. Scriptc also sets
`SCRIPTC_NO_CACHE=1`; existing Zig global toolchain and OS page caches remain.
Only Scriptc receives the pinned `print` adapter. The checked publication retains
source/tool hashes, diagnostics, raw wall/child-peak-RSS observations and portable
artifacts. Compilation success does not verify runtime output or conformance.

## Focused performance situations

Generate the checked programs for each performance hill.
They use retained Modules with untimed `prepare(input)`
and timed `run()`, shared by Scriptw and Node/V8:

```sh
mkdir -p target/situation-build
python3 -B scripts/performance_situations.py --output-dir target/situation-fixtures
python3 -B scripts/limited_verification.py -- cargo build --release --locked -p scriptw-cli --example compiler_precompiled_baseline --message-format=json-render-diagnostics > target/situation-build/cargo.jsonl
cp target/release/examples/compiler_precompiled_baseline target/situation-build/host
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-situations.py \
  --host target/situation-build/host --producer-log target/situation-build/cargo.jsonl \
  --fixtures target/situation-fixtures --output target/new-situation-reference \
  --node /path/to/node-24.12.0 --time /path/to/gnu-time
python3 -B scripts/limited_verification.py -- target/readme-chart-env/bin/python -B scripts/render-situation-benchmarks.py \
  --report target/new-situation-reference/report.json --output-dir target/situation-charts \
  --preview-dir target/situation-previews
```

Use fresh output directories. `--case <file.js>` selects a hill; `--policies fast`
or `--policies speed` selects a Scriptw compilation policy. Each default cohort
uses two balanced blocks and eight fresh processes per case/actor, with 100
checked warmups and 21 measured calls. Normal V8 JIT and natural GC remain
enabled; this warmup count does not prove optimized steady state. Native compilation
precedes those processes; every Wasmtime consumer has its compiler disabled. Record preparation
time and native-image size separately. Mutation and getter controls have their
own scores; their differences are not allocation counts or isolated stack costs.
Scriptc has no matched retained-Module consumer in this workflow.

After a later Cargo build replaces the original executable, a frozen host can
use `--producer-receipt <receipt.json>`. The receipt must retain the exact Cargo
`artifact` row, the frozen host's `sha256` and its `source_manifest_sha256`.
The runner verifies and archives it alongside the original Cargo stream.

## React and edge benchmarks

Use a fresh ignored tool directory with exact `react`, `react-dom` and
`react-server-dom-webpack` 19.3.1, esbuild 0.28.2 and Playwright 1.64.1. Keep its
package manifest, lockfile and installed package hashes with the results. The
fixture source lives in `crates/scriptw-cli/tests/fixtures/edge_rendering/`.

```sh
mkdir -p target/application-tools
python3 -B scripts/limited_verification.py -- pnpm --dir target/application-tools add --save-exact --ignore-scripts \
  react@19.3.1 react-dom@19.3.1 react-server-dom-webpack@19.3.1 \
  esbuild@0.28.2 playwright@1.64.1
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-react.py prepare \
  --tools target/application-tools --output target/new-application-fixtures --time /path/to/gnu-time
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-applications.py \
  --fixtures target/new-application-fixtures --frozen target/qualified-cohort \
  --scriptc /path/to/scriptc --runtime-pack /path/to/runtime-wasm32-wasi \
  --zig /path/to/zig-0.16.0/zig --time /path/to/gnu-time --timeout 60 --output target/new-application-reference
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-react.py run \
  --tools target/application-tools --fixtures target/new-application-fixtures \
  --frozen target/qualified-cohort --scriptc /path/to/scriptc \
  --runtime-pack /path/to/runtime-wasm32-wasi --zig /path/to/zig-0.16.0/zig \
  --chromium /path/to/chrome --time /path/to/gnu-time --timeout 60 --output target/new-react-reference
```

The portable cohort measures fresh Wasm public entry versus V8 `runInContext`,
with exact response output checked on every warmup and sample. Six edge cases use
plain request/response records; they do not implement Fetch or a deployed edge
platform. The additional synchronous React HTML renderer is the unmodified pinned
legacy production entry, selected explicitly during bundling.

Real RSC benchmarks serialize and fully consume Flight streams. They run with
Node's genuine Web APIs, initialize modules once per fresh process, and record
parse/initialization separately from request execution. Client React measures
real Chromium DOM mount, state update and hydration through commit plus forced
layout; validation, cleanup, screenshot and initial bundle loading are outside
that timer. Browser and server reference timings use different boundaries and
are not cross-engine speed ratios.
The [Server Components](https://react.dev/reference/rsc/server-components) and
[client DOM](https://react.dev/reference/react-dom/client) APIs are separate
scopes; synchronous HTML SSR is a third workload.

Current-source qualification covers the pinned React 19.3.1 sync and async RSC
fixtures, each checked against its exact Node Flight output. The matched Node
reference uses the same generated `rsc.js` and wrapper in one retained
`vm.SourceTextModule` and Realm, running sync then async with three checked
warmups and seven samples per mode. Its request clock ends when the full Flight
Promise resolves; JSON encoding and golden comparison follow the clock. Parse,
link and module initialization remain separate setup spans. This is a fixture
qualification and descriptive timing comparison, not full Web API or WHATWG
Streams conformance or an engine-parity claim.

The retained Scriptw probe uses the same generated inputs:

```sh
python3 -B scripts/limited_verification.py -- cargo build --release --locked -p scriptw-cli --example rsc_baseline
python3 -B scripts/limited_verification.py -- ./target/release/examples/rsc_baseline \
  target/new-application-fixtures/rsc.js target/new-application-fixtures/support.json \
  target/new-retained-rsc.json 7 3 fast
```

Build the production CLI separately as described above. This probe reports actual
native settings and separates setup from requests. `speed` selects the optional
Speed/Backtracking diagnostic; keep it outside the matched Fast cohort. A single
probe is not a complete comparison; freeze binaries before repeating the balanced
process campaign. The published evidence archive retains that campaign's driver.

Each reference uses two blocks, eight fresh processes and seven measured calls
after three checked warmups. Record Chromium and Node/V8 identities. Standalone
Scriptw/Scriptc attempts retain compilation and execution errors explicitly.
Scriptw's portable build and ordinary NativeHost run are separate probes,
and the latter recompiles source rather than executing the dumped portable pair.
The ordinary CLI selects native compilation automatically; large function bodies
can select size-optimized compilation instead of Fast.
Unsupported host APIs never become zero bars, wins or permanent exclusions. All
children remain serial under the resource wrapper. No product compiler rebuild
is needed when the recorded frozen Rust bytes still match.

## Web platform runtime

The standalone platform surface supports UTF-8 `TextEncoder`/`TextDecoder`, byte
`ReadableStream` sources with default readers, `AbortController`/`AbortSignal`,
standalone `EventTarget`/`Event`, `DOMException`, `queueMicrotask`, `setTimeout` and
`clearTimeout`. These use the existing compiled Wasm GC object model. UTF-8 label
aliases are accepted; other decoder encodings reject explicitly. BYOB readers,
generic object streams, pipe/tee operations, Fetch/URL and DOM nodes are separate
capabilities, not implied by this surface. The changes use Runtime ABI 47.
Current-source sync and async React RSC fixture output passes exact Flight checks;
that does not establish full Web API or WHATWG Streams conformance.

Microtasks drain at checkpoints. Timers use a distinct queue and monotonic
clock; the CLI waits for their deadlines under its execution limits. Embedding
`pump` polls ready tasks without sleeping, allowing the embedder to control its
own event loop. Callback exceptions are reported while the checkpoint continues.

## Artifact reuse

Reuse requires exact source, features, dependency graph, runtime/program identities
and resource policy; filenames alone prove nothing. Keep fresh evidence directories
for new cohorts and preserve failed receipts. Shared immutable runtime images and
bounded caches reduce rebuilds without sharing mutable JavaScript state.

Use a disk-backed `SCRIPTW_CACHE_DIR` for footprint comparisons and record the
cache filesystem and native backing mode alongside image digests. On Linux, the
Cargo runtime's private-file loader rejects tmpfs/ramfs and uses anonymous loading.
Moving a verification cache to `/tmp` or `/dev/shm` can change RSS for the same
image. Compare RSS only across matching cache filesystems and backing modes.

Keep evidence in durable workspace/archive storage; `/dev/shm` is scratch. Preserve
outputs by digest before moving them, inspect symlink/hardlink destinations before
a rebuild, and account for actual filesystem free space and cgroup file pages.
Metadata-only cached-extern checks are diagnostics, not product builds or native
acceptance. Never replace a failed original receipt with a passing diagnostic retry.
