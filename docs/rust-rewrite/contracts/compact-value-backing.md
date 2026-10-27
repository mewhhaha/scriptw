# Compact ordinary data backing

Runtime layout 25 keeps ordinary Array values with the same JavaScript object
identity and definition algorithms while specializing their occupied data slots.
The closed indexed storage modes are packed Numbers, packed general values,
dense full descriptors and sparse full descriptors. The Number representation
retains exact bits and a separate presence bitmap.

Packed general values retain immutable StoredValue records in nullable slots.
A null slot is a hole; an occupied StoredValue tagged Undefined is a present
property. Every packed slot has writable, enumerable and configurable data
attributes. The existing definition validation still runs before publication.
Accessors, nondefault attributes and consumers requiring mutable descriptors
materialize full descriptors through the existing one-way transition. Sparse
indices retain the existing sparse representation and capacity limits.

Deletion and length truncation clear reference edges, preserve occupied counts
and keep observable Array length separate from physical capacity. Same-receiver
fast writes require a present own slot. A missing slot or distinct receiver
still follows prototype and receiver definition semantics. Nonconfigurable
length-shrink rollback uses the descriptor representation.

Compact ordinary object literals retain separate tags, optional scalar bits
and optional reference backing. Null reference backing means every slot has a
null reference payload. The tag still distinguishes present Undefined, Null,
Boolean and Number values. The first nonnull reference write allocates backing
with exactly the shape extent. Once allocated, null writes clear old strong
edges. Transition reads and diagnostic snapshots use the same absent-backing
meaning.

GC fields encode reference backing nullability and publication mutability.
The runtime layout identity changes with these field/type changes; emitted
programs and trusted native images must match their actual runtime identity.
Diagnostic snapshots recognize every closed mode, validate active/inactive
backings, occupied counts, index bounds and stored-value tags, and synthesize
ordinary data descriptors without changing the observed program.

Intrinsic method installation shares a private descriptor-publication helper.
Callers retain the deferred recipe across aliases and pass its immutable value
edge to each installation. Each call allocates a distinct mutable data
descriptor, normalizes its private attribute booleans and invokes the existing
append algorithm. Recipe creation, property order and Realm ownership stay with
the existing caller. Focused controls inspect the emitted shared call and cover
alias identity, independent descriptor mutation, GC and cross-Realm behavior.
Code-size and residency effects require measurement.

Implementation, focused controls and documentation are being prepared together
in an isolated worktree. Compilation, semantic verification and matched RSS
measurements remain pending. No memory saving is claimed by this contract.
