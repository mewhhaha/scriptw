# References and updates

`scriptw-ir/src/reference.rs` owns a Reference's closed base, referenced name,
strictness and optional super receiver. Strictness is captured when the
Reference is created; later emission must not substitute its own context.

Evaluate an assignment target once. Reads borrow the resolved Reference;
writes consume it. Effectful base/key temporaries remain pinned until the
operation finishes. Compound assignments retain the old value before evaluating
the RHS. Logical assignments keep the RHS lazy and write only on the selected
branch. Numeric updates preserve ToNumeric, prefix/postfix result choice and
Number/BigInt identity.

A property Reference retains its original base and receiver, including super
and primitive receivers. Getters, setters, Proxy traps and failed writes use
the shared object operations and preserve the original completion. A failed
strict write throws; a sloppy write retains its specified nonthrowing result.

Suspension stores the resolved Reference, not instructions to resolve it again.
Await/yield, callbacks and coercions cannot change which binding or property a
resumed write targets. Deleting a super property retains the required receiver
and computed-key evaluation before its ReferenceError.

Environment References preserve TDZ and source mutability independently of
Reference strictness. Object Environment Records and with bindings retain the
actual binding object, required HasBinding/Get/Set observations and lazy outer
fallback; they do not re-resolve through a public global after source effects.

Ordinary module functions can carry a sealed proof of an existing owned or
captured cell. It binds the emitted function, source/storage names, slot and
capture ancestry. Proven reads and `typeof` use the same live cell and TDZ
operation directly; writes and calls retain held References. Retained cells keep
their allocation, aliases and unknown value facts. Dynamic environments, arguments cells,
module roots, ambiguous owners and resumable crossings retain ordinary lookup.

Private `var` bindings and simple parameters of admitted strict ordinary module
functions may instead use existing local storage, even when another function
observes arguments. Actual captures, arguments and synthetic bindings retain
cells. This promotion uses ordinary identifier value analysis; it is separate
from proving the identity of a retained cell. Unmapped arguments keep their
incoming snapshots, including shared object identities.
See [binding lifecycle](environment-record-tdz.md),
[property updates](property-reference-update-operation.md) and
[suspended References](suspended-call-argument-ownership.md).
