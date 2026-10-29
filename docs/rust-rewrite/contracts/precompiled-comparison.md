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

The workflow pins Rust to 1.99.1, Scriptc and its WASI runtime pack to 0.4.5,
and Zig to 0.16.0. The scoped build process preserves the explicit Rust toolchain;
the workspace's default toolchain is unchanged. Historical runs before this pin
used the then-current stable Rust. Kernel and runner differences remain recorded,
so the pin alone does not make cross-runner changes causal.
Scriptc receives the original input with only the existing print adapter.
Sources, expected outputs and all 17 workloads are unchanged. Builds and
measurements use one CPU, a 4-GiB process-tree cap, no swap and grouped OOM.
The workflow uploads raw evidence and does not write to the repository.

Cargo producer admission accepts exactly the default feature array `[]` or
`["benchmark-module-images"]`. The latter enables the shared host's excluded
phase-diagnostic address hook; ordinary captures do not invoke it. Unknown,
additional or duplicate features are rejected, and the admitted producer must
match an actual artifact record in the retained complete release Cargo transcript.

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

The published grid has one image per benchmark, with separate zero-based
execution-time and process-RSS panels. It uses the original measurements.

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
serialized native image. Its v2 report uses explicit phase acknowledgements,
rather than assigning phases to asynchronous sampled maxima.

Both adapters pause after the existing RSS snapshots at `after_transport_drop`,
`after_instantiate`, `after_execution` and `after_store_drop`. The last three
checkpoints select observation 16, the final observation after ten warmups and
seven samples. Scriptw setup precedes Realm initialization and guest execution;
those remain inside evaluate. Existing RSS snapshots and entry-API intervals stay
in place. Earlier setup waits still enter diagnostic total wall times, which the
raw report labels explicitly; no wait cost is subtracted. Pauses alter timing and
process history: the complete
invocation is unscored, including its RSS counters. Its phase maps never replace
canonical timings, RSS values or graphs.

The parent passes one dedicated Unix socket descriptor, a fresh 32-digit hex
nonce and the selected index through `SCRIPTW_PRECOMPILED_PHASE_FD`,
`SCRIPTW_PRECOMPILED_PHASE_NONCE` and `SCRIPTW_PRECOMPILED_PHASE_INDEX`. The helper
requires all three variables or none, admits an inherited Linux socket, and
transfers that descriptor's ownership only for the diagnostic invocation. A
bounded `SWPHASE1 nonce sequence phase index` event waits for its exact
`SWACK1` acknowledgement. Sequences 1–4 and the closed phase order cannot repeat
or change. Each handshake has a 30-second absolute deadline; missing or malformed
configuration, acknowledgements and EOF fail clearly.

Before each capture the parent checks the exact child executable path and inode,
then records its bounded smaps and optional executable page-presence bits. This
excludes the Python fork-before-exec window. Only present bits survive, never
PFNs. The parent acknowledges after capture, while independently draining stdout
and stderr. Phase attribution is admitted only after all four events and the
final normal completion, source/configuration, native-image and output checksums
validate. Raw mappings retain offsets, permissions, device/inode and RSS counters;
phase alignment does not by itself establish which allocation caused a mapping.

The diagnostic inherits the one-CPU, 4-GiB, no-swap grouped-OOM scope. Each
consumer has a 120-second deadline and four phase captures, with at most 2 MiB
per smaps read, 64 MiB of raw smaps across the campaign, and a combined 2-MiB
stdout/stderr prefix. Overflow or deadline terminates only the owned process
group and retains incomplete evidence; private native-backing writes have no
diagnostic file-size limit. CPU affinity and kernel memory counters are recorded.
Missing proc capabilities, invalid phase order, incomplete captures and failed
commands are explicit in the separate report, which the workflow retains even
on failure. Diagnostics never prevent retention of valid scored evidence.

Ordinary measurement processes strip all three barrier variables. The canonical
comparison reader also rejects any `phase_barriers` marker or diagnostic timing
label, even if its explicit unscored flag was removed. Diagnostic validation keeps original raw stdout and
uses a separately checked copy solely to validate identities and checksums.
`SCRIPTW_WASM_TRACE` remains confined to unscored consumers, recording the packed
input source kind without addresses or native bytes.

With the nondefault `benchmark-module-images` feature, excluded phase diagnostics
also join the actual retained P/R Module image/text ranges with bounded
present-bit-only evidence for readable VMAs. The lazy observation occurs only
after ACK4; host ELF symbol metadata is inspected after child exit. See the
[ownership contract](benchmark-module-images.md). This feature does not alter
scored values or grant native-image admission authority, and shared symbol pages
are nonexclusive overlaps rather than function RSS or execution evidence.

## Consumer capability experiment

Build `scriptw-precompiled-consumer` as a separate selected package, with
`--no-default-features`. Its default engine dependency excludes the
`native-source-compiler` capability. Native compiler callbacks, their Store
context/cache and source-compiling agent starts are absent; the Portable
embedding SDK retains its existing host, foreign-value and async implementations.
Unsupported source imports fail clearly. Static compiler/Cranelift dependencies
remain, so this is an intermediate runtime separation experiment.

The consumer has only identity and trusted-image execution commands. Its actual
engine capability must match the requested consumer feature, or startup rejects
Cargo workspace feature unification. Build the same package separately with
`--features native-source-compiler` for a capability-enabled control. Both builds
use identical producer images, target and runtime settings. Compile-time
source exclusion is distinct from the Wasmtime compiler-disabled runtime policy.

`scripts/benchmark-consumer-capability.py --help` describes the two-host diagnostic.
It retains balanced fresh-process observations, exact image/source identities and
all failures, and compares RSS, PSS, private-dirty memory and execution time.
Its reports are excluded from canonical scores. Fewer linked entrypoints or
allocations alone do not establish an RSS reduction.

The [fresh local checkpoint](../binary-scalar-20261029.json) at `d0a134cfd`
records 136 successful execution processes and 2,312 exact outputs across the
original 17 workloads, two variants, two adapters and two balanced rounds.
Both consumers include the optional `benchmark-module-images` feature; its
observer is absent during measurement. Scriptc 0.4.5 raw Wasm is preserved from
the published cohort; its frontend was not rerun.

Excluding the capability reduced Scriptw's geometric mean RSS by 8.0%, PSS by
9.2% and private dirty memory by 12.6%, with essentially unchanged execution time.
The WASI control moved 1.3% in RSS and 1.5% in PSS. Arithmetic means of per-case
RSS medians were 17.91 to 16.49 MiB for Scriptw and 14.55 to 14.37 MiB for WASI.
These endpoints include runtime, code and allocation history; they do not identify
individual retained owners or establish live-heap savings. The diagnostic remains
separate from canonical benchmark scores. The checkpoint's inert archive contains
producer receipts, original inputs, observations and failed attempts as data,
excluding executables and serialized native images. The
[earlier result](../memory-consumer-capability-20261028.json) retains its own source
and rejected trials.
