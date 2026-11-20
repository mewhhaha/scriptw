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
