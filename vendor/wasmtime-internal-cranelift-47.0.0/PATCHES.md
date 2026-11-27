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
below modify `src/func_environ{.rs,/gc.rs}` and `src/translate/`; the section
before the last adds `src/func_environ/gc/{base_home,frame_reloads}.rs` and
`src/func_environ/{branch_threading,set_tests}.rs`, the one after it adds
`src/func_environ/cold_paths.rs`, the one after it adds
`src/func_environ/operands.rs`, and the last adds the translation tests in
`src/compiler/translation_tests.rs`.
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

## Funcref stores without a call

`src/func_environ/gc{.rs,/copying.rs}`, `src/alias_region.rs`, `src/lib.rs`:
with the copying collector, a store of a `funcref` into a GC struct or
array (`struct.new`, `struct.set`, `array.new_fixed`, `array.set` and the
fills, which all go through `intern_func_ref`) no longer always calls
`intern_func_ref_for_gc_heap`. The null funcref is stored as id zero, which
the vendored runtime gives it in every table
([its notes](../wasmtime-47.0.0/PATCHES.md#funcref-stores-without-a-call)).
Any other address is looked up in the runtime's direct-mapped intern cache,
whose address the runtime publishes in `VMCopyingHeapData` (offset and
entry layout in the public `copying_heap_data` module): the entry at
`(address >> log2(4 * pointer bytes)) mod 256` is accepted when its address
word equals the address, and its second word is then the id. Every other
case calls the libcall, which interns the address and fills the entry. The
cache's address and both entry words are reloaded for each store, and the
entry loads carry no alias region, so none is carried across a call into
the host, the only writer. The builtin's signature and the stored ids are
those of the libcall; a `nofunc` operand keeps its trap.

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
  every element access. A field of the uninhabited `nofunc` type, whose value
  is known without a load, is read all the same when the read is the check. A
  `funcref` store keeps its explicit check, since the
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

## Generational write barrier

`src/func_environ/gc{.rs,/copying.rs}`, `src/lib.rs`: with the copying
collector and a nursery (`copying_heap_data::nursery_end` of the GC heap's
initial size, nonzero from 2 MiB), every reference stored into an existing GC
object (`struct.set`, `array.set` and the per-element loops of `array.fill`,
`array.copy` and `array.init_*` on initialized arrays) first compares the
slot's address with the heap base plus the nursery's end. Only a store into
the old generation leaves that compare: if the value is a nursery object (a
non-null, non-`i31` reference below the end), the slot's heap index is
appended to the remembered-slot buffer the runtime publishes in
`VMCopyingHeapData`, or the buffer's overflow flag is set. Those paths are
cold blocks. Initializing stores, table and global stores need none: tables
and globals are roots, and new objects are young or remembered whole.

## Frame reloads, home slots, proven references, threaded branches and set tests

`src/func_environ/gc{.rs,/base_home.rs,/frame_reloads.rs}`,
`src/func_environ/{branch_threading,set_tests}.rs`, `src/func_environ.rs`,
`src/bounds_checks.rs`, `src/alias_region.rs`,
`src/translate/{code_translator.rs,func_translator.rs,stack.rs}`. These change
which instructions a function executes, not what it computes: Wasm semantics,
trap codes and their order, the `VMContext` and store-context layouts, stack
maps, target and ISA settings and calling conventions are unchanged, so a
module compiled with them is interchangeable with one compiled without.
Callgrind instructions of one retained React Server Components request (sync,
median of 97) went from 3,645,562 to 3,236,556 over the steps below, each
measured on top of the ones before it.

### Loads from the frame, once per call (`frame_reloads`, `base_home`)

Two kinds of stack slot hold a value that a function reads far more often
than the value can change.

- `cranelift-frontend` keeps each GC reference that is live across a call in
  a stack-map slot and replaces every use of it, the ones before the first
  call included, by a load from that slot. Cranelift's redundant-load
  elimination stops at every instruction that can trap.
- The GC heap base (when the heap cannot move) and the function's `vmctx`
  parameter are one value for a whole activation. As one SSA value each is
  live across every call; in a large function the register allocator spills
  it and can reload it before every use after exhausting its split budget
  (`MAX_SPLITS_PER_SPILLSET`). Each
  therefore gets a home slot: the entry block stores the value there once,
  and every use is first a load from the home (`FrameHome::load` for the
  base, `FrameHome::of_parameter` after translation for the `vmctx`, which
  the translator passes directly). A function that makes no call gets no
  `vmctx` home; a heap that may move keeps the upstream load of its base
  from the store context at every use.

After translation and `FunctionBuilder::finalize`, `Reloads::forward` follows
through the whole function what each such slot holds: a store makes the
stored value known, a load of a slot whose content is known is removed, any
other load stays and makes its result known, a call ends what is known (a slot
holding the null reference still holds it), and a join keeps what all its
predecessors agree on. The head of a loop that reads a slot before it calls,
and whose paths back to the head know the content, takes the content as a
block parameter (`Reloads::hoist`): the path into the loop reads the slot
once, at its end, instead of every iteration reading it at its first use. A
load that stays addresses the slot through a
`stack_addr` of its own, at a distinct distance past the slot with the
opposite load offset: the x64 backend folds a `stack_addr` into a store or
load, but when it merges a load into the instruction that uses it, it
computes the address into a register, and one `stack_addr` shared by all
accesses would be computed where the slot is first written and kept in a
callee-saved register or a spill slot from there. A home that nothing reads
any more loses its store and its slot, so a function without calls is compiled
as upstream compiles it.

For cascaded cold guards, each collecting edge reloads its stack-map root
after the safepoint. The join combines that reference with original SSA
references from call-free edges. Those SSA parameters carry no assertion
about the slot's contents. A regression runs the translated edge shape
through forwarding and loop hoisting and checks that only the collecting
edges load the slot.

Why it is correct (the argument is in `frame_reloads.rs`): the pass handles
only slots whose address is used as nothing but the address of whole-slot
loads and stores of one type. Such a slot changes only at one of those stores and, for a
stack-map slot, while a call runs and the collector relocates the reference
in it (never a null one). Any solution of the forward equations therefore
names, at a removed load, a value that a store or load on every path to it
put or found in the slot with no call and no other store since; that value's
definition dominates the load, and no collection lies between. Stores to
stack-map slots and the stack maps themselves are untouched. `try_call` is a
call, so exception handlers start knowing nothing but nulls; a trap leaves the
function. The equations are iterated at most twelve rounds, after which the
function is left as it was; so is a function whose answer would hold more
than 64 entries per block on average (`MAX_KNOWN_PER_BLOCK` times the number
of reachable blocks, in total). Such a function keeps every load, so its
native code differs from what the pass would make, deterministically; a small
function with few blocks and many stack-map slots can exceed the budget
(for example 3 blocks and 200 slots stored without a call between), and its
Speed-policy cost is unmeasured. The byte-identity check in the Cranelift
frontend notes (`vendor/cranelift-frontend-0.134.0/PATCHES.md`) covers only the
listed fixtures, where the budget never fired. What is known at a block's
end can grow with the path to it, so deeply nested code kept a known set per
block proportional to its depth: half of a 1 GB native compilation of 500
nested `else if` branches, and 3.3 GB for 1,000 (now 216 MB and 392 MB, and
those functions keep their reloads). A loop head's parameter is the slot's content
because each edge passes what the slot holds at the end of its predecessor
(the known value, a zero, or a load placed before a `jump`), and nothing
writes the slot or calls before the block's first use of it; a load from the
frame cannot trap, so reading on the way into a loop that leaves before it
would have read costs one load and changes nothing else.

A call also ends what is known of a home, although a home never changes, and
no block other than a loop's head takes a parameter where paths bring
different values. The base was
first placed with SSA construction instead (a load right after each call that
a use follows, block parameters at joins, no split inside loops); with a call
every few blocks nearly every join took a parameter, and the moves cost more
than the loads they saved: the request runs 19,195 instructions fewer with the
base placed as described here than with that placement, and the same
placement applied to the `vmctx` ran 25,827 more than no `vmctx` home at all.
A narrower form was measured for all slots of this pass and not kept either:
a parameter only where every path onward reads the slot before it calls,
with a load on just the edges that do not know the content, ran 11,170 more.
At a loop's head the load saved is one per iteration: without it a scanning
loop of `String.prototype.split` read the heap base 2,026 times a request.

Measured: stack-map slots alone 3,645,562 to 3,552,613; with the `vmctx` home
and the base moved to this pass 3,344,847 to 3,307,690 (after the two
sections below); loop heads 3,246,099 to 3,236,556 (after all of them).

Costs: a function with several calls in a row that each use the base or the
`vmctx` afterwards pays a load per call where upstream, when the value won a
callee-saved register, paid nothing. A loop that calls reads what it uses
after the call in every iteration, and a loop entered without the content
known reads it once even when it then leaves at once. On targets whose backend does not fold a
stack-slot address into a load, each remaining load costs an address
computation as well; only x86-64 was measured.

Tests: `func_environ::gc::frame_reloads::tests::` (no call, straight-line
calls, joins that agree and that do not, loops with and without calls,
loop heads that take a parameter and that do not, `try_call` successors,
null references, a slot used for two values, slots
that escape or are read in part or as another type, home slots, the `vmctx`
parameter), each
verified by Cranelift's verifier after the rewrite.

### Branches on a truth value that paths only just chose (`branch_threading`)

`ref.test` followed by `if`, `br_on_cast` and Wasm blocks that yield a
condition all end in a block whose only instruction is a `brif` on its own
parameter, reached by edges that pass a constant or a comparison. Cranelift
compiles that as written: each path materializes the truth value, moves it to
the parameter and tests it again. After finalization, `thread` sends each
such edge where the block would: an edge passing a constant jumps to the
selected successor, an edge of a `brif` that passes its own condition goes to
the successor its outcome selects, and a `jump` passing any other value
becomes a copy of the block's `brif` on that value. It runs up to three
rounds, since a threaded predecessor can become such a block.

Why it is correct (the argument is in `branch_threading.rs`): a block is
handled only when the `brif` is its only instruction and nothing else in the
function uses any of its parameters, so entering it does exactly what the new
edge does; the block itself is unchanged for the edges that still enter it.
The values a new edge passes are the predecessor's own arguments and values
that are defined on every path into the block. No call, store, trap or stack
map is added, removed or reordered.

Measured: 3,552,613 to 3,441,005.

Tests: `func_environ::branch_threading::tests::` (constant and computed
arguments, the block's other parameters, an edge passing its own condition,
chains, parameters with other uses, blocks that do more), each verified by
Cranelift's verifier.

### Checks that an earlier check implies (`ProvenReferences` in `gc.rs`)

The emitter reads a nullable local and applies `ref.as_non_null` or `ref.cast`
at each use. Cranelift merges two such checks only when they test one SSA
value, and after a call a GC reference is a value read from the frame again;
a cast was also several blocks joined on a truth value.

- `ref.cast` to a non-nullable concrete struct, array or exception type is
  now a sequence of trapping checks (not null, not an `i31`, the header's
  type) with the trap code and order it had; other casts stay `ref.test` and
  a trap.
- While translating, the environment records for a CLIF value what control
  can only pass having established: not null after `ref.as_non_null`, after
  a struct or array access that checked for null, and where `br_on_null`
  falls through; a non-null instance of a concrete type after `ref.cast`,
  where `br_on_cast_fail` falls through, and in the consequent of an `if` on
  the result of `ref.test`. A later `ref.as_non_null`, null check of a GC
  access, `ref.cast` or `ref.test` of the same value that a recorded fact
  implies emits nothing (`ref.test` yields one).

Why it is correct (the argument is on `ProvenReferences`): a fact is recorded
with the innermost Wasm control frame, and for an `if` its arm, at the point
that establishes it, and used only while that frame is still open in the same
arm. Structured control flow reaches code after a point within one arm of one
frame only through that point; the value is one SSA value whose definition
dominates both places; and nullness and an object's type never change, a
moving collection included. So the later check cannot fail and omitting it
removes no trap. Unreachable code is not translated and records nothing.

Measured: 3,441,005 to 3,344,847 for the cast sequence and the facts from
checks, and 3,307,690 to 3,280,545 for the facts from branches and tests.

Tests: `crates/scriptw-engine/tests/aot_gc_entries/aot_gc_translation_probes.rs`
runs hand-assembled Wasm on the product's Wasmtime configuration: checks that
only one path, arm or loop body made, branches on fresh truth values for
structs, `i31` and null, and reads of a field around calls whose callee
collects, moving the object, and then stores to it.

### Tests of one value against a run of constants (`set_tests`)

The emitter asks whether a tag is one of several by or-ing `i32.eq` tests
against constants, starting from zero, and whether it is none of several by
and-ing `i32.ne` tests. Cranelift compiles each test and each `or` as
written. The translator remembers which CLIF values are such tests of which
value; an `i32.or` of two membership tests of one value (or an `i32.and` of
two non-membership tests) whose constants together are the integers `low` to
`low + count - 1` is translated as `value - low <u count` (or `>=u`), with
the subtraction wrapping, instead of the `bor` or `band`. `0 | x` is `x`,
and `1 & test` is the test. Constants with a gap, tests of different values
and `i64` operations are translated as before.

Why it is correct: the subtraction maps exactly the listed constants to
`0..count`, for every `i32`, and both forms yield 0 or 1. What is remembered
is a property of pure values, so it holds wherever they are in scope.

Measured: 3,280,545 to 3,246,099.

Tests: `func_environ::set_tests::tests::` (runs, runs from zero and at the
top of the range, chains that start from their identity or complete a gap,
inequalities, and the combinations that must stay as written), each verified
by Cranelift's verifier; the engine probes above call such chains with values
at and around the ends of each run.

## Unlikely code behind the likely code

`src/translate/code_translator.rs`, `src/func_environ/{cold_paths.rs,gc.rs,
gc/copying.rs}`, `src/func_environ.rs`. These change where a function's
blocks are emitted and nothing else: the mark they set (`Layout::set_cold`)
is read by the backend's block ordering alone. No instruction, value, edge,
trap, stack map or exception table depends on it, the verifier requires only
that the entry block is not cold, and no pass below marks it. A module
compiled with them computes what it computed before, and loads into an engine
configured either way (upstream already excludes `branch_hinting` from the
artifact compatibility check).

Upstream reads the Wasm branch hinting section (`metadata.code.branch_hint`,
behind `Config::wasm_branch_hinting`, which the product configuration now
turns on) at `if` and `br_if` and marks one block cold per hint. A throwaway
probe on the product configuration, before these changes, found: a hint
moves the marked block behind the function's other code under
`None`/`SinglePass`, `Speed`/`Backtracking` and `SpeedAndSize`/`Backtracking`,
identically for a module compiled directly and one precompiled and loaded;
the hint's offset is the byte offset of the instruction's opcode from the
start of the body's local declarations, and one byte off is no hint; the
section is honored before or after the code section; and with the feature
off nothing moves. Three things were missing for the emitter's use.

- **The edge, not the target, of an unlikely branch**
  (`conditional_branch`). For a `br_if` hinted unlikely taken, upstream marks
  the branch's destination cold. That is the block after a Wasm `block`,
  which the likely path usually reaches too, by falling out of the block or
  by another branch; Cranelift then emits it, and with the pass below all
  code it alone leads to, at the end of the function. The branch now enters a
  cold block of its own that only jumps to the destination. The destination's
  parameters are `Variable`s defined before the branch, as for the direct
  edge, so the jump carries nothing; the new block has the branch as its only
  predecessor and is sealed at once. A hint that the branch is likely taken
  marks the fallthrough block cold, as upstream does. `if` is unchanged: each
  arm starts in a block only the `if` enters.
- **Hints at `br_on_null`, `br_on_non_null`, `br_on_cast` and
  `br_on_cast_fail`.** The proposal defines hints for `if` and `br_if`; the
  section's records name only a byte offset, so the same record is read at
  these four instructions with the same meaning (whether the branch is
  likely taken) and translated by the same function. A consumer that follows
  the proposal alone never looks up those offsets.
- **What an unlikely block leads to** (`cold_paths::spread`, last in
  `place_frame_loads`). Cranelift moves exactly the blocks that are marked.
  The rest of a slow path, the blocks after its first branch or call
  continuation, the loop of a growth path, the block where two unlikely
  edges meet, stayed between the likely code. After translation and the
  passes above, every block that no path of unmarked blocks reaches from the
  entry is marked too. A block that a likely path reaches is never marked,
  so a slow path ends where it rejoins. Exception edges count as edges: a
  handler that a likely `try_call` names stays where it is.

Two slow blocks of the translation itself are now marked: the
`get_interned_func_ref` libcall behind the inline interned funcref read
(`gc/copying.rs`), and the `is_subtype` libcall behind the type-id equality
test of `ref.test` and the casts (`gc.rs`). The allocation slow path, the
write barrier's recording blocks and traps were marked already.

Measured on one retained React Server Components request (sync, median of
97, Valgrind with the machine's first-level caches) together with the
emitter's hints: instructions 3,072,705 to 3,072,105, simulated instruction
fetch misses 150,155 to 144,126. The executed code of the largest compiled
body went from 434 separate runs on 1,182 cache lines to 310 on 1,089.

Costs: a branch that is hinted unlikely and is taken after all pays a jump
to the end of the function and one back, and its code is far from the code
around it. An unlikely `br_if` adds one block and one jump to the CLIF; the
backend emits the jump only when the block is not empty after register
allocation's edge moves, which otherwise went on the same edge. `spread` is
one traversal of the function's blocks, skipped when nothing is marked.

Tests: `func_environ::cold_paths::tests::` (no mark, the blocks of a slow
path and the join after it, a fallback that only cold edges reach, a block a
likely edge also reaches, loops inside and around a slow path), each verified
by Cranelift's verifier; `crates/scriptw-engine/tests/aot_gc_entries/aot_gc_translation_probes.rs`
compiles a hinted branch of each of the six kinds both ways on the product
configuration, under the Fast and Speed policies, directly and precompiled,
and finds the rare side behind the other in the native code, and the two
arms of an `if` nested in a rare arm with it.

## Cheaper passes after translation

`src/func_environ/{operands.rs,branch_threading.rs,gc/frame_reloads.rs,
gc/base_home.rs}`, `src/func_environ.rs`. These change how the passes that
`place_frame_loads` runs find what they look for, not what they do: every
instruction, value, block, stack slot and edge they create, change or remove,
and the order in which they do so, is unchanged, so the CLIF and the native
code are identical. On the React Server Components program those passes were
about 14% of native compilation under either policy, most of it walking
every value of every instruction several times.

- `operands`: `DataFlowGraph::inst_values` chains an instruction's arguments
  with the arguments of its branch destinations and its exception table's
  contexts. Only a terminator has either, so the walks below read a
  non-terminator's arguments directly, in the same order (asserted in debug
  builds).
- `branch_threading::thread` decided in each of its up to three rounds which
  blocks qualify by walking every value of the function, to find block
  parameters used outside their block. It now counts those uses once, by
  parameter, and keeps the counts current through the only instructions it
  changes, the terminators it rewrites (uses out before, in after). A block
  of the handled shape qualifies exactly when none of its parameters has
  such a use, which is what the walk found; the edges are still collected in
  layout order and threaded in the same sequence.
- `Reloads::scan` maps a `stack_addr` result to its slot with a
  `SecondaryMap` instead of a `HashMap` and a membership set, and `hoist`'s
  added events are kept by block in `SecondaryMap`s instead of `HashMap`s;
  nothing iterated either map in an order that reached the output.
- `Reloads::solve` no longer re-evaluates a block none of whose predecessors'
  ends changed since the block was last evaluated (a block that loops to
  itself counts its own change). Its end is a function of those ends alone,
  so re-evaluating it would give the end it has. Which ends change in which
  round, the number of rounds (and so the twelve-round limit) and the
  solution are those of evaluating every block every round.
- `FrameHome::of_parameter` and `Reloads::scan` read each instruction's
  values through `operands`.

Measured (Callgrind instructions, one process compiling the React Server
Components program, `rsc_baseline`): `place_frame_loads` went from 8.19 G
to 4.88 G under either policy (`thread` 2.32 G to 0.77 G, `Reloads::solve`
1.58 G to 0.74 G, `Reloads::scan` 1.56 G to 1.06 G, `of_parameter` 1.26 G
to 0.90 G).

Checked: with the Fast and Speed policies, the native images of the React
Server Components program and of the runtime compiled in-process
(`rsc_baseline`), and both runtime images that the runtime-image build
script produces, are byte for byte those built before, except for the native
compiler fingerprint inside Wasmtime's module version string.

Tests: the crate's unit tests, among them `func_environ::branch_threading::tests::`
and `func_environ::gc::frame_reloads::tests::` (which cover the home slots),
pass unchanged in a debug build.


## Alias regions capped per function (2026-11-22)

`src/alias_region.rs`: a function gets an alias region of its own for at most
the first `ALIAS_REGIONS_OF_THEIR_OWN` (256) keys it uses; keys first used
after that share the new `AliasRegionKey::Shared` region, both for the
translator's own regions (`AliasRegions::region`) and for the safepoint
spiller's stack-map slots (`stack_map_region`).

Why: Cranelift's alias analysis (`cranelift-codegen` 0.134
`alias_analysis.rs`, `compute_block_input_states`) clones a `LastStores`
table of every alias region at each block visit and meets it into each
successor, so its cost is the number of blocks times the number of regions.
Every spill slot and every defined global a function touches had its own
region, and a single body with thousands of blocks, spill slots and globals
spent its compilation there: Test262
`staging/sm/String/string-code-point-upper-lower-mapping.js` (1,200
top-level asserts, one 2.2 MB code body) took 178 s in the 2026-11-18
publication and timed out at 360 s afterwards; its Speed precompile ran past
900 s and 4 GB. gdb stacks sampled during that compile were all in
`compute_block_input_states` / `LastStores::meet_from`. With the cap it
precompiles in 19 s at 1.3 GB. A cap on stack slots alone (64) only brought it
to 461 s, since defined globals also number in the thousands there.

Why it is sound: two accesses in one region are only assumed to possibly
alias; that removes load forwarding between them, never adds any. A function
that uses at most 256 regions inserts exactly the keys it did before, so its
native code is unchanged. The React Server Components request is unchanged
(Callgrind, Speed: sync 2,713,498 -> 2,712,790.5, async 2,939,522.5 ->
2,939,548.5 Ir, both against the null-stack-map build, within noise; exact
Flight output on all observations).

## Conjunction proofs for `ref.test` (2026-11-22)

(The functions named here are now `gc::prove_condition`, which the next
section describes; this one's behavior is part of it.)

`gc::prove_tested` (`src/func_environ/gc.rs`), which records that an `if`'s
consequent may assume the latest `ref.test` succeeded, now also accepts a
condition that is an `i32.and` tree (at most 8 deep) with that test's result
as a leaf (`conjunction_contains`). A `ref.test` result is 0 or 1, so a
conjunction with it that is not zero has it at 1 whatever the other operands
are. Scriptw's emitter guards casts with `tag == X & ref.test T v`, and the
cast in the consequent was translated again; `FuncEnvironment::prove_tested`
now takes the data-flow graph, passed from `Operator::If`.

Sound for the same reasons as the existing direct case (see
`ProvenReferences`): the fact is recorded at the start of the consequent,
which control enters only when the condition is not zero, and is used only
while that arm is open.

React Server Components request (Callgrind, Speed, against the build with
the null-stack-map and alias-region patches): sync 2,712,790.5 -> 2,696,229
Ir (-0.61%), async 2,939,548.5 -> 2,921,929 (-0.60%); exact Flight output on
every observation. Engine aot_gc_entries, aot_language, aot_builtins,
aot_realm_modules and lib, and the CLI suite pass.

## Null tests, deferred `ref.as_non_null`, cast null checks in the guard and the type-id home (2026-11-22)

`src/func_environ{.rs,/gc.rs}`, `src/translate/{code_translator.rs,stack.rs}`,
`src/alias_region.rs`; tests in `src/compiler/translation_tests.rs`. Wasm
semantics and the code of every trap that an intact heap can raise are
unchanged; the `VMContext` layout, stack maps, calling conventions and target
settings are untouched.

- **Facts from null tests** (`gc::prove_condition`, which replaces
  `prove_tested` and `conjunction_contains`). The translator remembers which
  CLIF value is `ref.is_null r` of a nullable reference and which is
  `i32.eqz x`. Code that only a branch on a truth value enters with a known
  outcome learns what that outcome implies: the consequent of an `if` (the
  condition is not zero), and now also the alternative of an `if` and the
  path a `br_if` falls through to (the condition is zero). `ref.is_null r`
  zero means `r` is not null; `i32.eqz x` flips what is known of `x`; an
  `i32.and` (a `band`) that is not zero has every operand not zero, an
  `i32.or` (a `bor`) that is zero every operand zero (at most 8 operations
  deep); the latest `ref.test` not zero is the cast it tests, as before. So
  `br_if (ref.is_null x)`, `if (i32.eqz (ref.is_null x))`, the `else` of
  `if (ref.is_null x)`, `br_if (i32.or (ref.is_null x) (ref.is_null y))` and
  `br_if (i32.eqz (ref.test $t x))` all prove what follows them, and the
  `ref.as_non_null`, null checks of GC accesses and casts that the proofs
  imply emit nothing. The `if` frame keeps its condition for the `else`.
  Why it is correct: each step holds for every bit pattern; the `else`
  block's only predecessor is the `if`'s head and a `br_if`'s fall-through
  block's the `br_if`, so the fact holds at the start of that arm or path,
  and it is recorded and used under the scoping rules of `ProvenReferences`
  (innermost frame and arm), unchanged.
- **`ref.as_non_null` checked by the next access** (`gc::DeferredNullCheck`).
  Where an access can be its own null check (the copying heap's null guard,
  see "GC access code"), the check of `ref.as_non_null` of a nullable `any`
  hierarchy reference is deferred while only `local.get`, `local.set`,
  `local.tee` and `*.const` follow. The next `struct.get*`, `struct.set`,
  `array.len`, `array.get*` or `array.set` whose object operand is that value
  makes it as its own null check, with `TRAP_NULL_REFERENCE`, the code both
  had; an access whose check cannot be the access (a `funcref` store, a field
  beyond the guard) emits the explicit check first; any other operator first
  emits the check exactly as `ref.as_non_null` did, with its source location.
  Why it is correct: the operators in between cannot trap and have no effect
  that outlives a trap, which ends the activation with its locals and operand
  stack; each access makes its null check before anything else it does
  (`emit_null_check` comes first), and translation asserts that the access it
  was left to made it. Only the source location recorded for the trap moves,
  to the access, when the access is the check. Nothing is deferred when fuel
  is consumed or guest debugging keeps per-operator state, and the fact that
  the value is not null is recorded only where the check is made.
- **Casts: the `i31` test first, the null check in the header load**
  (`gc::translate_ref_cast`). In a trapping `ref.cast` to a non-nullable
  concrete type with a possibly null operand, when the header's
  type-id load lies within the null guard, the load carries
  `TRAP_CAST_FAILURE` and is the null check; the `i31` test (when the operand
  can be an `i31`) comes before it. Reference 0 has the `i31` bit clear,
  passes that test and faults in the guard, so a null operand still traps
  with `TRAP_CAST_FAILURE`; an `i31` traps at its test as before; nothing
  between the two can trap, and the trap code comes from the faulting
  instruction's trap record, which the runtime looks up whatever the
  address (`wasm_fault` accepts the guard, and an interrupt is recognized by
  the address it made inaccessible, never by the code). Scriptw maps no
  cast or null trap to a JavaScript value (only stack overflow and
  interrupts are distinguished), and both codes stay what they were. A
  heap without the guard keeps the explicit null check, in the old order.
  A non-null reference into a corrupt heap that faults on the header now
  reports a cast failure instead of `TRAP_GC_HEAP_CORRUPT`, as field
  accesses that are their own null check already report a null reference.
- **Statically non-null cast operands** (`gc::{translate_ref_test,
  translate_ref_cast}`). Static reference types and proven non-null facts
  bypass the null branch in tests and retain the heap-corruption trap code
  on the specialized cast's header load. Tests still distinguish `i31`
  operands before reading headers and pass their original target type to
  branch fact tracking.
- **The type-id array's base in a home slot** (`type_ids_array` in
  `func_environ.rs`, the `TypeIdsHome` alias region). Every type test,
  allocation, typed funcref read and `call_indirect` signature check read
  `[vmctx + type_ids_array]` at its use, and after a call that was the
  `vmctx`'s home load, then the base, then the id: three dependent loads.
  The base is now loaded once at the top of the entry block (`readonly`,
  `can_move`, as before) into a `FrameHome` as the GC heap base is, and
  `place_frame_loads` handles the third home like the other two: after a
  call one load from the home, then the id. A function without a call loses
  the home, its store and its slot, and reads the base from the `vmctx`
  value at hand as it did. Costs: a function that calls and uses type ids
  only on a rare path stores the base on every entry.

Tests: `compiler::translation_tests::` translate and compile hand-assembled
Wasm with this crate's `Compiler` on the product's GC heap settings and count
CLIF traps by code, loads carrying a trap code and loads per alias region:
facts after `br_if`, in each `if` arm, after the frame closes, from
disjunctions and not from conjunctions, and from `br_if (i32.eqz (ref.test))`;
the cast's two traps and faulting header load in the guard and its three
traps without it; deferral to a field read and its flushing by a call and by
an access through another reference, and no deferral without the guard; one
base load per function, one home load after a call and no home without a
call. With `SCRIPTW_TRANSLATION_DUMP=<dir>` they also write each function's
CLIF and machine code. The crate's 57 unit tests pass.

Native code (x86-64, `opt_level=speed`, from those dumps): a type test after
a call reads `mov rax,[rsp]` (the home) and compares the header's id with
`[rax]`; before, the same test read the `vmctx` home, then
`[vmctx+0x28]`, then the id (`mov rax,[rsp+0xb0]; mov rax,[rax+0x28];
cmp ecx,[rax+0x1e4]` in the React Server Components program's `run#f4`), one
dependent load more. The entry block now stores the base once
(`mov rcx,[rdi+0x28]; mov [rsp],rcx`). The cast of an `anyref` to a final
struct type is `test edx,1; jne <trap>`, the heap base, then the header id
load `mov eax,[r10+r11+4]`, which faults for reference 0, and the compare:
no null test. Not yet measured with Callgrind.
