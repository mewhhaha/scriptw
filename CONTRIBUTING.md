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
git sparse-checkout add .cargo .github crates scripts vendor test262 docs
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
defaults remain. The cap is read from cgroup v2 and, on a hybrid machine, from
the cgroup-v1 memory and CPU hierarchies. Optional cloud CPU settings never
authorize changing a campaign's workers, deadlines or cache policy. Missing
resource controls must fail clearly. `--cloud-cpus auto` exports the CPU count
as the native compilation job count, which engine tests that require a one-job
pool reject; run test binaries with `--cloud-worker-cpus auto` and an explicit
`--test-threads` instead.

Prefer persistent Cargo output directories. If filesystem pressure requires
temporary tmpfs outputs, restore verified persistent copies before runtime tests.
Tmpfs pages consume the producing cgroup's memory budget after compilation ends.

### Dist compiler binary

`scripts/build-dist.sh` builds a local `scriptw` for compilation panels, frozen
benchmark cohorts and long local runs: the `dist` Cargo profile (release plus
fat LTO, one codegen unit and no DWARF) with jemalloc as the global allocator
(`--features jemalloc`). It is not the shipped binary: CI's native release
artifacts and Test262 publication still use `cargo build --release` (glibc
malloc, no LTO). A binary that links jemalloc must travel with
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md), which reproduces jemalloc's
BSD-2-Clause notice; that file does not yet cover every Rust dependency, so
do not publish dist binaries as release artifacts. Development and test
builds keep their profiles and glibc malloc. `--all-features` builds now
compile jemalloc's C code (`make` and `cc`) and run scriptw-cli's integration
tests on jemalloc. The recipe is Linux x86_64 with lld only, and no CI job
builds `--features jemalloc` or `--profile dist`.

```sh
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  scripts/build-dist.sh plain target/dist-out
```

The recipe needs `--cloud`: its fat-LTO link alone peaks at 3.8 GB RSS, which
does not fit the 4 GiB local default. A local run needs an explicit
`--memory-mib 6144` or more, outside the AGENTS.md local default. Cargo's job
count follows the wrapper's `SCRIPTW_JOBS` (the CPU count with `--cloud-cpus
auto`); `SCRIPTW_DIST_JOBS` overrides it. `scripts/build-dist.sh --check
[plain|pgo]` validates the host, toolchain, environment and directories without
building. A cold build takes about 25 minutes on four shared vCPUs. The binary
lands in `target/dist-out/scriptw`, next to a copy of `THIRD-PARTY-NOTICES.md`
and `scriptw.build-info`: the source commit and a digest of the tracked diff
and untracked files, read before the build starts and checked again before
publication (`source_unchanged_during_build`), the binary's own
`scriptw compiler-identity` output, toolchain, rustflags, profile and training
inputs, and the binary SHA-256. The out-dir must be dedicated: the script
refuses `target/` itself, its `debug`, `release` and `dist` directories, and
a directory holding a `scriptw` without a build-info. The dedicated target directory `target/dist-build`
(`SCRIPTW_DIST_TARGET_DIR`, never the shared `target/`) peaks at about 3.2 GB.
The script holds an exclusive `flock` on the target and output directories for
the whole run, so a concurrent run fails at once instead of deleting the
other's units or profiles. It deletes the target units after copying the
binary but keeps the host units (build scripts and the runtime-image
producer). It always passes `--target <host>`, so its rustflags reach only the
target units, and it repeats the lld link argument, which they would otherwise
replace; it refuses a pre-set `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS`.

On the inputs compared so far, dist and release binaries produced the same
artifacts byte for byte: P and the `scriptw precompile` images of the 17 panel
kernels, the RSC and `client.js` images and the kernels' `scriptw run` output.
The 44 cases of one Test262 directory passed under each binary by the stdout
summary only; their per-case outcomes were not compared. R comes from the build-time
runtime-image producer, a host unit that gets neither LTO nor PGO, so its
identity says nothing about the dist code. No committed command checks this
yet; compare a new binary against release yourself before trusting its
output elsewhere. Images embed the source fingerprint, which covers
`Cargo.toml` and `Cargo.lock` but not Cargo features or
`crates/scriptw-cli/Cargo.toml`: release builds with and without jemalloc
share a fingerprint but not an `executable_sha256`, so Test262 evidence from
one cannot be published by the other.

`scripts/build-dist.sh pgo` adds Rust PGO. It needs rustup's `llvm-tools`
component (`rustup component add llvm-tools`), whose `llvm-profdata` matches
rustc's LLVM; the system `llvm-profdata` is older and cannot read the profiles.
It builds an instrumented dist binary, runs `scripts/dist-pgo-train.sh`, merges
the single online-merged `.profraw` and rebuilds with the profile; the previous
profile stays until the new one is merged. Set
`SCRIPTW_DIST_PGO_APPLICATION_FIXTURES` to a generated application fixture
directory (`benchmark-react.py prepare`) to include the RSC and edge programs,
as the measured binary did; without it the training uses only the
in-repository kernels and Test262. The script header lists the held-out
inputs. PGO costs two dist builds plus about four minutes of training, and the
instrumented build's target units take 1.8 GB. On a held-out input the PGO
binary precompiled `client.js` 22% faster than release; see the
[measurements](docs/rust-rewrite/benchmarks.md#compiler-binary-build-2026-11-20)
and the source state they came from. Retrain after changing the allocator or
hot compiler code. A stale profile only costs speed, but rustc does not warn
when the profile no longer matches the code. `SCRIPTW_DIST_BOLT_PREP=1` links
the PGO binary with `--emit-relocs` and `-z keep-text-section-prefix` for a
later BOLT pass; it is off by default because BOLT is not part of the recipe
(LLVM 18's instrumentation pass needed more than 8 GB of RAM on this binary)
and the relocations add about 19 MB to the executable.

Freeze a dist binary, not a release one, for a new compilation or RSC
wall-time campaign, and keep its `scriptw.build-info`. For a long Test262
sweep prefer the plain mode: the speed of a PGO binary on a full sweep is
unmeasured, and compiler paths the training never reaches are optimized as
cold. A PGO build is not reproducible, because the profile depends on the
training run, and any rebuild changes `executable_sha256`. Every
`build-dist.sh` run deletes `target/dist-out/scriptw` first, so freeze a
binary before a sweep: copy `scriptw`, `scriptw.build-info` and
`THIRD-PARTY-NOTICES.md` to a directory of their own (for example
`target/frozen/<binary_sha256>/`) and use only that copy. Resume and publish
a dist sweep with the same frozen copy (`SCRIPTW_BIN=target/frozen/<sha>/scriptw`
for `scripts/publish-real-status-low-ram.sh`, or that copy's own
`test262 publish-status`); `cargo run -p scriptw-cli -- test262 publish-status`
rejects its evidence.

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

`scripts/ci-fast.sh [BASE_REF]` runs, in order, every guard step CI's `validate` job runs before it builds, including the three above, and lists the failing steps.

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
python3 -B scripts/limited_verification.py -- cargo test --release --locked -p scriptw-cli --test cli -- --test-threads=1
python3 -B scripts/limited_verification.py -- ./scripts/check-no-interpreter-in-product-graph.sh
```

Include `-p scriptw-cli --test cli` whenever lowering, analysis, the builtin
catalog or the global environment changes: its `inspect_reports_*` tests assert
exact `scriptw inspect` counts (exact-context clones, resolved intrinsics,
`global_bindings`, `builtin_globals`), so such a change moves them without any
fixture edit. Recount with `scriptw inspect <fixture>` and record the cause in
the assertion's comment.

The Boa differential oracle compiles only under `scriptw-spec-exec`'s `oracle`
feature, so the default workspace runner and `ci.yaml` skip that crate's own
library and integration tests. The differential workflow runs them; run them
locally when the oracle or its embedding changes:

```sh
python3 -B scripts/limited_verification.py -- cargo test --locked -p scriptw-spec-exec --features oracle
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

For a subtree of more than a few cases, add `--reuse-case-workers`: each worker
process then serves up to 64 cases, each with a fresh Engine, Realm and Store
and its own deadline, instead of loading the runtime and recomputing the suite
pin once per case (`built-ins/WeakMap`, 281 executions: 194 s one-shot, 11 s
reused). On a managed cloud machine also pass `--cloud-worker-cpus auto` to the
wrapper; without it the run is pinned to one CPU, and parallel runs share it.

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

The shared host gives Scriptw and Scriptc the same Store allocation: a one-slot,
kept-resident pool by default, or on-demand mappings for both under
`SCRIPTW_PRECOMPILED_STORE_ALLOCATION=on-demand`. Each Wasm report records its
`store_allocation`; the runtime-comparison preflight and both panels' publishers
refuse a campaign whose Scriptw and Scriptc processes differ.

`scripts/benchmark-store-allocation.py` reproduces the pooled against on-demand
A/B of the benchmark details. `prepare` builds one host's images: Scriptw sources
through `prepare-scriptw`, Scriptc 0.4.5 WASI programs through the same print
adapter, `scriptc build` and `prepare-wasi` commands as the application panel.
`run` alternates the two modes in fresh processes pinned to one CPU and counts
minor faults inside the reported entry through the host's phase barriers;
`summarize` gives per-block medians. It is an unscored diagnostic:

```sh
python3 -B scripts/limited_verification.py --cloud -- python3 -B scripts/benchmark-store-allocation.py prepare \
  --host target/release/examples/compiler_precompiled_baseline --output target/store-ab \
  --actor scriptw --actor scriptc --scriptc /path/to/scriptc --runtime-pack /path/to/runtime-wasm32-wasi \
  --zig /path/to/zig-0.16.0/zig crates/scriptw-cli/tests/fixtures/edge_rendering/edge-router.js \
  crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths/integer.js
python3 -B scripts/limited_verification.py --cloud -- python3 -B scripts/benchmark-store-allocation.py run \
  --prepared target/store-ab --output target/store-ab/ab.jsonl --cpu 3
python3 -B scripts/benchmark-store-allocation.py summarize target/store-ab/ab.jsonl
```

`scriptw-precompiled-consumer` only consumes images. Produce them with
`compiler_precompiled_baseline prepare-scriptw`/`prepare-wasi` built from the same
source; admission refuses another source fingerprint and names both. Its end-to-end
check builds the package on its own, as feature unification requires:

```sh
python3 -B scripts/limited_verification.py -- cargo test --release --locked -p scriptw-precompiled-consumer --test end_to_end
```

For JavaScript-to-portable-Wasm compilation, select a frozen ordinary CLI
(`scriptw`, not a precompiled execution host), Scriptc 0.4.5 and Zig 0.16.0:

```sh
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-compilation.py run \
  --scriptw target/qualified-cohort/candidate-ordinary/scriptw \
  --scriptc /path/to/scriptc/node_modules/.bin/scriptc \
  --runtime-pack /path/to/scriptc/node_modules/@scriptc/runtime-wasm32-wasi \
  --zig /path/to/zig-0.16.0/zig \
  --time /path/to/gnu-time \
  --package-lock /path/to/scriptc/package-lock.json \
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

### Comparison with Porffor

`scripts/benchmark-porffor.py` compares Scriptw with [Porffor](https://porffor.dev), another
JavaScript-to-Wasm AOT compiler, on the original17 kernels: runtime and compilation. Install
the pinned tools into one directory under `target/`, never globally, and keep its
`package-lock.json`:

```sh
npm install --prefix target/wasm-peer-tools --no-audit --no-fund --ignore-scripts --save-exact porffor@0.61.13
python3 -m venv target/wasm-peer-tools/wasmtime-env
target/wasm-peer-tools/wasmtime-env/bin/pip install wasmtime==47.0.1
```

The driver checks both pins and Node 24.22.1. It takes a shared precompiled host
(`compiler_precompiled_baseline`) and the `scriptw` CLI built from the same source, and
never builds them. Give every timed command the same CPUs (here 2–3, the run stays on both;
compilation takes the first) and unset the session's `NODE_OPTIONS`:

```sh
taskset -c 2,3 env -u NODE_OPTIONS python3 -B scripts/limited_verification.py --cloud --cloud-worker-cpus auto -- \
  python3 -B scripts/benchmark-porffor.py prepare --cloud --output target/peer-prepared \
  --tools target/wasm-peer-tools --wasmtime-python target/wasm-peer-tools/wasmtime-env/bin/python \
  --host /path/to/compiler_precompiled_baseline --scriptw /path/to/scriptw \
  --node /path/to/node-24.22.1 --time /usr/bin/time
taskset -c 2,3 env -u NODE_OPTIONS python3 -B scripts/limited_verification.py --cloud --cloud-worker-cpus auto -- \
  python3 -B scripts/benchmark-porffor.py run --cloud --prepared target/peer-prepared --output target/peer-run --time /usr/bin/time
taskset -c 2,3 env -u NODE_OPTIONS python3 -B scripts/limited_verification.py --cloud -- \
  python3 -B scripts/benchmark-porffor.py compile --cloud --prepared target/peer-prepared --output target/peer-compile --time /usr/bin/time
python3 -B scripts/benchmark-porffor.py publish --prepared target/peer-prepared --run target/peer-run \
  --compile target/peer-compile --output docs/rust-rewrite/wasm-peer-current.json
python3 -B scripts/benchmark-porffor.py tables docs/rust-rewrite/wasm-peer-current.json
```

`prepare` builds Porffor Wasm (default `-O1`, Scriptc's `print` adapter) and Scriptw native
images untimed, then runs every actor/kernel once with checked output. Compile failures,
rejected modules, runtime failures and wrong outputs are retained as failed attempts, with
Porffor's own runner on the same source as a cross-check, and are never timed. `run` uses the
runtime panel's sampling (21 entries after ten checked warmups, eight fresh processes per
actor/kernel in two reversed blocks, Williams order) for four actors: Scriptw on the shared
Wasmtime host; Porffor's Wasm on Wasmtime through `scripts/wasm_peer_wasmtime.py` with the
host's Cranelift settings and Store pool; the same Wasm on Node's WebAssembly engine through
`scripts/wasm_peer_node.cjs`; and the source on Node/V8. `--smoke` and `--case` select tooling
scopes that `publish` refuses. `compile` times four fresh `scriptw build wasm` and Porffor `wasm`
processes per kernel on one CPU. `publish` writes the checked summary without host paths; paste
the `tables` output into the benchmark details unedited.

### Process density

`scripts/benchmark-density.py` measures how many processes fit in memory, process wall time and
per-entry runtime for Scriptw, Node 24.22.1 (default and lean flags), Node 22.23.4 and Porffor
native on a startup program and five kernels. It reuses the Porffor tools above and needs `cc`
(Porffor `native` compiles its C with it) and ptrace of the driver's own children. It never builds
Scriptw. Keep the CPUs and cleared Node settings of the Porffor panel:

```sh
taskset -c 2,3 env -u NODE_OPTIONS -u NODE_EXTRA_CA_CERTS -u NODE_PATH \
  python3 -B scripts/limited_verification.py --cloud --cloud-worker-cpus auto -- \
  python3 -B scripts/benchmark-density.py prepare --cloud --output target/density/prepared \
  --tools target/wasm-peer-tools --host /path/to/compiler_precompiled_baseline --scriptw /path/to/scriptw \
  --node /path/to/node-24.22.1 --node22 /path/to/node-22.23.4 --time /usr/bin/time
taskset -c 2,3 env -u NODE_OPTIONS -u NODE_EXTRA_CA_CERTS -u NODE_PATH \
  python3 -B scripts/limited_verification.py --cloud --cloud-worker-cpus auto -- \
  python3 -B scripts/benchmark-density.py run --cloud --prepared target/density/prepared --output target/density/run --time /usr/bin/time
python3 -B scripts/benchmark-density.py publish --prepared target/density/prepared --run target/density/run \
  --output docs/rust-rewrite/process-density-current.json
python3 -B scripts/benchmark-density.py tables docs/rust-rewrite/process-density-current.json
python3 -B -m unittest scripts.tests.test_benchmark_density
```

`run` holds N = 1, 8 and 32 concurrent processes per actor and program (two groups each) and admits
a group only if its unshared RSS estimate leaves 6 GiB of `MemAvailable` free; skipped groups are
recorded. `--smoke` (N = 1 and 4, short harness runs) checks the tooling and `publish` refuses it.

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
  --node /path/to/node-24.22.1 --time /path/to/gnu-time
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

Check the published situation cards and the separate fixed-yield width panel together:

```sh
python3 -B scripts/limited_verification.py -- target/readme-chart-env/bin/python -B scripts/render-situation-benchmarks.py \
  --report docs/rust-rewrite/situations-current.json \
  --width-report docs/rust-rewrite/for-in-width-current.json \
  --output-dir docs/rust-rewrite/assets/situations-current --check
```

The width panel keeps 1,024 yields per call across 8–256 keys, with counting and
value reads measured separately. Its paired controls retain both compiler identities;
charts show the current compiler and V8.

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

RSC qualification uses the pinned React 19.3.1 sync and async fixtures, each
checked against its exact Node Flight output. Both hosts use the same generated
`rsc.js` and module wrapper. Timing ends after Flight serialization, complete
stream consumption, decoding and the fixture's internal sanity checks. The
outer result's JSON encoding and golden comparison follow the clock. Scriptw
includes host entry and required Promise pumps; Node includes await completion
and its Web APIs. Setup is separate. This is a fixture comparison, not full Web
API or WHATWG Streams conformance or an engine-parity claim.

The retained Scriptw probe uses the same generated inputs:

```sh
python3 -B scripts/limited_verification.py -- cargo build --release --locked -p scriptw-cli --example rsc_baseline
python3 -B scripts/limited_verification.py -- ./target/release/examples/rsc_baseline \
  target/new-application-fixtures/rsc.js target/new-application-fixtures/support.json \
  target/new-retained-rsc.json 21 100 fast rsc-sync
```

Build the production CLI separately as described above. This probe reports actual
native settings and separates setup from requests. `speed` selects the optional
Speed/Backtracking diagnostic; keep it outside the matched Fast cohort. A single
probe is not a complete comparison. Freeze each host before changing compiler
source, retaining its Cargo JSON producer and source diff, then run:

```sh
python3 -B scripts/limited_verification.py -- python3 -B scripts/benchmark-rsc.py \
  --host baseline=target/rsc-baseline/host --host candidate=target/rsc-candidate/host \
  --producer baseline=target/rsc-baseline/cargo.jsonl \
  --producer candidate=target/rsc-candidate/cargo.jsonl \
  --fixtures target/new-application-fixtures --node /path/to/node --time /path/to/gnu-time \
  --output target/new-rsc-comparison
```

On a managed cloud machine, which has no delegated 4 GiB cgroup scope, pin two
CPUs, clear the proxy's Node settings and pass `--cloud` to the runner; the
report records the inherited cap and the affinity as its resource scope. Every
actor gets the same two CPUs because V8's compiler and collector threads
otherwise preempt its main thread inside timed calls:

```sh
env -u NODE_OPTIONS -u NODE_EXTRA_CA_CERTS -u NODE_PATH taskset -c 2,3 \
  python3 -B scripts/limited_verification.py --cloud --cloud-worker-cpus auto -- python3 -B scripts/benchmark-rsc.py --cloud \
  --host baseline=target/rsc-baseline/host --host candidate=target/rsc-candidate/host \
  --host candidate-speed=target/rsc-candidate/host --policy candidate-speed=speed \
  --fixtures target/new-application-fixtures --node /path/to/node --time /usr/bin/time \
  --samples 101 --output target/new-rsc-comparison
```

`scripts/publish-rsc-current.py --campaign target/new-rsc-comparison ...` reduces
a complete campaign of exactly those four actors to
`docs/rust-rewrite/rsc-current.json`, and
`scripts/render-application-benchmarks.py --rsc-json docs/rust-rewrite/rsc-current.json`
draws its three charts (`--check` verifies them).

The other campaigns take `--cloud` under the same wrapper and build their own
frozen cohort from the committed tree:

- **Kernels:** `benchmark-runtime-comparison.py prepare-cohort --cloud --output <cohort> ...`,
  then `benchmark-runtime-comparison.py --cloud --frozen <cohort> --qualification-key cohort ...`,
  `benchmark-runtime-comparison.py publish <measurement>` and `render-runtime-comparison.py`.
- **Compilation:** `benchmark-compilation.py run --cloud --scriptw <cohort>/candidate-ordinary/scriptw ...`
  on one CPU for both compilers, then `publish`, `install` and `render`.
- **Edge and React:** `freeze-application-cohort.py --output <cohort>`,
  `benchmark-react.py prepare --cloud`, `benchmark-applications.py --cloud`,
  `benchmark-react.py run --cloud`, `publish-application-benchmarks.py` and
  `render-application-benchmarks.py`. Pass Scriptc's real binary
  (`node_modules/scriptc/bin/scriptc.exe`), not the package manager's shim.
- **Situations:** `benchmark-situations.py --cloud`, `publish-situations-current.py publish`,
  `render-situation-benchmarks.py`, and `publish-situations-current.py tables` for the
  Markdown the details page quotes. The driver and its V8 adapter pin Node 24.22.1.

A cloud cohort's qualification is its freeze and its output checks; it records
that no native test selection was run.

The maintained campaign gives each case its own process and retained Realm,
avoiding sync-to-async warmup contamination. Defaults are 100 checked warmups,
21 samples, and eight fresh processes per actor/case in two reversed blocks.
`--warmups`, `--samples`, `--processes-per-block` and `--case` select explicit
scopes; warmup count does not prove optimized steady state. Reports retain all
warmup and measured times, checked output identities, per-process means,
medians and nearest-rank p95, process summaries, cold phases and whole-process
high-water RSS. Request samples in one process are dependent; p95 is descriptive,
not a confidence bound. Cargo logs are retained, not feature-graph qualified.
Tracing, profiling and artifact-export runs are rejected as scored inputs.
The older published seven-sample campaign retains its original runner and Node
driver in its evidence archive; its fixed sync-then-async order is historical.
Repeat `--host NAME=PATH` and select `--policy NAME=speed` to include an explicit
native-policy diagnostic in the same balanced schedule. Each actor retains its
own policy and source identity; never blend its samples with Fast.

The host's `fast` and `speed` modes are the embedding API's two
`NativeCompilationPolicy` variants; `speed` is that API's default.
For excluded compiler-policy diagnostics, `fast-backtracking` and
`speed-single-pass` cross optimization and register allocation independently.
Record their actual configuration and separate process cohorts alongside the
ordinary Fast/Speed controls.

[RSC attribution](docs/rust-rewrite/react-attribution-current.json) separates
request-window hardware counters, frame-pointer samples and exact native entry
counts. Its [inert evidence archive](docs/rust-rewrite/rsc-attribution-evidence-2026-11-07.tar.xz)
retains the drivers, input identities, emitted samples, same-process maps and
known-count controls. Drivers require a qualified frozen host and its matching
baseline JSON; rebuilding requires a new baseline identity. These diagnostics
include caller control edges, exclude setup/output checks and never supply scored
request timings. Binary perf data and native executables are not in the archive.

Where no hardware counter is available (a virtual machine without a PMU),
`scripts/rsc_instruction_counts.py` counts instructions per retained request
with Callgrind instead, bounding each request at the same host barrier:

```sh
python3 -B scripts/limited_verification.py --cloud -- python3 -B scripts/rsc_instruction_counts.py \
  count target/rsc-candidate/host target/new-application-fixtures target/new-rsc-counts --mode sync
```

It reports the median and mean over 97 requests after 100 warmups. With
`--profile` it keeps the per-request dumps, which `symbols`, `callers
<helper>` and `function <program.cwasm> <symbol> --listing <file>` read:
own instructions per native body, calls and inclusive cost by caller, and the
executed native instructions of one body with their counts. Native code is
named through the host's perf map; `program.cwasm` is the image the host
writes under `SCRIPTW_RSC_ARTIFACT_DIR`. These are Callgrind instruction
references: exact and repeatable, about 0.8% above hardware user-instruction
counts on the retained sync request, and never a time or a cycle count.
Compare two hosts only through the same tool, and build the baseline host in
the same Cargo target directory before or after the candidate rather than in
a second build tree when disk is short. Copy a host out of
`target/release/examples` before the next build replaces it.

`count --cache` adds Valgrind's cache and branch simulation and reports the
simulated instruction fetch misses, data misses and mispredicted branches
per request; `symbols --event I1mr` attributes one of those events instead
of instructions. A `rep stos` fill counts once per byte in Callgrind, so
`memset` looks many times its hardware cost.

To measure what an external Wasm optimiser would gain over the emitter, build
the host with `--features module-replacement` and set
`SCRIPTW_RSC_PORTABLE_BUNDLE` to a portable bundle that holds a rewritten P or
R: start from the `portable.bundle` that `SCRIPTW_RSC_ARTIFACT_DIR` writes,
replace a module and recompute its checksums and runtime key. The host admits
it through `Engine::load_portable_bundle` and checks the exact Flight output
as usual. A rewritten R must keep its `f<index>`/`g<index>` exports, imports
and data segments, and it compiles from bytes instead of loading the embedded
native R image. Such runs are diagnostics, never a scored cohort.

Instructions and simulated misses judge a change; only time on a quiet
machine compares with Node. `scripts/rsc_wall_clock_compare.py` interleaves
fresh processes of the host and of Node `--jitless` on one CPU and reports
the median of process medians and the mean of process means:

```sh
python3 -B scripts/limited_verification.py --cloud --cloud-worker-cpus auto -- \
  python3 -B scripts/rsc_wall_clock_compare.py target/rsc-candidate/host \
  target/new-application-fixtures /opt/node24/bin/node 12 2
```

Twelve processes per engine and case settle the medians on a shared virtual
machine; eight do not, and neither does a run beside a build.

On a cloud machine, build with `--cloud --cloud-cpus auto` and run test
binaries with `--cloud --cloud-worker-cpus auto`: the first form exports
four compilation jobs as `SCRIPTW_JOBS`, which also sets the engine's default
pool size, and tests that configure a one-job pool then fail with "Wasm
compilation pool already configured for 4 jobs". The second form exports
`SCRIPTW_JOBS=1`, so a test that compiles before configuring chooses the same
one-job pool. `cargo test -p scriptw-engine` stops at the first failing
binary, so pass `--no-fail-fast`.
Do not edit a source file while a build or a suite is in flight: the runtime
image and the emitter must come from one source state, or a host stops with
"immutable runtime receipt failed admission". A Test262 run writes
git-ignored `test262/snapshots/latest-*` and per-case `*-case-*` files; delete
a stale `.txt` summary together with its `.json`, because a summary left
alone makes the next run of that subtree refuse.

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
capabilities, not implied by this surface. The README records the current
Runtime ABI. Current-source sync and async React RSC fixture output passes exact Flight checks;
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

Compiler-bound caches are keyed by the build source and the running
executable's linker build ID, read from the loaded image without hashing the
file ([contract](docs/rust-rewrite/contracts/compiler-cache-image-identity.md)).
An executable whose bytes were edited after linking, or one linked with a
constant `--build-id`, must run with `SCRIPTW_COMPILER_CACHE_IMAGE_HASH=blake3`
(or `sha256`), which hashes the whole file. Evidence (`scriptw
compiler-identity`, Test262 and benchmark reports) hashes it, except benchmark
host reports run with `SCRIPTW_BENCHMARK_REPORT_IDENTITY=build-source`, which
carry a null `executable_sha256` and leave the executable check to the driver.

Whole native modules compiled at run time are stored uncompressed in
`native-modules-v1` under the cache root and mapped by later processes
([contract](docs/rust-rewrite/contracts/runtime-byte-ownership.md#native-module-cache)).
Never truncate or rewrite a cache entry in place: a running process may be
executing from it. Remove entries with `scriptw cache prune` or by unlinking.
No tier is listed when a process opens or reads it; a process that inserts
lists the tier once and prunes it if it is over its limit. `scriptw cache
status` reports the build-only Wasmtime module cache separately, as
`build-module-cache`.

Give an experimental source copy its own `CARGO_TARGET_DIR`, or keep the copy
outside `target/`. Cargo identifies workspace packages by their path relative
to the workspace root and records sources under the target directory relative
to it. A copy under `target/` built into the shared directory therefore
replaces the canonical units, and the canonical build then reports them fresh
and links the copy's code even after canonical sources change. If that
happened, remove every `target/<profile>/.fingerprint/` directory whose
`dep-*` files name the copy, then rebuild.

For footprint comparisons, record the native backing mode from the load trace
(`SCRIPTW_WASM_TRACE=1`: `backing: executable-mapping` or `backing: copied`)
alongside image digests. On Linux the Cargo runtime image is mapped from the
running executable, whatever `SCRIPTW_CACHE_DIR` is; without procfs it is
copied into anonymous memory, private to each process. A cached program
module is mapped from its file in `SCRIPTW_CACHE_DIR`, or read into private
memory when that directory is mounted `noexec`. An executable run from
tmpfs keeps its pages in memory either way. Compare PSS or USS only across
matching backing modes and executable filesystems.

Keep evidence in durable workspace/archive storage; `/dev/shm` is scratch. Preserve
outputs by digest before moving them, inspect symlink/hardlink destinations before
a rebuild, and account for actual filesystem free space and cgroup file pages.
Metadata-only cached-extern checks are diagnostics, not product builds or native
acceptance. Never replace a failed original receipt with a passing diagnostic retry.
