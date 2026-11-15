# Continue here

Handoff for the React Server Components (RSC) performance work, last updated
2026-11-15.
Delete this file when the work below has landed; the lasting record is
[benchmark details](docs/rust-rewrite/benchmarks.md) and
[current work](docs/rust-rewrite/current-inventory.md).

## Goal

Make the retained RSC request run at least as fast as V8 without its JIT
(Node 24 `--jitless`), for the sync and async cases, in medians and in means.

The rules for getting there:

- ECMAScript-exact semantics and byte-exact Flight output.
- No special cases for this benchmark.
- No heap sizing or collection-trigger changes made to dodge collections. The
  initial heap budget stays 16 MiB.
- Structural changes are allowed, including a rehaul of a subsystem.
- Verified work is fast-forwarded to `origin/main` as it lands.

## Where it stands

`main` holds the first handoff's work. The arguments work below is on the
branch `claude/festive-johnson-n50tm4`, verified and waiting for the owner to
fast-forward `main`. Two kinds of count appear; do not mix them.

Hardware user-mode counts per request, Speed policy, taken on the first
machine before the arguments work (Scriptw over 900 requests for
instructions and 100 for cycles):

| | Scriptw sync | Node sync | Scriptw async | Node async |
| --- | ---: | ---: | ---: | ---: |
| Instructions, median | 5.05M | 4.37M | 5.53M | 4.62M |
| Instructions, mean | 5.17M | 4.41M | 5.66M | 4.64M |
| Cycles, median | 2.42M | 2.59M | 2.80M | 2.79M |
| Cycles, mean | 2.49M | 2.61M | 2.91M | 2.81M |

Callgrind instruction references per request, Speed policy, on a cloud
machine without hardware counters (97 requests; about 0.8% above the
hardware counts on the same source):

| | Sync median | Sync mean | Async median | Async mean |
| --- | ---: | ---: | ---: | ---: |
| `b6f536be0`, before the arguments work | 5.095M | 5.195M | 5.588M | 5.690M |
| Arguments reads without the object | 4.911M | 4.995M | 5.390M | 5.498M |

- The arguments work removes 3.6% of sync and 3.5% of async. Sync is then
  about 1.11 times Node in instructions, if the 0.8% offset holds.
- No cycles and no Node control were measured after it: the cloud machine has
  no counters and Node 24 could not be installed there. Re-measure both
  engines together on a quiet machine with counters before claiming the
  comparison. The work started at 20.12M instructions per sync request.

## Done since the first handoff

Emitter round 6, step 1 (`arguments` object). The closed walk in
`arguments_protocol/strict_projection.rs` refused every method call, because
`recv.m(a)` lowers to a `MaterializeBinding` temporary. It now admits one
whose receiver is not the `arguments` binding or a read of it, which admits
`createElement`, `cloneElement`, the error formatter and `Ct` (which reads
its optional fourth and fifth arguments). The probe test is gone.

Verified on that tree: the engine suite (4,210 passed, and the one
`runtime_cache` failure below), the engine doctest, the emitter's
arguments-protocol unit tests, the source guards, Test262
`language/arguments-object` (460 of 460) and `language/function-code` (279 of
281, both `S10.2.1_A4_T2.js`). Not run: the emitter's whole `--lib` suite and
any other Test262 subtree.

## What to do next, by measured size

[The attribution section](docs/rust-rewrite/benchmarks.md#arguments-reads-and-instruction-attribution-2026-11-15)
has the per-body table and the listing findings. Sync request, 4.997M mean:

1. **`instanceof` against a global constructor, about 260K.** `vn` runs
   nineteen tests for each of 35 plain objects at about 390 instructions a
   test: about 150 for the global read of the constructor, about 130 to
   re-prove the site record and walk the chain. A global read that proves its
   entry with fewer dependent loads helps every global read, not only these.
2. **Stack traffic in large bodies.** A third of `vn`'s plain-object path
   reloads or spills, 662 of 7,480 instructions for the GC heap base alone
   (`vn` is 587 KB of native code and keeps the base in a stack slot). This
   is the structural lever the first handoff listed; it now has a number.
3. **Read records, 438K inclusive in `cached_property_read`** over 1,171
   entries, 568 of them from `vn` and 260 from `createElement` (`config.key`
   and `type.defaultProps`, which are absent or on another shape each time).
   Records cannot hold a proven absence or a second receiver shape.
4. **By-name identifier helpers, 130K.** `typeof` of a missing global in `vn`
   (70 calls, 48K) and free identifiers in the fixture's arrow functions
   (86K), which are not module-local owners.
5. **`createElement`'s activation record, about 30K.** An admitted function
   still allocates its environment for the `arguments` cell (about 480
   instructions before its first call, 68 calls). A strict function cannot
   assign or redeclare `arguments`, so when no nested function names it the
   binding could be a module-local like any unobserved one
   (`analysis/module_locals.rs` keeps it a cell whenever the body names it).
6. `computed_property_write` (225K inclusive, 241 entries from `ft`),
   `coercive_add` (207K, 420 entries), then the first handoff's remaining
   steps: write records for non-ordinary receivers, dense
   `Array.prototype.splice`, and eliding temporal-dead-zone checks.

Not started, from the first handoff: growth of the weak-key table and
`FuncRefTable::intern` on the host side of async, the duplicate `index <
length` compare, function environments at six objects each, and the builtin
call site (895 instructions against Node's 392).

## How to measure

Build the fixtures and the retained host:

```sh
pnpm --dir target/application-tools add --save-exact --ignore-scripts \
  react@19.3.1 react-dom@19.3.1 react-server-dom-webpack@19.3.1 esbuild@0.28.2 playwright@1.64.1
mkdir -p target/rsc-fixtures
node crates/scriptw-cli/tests/fixtures/edge_rendering/react-driver.cjs build \
  "$PWD/target/application-tools" "$PWD/crates/scriptw-cli/tests/fixtures/edge_rendering" "$PWD/target/rsc-fixtures"
python3 -B scripts/limited_verification.py --cloud --cloud-cpus auto -- \
  cargo build -j 4 --release --locked -p scriptw-cli --example rsc_baseline
```

- With hardware counters, use the attribution drivers described in
  [CONTRIBUTING.md](CONTRIBUTING.md). The first handoff's helper scripts
  lived under the git-ignored `target/` of the first machine and are gone.
- Without them, `scripts/rsc_instruction_counts.py` gives the Callgrind
  counts above, a per-body table, callers of a helper and the executed
  native instructions of one body. One count takes about two minutes, of
  which the host's setup under Valgrind is 70 seconds. Pin each run with
  `taskset -c N`.
- Copy the host out of `target/release/examples` before the next build
  replaces it. For a baseline, restore the old source in place, build, copy
  the host, restore the new source and build again: a second build tree does
  not fit the cloud disk allowance beside the engine test binaries.
- Instructions judge a change. Only cycles on a quiet machine compare with
  Node.

## Running on the cloud machine

- The owner allows all four CPUs there as long as the machine does not run
  out of memory (13.4 GiB cap). Pass `-j 4` to Cargo and `--test-threads=3`
  to test binaries explicitly; the wrapper's own defaults stay serial.
- Build with `--cloud --cloud-cpus auto`. Run test binaries with `--cloud
  --cloud-worker-cpus auto`: the first form exports four compilation jobs,
  and tests that configure a one-job pool then fail with "Wasm compilation
  pool already configured for 4 jobs".
- `cargo test -p scriptw-engine` stops at the first failing binary. Pass
  `--no-fail-fast`, or name the remaining `--test` targets.
- The engine suite takes about 25 minutes that way, a release build after an
  emitter edit 3 to 5, and one Test262 case about 8 seconds on one thread.
- A Test262 run writes `test262/snapshots/latest-*` files. Delete them
  instead of committing them.
- Do not edit a source file while a build or a suite is in flight: the
  runtime image and the emitter must come from one source state, or the host
  stops with "immutable runtime receipt failed admission".

## Failures that predate this work

- Four emitter `--lib` tests: `bootstrap_compact_metadata`,
  `error_demand_initialization`, `intrinsic_property_layout_allocation`,
  `ordered_bootstrap_allocation`.
- Five vendored `pagemap::tests::*`.
- Test262 `language/function-code/S10.2.1_A4_T2.js` crashes (listed in current
  work).
- In strict code, a write to a top-level `var` whose global property was deleted
  re-creates the property instead of throwing (listed in current work).
- On the cloud machine only, with and without this work:
  `runtime_cache::embedded_runtime_executes_fresh_programs_without_a_native_runtime_compile`
  (its trace has no "runtime-native-bundle hit" line there) and
  `scripts/intl_source_identities.py` ("non-reproducible
  crates/scriptw-intl/data/segmenter-icu-2/kernel-source-manifest.json").
  `cargo fmt --all -- --check` also reports differences in files this work
  does not touch.

## Decisions left to the owner

- Whether Speed or Fast is the default policy. Every number here is Speed.
- The CPU baseline. It is x86-64 without SSE4.1 today.
