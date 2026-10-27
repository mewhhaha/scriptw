# Compiler-disabled Scriptw and Scriptc comparison

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
presented as an uninstrumented latency measurement. Initialization performed by
the entry API remains inside execution; registration and instance setup remain
outside it. No cost is subtracted from either result.

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
Neither a Scriptw–Scriptc RSS difference nor an ordinary-minus-precompiled
subtraction isolates compiler or JavaScript heap memory. The same executable
can fault different code and data pages, while native optimization policy,
runtime bindings, collector/allocator retention and warmup history also affect
the process footprint.

The workflow subsequently runs a separate, unscored Linux mapping diagnostic for
`integer.js` and `retained-objects.js`, serially for both adapters. It reuses only
that job's original frozen consumer and freshly produced native images after
validating the completed scored report. It never compiles sources or executes
native images from downloaded evidence. The diagnostic command is for a trusted
live campaign owned by the caller; checksums alone do not grant native authority.
It checks original producer/consumer paths, executable/image identities, source
commit/fingerprint, compiler-disabled configuration, ten warmups, seven samples,
and every output against the existing comparison reader.

`precompiled-mapping-diagnostic-unscored` retains raw stdout/stderr, bounded
`smaps` snapshots and a separate diagnostic report; it contains no executable or
serialized native image. Samples require the exact Rust executable path and
inode with an executable mapping, excluding the Python fork-before-exec window.
Optional executable page-presence bitmaps retain only present bits, never PFNs.
Raw mappings include offsets, permissions, device/inode and RSS counters so code,
read-only data, private native backing and anonymous mappings can be inspected
without assigning their cause from pathname or reserved size alone. Consumer
stdout contains the validated lifecycle endpoints. Parent-polled sampled maxima
remain asynchronous observations and cannot be labelled Store-drop endpoints.
These observations never enter scored graphs or replace scored timings/RSS.

The diagnostic inherits the same one-CPU, 4-GiB, no-swap grouped-OOM scope. Each
consumer has a 120-second deadline, 128-snapshot ceiling, 16-MiB raw-smaps budget
(64 MiB across all four), and a combined 2-MiB retained stdout/stderr prefix. The parent drains both
nonblocking pipes through EOF before validating JSON. Overflow or deadline
terminates only its owned process group and records a capture failure; child
private-file writes have no diagnostic file-size limit. Snapshots are polled at
20 ms while the bounds permit; short phases and processes may be unobserved.
The diagnostic records actual CPU affinity and before/after kernel memory
budget counters, and checkpoints its initial status and each active/completed
case before long captures. Missing proc capabilities, incomplete captures, exhausted sampling limits and
failed commands are explicit in the separate report. The workflow retains this
artifact even on failure and reports availability separately; unavailable
diagnostics do not prevent retention or publication of valid scored evidence.
`SCRIPTW_WASM_TRACE` is enabled only in these unscored consumers so their stderr
can record the actual packed-input source kind without addresses or native bytes.
