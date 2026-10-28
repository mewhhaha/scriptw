# Wasm AOT semantic records

## Atomic source draft — 2026-10-05

The compiler's atomic source draft represents JavaScript identities, values and
record edges with Wasm GC. The final heap cleanup retains scalar domains and
private byte transport; it retires the passive manual-heap offset, root and
weak-edge inventories. Temporal production source is sealed, and a bounded
caller census found no remaining references to the retired heap providers or
passive metadata. Compiler-control and guard composition have finished
independent source review. The whole source checkpoint is composed for one
combined integration. The complete source and integration repairs pass the
whole-workspace, all-feature, all-target Rust type check on 2026-10-05 under the
confirmed 4 GiB aggregate cap. Source guards, emitted Wasm validation and semantic
execution remain pending; authored Rust controls have not been executed.

## Values and records

`gc_types/value.rs` owns `ValueLocals`: an I32 tag, an I64 immediate scalar and
an actual nullable GC reference. Numbers preserve their IEEE-754 bits. Heap
identities travel in the reference component. Stored values and completion
records retain the complete value, including thrown objects and Symbols.

`gc_types/layouts.rs` declares the recursive struct and array families. Its
typed construction and field accessors share that declaration authority with
the complete value operations. Field owners, nullability, mutability and closed
scalar domains are enforced by the Rust API before raw Wasm encoding. Builtins
consume these APIs; they do not publish a second byte-record representation.

The record families cover ordinary objects/descriptors, sparse Array indices,
functions and captures, environments and binding cells, Realms and intrinsics,
strings, arbitrary-precision BigInts, Symbols, binary-data views, collections,
iterators, generator/async activations, Promise jobs, resource disposal,
Temporal and Intl. Dynamically owned code units, limbs and unshared bytes use
GC arrays. Sparse Array storage tracks present descriptors independently from
the JavaScript length, and its published own-key arrays are immutable.

## Callable and helper boundaries

The function planner retains callable origin and protocol with the declaration
and body identities. Actual calls retain their family-specific environment,
receiver, argument vector, new.target or activation fields. Native constructors
use the shared Realm/prototype publication path. Internal continuation captures
are concrete strong GC fields.

The runtime helper registry declares the actual typed operands and results for
each helper. Whole-value and completion adapters use that same row. A raw
semantic address cannot be passed as a replacement for a typed reference.

## Roots and collection

The consumed module type/global package declares concrete rooted globals from
the actual encoded section. Frame values, environments, captures, stored
completions and job records remain typed references while execution can
allocate or call user code. The native `gc()` host import requests collection
from the selected Wasmtime collector.

Strong collection does not supply weak or ephemeron edges. The selected SDK's
facility remains explicitly unavailable. Weak builtin validation retains its
observable prefix before capability rejection; no strong-retaining weak
representation, manual heap or silent skip replaces that missing facility.
See [the capability contract](../../../docs/rust-rewrite/contracts/weak-unavailable-runtime-boundary.md).

## Byte transport and host ownership

Immutable compiler data and call-scoped wire/scratch buffers may use private
linear memory. The checked allocator validates memory32 start/end bounds and
growth before committing its cursor. It does not allocate semantic objects.
Shared backing resources own bytes through a typed native resource reference;
the JavaScript wrapper retains the sole language identity.

Host imports borrow linear buffers for the duration of the call. Complete GC
references use the registered typed host boundary, and exported results are
decoded while their instance/store is alive. Hosts retain owned results rather
than JavaScript record addresses.

## Verification

The old linear-layout unit/source mirrors and their historical results are
preserved in the source packet. Finite semantic Engine/CLI controls exercise
the actual compiler path; all new controls remain unrun. Complete all task
source, types, controls and documentation before verification. Later stages use
the confirmed aggregate 4096 MiB cgroup cap, swap zero and serial workers.
See [the development workflow](../../../CONTRIBUTING.md#verification).
