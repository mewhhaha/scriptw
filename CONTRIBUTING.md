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

## Development workflow

1. Start from a reproducible failure or missing invariant and choose one coherent
   batch. Give shared files one owner; implement independent chunks concurrently
   when that helps. Prefer existing domain boundaries over speculative abstractions.
2. Finish the code, appropriate regressions, types and documentation before costly
   verification. Use inspection and cheap checks while editing; compile early only
   to resolve a blocking uncertainty or validate a risky foundation.
3. Compile once, run focused and adjacent checks, then the required broader
   checkpoint in sequence so Cargo reuses artifacts. Investigate failures and rerun
   affected checks. Report commands, actual inventories, results and unverified work.

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

They compile source-only Rust harnesses. Typed IR/Wasm modules remain in the full
Cargo structural targets; guards do not replace native or semantic acceptance.
Then run a focused target and the batch's broader checks under the same wrapper:

```sh
python3 -B scripts/limited_verification.py -- cargo check --locked --workspace --all-targets
python3 -B scripts/limited_verification.py -- cargo test --locked -p scriptw-aot-wasm --test structure_language -- --test-threads=1
python3 -B scripts/limited_verification.py -- ./scripts/check-no-interpreter-in-product-graph.sh
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
Fake suites and selected replays are separate evidence. `Unsupported`, timeout,
crash and bug outcomes are non-passing; retain all four documented upstream
disagreements as ordinary failures. A new complete publication is required to
replace the [published full-suite result](docs/rust-rewrite/test262-current.md).

## Artifact reuse

Reuse requires exact source, features, dependency graph, runtime/program identities
and resource policy; filenames alone prove nothing. Keep fresh evidence directories
for new cohorts and preserve failed receipts. Shared immutable runtime images and
bounded caches reduce rebuilds without sharing mutable JavaScript state.

Keep evidence in durable workspace/archive storage; `/dev/shm` is scratch. Preserve
outputs by digest before moving them, inspect symlink/hardlink destinations before
a rebuild, and account for actual filesystem free space and cgroup file pages.
Metadata-only cached-extern checks are diagnostics, not product builds or native
acceptance. Never replace a failed original receipt with a passing diagnostic retry.
