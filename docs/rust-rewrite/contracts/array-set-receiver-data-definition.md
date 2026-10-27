# Array Set receiver data definitions

OrdinarySet resolves inherited setters and read-only rejection before its data
receiver step. That step acquires the receiver’s own descriptor, rejecting an
accessor or nonwritable data property. Only then may it update an existing data
value or create a new complete writable/enumerable/configurable data property.

The private `SetReceiverDataDefinition` enum admits exactly those two forms.
Its consuming factory builds normalized data-only fields and validates them into
`WasmDescriptor`. No arbitrary accessor descriptor or unchecked callability
claim can enter this boundary. Present undefined remains a value, not absence;
an update carries no attribute changes, while creation carries all three true
attributes.

An actual `ArrayObject` consumes that witness through the existing Array
DefineOwnProperty owner. A Proxy wrapping an Array fails the concrete reference
test. Other receiver kinds keep their existing private descriptor carrier and
native Reflect.defineProperty bridge. General descriptor conversion, accessor
callability validation and public definition algorithms retain their owners.

This path performs no raw indexed publication and adds no second length
algorithm. ArraySetLength still performs both observable numeric conversions,
checks uint32 equality, reads the current length and writability after reentry,
and preserves shrink deletion order and rollback. A failed definition returns
Boolean false; the outer strict Set or throwing builtin owns its TypeError.
Original abrupt completions and executing-operation error realms are retained.

Internal Array descriptor acquisition uses a typed Array-only storage lookup.
It checks ordinary header properties first, then shares the existing Array
indexed/virtual-length tail with the public non-Proxy dispatcher. It does not
emit namespace, Arguments, boxed-string or typed-array dispatch beneath an
actual Array proof. The completed internal result remains a fresh mutable
descriptor snapshot with shared immutable value edges.

Strict/sloppy Wasm-AOT controls cover length callbacks changing storage and
writability, mismatched conversions, original throws and foreign errors,
distinct receivers, nonextensible existing/new properties, Symbol and named
attributes, shrink rollback, outer strict errors and the non-index uint32
boundary. Existing Array snapshot, inherited-setter and Proxy controls remain
part of the adjacent verification scope. The batch’s verification and matched
memory effects remain pending; no RSS reduction is claimed by this contract.
