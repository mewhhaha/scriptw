# Allocator scratch sets without whole-capacity clears

This directory vendors the crates.io `regalloc2` 0.15.2 release, retaining
its Apache-2.0 WITH LLVM-exception license. The original crate checksum is
`757712e8e61590d6d4f5d563483755538b5aa13467837a3b41cd9832509a7f85`.
Its recorded upstream commit is
`2fe490bc9dda433f70c54f90f7633ed929f693d9`.
The workspace selects this copy through `[patch.crates-io]`; dependency
versions, features, target/ISA settings and generated machine-code policy are
unchanged. The registry installation marker was omitted; all other release
files, including the license's trailing blank line and the crate's own
`.gitignore` (which would ignore its `Cargo.lock`, added regardless), are kept
byte for byte.

Cranelift uses this crate's backtracking allocator (`ion`) under Scriptw's
Speed policy; the single-pass allocator (`fastalloc`) of the Fast policy is
untouched. Two of `ion`'s scratch sets were hash tables that are cleared far
more often than they are filled. `hashbrown`'s `clear` writes every control
byte of the table's capacity, and a reused table keeps the capacity of its
largest use:

- `src/ion/moves.rs`: `apply_allocations_and_insert_moves` keeps, for the
  virtual register it is processing, its allocation at the end of each block
  that its live ranges leave (`inter_block_sources`), and cleared that map
  once per used virtual register. The register live in the most blocks
  sized the map for every later one. It is now a vector indexed by block,
  with the blocks that have an entry listed so that only those are reset
  before the next register. Lookups and the occupied/vacant update are
  unchanged, and a lookup of a block without an entry still panics.
- `src/ion/data_structures.rs`: `try_to_allocate_bundle_to_reg`
  (`src/ion/process.rs`, unchanged) records which bundles it has already
  listed as conflicts (`conflict_set`), cleared on every call, which is once
  per register probed for a bundle. It is now a `ConflictSet`: a stamp per
  bundle index, a member when its stamp equals the current generation, so
  clearing is one increment (with a full reset on the rare wraparound).
  Insertion still reports whether the bundle was new.

Why the output is unchanged: both structures were only ever used for
membership, insertion and lookup by key, never iterated, so no hash order
could reach the output. The replacements answer each of those operations as
the hash tables did, in the same sequence, so every allocation decision,
inserted move, spill slot and the `Output` are identical.

Checked: with the Fast and Speed policies, the React Server Components
program's native image and the runtime image compiled in-process
(`rsc_baseline`, `SCRIPTW_RSC_ARTIFACT_DIR`), and both runtime images that
the runtime-image build script produces, are byte for byte those built with
the unmodified crate, except for the 64-character native compiler
fingerprint inside Wasmtime's module version string, which hashes the
vendored sources. A differential harness (outside the tree) generated 18,453
random functions with the crate's own `fuzzing` generator (fixed and reused
registers, clobbers, reference types, call-site constraints; 1 to 200
blocks), allocated each with this copy and with the crates.io release, each
reusing one `Ctx` throughout, and found every `Output` equal; the crate's
checker accepted a sample of them.

Measured (Callgrind instructions, one process compiling the React Server
Components program with the Speed policy, `rsc_baseline`): the allocator
went from 15.24 G to 13.53 G of the program's 57.11 G native compilation;
the map's clears alone had written 1.16 G instructions of `memset`.

Tests: the crate's own unit tests pass unchanged.

## Backtracking split budget

`src/ion/data_structures.rs` lets backtracking try four splits per spill set
before `split_and_requeue_bundle` falls back to minimal bundles. The limit
only guards an increment of the spill set's `u8` counter.
