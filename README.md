# scriptw

Scriptw is an experimental JavaScript-to-WebAssembly compiler written in Rust.
The goal is ECMAScript correctness with competitive compilation and execution.
Programs and builtins compile to real Wasm; WebAssembly is the sole output target.

## Try it

Requires Rust 1.94 or newer and the repository's experimental Wasmtime 47 backend.

```sh
cargo build --locked --release -p scriptw-cli --bin scriptw
printf 'print("Hello Wasm");\n' > hello.js
./target/release/scriptw run hello.js
SCRIPTW_WASM_DUMP=hello.wasm ./target/release/scriptw build wasm hello.js
```

Rust applications can [embed Scriptw](docs/rust-rewrite/contracts/foreign-modules.md)
in their own Wasmtime Engine, Store and Linker, with synchronous and async host functions.

## Benchmarks

Each panel names its date and build; they are measured separately and are not
requalified on every commit. [Benchmark details](docs/rust-rewrite/benchmarks.md)
hold the full tables, methods and limits.

### React Server Components

**React 19.3.1 RSC · 2026-11-16**, source `067e69797abb`; exact Flight output on every call.

| Retained request | Scriptw Fast | Scriptw Speed | Node 24 / V8 |
| --- | ---: | ---: | ---: |
| Sync | 0.776 ms | 0.271 ms | 0.164 ms |
| Async | 0.818 ms | 0.341 ms | 0.177 ms |

![Retained RSC requests](docs/rust-rewrite/assets/rsc-current/runtime.svg)

Medians of eight fresh processes per engine and case, 100 warmups and 101 timed
requests each, on one CPU of a shared cloud machine. Fast and Speed are
Scriptw's two native compilation policies; Node runs with its JIT.
Speed takes 1.7× Node's time sync and 1.9× async; the 2026-11-09 panel, on
another machine, had 13.9×.
With V8's JIT off (`node --jitless`), three separate runs put Speed 7 to 13%
ahead sync and between 8% ahead and 1% behind async.
[Means, tails, warmup curves and limits](docs/rust-rewrite/benchmarks.md#react-server-components) ·
[data](docs/rust-rewrite/rsc-current.json).

### Runtime kernels

<!-- runtime-comparison:start -->
**Runtime kernels · 2026-11-02**, compiler `87385fff34d1`; Node **24.12.0**, V8 **13.6.233.17-node.37**.
Scriptw is faster than Scriptc in **17/17 workloads** in both blocks; block execution geometric means are **0.455–0.458× Scriptc**.
Scriptw is faster than V8 in **6/17 workloads** in both blocks; block execution geometric means are **1.098–1.106× V8**.
Wasm is compiled before timing; Scriptw entry includes Realm bootstrap. V8 creates a fresh context before timing; entry checks outputs.
See [measurement details and charts](docs/rust-rewrite/benchmarks.md#runtime-comparison), [data and identities](docs/rust-rewrite/execution-current.json), and [workflow](CONTRIBUTING.md#benchmark-charts).
<!-- runtime-comparison:end -->

<img src="docs/rust-rewrite/assets/benchmarks-current/retained-objects.svg" width="440" alt="retained-objects.js: Scriptw, Scriptc and V8 time and memory">

`retained-objects.js` is the middle of the 17 workloads by Scriptw's ratio to V8 (1.74×); the range is 0.03× to 4.25×.

### Compilation

**JavaScript → portable Wasm · 2026-11-02:** Scriptw is faster than Scriptc in
**17/17 workloads**, taking **0.524× Scriptc time** by geometric mean.
Includes CLI startup and output I/O; native compilation and execution are separate.
[Data](docs/rust-rewrite/compilation-current.json).

![JavaScript to portable Wasm compilation](docs/rust-rewrite/assets/compilation-current.svg)

### Edge kernels and focused situations

<!-- application-benchmarks:start -->
**Edge kernels · 2026-11-02**, compiler `87385fff34d1`:
Scriptw takes **3.8–9.3× Scriptc time** across 5 completed comparisons.
These use plain request/response records; networking and Web APIs are outside the workload.
[Application charts and failed attempts](docs/rust-rewrite/benchmarks.md#application-workloads) ·
[Data](docs/rust-rewrite/applications-current.json).
<!-- application-benchmarks:end -->

<img src="docs/rust-rewrite/assets/applications-current/edge-query.svg" width="440" alt="edge-query.js: Scriptw and Scriptc time and memory">

**Focused performance situations · 2026-11-08**, source `453a9074fcac`: 21 standalone programs compare Fast, Speed and V8.
[Scores and workload bounds](docs/rust-rewrite/benchmarks.md#focused-performance-situations) · [Situation data](docs/rust-rewrite/situations-current.json).

![Global lookup situations](docs/rust-rewrite/assets/situations-current/global-lookup.svg)

The kernel, compilation, edge and situation panels predate the work that
produced the RSC figures above; they have not been remeasured since.

## Current Status
<!-- scriptw-status:start -->
Published Test262 (`wasm-aot`, `2026-10-23`): `102990/102994` passed; `4` failed.
Native-host conformance includes cases unsupported by standalone Wasm; it does not establish current HEAD coverage.
Pinned revisions: `ecma262=5345883164f463e87f8b40aca4956157ecba8783` `test262=6a4268a9354a545d41c3b62efebc478ed8c521f4`
Compiler build fingerprint: `7337ccbc2834d0d17902382431789227b6dddf8fda40842bf19c4243161d56f9`; executing image SHA-256: `32ee09a9a33daaf7d27fcb1b649db7223fcbd3ce00f0943cf19cce5ef886101b`
Published status artifacts: `/workspace/scriptw-full-suite-20261022/test262/snapshots/scriptw-hot-reads-b9ecfe64b-20261022-aggregate-13327628081868727304.json` and `/workspace/scriptw-full-suite-20261022/test262/snapshots/scriptw-hot-reads-b9ecfe64b-20261022-aggregate-13327628081868727304.txt`
[Full test details](docs/rust-rewrite/test262-current.md) · [Refresh workflow](CONTRIBUTING.md#test262).
<!-- scriptw-status:end -->

<!-- scriptw-test-graph:start -->
![Test262: 102,561 passing in scope, 4 failing, 0 not implemented, 429 out of standalone Wasm scope](docs/rust-rewrite/assets/test262-status.svg)

**Published full run, 2026-10-23:** 102,994 mode-specific
executions at pin `6a4268a93`, compiler `b9ecfe64b`.
The **429 out-of-scope executions** passed through the native host;
standalone Wasm excludes their dynamic source compilation. All four categories
retain the full denominator, including every failure.
[Scope inventory](docs/rust-rewrite/test262-chart-full-20261022.json) · [Published result](test262/snapshots/published-status-wasm-aot-857165c90f0fb4ac3ec0493a3b4470977007808c5b1c10ecf5512840edc80f46.json) ·
[Scope policy](docs/rust-rewrite/runtime-requirements.md#out-of-scope-tests).
<!-- scriptw-test-graph:end -->

Latest engine suite (**2026-11-16**, source `3351200c5`): 4,265 tests and the doctest pass; one runtime cache test fails on the cloud machine only. Full Test262 has not been rerun since the published run above; [current work](docs/rust-rewrite/current-inventory.md) lists what is open.

## Caveats

- **Experimental backend:** Wasm GC, typed function references, exception handling
  and related features are required. Browser/other-engine integration is unverified.
  A bare WASI host does not provide Scriptw's runtime or host imports.
- **Incomplete conformance:** failures remain counted. The published run belongs
  to its recorded compiler; selected checks do not establish full ECMAScript coverage.
- **Dynamic source compilation:** standalone Wasm has no parser or general runtime
  `eval`/`new Function`. Supported prepared sources and native-host behavior have
  [separate limits](docs/rust-rewrite/runtime-requirements.md).
- **Bounded Web APIs:** UTF-8 codecs, byte streams with default readers, events,
  abort signals and tasks support the tested RSC fixtures. Fetch/Request/URL,
  DOM nodes and full WHATWG conformance remain unsupported or unverified.
- **Deadlines:** compiled code has no deadline checks. An execution timeout
  removes access to the Store's GC heap and private linear memory, and the fault
  stops the code; this needs Unix signal handling and is exercised on Linux
  x86-64 ([contract](docs/rust-rewrite/contracts/execution-deadlines.md)).
- **Runtime ABI 77:** rebuild Portable bundles and native images built under an
  earlier ABI ([details](docs/rust-rewrite/runtime-requirements.md#runtime-abi)).

[Development and verification](CONTRIBUTING.md) ·
[Current work](docs/rust-rewrite/current-inventory.md) ·
[Compiler/runtime contracts](docs/rust-rewrite/contracts/README.md).
