# Bounded idle compiler scratch

This directory vendors the crates.io `wasmtime-internal-cranelift` 47.0.0
release, retaining its Apache-2.0 WITH LLVM-exception license. The original
crate checksum is
`31bd91ac4567413dfe14f5f4e32dbcc45e45c2d08f280245de06629e362b2b02`.
Its recorded upstream commit is
`39d67168cdb6911ffdc961381314b6fed62e3c93`, path `crates/cranelift`.
The workspace selects this copy through `[patch.crates-io]`; dependency
versions, target/ISA settings and generated machine-code policy are unchanged.

The idle scratch policy modifies `src/compiler.rs`; the added private
`src/compiler/context_pool.rs` and its tests own it. The translation changes
below modify `src/func_environ{.rs,/gc.rs}`.
Registry installation markers were omitted; all other release files are kept.
The license text is unchanged; its trailing empty line is normalized.

Upstream returns each completed context to a compiler-owned pool and clears its
final CLIF only when another compilation checks it out. A long-lived Engine can
therefore retain that final IR and earlier scratch capacities indefinitely.
The compiled machine-code buffer is already moved out by
`Context::take_compiled_code`; this patch does not claim it was retained there.

Successful returns now clear final CLIF before reuse. At most two small idle
contexts are retained per compiler. A context is dropped completely when its
observed pre/post compilation extent exceeds any of these limits:

| Extent | Maximum retained work size |
| --- | ---: |
| CLIF instructions allocated | 4,096 |
| CLIF values allocated | 8,192 |
| CLIF blocks allocated | 1,024 |
| Actual declared Wasm locals, including parameters | 2,048 |
| Compiled native body bytes | 256 KiB |

These are conservative reuse criteria, not measured allocator bytes or a total
heap ceiling. Instruction/value/block counts include entities removed from the
layout, and the maximum before and after optimization is recorded. The local
count also catches a large unused local declaration whose final CLIF is small.
Dropping an owner releases its opaque translator, validator and Cranelift
capacities together. Small owners retain their allocations for useful reuse.
Normal upstream Wasm translation currently drops its validator allocations;
the policy does not introduce new validator caching.

The existing `Compiler::append_code` module assembly boundary rechecks and
clears idle owners under the pool mutex. An active job owns its context outside
that pool and cannot be cleared by another module's assembly. The same bound
applies on every successful function return, including jobs completing after a
different module's assembly. Failed compilation drops its checked-out owner
without recycling partial IR. Retired incremental-cache hit/insert counts are
folded into fixed counters so the existing compiler-drop diagnostic remains
accurate. Modules, code images, type registrations, GC metadata, ISA settings
and cache contents are unaffected.

The focused unit filter is `compiler::context_pool::tests::`. Its seven cases
cover actual instruction-allocation reuse after clearing, each oversized
extent, preoptimization high-water retention, the real module assembly call
with an active job, bounded returns with independent compiled output, a real
verifier failure followed by successful compilation, and cache count ownership.
The patch has no measured RSS or latency claim before the coordinated native
verification and memory campaign. Reducing reuse can increase compilation
allocation work for larger functions or broad parallel compilation.

## Funcref reads and short array fills

Two translation changes accompany the vendored Wasmtime runtime
([its notes](../wasmtime-47.0.0/PATCHES.md)); both keep the builtin
signatures and Wasm semantics.

- `src/func_environ/gc.rs`: reading a typed `funcref` from a GC struct or
  array passes `get_interned_func_ref` the engine-level type that the field's
  declared type maps to, loaded from the `VMContext` type-id array, instead of
  the module-level index the runtime then had to translate through the
  module. An untyped `funcref` still passes the reserved index. The runtime
  still checks the interned function's type against it.
- `src/func_environ/gc{.rs,/copying.rs}`, `src/alias_region.rs`, `src/lib.rs`:
  with the copying collector, that read first looks the id up inline in the
  interned funcref table the runtime publishes in `VMCopyingHeapData` (offsets
  in the public `copying_heap_data` module): an id below the published length
  yields its entry when the entry is null or, for a concrete expected type,
  when the entry's registered `type_index` equals the expected engine type;
  every other case calls `get_interned_func_ref` as before. The length and
  base are reloaded after any call, since only host code grows the table.
- `src/func_environ.rs`: a byte-fillable GC array fill (`array.new`,
  `array.new_default`, `array.fill` of a non-reference element whose bytes are
  all equal) of at most 64 bytes is stored inline, eight bytes at a time and
  then singly, instead of calling the `memory.fill` libcall. Longer fills keep
  the libcall. Bounds checks precede both paths, and memories and tables are
  unchanged.

## GC access code

These change the code for GC struct and array accesses and type tests
([measurements](../../docs/rust-rewrite/benchmarks.md#gc-access-code-2026-11-14));
Wasm semantics and trap codes are unchanged.

- `src/translate/code_translator.rs`, `src/func_environ{.rs,/gc.rs}`:
  `struct.get`/`struct.set` and `array.len`/`array.get`/`array.set` learn
  whether their reference operand's validated type is non-nullable. Such an
  access needs no null check.
- `src/func_environ/gc.rs`, `src/lib.rs`: otherwise, with the copying
  collector, signal-based traps, a GC heap that cannot move and whose
  reservation plus guard covers every 32-bit reference (the condition under
  which field accesses already omit heap-bound checks), an access whose bytes
  end within `copying_heap_data::NULL_GUARD_BYTES` (4096) of the object start
  is its own null check. It carries the null-reference trap code, and through
  reference 0 it faults in the heap's null guard, which the vendored runtime
  keeps inaccessible ([its notes](../wasmtime-47.0.0/PATCHES.md)). That
  access is the struct field access, or the array length load that precedes
  every element access. A `funcref` store keeps its explicit check, since the
  value is interned by a libcall first. So do `ref.test`, `ref.cast`,
  `br_on_cast` and `ref.is_null`, whose null outcome is a value.
- `src/func_environ/gc.rs`: after the `index < length` check, an array
  element's address is the heap base plus `ref + base_size + index *
  elem_size`, computed in wrapping 32-bit arithmetic and then zero-extended.
  The product and both sums wrap at 2^32, so whatever a corrupt length let
  `index` be, the element starts below 2^32 and the access, at most 16
  bytes, ends below 2^32 + 16: inside the 4 GiB reservation plus its guard,
  where every byte is heap or faults. For an intact heap nothing wraps, since
  the array lies inside the 32-bit heap. This replaces the object-size
  computation, its overflow traps and the backwards offset from the object's
  end; it applies when the reservation plus guard is at least 2^32 plus the
  element size, and the original code remains otherwise.
- `src/func_environ/gc.rs`: `ref.test` and the casts test for `i31` only when
  the operand's static heap type is `any`, `eq` or `i31`; a struct or array
  reference never holds one.
