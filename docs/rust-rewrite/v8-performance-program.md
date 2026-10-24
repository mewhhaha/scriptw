# V8 performance implementation

This program implements and measures the six performance workstreams. The
[2026-10-24 checkpoint](performance-six-workstreams-20261024.md) records the
balanced results, regressions, verification and rejected experiments. The README
benchmark table now uses that local compiler; the full Test262 chart retains
its historical compiler identity. Performance changes do not establish a new
full-suite result.

## Resource contract

Local builds, tests and benchmarks use:

```sh
python3 scripts/limited_verification.py -- <command>
```

The wrapper confirms a kernel-enforced 4096-MiB process-tree limit, zero swap,
grouped OOM termination, one CPU, and serial Cargo/libtest/compiler pools. It
refuses an uncapped fallback. Run expensive commands sequentially and leave
benchmarks idle of builds/tests. The memory-module cache is separately bounded;
neither a job count nor a cache limit replaces the kernel memory limit.

The current local machine has 16 logical CPUs and about 96 GiB of physical RAM.
This program does not use the historical cloud machine's four-CPU/32-GiB policy.
Hardware, Node/runtime versions, resource limits and cache states remain part of
each new measurement identity.

## Implemented paths

| Workstream | Concrete behavior | Acceptance boundary |
| --- | --- | --- |
| Measurement | Checked tuning/validation corpora; separate source, precompiled startup and existing-Realm call scoreboards, including supported React SSR | Raw source/output/artifact admission; separate Realm clusters and callback timestamps; no empty subtraction |
| GC and native policy | Explicit 16/32/64/128-MiB initial capacities, Fast/SizeOptimized/Speed program compilation, and independent register allocation | Diagnostic cohorts report actual Engine policy; ordinary defaults remain unchanged |
| Property reads and literals | Bounded fresh data literals use compact SoA values and immutable source shapes; static read-site caches check live storage/shape in optimized R; Array index/length reads retain runtime Get | Cache metadata retains no receiver/value/Symbol; shape mutations use dictionary conversion; generic Get retains prototype/accessor/proxy semantics |
| Storage | One ordinary GC storage base admits compact/default-data or complete dictionary states; Arrays can use packed Number bits/presence or complete dense/sparse descriptors | Holes, attributes, accessors, order, length, value roots and public descriptor identity retain their original semantics |
| Scalar regions and calls | Proven Number arithmetic retains scalar intermediates; fixed arguments reuse scratch and immutable snapshots; validated leaf entries can omit unobservable arguments/mapping environments | Operand/coercion/throw order, IEEE Number behavior, source activation and argument identity remain observable |
| Startup | Separate admission and host-registration subspans identify setup costs; Engine-owned immutable setup can be reused without sharing a Store or Realm | Reuse needs actual Engine/signature authority and independently measured cold/repeated behavior |

Layout 18 has its own source-bound native and performance checkpoints. Its
selected Test262 campaign is recorded separately from the historical full
suite. It does not inherit first-batch results.
[Compact ordinary storage](contracts/compact-ordinary-data-storage.md) admits
only fresh source literals with 1–16 unique static data keys and default
attributes. Mutable tags/scalars/references are per object; string-only immutable
shapes are per literal/ProgramInstance. The source plan owns allocation and
preserves source Realm, value evaluation and abrupt completion. Static data
literals with 17–64 fields retain descriptor append, and special forms retain
general definition. This adds no literal-capacity reservation policy.

Static string read sites call guarded own-data cache logic in optimized R,
including unresolved literal-key GetV reads after their existing ToObject and
ToPropertyKey steps. Lookup, original receiver and canonical key remain explicit
at the runtime boundary. Actual live storage and immutable shape identity
precede current-value loads. Calls and structural mutations cannot leave an
unchecked slot proof behind. Cache metadata cannot retain user objects,
prototypes, Symbols or values. Generic Get handles every miss, inherited value,
accessor and proxy. Source shape facts alone remain insufficient to authorize
physical slot access.

Array index and length reads retain the compiled shared runtime Get dispatch.
The closed indexed-storage mode selects packed Number bits plus a hole bitmap,
complete dense descriptors or sparse descriptor nodes. Registered mutation
owners publish mode changes and clear inactive backings. Logical length never
sizes a backing; the occupancy bound and 1,048,576-slot ceiling limit geometric
growth. Default-data producers call one typed R publication helper, keeping
growth, copying and descriptor fallback out of repeated source literal sites.
Descriptor consumers materialize complete descriptors before exposing
mutable descriptor references. Specialized backing changes and cache placement
need their own paired measurements; native-policy defaults remain unchanged.

Layout 18 introduced these GC subtypes and primitive arrays. Current private
layout 19 versions indexed ProgramInstance metadata and shared operation
signatures; R/P identity checks reject older layouts. The
[efficiency checkpoint](compiler-efficiency-20261024.md) records those changes
separately from the historical layout-18 measurements.
The private undefined snapshot remains immutable; observable objects and
reflective descriptor snapshots remain fresh.

## Measurement and decisions

Use the [performance-boundary contract](contracts/performance-boundaries.md) for
the corpus, scoreboard and explicit native-policy flags, and the existing
[baseline workflow](v8-baseline.md#current-performance-and-memory-tooling) for
identity-bound builds and paired observations.

The separate [portable-boundary cohort](contracts/portable-performance-boundaries.md)
adds fresh source-free bundle admission and callback-delimited execution in one
existing Realm, with ordinary V8 cachedData controls. Its nonce-bound ready pipe
measures spawn to checked result before post-result provenance work. The
[native policy controls](contracts/native-policy-break-even.md) separately
measure cold P compilation and the observed execution savings needed to repay
it. These cohorts retain distinct identities and cannot replace historical
baseline samples.

Begin with the unchanged Fast/16-MiB/SinglePass cohort. Sweep one GC capacity or
native-code policy at a time, retaining small-program, retained/ephemeral,
startup, memory and generated-code-size controls. Capacity is reserved space,
not live JavaScript bytes. Compare policy experiments as labelled configurations,
not as compiler-source speedups. Use repeated balanced observations to confirm
effects; seven samples alone do not establish a reliable tail-latency bound.

Keep conformance and performance verification separate. Run focused semantic
and emitted-artifact controls first, adjacent Array/descriptor/call/GC scopes
next, then the appropriate native and original pinned Test262 campaign. Keep
the four recorded original failures visible. A complete new Test262 result
must use the native status publisher; this document does not refresh its counts.

General small-arity entry ABIs, broader source-body inlining and profile-guided
AOT compilation remain measured follow-up decisions. A source-known function binding alone
cannot authorize bypassing legacy arguments/caller, source-depth, Realm or
capture state. Proven unobservable leaf entries can avoid argument objects and
unnecessary parameter environments while retaining the normal source body and
activation protocol. The separate bounded numeric leaf plan can additionally
avoid argv and substitute its pure expression after actual code/program,
Number and depth guards; it does not establish a general entry ABI or inliner.
Native Wasm inlining can preserve the general activation protocol; its
explicit bounded cohort requires separate code-size and compilation controls.
Its first whole-program experiment was OOM-killed at the unchanged 4-GiB cap;
it remains disabled by default.
