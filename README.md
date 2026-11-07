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
Top-level embedded calls use a guarded stack on the caller’s thread.
Module `for-in` and `for-of` loops work across top-level `await`.

## Benchmarks

Latest published measurements follow. Each cohort identifies its recorded build;
these figures do not requalify every workload on current HEAD.

<!-- runtime-comparison:start -->
**Runtime kernels · 2026-11-02**, compiler `87385fff34d1`; Node **24.12.0**, V8 **13.6.233.17-node.37**.
Scriptw is faster than Scriptc in **17/17 workloads** in both blocks; block execution geometric means are **0.455–0.458× Scriptc**.
Scriptw is faster than V8 in **6/17 workloads** in both blocks; block execution geometric means are **1.098–1.106× V8**.
Wasm is compiled before timing; Scriptw entry includes Realm bootstrap. V8 creates a fresh context before timing; entry checks outputs.
See [measurement details and charts](docs/rust-rewrite/benchmarks.md#runtime-comparison), [data and identities](docs/rust-rewrite/execution-current.json), and [workflow](CONTRIBUTING.md#benchmark-charts).
<!-- runtime-comparison:end -->

**JavaScript → portable Wasm · 2026-11-02:** Scriptw is faster than Scriptc in
**17/17 workloads**, taking **0.524× Scriptc time** by geometric mean.
Includes CLI startup and output I/O; native compilation and execution are separate.
[Data](docs/rust-rewrite/compilation-current.json).

![JavaScript to portable Wasm compilation](docs/rust-rewrite/assets/compilation-current.svg)

<!-- application-benchmarks:start -->
**Edge kernels · 2026-11-02**, compiler `87385fff34d1`:
Scriptw takes **3.8–9.3× Scriptc time** across 5 completed comparisons.
These use plain request/response records; networking and Web APIs are outside the workload.
[Application charts and failed attempts](docs/rust-rewrite/benchmarks.md#application-workloads) ·
[Data](docs/rust-rewrite/applications-current.json).
<!-- application-benchmarks:end -->

**React 19.3.1 RSC · 2026-11-07**, compiler `0ca455c47636`: exact Flight output.

| Retained request | Scriptw Fast | Node 24 / V8 |
| --- | ---: | ---: |
| Sync | 7.15 ms | 0.578 ms |
| Async | 7.78 ms | 0.460 ms |

![Retained RSC requests](docs/rust-rewrite/assets/rsc-current/runtime.svg)

Eight fresh processes per engine; three checked warmups and seven measured
requests per mode. Full Flight serialization and consumption are timed;
compilation, initialization and final validation are outside request timing.
32-row catalog; async adds a resolved Promise, with no I/O.
[Request profiling](docs/rust-rewrite/benchmarks.md#rsc-request-attribution)
finds roughly 19× V8's instructions, with object/property machinery the largest
named CPU bucket. These diagnostics are separate from the timing scores.

**Focused performance situations · 2026-11-07:** 21 checked programs compare
lookup, calls, arguments, properties, type tests, equality and enumeration
against V8. A separate fixed-yield width panel checks 8–256 own keys.
Eight fresh processes per configuration; 100 checked warmups and 21 samples.

[Scores, width panel and caveats](docs/rust-rewrite/benchmarks.md#focused-performance-situations) ·
[RSC data](docs/rust-rewrite/rsc-current.json) · [Situation data](docs/rust-rewrite/situations-current.json).

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

Latest scoped verification (**2026-11-07**, compiler `0ca455c47636`): 1,093 Engine unit
and 1,781 integration cases pass; 70 emission, 637 structural
and 380 source-guard checks pass. AOT units pass 664 cases; four known
bootstrap assertions remain failures. 1,630 IR unit cases pass on this source; 771 IR integration
cases retain original source/binary receipts with an unchanged-input proof.
All 60,984 focused calls, 43,560 width-control calls
and 480 exact RSC checks pass.
These scopes do not replace the published full Test262 run. Full workspace CI
was not rerun; eleven formatting failures and fourteen stale Intl recipes
remain [owned](docs/rust-rewrite/current-inventory.md).

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
- **Runtime ABI 52:** private for-in cursors now retain hashed Set storage.
  Rebuild older Portable bundles and native images.

[Development and verification](CONTRIBUTING.md) ·
[Current work](docs/rust-rewrite/current-inventory.md) ·
[Compiler/runtime contracts](docs/rust-rewrite/contracts/README.md).
