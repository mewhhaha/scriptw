# Bounded idle compiler scratch

This directory vendors the crates.io `wasmtime-internal-cranelift` 47.0.0
release, retaining its Apache-2.0 WITH LLVM-exception license. The original
crate checksum is
`31bd91ac4567413dfe14f5f4e32dbcc45e45c2d08f280245de06629e362b2b02`.
Its recorded upstream commit is
`39d67168cdb6911ffdc961381314b6fed62e3c93`, path `crates/cranelift`.
The workspace selects this copy through `[patch.crates-io]`; dependency
versions, target/ISA settings and generated machine-code policy are unchanged.

The only modified upstream source is `src/compiler.rs`. The added private
`src/compiler/context_pool.rs` and its tests own the idle scratch policy.
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
