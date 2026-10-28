# Array quantifier entry boundary

## Current generic-loop boundary (2026-09-06)

The generic Array and strict TypedArray entry families remain disjoint. The two
Array wrappers now select `ArrayCallbackIterationKind::{Every, Some}` in the
shared [callback iteration compiler](../aot-array-callback-iteration.md). That
kind selects only result policy, never a receiver-validation mode. Borrowed
TypedArray HasProperty/Get now delegate to the common property operations; the
generic loop no longer owns a private view or witness cache. Quantifier result
policy emits no species operation or result allocation. Its wrappers reserve no
temporaries; the shared compiler's complete lifecycle has its own LIFO guard.

The updated entry-family guard pins this delegation, bans strict TypedArray entry
validation in the generic loop, and retains the exact separate TypedArray
producer/consumer checks.

## Historical boundary and residue checkpoints

The old per-wrapper witness and 28-local census below describe the implementation
before shared callback iteration, not the current contract.

Status: implemented and focused-verified for the Wasm-AOT
`Array.prototype.every` and `some` entry-family separation, 2026-08-26. The
2026-08-29 Array-result residue cleanup is implemented with verification
pending.

## Boundary

`Array.prototype.every` and `Array.prototype.some` are generic methods. They
perform the ordinary Array-like receiver and length observations even when
borrowed by a TypedArray. The distinct `%TypedArray%.prototype.every` and
`some` builtins instead validate a TypedArray receiver and capture their entry
length through the closed `TypedArrayQuantifierKind::{Every, Some}` compiler.

Those entry policies are separate compiler families:

- `ArrayPrototypeEvery` owns `compile_array_prototype_every_builtin`;
- `ArrayPrototypeSome` owns `compile_array_prototype_some_builtin`;
- `TypedArrayPrototypeEvery` selects `TypedArrayQuantifierKind::Every`; and
- `TypedArrayPrototypeSome` selects `TypedArrayQuantifierKind::Some`.

The generic compilers take no receiver-policy argument. There is only one
valid generic entry state, so a Boolean or enum would make an unreachable
strict TypedArray state representable. The dispatcher call shape is the proof
that an Array entry cannot select TypedArray method-entry validation.

## Preserved borrowed-TypedArray observation

Removing the unreachable strict entry mode does not remove TypedArray support
from the generic methods. Each generic compiler retains its immutable
`TypedArrayViewLocals` and exactly one live
`TypedArrayWitnessUse::IntegerIndexedProperty` projection in the indexed
presence path. That fresh witness preserves generic Array behavior across
detachment, resize and fixed-view out-of-bounds transitions.

Neither generic body may contain a `ValidatedMethodEntry` projection, a strict
TypedArray receiver-brand error, `typed_brand_local` or `typed_array_only`.
Those authorities belong only to the canonical TypedArray quantifier compiler
described by
[`gc-typed-array-native-methods.md`](gc-typed-array-native-methods.md).

## Producer and consumer census

The entry boundary has four public producers:

- two generic Array producers, one for `every` and one for `some`; and
- two strict TypedArray producers, one for each exhaustive quantifier kind.

The retired Boolean had only the two literal-`false` Array producers and no
`true` producer. Its four projections in each generic compiler selected the
method name, callback diagnostic, strict entry validation and indexed presence
policy. Deleting those eight projections also deletes one dead
`typed_brand_local` lifecycle and one dead `ValidatedMethodEntry` use from each
generic body. The two live `IntegerIndexedProperty` consumers remain, one per
generic compiler.

## Boolean result ownership

Every and Some return Booleans. Their generic compilers therefore have no
output Array, target object or species-constructor state. The old bodies still
emitted two kinds of copied Array-producing work: a complete constructor and
`Symbol.species` path below a literal-false Wasm branch, and a later
runtime-executed zero-length Array allocation whose result had no live consumer.

The cleanup deletes both paths. It also removes the target, output-index,
flattening, child and declaration-only TypedArray temporaries that existed only
for copied Array-producing code. Each compiler now reserves 28 top-level
temporaries instead of 50. The five locals that represent a borrowed TypedArray
view remain because generic Every and Some still need fresh integer-indexed
witnesses.

## Durable regression

`crates/scriptw-aot-wasm/tests/typed_array_quantifier_family_witness_structure.rs`
bounds the generic `every` body through `some` and the generic `some` body
through `filter`. Its entry-family regression requires:

- no policy Boolean, dead brand local, strict receiver diagnostic or
  `ValidatedMethodEntry` in either generic body;
- no Array constructor lookup, species read, result allocation, construction
  call or copied result local in either generic body;
- exactly 28 top-level reservations with reverse-order release in each body;
- exactly one `IntegerIndexedProperty` witness in each generic body;
- exactly one argument-free dispatcher producer for each generic compiler;
  and
- the existing exact TypedArray-to-`TypedArrayQuantifierKind` mappings to
  remain unchanged.

These are source-structure mutation guards. Runtime evidence remains necessary
for ordinary Arrays, generic TypedArray borrowing and the canonical strict
TypedArray entries.

## Focused evidence

The focused CLI witnesses are:

- `wasm_array_every_core.js` and `wasm_array_some_core.js` for ordinary and
  generic Array-like behavior, callback validation and short-circuit polarity;
- `wasm_array_every_resizable_typedarray.js` and
  `wasm_array_some_resizable_typedarray.js` for generic borrowing across
  backing-buffer resize;
- `wasm_array_some_sparse_accessor_index.js` for the ordinary sparse/accessor
  presence path;
- `wasm_array_quantifiers_ignore_constructor_species.js` records that Every and
  Some ignore throwing constructor and `Symbol.species` getters; and
- `wasm_typedarray_every_some.js` for the separate strict entry family,
  callable Proxy callbacks, private-slot spoofs and live TypedArray reads.

The bounded structure target passes `4/4`. The two Array `every` fixtures pass
`2/2`, the three Array `some` fixtures pass `3/3`, and the strict TypedArray
fixture passes `1/1`, for all six focused CLI witnesses green. The focused
build emitted pre-existing warnings; no warning was introduced or repaired by
this boundary cleanup.

The two bounded structure targets pass `10/10`. The new constructor/species
non-observation fixture and four neighboring Array/TypedArray core and
resizable-buffer controls pass `5/5`. The fixture documents the required
runtime semantics, but it does not prove the cleanup: the old constant-false
Wasm branch also skipped both getters. The source guard is the evidence that
the compiler no longer emits the branch or the unused allocation.

`cargo xc` is green. The following workspace semantic golden passes `2/2` in
771.49 seconds with 669 dumps, adds only the independent Temporal arithmetic
witness, removes none, and leaves 667 of 668 retained dumps equal after
accounting normalization. The sole retained structural change is the
independent Promise callback Realm witness.

The entry-boundary cleanup changed no JavaScript result and added no fixture or
Test262 inventory entry. The later result cleanup adds one runtime control and
removes a runtime heap allocation that could only consume memory or trap. It
does not change a successful ECMAScript result. Wasm dumps that root generic
Array quantifiers have fewer temporary locals, downstream local renumbering and
smaller emitted bodies. Roots, builtin ownership, imports, exports, globals,
memories, data segments and names are otherwise expected to remain stable.

## Nonclaims

This boundary does not change callback evaluation order, `thisArg`, argument
construction, Proxy-aware invocation, truthiness, short-circuit polarity,
Array-like length observation, integer-indexed exotic semantics or the shared
TypedArray witness implementation. It removes no Test262 materializer, claims
no new conformance pass and does not close either T16 or T17.
