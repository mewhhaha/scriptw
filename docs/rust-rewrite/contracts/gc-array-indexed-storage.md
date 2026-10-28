# Array indexed storage

ArrayObject retains one ArrayIndexStorage and an independent unsigned logical
length. `gc_types/layouts.rs` closes its four modes: dense descriptors, sparse
descriptors, packed Numbers and packed general values. Length does not size a
hole allocation, and named properties remain separate from indexed occupancy.

Packed Numbers retain exact bits plus a presence bitmap. Packed general values
retain immutable StoredValue records in nullable slots: null is a hole, while
a stored Undefined is a present property. Packed entries have standard writable,
enumerable and configurable data attributes. Accessors and nondefault attributes
materialize real descriptors through the existing transition owner.

Sparse descriptor storage uses strong GC bucket chains with exact index words.
Mutation admits only valid Array indices, finishes each node before publication
and keeps occupancy consistent. Growth and shrinking rehash existing nodes
without losing their rooted next edges. Resource limits are allocation failures,
not invented JavaScript index restrictions.

OwnKeys returns occupied indices in ascending order. ArraySetLength deletes in
descending order and restores the first undeletable boundary, including the
requested final nonwritable policy. Deletion/truncation clears old strong edges;
holes and semantic length remain independent of physical backing capacity.

Prototype lookup, Proxy operations, nonextensibility, species creation and
partial-descriptor compatibility still use the shared object algorithms.
Fresh literal initialization owns an unpublished Array and preserves left-to-right
evaluation, complete abrupt checks and standard property attributes. Elisions
publish nothing, even when the prototype supplies that index; explicit undefined
remains an own property. Source effects cannot observe a partially initialized
literal or cause later elements to evaluate after a throw.

See [Array methods](native-array-gc-methods.md),
[packed storage](packed-array-storage.md) and
[descriptor rules](property-descriptor-lattice.md). The current private ABI is
layout 32; artifact admission, snapshots and field access derive from the same
schema rather than a historical list of raw offsets.
