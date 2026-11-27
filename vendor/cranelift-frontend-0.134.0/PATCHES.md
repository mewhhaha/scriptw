# Dense safepoint liveness

This directory vendors the crates.io `cranelift-frontend` 0.134.0 release,
retaining its Apache-2.0 WITH LLVM-exception license. The original crate
checksum is
`61325401051d35ddbf8e2b5d5b5dec8dd54688c9054046b62e76146e61c88a99`.
Its recorded upstream commit is
`39d67168cdb6911ffdc961381314b6fed62e3c93`, path `cranelift/frontend`.
The workspace selects this copy through `[patch.crates-io]`; dependency
versions, features, target/ISA settings and generated machine-code policy are
unchanged. The registry installation marker was omitted; all other release
files, including the license's trailing blank line, are kept byte for byte.

`src/frontend/safepoints.rs`, `src/lib.rs`: `FunctionBuilder::finalize` runs
the safepoint spiller on every function that declares a needs-stack-map value.
Upstream keeps its liveness sets and maps in `std` hash collections with
SipHash. In Scriptw's GC-heavy programs, which have many reference-typed
values and many calls, that hashing was most of the pass and the pass about
40% of native compilation. The pass now uses these structures:

- The needs-stack-map values are numbered densely in ascending `Value` order
  (`DenseValues`). Each block's live-in and live-out sets are sorted vectors
  of those numbers, merged instead of hashed; the set that the backwards walks
  update instruction by instruction is a sparse set (constant-time insertion,
  removal and clearing).
- One backwards walk per block first summarizes it as the values it defines
  and those it uses before defining them, so the fixed-point computes
  `live_in = (live_out - defs) + upward-exposed uses` with sorted merges
  instead of re-walking the block's instructions on every visit. The
  composition of the per-instruction steps is exactly that form, the worklist
  order and change test are unchanged, and the result is the same least
  fixed-point. The final pass that records each safepoint's live set still
  walks the instructions, and still asserts in debug builds that it reproduces
  each block's live-in set.
- The worklist's pending marks are a vector of booleans by post-order index,
  the safepoint live sets one flat vector indexed through a
  `SecondaryMap<Inst, _>`, and the value-to-stack-slot map a
  `SecondaryMap<Value, PackedOption<StackSlot>>`.

Why the output is unchanged: upstream already never let hash iteration order
reach its output (its maps use per-process random seeds). It sorted each
safepoint's live values before adding stack map entries, and sorted each
block's live-out values before assigning stack slots eagerly. Both are still
sorted, by dense number, which is the order of `Value`s. Every other use was a
membership test, an insertion, a removal or a union. Stack map entries, stack
slot numbering and reuse, spill and reload placement, and the order in which
instructions and values are created are therefore identical, and so is the
native code.

Checked: with the Fast and Speed policies, the React Server Components
program's native image (4.45 MB of Wasm) and the runtime image compiled
in-process (`rsc_baseline`, `SCRIPTW_RSC_ARTIFACT_DIR`), and both runtime
images that the runtime-image build script embeds (decoded), are byte for byte
those built with the unmodified crate, except for the 64-character native
compiler fingerprint inside Wasmtime's module version string, which hashes the
vendored sources. For seven further programs (`target/rsc-fixtures/edge-*.js`
and `web.js` through `scriptw run`), every per-function entry of the Cranelift
function cache has the same bytes. With a build whose fingerprint happened to
match, the entries' keys, which hash the final CLIF, matched as well.

Measured (Callgrind instructions, one process compiling the React Server
Components program with the Fast policy, `rsc_baseline`): the safepoint pass
went from 28.76 G to 7.15 G, Wasm-to-CLIF translation from 45.01 G to 23.29 G,
and the program's whole native compilation from 71.30 G to 49.68 G (-30%);
code generation is unchanged at 26.2 G. Peak resident memory of that process
under Valgrind went from 599 MB to 555 MB. For `scriptw run` of
`edge-json-api.js`, native compilation went from 4.30 G to 3.34 G.

Memory: live sets take four bytes per live value per block where upstream's
hash sets took several times that; the sparse set takes eight bytes per
needs-stack-map value. A first version with dense bitset rows (one bit per
needs-stack-map value per block) was slower and used more memory on the
programs measured, whose live sets are sparse (60.03 G instead of 49.68 G for
the compilation above, and 904 MB peak memory). The per-block vectors keep
their allocations for the next function; upstream kept only the outer vector.

Tests: the crate's own `frontend::safepoints::tests::` (upstream's expected
CLIF, stack maps and slot assignments) pass unchanged.

## Instruction values without the terminator chain

`src/frontend/safepoints.rs`: the spiller's three walks over every
instruction (block summaries, the final liveness walk and the rewrite) read
an instruction's values through `DataFlowGraph::inst_values`, which chains
its arguments with the arguments of its branch destinations and the
contexts of its exception table. Only a terminator has either. The walks
now read a non-terminator's arguments directly (`for_each_inst_use`; the
rewrite copies them as a slice) and keep `inst_values` for terminators, so
every walk sees the same values in the same order (the rewrite overwrites
them in that order), and the output is unchanged; debug builds assert that a
non-terminator has neither part. Measured as above, the spiller went from
6.70 G to 5.55 G under either policy, with the same native images (checked
as above, for both policies).

# Sparse variable definitions

`src/ssa.rs`: `SSABuilder` kept the last definition of every variable in
every block as a dense map per variable, sized by the highest block that
defines it (upstream's own TODO: "Consider a sparse representation rather
than SecondaryMap-of-SecondaryMap"). Wasmtime defines each Wasm local in the
entry block and wherever it is written, so a large function whose many
locals are each written in a few late blocks cost locals x blocks: measured
with massif, 79% of a 309 MB native compilation of a 500-term top-level `+`
chain, and a 2,000-term chain took 2.6 GB. Each variable now keeps whichever
form is smaller (`VariableBlocks`): a dense map while its highest defined
block is at most 8 times its number of definitions, otherwise a hash map
from block to value, switching back once its definitions fill half the
range. Memory is linear in the number of definitions. A variable defined in
most blocks keeps the dense map, so the upstream fast path is kept where it
was cheap; a hash-only first version made a 1,000-link member chain slower
(6.5 s to 14.5 s) and larger (2.2 to 3.3 GB).

`find_var` also cleared its cycle-detection set once per walk, which rewrote
the set up to the highest block it had held; it is now a generation stamp
per block (`VisitedBlocks`), cleared by one increment.

Why the output is unchanged: neither structure is iterated. Every lookup and
definition is the one the dense map answered, in the same order, so every
SSA value, block parameter and instruction is created as before. Checked
with an out-of-tree harness that compiles Wasm with Scriptw's Wasmtime
feature set and a fixed module version, built once with the original crate
and once with this one: see the results below.

Checked (2026-11-20, together with the frame-reload budget in the vendored
`wasmtime-internal-cranelift`, each Wasm compiled with the Fast and the Speed
policy in one process under a 4 GiB cgroup): the serialized native code is
byte for byte that of the original crates for the runtime image R (39.3 MB of
Wasm), the `client.js` (21.3 MB), `ssr.js` (6.9 MB), `edge-json-api.js` and
`web.js` programs of `target/rsc-fixtures`, the hello fixture and a 250-term
`+` chain. Peak memory fell from 1,266 to 707 MB for `client.js`, 720 to 300
MB for `ssr.js` and 706 to 564 MB for R; times are unchanged (139 s and 141 s
for `client.js`). The React Server Components program itself
(`target/rsc-fixtures/rsc.js`) could not be emitted within the 4 GiB cap by
either compiler, so it was not compared.

## Null reference constants stay out of stack maps (2026-11-21)

`FunctionBuilder::finalize` now removes from the needs-stack-map set every
value whose alias-resolved definition is `iconst 0`
(`exclude_null_constants_from_stack_maps` in `src/frontend.rs`) before the
safepoint spiller runs. In Wasmtime's GC heap a null reference is the
constant zero and a reference that is not null is never zero, and the
collector neither traces nor moves a null, so a stack slot, its store after
the definition, a stack-map entry at each safepoint and a reload before each
use buy nothing. Lowering rematerializes the constant at its uses instead.
Only values that are themselves the constant are removed: block parameters
and any other value that may hold a null at run time keep their slots.

This changes native code by design (fewer stores, slots, stack-map entries
and reloads), unlike the SSA patch above. Tests: the new
`null_reference_constants_are_left_out_of_stack_maps`, and the two upstream
tests that used `iconst 0` as a stand-in reference
(`needs_stack_map_simple`, `series_of_non_overlapping_live_ranges_needs_stack_map`)
now use 4; all 64 crate tests pass (run out of tree: the crate is not a
workspace member).

## Root stores on cold call edges

`src/frontend/safepoints.rs` uses the liveness pass's CFG to recognize a
single-entry cold arm and a call-free fast path to a common join. A root
defined after the source's last safepoint can spill in the cold arm if it
dies in the diamond and no join safepoint needs it. The fast edge passes
its original SSA reference when the join uses the root. The cold edge
passes the value reloaded after the safepoint. The existing spill path
covers extra entries, live roots at the join's safepoints and exceptional
call edges.

## Joins of one repeated constant need no block parameter (2026-11-22)

`src/ssa.rs`, `finish_predecessors_lookup`: upstream removes a would-be
block parameter only when every predecessor passes the same `Value` (after
alias resolution, ignoring the parameter itself on cycles). Wasmtime's
translator materializes each Wasm `i32.const 0`, `i64.const 0` or
`ref.null` with its own `iconst`, so a local written with the same constant
on two paths met as a block parameter; the egraph later proved it constant
but cannot rematerialize a block parameter, so the zero was copied into its
parameter slot on every edge and reloaded at its uses (about 29K loads and
17K stores per request on two functions of the React Server Components
program; target/deepdive/frames/REPORT.md, fix 1).

When the predecessors disagree on the `Value` but every one other than the
parameter itself is defined by the same constant (`RepeatedConstant`: the
same opcode among `iconst`, `f32const`, `f64const` and `vconst`, the same
type, and the same immediate bits or vector-pool contents), the parameter
is now removed as upstream removes it, and aliased to a fresh copy of that
constant at the block's first insertion point. The fresh copy is needed
because no predecessor's constant need dominate the block; the block's top
dominates every use of the parameter, exactly as for the zero that upstream
emits there for a variable without a definition, and like that zero the
block is reported in `instructions_added_to_blocks`.

Why it is exact: on every path into the block the variable holds that
constant, or the parameter's own value on a cycle, so the parameter always
equals the constant. Floating-point constants are compared by their bits
(+0.0 and -0.0, or two NaN payloads, stay distinct). For a needs-stack-map
variable only the null reference (`iconst 0`) is folded: the collector never
traces or moves a null, so the fresh constant needs no stack slot and is not
a needs-stack-map value (the parameter that becomes its alias was one, and
`exclude_null_constants_from_stack_maps` would drop it anyway). A non-null
constant of such a variable keeps its parameter, because a moving collector
may give the parameter a different value than the constant.

This changes CLIF and native code by design (fewer block parameters, branch
arguments, edge copies and slot reloads). Tests: `join_of_one_integer_constant_is_a_fresh_constant`
and `join_of_one_float_or_vector_constant_is_a_fresh_constant` (both
sealing orders), `join_of_different_constants_keeps_its_parameter` (different
immediates, +0.0 against -0.0, equal bits from different opcodes, a constant
against a computed value), `stack_map_variable_folds_only_the_null_constant`,
`loop_header_of_equal_entry_and_back_edge_constants_is_a_fresh_constant`
and `join_with_a_cycle_back_to_itself_ignores_the_sentinel`; the five that
expect a fold fail without the patch. All 70 crate tests pass, run out of
tree as before: copy the crate to a scratch directory, append `[workspace]`
to its `Cargo.toml` and run `cargo test --offline`.
