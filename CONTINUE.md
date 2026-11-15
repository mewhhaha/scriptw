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

`main` holds the first handoff's work. The arguments work and the site
records below are on the branch `claude/festive-johnson-n50tm4`; the site
records (ABI 71) still need the whole engine suite. Two kinds of count
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

- The day's changes remove 21% of sync and of async. Node's whole process
  runs about 3.82M and 3.94M, so 0.2M (5%) is left on sync and 0.47M (12%)
  on async in instructions.

**The comparison with Node does not hold on the cloud machine.** Measured
there on the tree above with Node 24.22.1 `--jitless` (the same fixture
through `react-driver.cjs retained-module`), eight fresh processes per engine
and case, interleaved and pinned to one CPU, 100 warmups and 101 requests:

| Wall clock per request | Scriptw median | Node median | Scriptw mean | Node mean |
| --- | ---: | ---: | ---: | ---: |
| Sync | 457 µs | 306 µs | 482 µs | 350 µs |
| Async | 580 µs | 448 µs | 666 µs | 481 µs |

- Scriptw is 1.49 times Node on the sync median and 1.38 times on the mean;
  1.29 and 1.38 times on async (Node's async processes varied from 326 to
  about 450 µs).
- Node's whole process, counted with Valgrind by differencing 1 and 201
  requests, runs 3.82M instructions per sync request and 3.94M per async
  request, its out-of-clock output validation included. Scriptw's request
  alone was 4.84M and 5.32M when the wall clock above was taken, at least
  1.27 and 1.35 times Node; it is now 4.02M and 4.41M, 1.05 and 1.12 times.
  The wall clock was not taken again. When it was taken Node ran more
  instructions per microsecond than Scriptw on sync (12.5 against 10.6) and
  about the same on async, so sync needs more than instruction parity.
- The first handoff's hardware table (Node at 4.37M instructions and 2.59M
  cycles, Scriptw ahead in sync cycles) came from another machine, Node 24.12
  and helper scripts that are gone. Nothing here reproduces it. Treat the
  goal as open by about a third in instructions and in time.
- A Scriptw request without a collection takes about 375 µs; one in four
  runs a minor collection and takes 110 to 200 µs longer for 0.47M more
  instructions, and times drift up between collections as the nursery's
  cold memory is touched. Allocation volume (480 KB a request) costs more in
  time than its instruction count shows.
- The work started at 20.12M instructions per sync request.

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

## What to do next, by measured size

Sync request on main `f85bc4a8c`, 4.108M mean, own instructions unless
marked ([the day's sections](docs/rust-rewrite/benchmarks.md#read-record-chains-weak-entries-and-operand-tests-2026-11-15)
have the reasoning for what is done):

1. **Built-ins that are native in Node.** `JSON.stringify` (one call, about
   350K inclusive for 242 values: `json_stringify_value` 233K own,
   `object_own_keys` 38K), `TextEncoder.encode` 104K, `TextDecoder.decode`
   71K, `TypedArray.prototype.set` 77K and one `String.prototype.includes`
   52K: per-value and per-byte loops.
2. **Read records, 284K in the hit entry.** A prototype-place hit takes
   about 240 instructions; going lower needs identity witnesses or a typed
   prototype edge. Eighteen reads a request fall to the general Get (71K):
   typed-array accessors, a Set's `size`, stream controller methods.
3. **`vn` 571K, `ft` 435K, `createElement` 182K of their own code.** In
   `vn` the nineteen `instanceof` tests walk one candidate's chain nineteen
   times (about 100K); sharing them needs a proof that nothing between two
   tests changes the chain. The instance pointer is reloaded per use as the
   heap base was (17,710 times a request in `vn`).
4. **Stores.** Array append `c[f] = v` (176 calls at about 490 between
   `computed_property_write` and `array_indexed_publish_default_data`: each
   element is boxed and a two-prototype walk proves no indexed property), a
   named add (131 calls at about 480), `ordinary_property_append` 74K.
5. **`for_in_advance` 54K, `hasOwnProperty.call` 43K** and the read of
   `.call` before it (about 238 an iteration in `ft`).
6. **Allocation and collection.** The mean is 92K above the median: a minor
   collection every fourth request. The three object-allocation helpers make
   461 `is_subtype` libcalls a request (43K).
7. **By-name identifier helpers, about 37K.** `function Row({id})` in the
   fixture and the fixture's request function: a proven binding cell
   (`ProvenBindingCellIr::analyze` in `analysis/module_locals.rs`) is not
   granted to a function with a destructured parameter.
8. **`createElement`'s activation record, about 30K.** An admitted function
   still allocates its environment for the `arguments` cell.

Not started, from the first handoff: `FuncRefTable::intern` on the host side
(144 calls a request), the duplicate `index < length` compare, function
environments at six objects each, and the builtin call site (`proxy_call`,
49K over 160 calls). The host's own allocator grows about 5 KB a request
through a run (the `mprotect` calls seen under `strace`); what allocates it
is not traced.

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
