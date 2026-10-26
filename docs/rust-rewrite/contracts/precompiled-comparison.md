# Compiler-free Scriptw and Scriptc comparison

This cohort measures the complete 17-case execution corpus on one GitHub runner,
using the same host executable for both compilers. Separate producer processes
compile JavaScript to Wasm and Wasm to native images. Every measured consumer
starts in a fresh process with Wasmtime's compiler disabled. The consumer does
not invoke a JavaScript compiler or fall back to compiling a missing image.

Both application modules use Wasmtime 47.0.0, size optimization, backtracking
register allocation, the explicit generic Cargo target and disabled Wasm
backtraces. These settings also match Scriptw's prebuilt Cargo runtime. Both
adapters use the same required Wasmtime features, copying-GC configuration,
stack limits and linear-memory limits. Scriptc's use of linear memory and
Scriptw's use of Wasm GC remain properties of their generated runtimes.

Run the read-only measurement workflow after the benchmark source is committed:

```sh
gh workflow run precompiled-benchmarks.yaml --ref main
```

The workflow pins Scriptc and its WASI runtime pack to 0.4.5 and Zig to 0.16.0.
Scriptc receives the original input with only the existing print adapter.
Sources, expected outputs and all 17 workloads are unchanged. Builds and
measurements use one CPU, a 4-GiB process-tree cap, no swap and grouped OOM.
The workflow uploads raw evidence and does not write to the repository.

Each of two rounds has ten checked warmups followed by seven measured
invocations. Every invocation uses a fresh Store/Realm or Store/WASI instance.
The second round reverses workload order; the first runtime alternates.
Post-execution RSS, PSS and private-dirty endpoints are captured while the Store
is alive. Before/after loading and Store-drop endpoints are also retained.
Native loading has a separate 10-ms sampled RSS maximum, which may miss shorter
transients. These measurements are process footprints, not live JS heap bytes.

Execution time measures each runtime's public entry API and output capture.
Scriptw includes its internal worker and outcome handling; WASI calls `_start`.
Image loading, Store/instance setup and post-timer checksum validation remain
separate. The total observation time includes memory probes and must not be
presented as an uninstrumented latency measurement. Runtime startup remains
inside execution; no cost is subtracted from either result.

The native-image transport buffers are dropped before the first Store. Both
native modules are deserialized from bytes into the same configured engine.
Scriptw additionally admits its matching Cargo runtime and host bindings;
Scriptc uses WASI bindings. Those required loading and host costs stay visible.
The shared executable still contains producer code; compiler-disabled execution
does not claim a separately linked minimal runtime binary.

Failures retain their commands, exit status and raw streams. A failure in
either round removes that runtime's bars for the workload. Missing cases,
changed checksums, enabled compilers, policy mismatches and unbound image or
executable identities cannot silently become successful results.

After downloading the `precompiled-measurements` artifact, validate and publish:

```sh
python3 -B scripts/precompiled_comparison.py validate PATH/report.json
python3 -B scripts/precompiled_comparison.py publish PATH/report.json
python3 -B scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json
```

An existing publication requires explicit `--replace`. The bounded archive
retains original sources, raw Wasm, producer and consumer streams, native-image
hashes/framing, tool versions and the actual Cargo producer transcript. It
excludes executables, serialized native images and caches. Native checksums
detect corruption and bind evidence; they do not authorize executing external
machine code. The Rust loader retains its explicit trusted-producer contract.

The ordinary Scriptw/Scriptc/V8 comparison remains a separate cohort. Results
from different machines or execution boundaries are not combined. These are
descriptive measurements of this corpus, not a conformance or significance claim.
