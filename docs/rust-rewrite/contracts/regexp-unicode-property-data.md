# First-use RegExp Unicode property data

R retains the complete Unicode property catalogue as passive data. Its immutable
virtual addresses, alias rows, code-point ranges and string-sequence pointers are
unchanged. The sole runtime property lookup owner initializes the exact reserved
range before reading the table. No parser row ABI or second representation is
introduced.

Each actual R instance starts with an independent false readiness global. The
first lookup copies the complete range with memory.init, then publishes ready.
There is no allocation, coercion or JavaScript call between copy and publication.
Later lookups reuse the initialized image. The segment is retained; normalization
has its own segment and readiness, so either initialization order remains valid.

R's physical data order is passive UTF-16, normalization and property catalogue,
followed by three active fragments covering the complement of both cold ranges.
No active zero-filled placeholders overwrite the deferred ranges. Memory extent
and all virtual offsets stay intact. P's four passive data roles are unchanged.
DataCount derives from the actual emitted segments.

The immutable Cargo receipt binds all six physical payload spans with checked
lengths and individual hashes. Reconstructing the canonical linear image uses
physical span order [3, 2, 4, 1, 5], while UTF-16 remains separate. Exact total
length and every span digest remain required. Receipt magic SCRWRM02 and runtime
layout 27 record the changed private data contract; native images and consumers
must agree on the actual runtime identity.

Prepared controls cover source-owned spans and receipt mutations, init-before-
ready/no-drop emission, computed property aliases and complements, u/v modes,
string properties, syntax failures, recompile, GC, independent Stores and both
normalization/property initialization orders.

Whole-batch compilation, semantic verification and matched measurements remain
pending. Passive storage does not establish reduced native-map residency: native
admission may still read the bytes. No RSS or execution-time saving is claimed.
