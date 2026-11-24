# Source, portable startup and existing-Realm call measurements

This diagnostic cohort measures three boundaries with one checked corpus.
It has its own `scriptw-v8-portable-boundaries-v1` receipt; its
ratios must not be merged with ordinary startup, resident-process fresh-Store
invocations, policy experiments or amortized guest batching.

| Boundary | Scriptw | Normal Node/V8 control |
| --- | --- | --- |
| Source-to-result startup | A new child reads JavaScript, prepares/lowers/emits Portable Wasm through the same pipeline/options as `prepare`, then executes the actual product artifact. Source compilation is included. | A new ordinary-flag Node child reads the same source and constructs `vm.Script` without cachedData, then executes in a fresh context. |
| Fresh precompiled-program startup | A new child reads and safely admits a raw Wasm P plus its exact R, then executes through the product Wasm path in a fresh Store/Realm. No JS source path is supplied. | A new ordinary-flag Node child reads the exact retained JS and V8 `cachedData`, constructs `vm.Script`, verifies `cachedDataRejected === false`, then runs in a new context. Cache rejection is an error before execution. |
| Isolated existing-Realm calls | One compiled JS entry initializes once, executes ten checked guest warmups, then calls its compiled `batch(seed)` function seven or more times in the same Store/Realm. | The identical marked source performs the same setup, warmups, calls and checks in one V8 context. |

The primary source/fresh clock starts in the parent immediately before spawning the
actual GNU-time supervisor/runtime child and stops on receipt of a bounded
checked-result marker through a dedicated inherited pipe. A new 32-hex nonce
and exact stdout SHA-256 bind that marker to the child's final JSON. Both
runtimes emit it only after the real result and any call protocol have been
checked, before expensive post-result artifact/cache/source/executable proof
hashing. The parent records actual marker arrival; it subtracts no duration.
Missing, truncated, duplicate, oversized, wrong-nonce or wrong-output markers
fail, as do later child failures or a final proof that does not bind the event.
The process-group deadline bounds stalled or missing markers. The signal is
opt-in; manual example runs without the paired descriptor/nonce retain their
honestly labelled internal and whole-child measurements only.

Separate external whole-child wall/CPU/high-water RSS retains process launch,
all diagnostic receipt/provenance work and teardown. GNU time measures the
actual executable child's RSS, excluding the Python launcher's pre-exec
high-water floor. An internal load-to-result span begins before bundle/source
and cache file reads and includes admission, native compilation/cache lookup,
fresh state initialization and execution. It excludes final receipt hashing.
These are instrumented diagnostic boundaries. Wasm here is precompiled
*from JavaScript*, not native machine code. V8 cachedData is also not a promise
of eager native-code compilation; lazy function compilation can remain.

Source and precompiled-fresh startup share the exact input file/oracle bytes.
Source compilation has raw preparation/lowering/emission/validation spans,
and every measured source artifact must match its retained preparation P/R.
The source mode is explicitly Portable; ordinary CLI NativeHost startup data
is a separate cohort. No cache-hash benefit is predicted from these labels:
actual source, admission and execution observations decide the result.

Each fresh pair has one recorded, unmeasured child warmup and seven measured
fresh children by default. Scriptw retains a private bounded native disk cache
within that receipt; raw native module/cache phases remain available, so this
does not claim cold native compilation or OS page-cache eviction. Since
2026-11-24 a native-module hit maps its cache file
([contract](runtime-byte-ownership.md#native-module-cache)) instead of
decompressing it into private memory, so receipts from before and after that
date differ in module-load time and in which pages are private. Alternating
runtime order reduces a simple order bias without establishing statistical
confidence.

Calls use a dedicated bounded ordered marker protocol. A measured interval is
the recorded end of the start-marker observer callback to the entry of the
matching end-marker observer callback. It includes marker return, seed
calculation, the actual compiled JS call and end-marker argument/print work
before that observer. Setup, guest warmups and result printing are outside the
span. Capture of the end marker before observer dispatch remains included.
Host callback implementation and clock overhead are visible limitations; no
empty-program subtraction is performed. Raw callback entry/exit timestamps
must reproduce every reported duration exactly. Missing, repeated, excess or
out-of-order markers fail. Each initialized Realm is retained as a separate
cluster; two sessions do not establish confidence.

Every warmup and measured call contributes an exact expected numeric output.
The stateful closure fixture checks that the same capture persists through
warmups and samples. The generated cases cover calls, closures, ordinary
objects, dense arrays, strings, Map and JSON; polymorphic calls, holey arrays and
object mutation are explicitly held out. Independent Node controls verify
the Python arithmetic oracle and accepted cachedData. No interpreter is used
in Scriptw preparation, bundle admission or execution.

The optional `react-ssr-synchronous` holdout restores checksum-checked original
source and expected HTML from `react-ssr-20261020.json`, without network or npm
installation. It is React/ReactDOM 19.3.1 with the previously documented alias
to the unmodified synchronous production renderer and exactly three checked
HTML cases. Its fresh score includes React initialization and rendering.
For isolated calls, exact checksum-checked upstream React, ReactDOM and the
synchronous renderer production CommonJS bodies run through small ordinary JS
factory wrappers. The three original entry trees, Context and package modules
initialize once before the observed warmups/calls. Each `batch(seed)` renders
all three cases and returns their exact JSON/HTML; output checks remain outside
the observed interval. The factories contain no eval/parser/interpreter or
source-loading VM: the entire fixture is ordinary input compiled through
Scriptw's JS-to-Wasm product path. Exact upstream source bodies, original
entry, licences and combined fixture bytes are retained. Independent normal
Node controls check fresh and isolated-render outputs against the historical
three-case reference. Streaming, public browser-entry support and general
React compatibility are not implied. Current Scriptw execution must pass
before this fixture supplies a performance observation.

Executable cache hashing is explicitly a cohort choice: this harness sets
`SCRIPTW_COMPILER_CACHE_IMAGE_HASH=sha256` (the v4 domain) unless the caller
sets `blake3` (the distinct v5-blake3 domain); it does not measure the
compiler's own `build-id` default. Every child must report the requested actual
algorithm/domain/digest and agree with preparation. Full loaded executable
SHA-256 evidence remains mandatory after the ready signal in both cohorts.
Receipts label these as separate cohorts, including when the emitted Wasm is
identical; their ratios cannot be silently merged into default/ordinary data.

The raw bundle (`SCRWPB01`, at most 256 MiB) binds the current compiler source
fingerprint, supported pinned Intl profile, P/R lengths, exact R key and whole
frame SHA-256. Nested R package admission verifies the compiler source,
physical Intl/source-compilation profile, checksum, complete raw Wasm bodies
and current typed layout. P is validated inert under the product Wasmtime
capability policy and must physically declare Portable authority and agree on
whether it imports R. Typed P/R linking still happens in the same executing
Wasmtime Engine. A corrupted or mismatched package returns `Err`; it never
uses `load_or_emit`. This is safe raw-Wasm admission, not an assertion that
arbitrary re-signed executable input came from an authentic source producer.
NativeHost source callbacks and native-code deserialization authority are
never granted. Custom Intl projection manifests are outside this container's
current scope and reject explicitly.

After the coherent implementation batch, build and verify serially under the
local cap. These commands are examples, not recorded passing checks:

```sh
python3 scripts/limited_verification.py -- cargo build -p scriptw-cli --release --example runtime_boundaries
python3 scripts/limited_verification.py -- cargo test -p scriptw-aot-wasm --lib strict_public_portable_package_codec -- --test-threads=1
python3 scripts/limited_verification.py -- cargo test -p scriptw-engine --lib portable_bundle -- --test-threads=1
python3 scripts/limited_verification.py -- cargo test -p scriptw-cli --example runtime_boundaries -- --test-threads=1
python3 scripts/limited_verification.py -- python3 -m unittest discover -s scripts/tests -p 'test_boundary_corpus.py'
python3 scripts/limited_verification.py -- python3 -m unittest discover -s scripts/tests -p 'test_benchmark_boundaries.py'
python3 scripts/limited_verification.py -- python3 -m unittest discover -s scripts/tests -p 'test_boundary_process.py'
python3 scripts/limited_verification.py -- python3 scripts/benchmark_boundaries.py --runner target/release/examples/runtime_boundaries --time /path/to/GNU/time --output-dir target/new-boundary-receipt
```

The driver checks the inherited local 4 GiB/no-swap/grouped-OOM limit and one
CPU before launching children, retains serial worker/cache limits and creates
a new output directory without overwriting prior evidence. Preparation is
recorded outside scores. The default workload selection is calls, arrays and JSON;
select each additional tuning or holdout explicitly with repeatable `--case`.
Use `--case react-ssr-synchronous` to exercise the supported SSR boundaries.
All three scoreboards run by default. Repeat `--boundary source|fresh|calls`
to select a bounded subset; `--boundary source --case arrays` supplies a
single source-startup control for separately orchestrated SHA/BLAKE A/B/B/A
runs without redoing the other boundaries. The algorithm/domain is explicit
in configuration and must match actual SDK observations and final full SHA.
All sources, output oracles, raw P/R bundles, V8 cachedData, command receipts,
failed stdout/stderr, binary/harness checksums and raw timings remain on disk.
Per-Store allocation/collection counters and peak live GC bytes remain
unavailable rather than inferred from capacity or process RSS.
