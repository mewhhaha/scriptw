# Compiler and runtime efficiency — 2026-10-24

This batch implements the first seven findings from the compiler efficiency
review of `0443509f370dde7ed1a2c690770e9111ddb5a1f9`. It preserves the real
JavaScript-to-Wasm path and the experimental Wasmtime GC/reference feature
requirements. Conformance checks remain authoritative; an optimization that
cannot establish its preconditions uses the ordinary compiler/runtime path.

## Changes and limits

1. **Property-cache metadata.** Source bodies share a checked, dense program
   site namespace for each cache kind. Each ProgramInstance owns indexed nullable
   GC tables for literal shapes and own-data read metadata. A two-level table
   allocates 64-slot leaf pages only when touched; its outer table grows
   geometrically. Untouched pages allocate no leaf slots; each touched page
   reserves 64 nullable slots, with metadata records created only on use.
   Warmed lookup no longer searches a linked list. Receiver,
   live storage, shape and key guards remain in place. Metadata retains neither
   receivers nor user property values.
2. **Map/Set storage.** Physical entry storage compacts in insertion order when
   full, and clear drops its references to the old backing arrays immediately.
   Their memory is reclaimed by GC. A private stable
   ordinal identifies each insertion. Live cursors retain that ordinal plus a
   physical index and storage version, re-seeking only after storage changes.
   Iterators, forEach, Set operations and groupBy consume the same protocol.
   Exhausted iterators stay exhausted. The tradeoff is an additional raw i64
   ordinal array, eight bytes per capacity slot, plus cursor/version fields.
   This bounds lifetime churn; it does not promise that deleting an entry
   immediately shrinks every backing array.
3. **Ordinary properties.** A full ordered table compacts live properties before
   growing. Positions and hash buckets are rebuilt without changing insertion
   order or descriptors. Entries whose positions remain unchanged are reused.
   A single capacity calculation preserves the power-of-two hash-mask invariant
   and reserves spare slots so high-occupancy churn remains amortized.
   Hash tombstones were already reusable; this fixes the separate ordered
   history that inflated subsequent own-key enumeration.
4. **Call arguments.** Spread/mixed argument construction uses a geometrically
   grown GC array rather than a linked node per element. Captured lists reuse
   immutable value snapshots. A bounded numeric leaf plan can avoid argument
   boxes and the argument array after actual callable/program identity and
   Number guards. Its fallback retains ordinary dispatch, evaluation order,
   roots, argument observation and the source activation-depth limit. General
   fixed calls still use the existing boxed ValueArray ABI.
5. **Prepared-source discovery.** A conservative purity certificate permits
   memoized discovery over newly admitted sources for simple fresh-ShadowRealm
   module chains, before final whole-graph semantic lowering. Shadowed names,
   unknown effects, preludes and other unproved contexts retain the existing
   flow-sensitive discovery path. This is not eager loading by source spelling
   and does not grant additional filesystem or Realm authority. Growing chains
   still scan graph metadata to recheck the certificate; this does not claim
   fully linear compilation or remove the general module-source reparse path.
6. **Warm module caches.** The current loader first admits/parses the static
   graph and hashes its dependency bytes, resolutions and compiler options.
   A cache candidate is checked before IR lowering. Only a successfully closed
   graph publishes its final key, so discovery that expands a graph cannot
   accidentally hit its incomplete candidate. Closed static graphs avoid
   lowering on a hit. Dependency changes and parse failures remain visible.
7. **Inference.** Ordinary inference streams completed statements through the
   same lowering and caller-effect proof used for emitted bodies, retaining
   summaries instead of complete discarded function bodies and metadata.
   Resource/suspension cases retain full-body processing. This reduces retained
   intermediate IR; it does not remove all statement construction or replace
   conservative inference fixed points with speculative results.

Runtime loops that prepare/compact storage and append argument buffers are
outlined in the shared runtime. This batch does not enable whole-runtime native
inlining, enlarge GC defaults, relax cache admission or increase worker counts.

## Verification

Frozen verification source: `91ad82c6a5098c2658f68cf73f555f73fcb52aa8`.
All product files in this patch match that checkout. The final documentation
and evidence are recorded separately from the tested product source.

The 66 affected native scopes contain 8,407 tests: **8,406 passed and one
remains incomplete at the enforced 4 GiB limit**. This includes all 1,020
engine-library tests, 861 CLI tests, and 151 of 152 RegExp integration tests.
The large CLI target ran in four serial processes. Engine tests used three
isolated runtime-profile controls plus batches of at most 32; RegExp tests
used an isolated stress case plus batches of at most eight. Inventories,
source fences, executable identities and terminal outcomes were checked.
Overlapping focused checks and the 278 source-only checks are not added to
the native total. The final default-feature release CLI and runtime runner
built successfully, and all eight new workloads produced their exact expected
outputs under both Scriptw and V8 (16 functional checks).

The selected Test262 replay completed on 2026-10-24: **14,927 passed / 4 failed /
0 crashed / 0 unimplemented / 0 timed out** across 14,931 exact executions.
The four failures are unchanged: the sloppy implicit-arguments Annex B case,
both modes of `intl402/Temporal/ZonedDateTime/links.js`, and the sloppy labelled
block-function case. All remain counted as failures; the native suite's actual
exit code was 1. The replay took 9,537.6 seconds on one CPU and peaked at
3,657,621,504 cgroup bytes, with zero OOM events. Cgroup memory includes process
and file-cache charges, not a measurement of live JavaScript bytes.

The complete current 102,994-execution inventory matched the earlier inventory
byte for byte before selecting these cases. This replay is separate from the
full pinned suite and does not refresh its README chart.

No timing speedup is claimed. Other work was active on the machine, so the
README performance graphic retains its existing measured compiler.

Local native verification uses `scripts/limited_verification.py` with a
kernel-enforced 4 GiB process-tree cap, zero swap, grouped OOM termination,
one CPU, serial Cargo/libtest and bounded retained-module caches. Temporary
build files are placed on the workspace filesystem, not the nearly full `/tmp`
tmpfs. Verification records are retained under
`target/compiler-efficiency-20261024`.

### Existing large-function native-compilation limit

`aot_regexp_iv_class_strings::property_and_direct_strings_fold_circled_m_before_algebra`
also exceeds 4 GiB in a fresh process. The retained pre-task CLI at
`559151e323dfba7a85d046dc0e1394d83798df69` has identical product sources and
dependencies to the review base `0443509f3`; it exceeds the same cap on the
same ten-RegExp expression. This establishes an existing resource limit,
not a passing test or a new conformance result.

The baseline trace finishes parsing, lowering, emission and embedded-runtime
loading, then stops during native compilation of P. Its largest code body is
1,043,341 bytes, below the existing 1,048,576-byte switch to size-optimized
native compilation. Execution was never reached. The CLI diagnostic uses a
filename/cache wrapper and has no execution timeout; those differences do not
account for a failure before execution. The original native test retains its
30-second execution deadline. All original interruptions remain in the evidence.
The compiler's large-function sizing policy is a separate follow-up; this batch
does not raise the memory limit or change native optimization defaults.

## Evidence and refresh

The [checkpoint record](compiler-efficiency-20261024.json) retains exact native
scope counts, compiler digests, pins, resource limits and the selected-suite
result. The [evidence archive](compiler-efficiency-20261024.tar.gz) contains the
closed file/hash inventory, original failed attempts, kernel OOM records,
successful continuations, Cargo producers, all native and Test262 execution
inventories, the native snapshot/attempt ledger, and the exact source bundle.
Executables and caches are omitted. The source bundle requires the public review
base `0443509f3` and provides the tested `91ad82c6a` HEAD.

After extracting the archive, use its `selected.executions` list with a fresh
output directory. From the frozen source, the default build and replay commands
are:

```sh
python3 -B scripts/limited_verification.py -- \
  cargo build --locked --offline --release --jobs 1 -p scriptw-cli \
  --bin scriptw --example runtime_baseline --message-format=json

python3 -B scripts/limited_verification.py -- \
  python3 -B scripts/replay-test262-executions.py /path/to/selected.executions \
  --binary target/release/scriptw --suite-root test262/vendor/test262 \
  --output-dir target/compiler-efficiency-replay-fresh --workers 1 \
  --reuse-case-workers
```

The archive retains the actual argv and source fences for the 64 complete native
scopes and the two exact-inventory batch continuations. Those continuations
preserve all 8,407 qualified IDs and keep the known resource interruption
explicit. The ordinary complete-target invocation can still hit that existing
large-function limit under 4 GiB; increasing the cap is not part of this refresh.
