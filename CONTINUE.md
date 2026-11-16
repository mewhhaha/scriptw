# Continue here

Handoff for the React Server Components (RSC) performance work, last updated
2026-11-16.
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

Everything below is on `main` (`ec6b48c62`). Sync is level with Node in wall
clock on the cloud machine; async is 7% to 8% behind. Two kinds of count
appear; do not mix them.

Hardware user-mode counts per request, Speed policy, taken on the first
machine before the arguments work (Scriptw over 900 requests for
instructions and 100 for cycles):

| | Scriptw sync | Node sync | Scriptw async | Node async |
| --- | ---: | ---: | ---: | ---: |
| Instructions, median | 4.87M | 4.37M | 5.34M | 4.62M |
| Instructions, mean | 4.95M | 4.41M | 5.44M | 4.64M |
| Cycles, median | 2.42M to 2.54M | 2.59M | 2.80M | 2.79M |
| Cycles, mean | 2.49M to 2.65M | 2.61M | 2.91M | 2.81M |

Callgrind instruction references per request, Speed policy, on a cloud
machine without hardware counters (97 requests; about 0.8% above the
hardware counts on the same source):

| | Sync median | Sync mean | Async median | Async mean |
| --- | ---: | ---: | ---: | ---: |
| `b6f536be0`, before the arguments work | 5.095M | 5.195M | 5.588M | 5.690M |
| Arguments reads without the object | 4.911M | 4.995M | 5.390M | 5.498M |
| Arrow functions read proven cells | 4.844M | 4.928M | 5.324M | 5.431M |
| Global read, `typeof` and `instanceof` site records (ABI 71) | 4.675M | 4.763M | 5.144M | 5.249M |
| Heap base home slot, chained read records, weak entries by identity, operand tests (ABI 74, main `f85bc4a8c`) | 4.017M | 4.108M | 4.411M | 4.538M |

| JSON members, byte runs and composite receivers (ABI 75, main `bd8e98ca6`) | 3.646M | 3.742M | 4.011M | 4.118M |
| Stores, arguments objects and prototype casts (main `2be3d5542`) | 3.475M | 3.566M | 3.827M | 3.929M |
| Frame reloads, proven references and threaded branches in the translation (main `ec6b48c62`) | 3.073M | 3.164M | 3.382M | 3.484M |

- The work since `b6f536be0` removes 40% of sync and 39% of async. Node's
  whole process runs about 3.82M and 3.94M (Valgrind, differencing 1 and
  201 requests, its out-of-clock output validation included): sync is 20%
  below it in instructions and async 14% below.
- The work started at 20.12M instructions per sync request.

**Wall clock on the cloud machine**, main `ec6b48c62` against Node 24.22.1
`--jitless` (the same fixture through `react-driver.cjs retained-module`),
twelve fresh processes per engine and case, interleaved and pinned to one
CPU, 100 warmups and 101 requests:

| Wall clock per request | Scriptw median | Node median | Scriptw mean | Node mean |
| --- | ---: | ---: | ---: | ---: |
| Sync | 306.5 µs | 307.3 µs | 345.4 µs | 347.5 µs |
| Async | 362.0 µs | 335.9 µs | 405.7 µs | 379.2 µs |

- Sync: 0.997 of Node's median and 0.994 of its mean, which is level, not
  ahead; process medians run from 288 to 373 µs against Node's 298 to 317.
  Async: 1.078 and 1.070. Before the day's work the ratios were 1.49 and
  1.38 sync, 1.29 and 1.38 async. Eight processes each do not settle the
  median on this machine (the same host gave 363 against 309 sync).
- **The gap is no longer instructions.** Node retires about 12.4 Callgrind
  instructions a nanosecond here, Scriptw 10.0 sync and 9.3 async.
  Valgrind's cache simulation gives the sync request 150K instruction
  fetch misses where Node has 8K, with the same data misses (28K): the
  request fetches from about 13,300 distinct 64-byte lines of native code
  (828 KiB), 1,182 of them in `vn`, whose 527 KiB body runs in 434 separate
  pieces
  ([measurement](docs/rust-rewrite/benchmarks.md#wall-clock-against-node-and-code-locality-2026-11-16)).
- The async request runs 310K instructions more than sync; Node's runs
  about 120K more.
- The first handoff's hardware table above (Scriptw ahead in sync cycles at
  4.87M instructions) came from another machine, Node 24.12 and helper
  scripts that are gone. Nothing here reproduces it.
- One request in four or five runs a minor collection; the mean is about
  40 µs above the median for both engines.

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

Proven cells for arrow functions. `analysis/module_locals.rs` granted a
proven binding cell only to ordinary functions; an arrow function with plain
parameters now gets one for its own and its captured bindings. Verified: the
engine suite (4,211 passed and the doctest, same one failure), the IR crate's
unit tests (2,403), the source guards and Test262 `language/module-code` (602
of 602). Script-goal Test262 cases have no proven cells and were not rerun.

Site records for global reads (ABI 71). A read that starts at the Global
Environment Record keeps the global object's dictionary entry record for its
name in a P global and reads the value from it when the entry's descriptor
is a data descriptor; a property keeps one entry record while it lives
(compaction moves it in place) and deletion gives it a dead accessor
descriptor. A `typeof` site also answers a name that is nowhere from a
witness of the global object's dictionary and its one ordinary prototype. An
`instanceof` record decides its facts in one branch, and `V instanceof G`
for a global read G answers without reading G while G's property holds the
value cell the record was made for.
[The record section](docs/rust-rewrite/benchmarks.md#global-read-typeof-and-instanceof-site-records-2026-11-15)
has the reasoning and the steps. `vn` runs 4,577 instructions for a plain
object where it ran 7,480.

Verified on that tree: the engine binaries `aot_language` (560, with six new
Node-checked tests), `aot_builtins` (619) and `aot_realm_modules` (467); the
emitter's integration tests but `intl_temporal` (five binaries pass, and in
`emission` 82 of 83: `strict_arguments_projection::actual_observed_workload_and_five_formals_keep_vector_abi_with_count_guarded_reads`
fails on a body this work does not change); and Test262
`language/expressions/instanceof` (85 of 85), `language/expressions/typeof`
(32 of 32), `language/global-code` (75 of 75) and
`language/identifier-resolution` (22 of 22). Not run: the other engine
binaries, the emitter's `--lib` suite and any other Test262 subtree.

Chained read records, weak collection entries by identity, operand tests
before the generic kernels and the GC heap base home slot (ABIs 72 to 74),
all on main `f85bc4a8c` with main's own finish of the arguments projection
merged in.
[Their section](docs/rust-rewrite/benchmarks.md#read-record-chains-weak-entries-and-operand-tests-2026-11-15)
has what each proves. Verified on the merged tree: the engine suite (4,231
passed and the doctest, the same one `runtime_cache` failure), six emitter
integration binaries (`emission` now 83 of 83) and the source guards. Test262
subtrees ran on each change's own tree, not on the merged one.

JSON members read directly, ASCII run kernels, bulk typed-array copies and
a chain walk for composite receivers in the general Get (ABI 75, main
`bd8e98ca6`):
[their section](docs/rust-rewrite/benchmarks.md#json-byte-runs-and-composite-receivers-2026-11-15).
Verified on the merged tree: the engine suite (4,236 passed and the doctest,
the same one failure).

Stores, arguments objects and prototype casts (ABI unchanged, main
`2be3d5542`), and frame reloads, proven references, threaded branches and
set tests in the vendored translation (main `ec6b48c62`):
[stores](docs/rust-rewrite/benchmarks.md#stores-arguments-objects-and-prototype-casts-2026-11-16)
and
[translation](docs/rust-rewrite/benchmarks.md#frame-reloads-proven-references-and-threaded-branches-2026-11-16).
Verified on the merged tree: the engine suite (4,245 passed and the doctest,
the same one failure), the translator's unit tests (48), Test262
`built-ins/Array` (6,119) and the first 4,240 cases of
`language/statements/class`. An independent read of the translation passes
found one hole (`struct.get` of a `nofunc` field made no null check), fixed
with a probe.

## What to do next

Async needs about 26 µs a request (7%); sync needs a margin. Both come from
the same two places, on main `ec6b48c62`
([numbers](docs/rust-rewrite/benchmarks.md#wall-clock-against-node-and-code-locality-2026-11-16)):

1. **Distinct code per request (150K simulated fetch misses against Node's
   8K).** Measure with `rsc_instruction_counts.py count --cache` and
   `symbols --event I1mr`; a change here can leave the instruction count
   alone and still pay.
   - Slow paths are compiled in line between fast paths. Wasmtime 47 lays
     out blocks that the Wasm branch hinting section marks unlikely at the
     end of the function; the emitter writes no hints and the engine does
     not enable `wasm_branch_hinting`. The emitter knows its helper
     fallbacks, abrupt-completion checks and throw paths.
   - Each site carries its own copy of every expansion. `vn` executes
     10,421 distinct instructions a request; the `!=` chain over the tags
     {0, 13, 14, 17, 58} alone stands at 1,013 sites.
   - The register allocator reloads a spilled value at each use (about
     274K reloads and 217K spills a request) and GC reference spill stores
     are untouched (139K a request, about 25K of them null constants).
2. **Instructions the async request adds (310K against Node's 120K)**: `vn`
   29K, the general Get 27K (`object_read` is 146K own in sync as well),
   read records 22K, `ft` 19K, the ordinary Set 15K, `FuncRefTable::intern`
   7.5K.
3. **Zero fills of arrays that are then overwritten**: 39K a sync request
   and 49K async through the `memory_fill` libcall (text decoder, String
   concatenation, `JSON.stringify`, `trimEnd`, `slice`).
4. **Arguments objects and by-name bindings.** `to({id:n})` still
   materializes its arguments object for its destructured parameter (48K),
   its binding is resolved by name (37K), and `createElement` allocates an
   environment for its `arguments` cell (30K).
5. **`for_in_advance` 44K and `hasOwnProperty.call` 33K**, with the read of
   `.call` before it; a lean front for a compact phase under way saves
   about 10K.
6. **Read records, 252K in the hit entry**, about 240 a prototype-place
   hit; `computed_property_write` 109K, about 245 an append.
7. `switch` on a String makes 206 `string_equality` calls a request (15K)
   for equal-length strings; the nineteen `instanceof` tests in `vn` walk
   one chain nineteen times.

Not started, from the first handoff: function environments at six objects
each and the builtin call site (`proxy_call`, 40K over 160 calls). The
host's own allocator grows about 5 KB a request through a run (the
`mprotect` calls seen under `strace`); what allocates it is not traced.

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
- Instructions and simulated fetch misses (`count --cache`) judge a change.
  Only time on a quiet machine compares with Node:
  `python3 scripts/rsc_wall_clock_compare.py <host> target/rsc-fixtures
  /opt/node24/bin/node 12 2`, with nothing else running. Eight processes
  are too few.

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
- A Test262 run writes `test262/snapshots/latest-*` files. Delete the
  untracked ones instead of committing them, the git-ignored `.txt` summary
  with its `.json`: a summary left alone makes the next run of that subtree
  refuse. `language/statements/class` takes about two hours on two threads.
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
