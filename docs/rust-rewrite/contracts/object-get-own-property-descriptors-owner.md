# Object.getOwnPropertyDescriptors compiler owner

The 2026-10-05 GC source draft retains the private
`builtins/object/get_own_property_descriptors.rs` owner and its fixed dispatcher
entry. It completes ToObject, takes one complete own-key snapshot, then obtains
and publishes each surviving own descriptor in snapshot order.

The result is a fresh ordinary object with the executing Realm's Object
prototype. Per-key acquisition uses the typed ObjectGetOwnProperty runtime
helper, which preserves Proxy traps and whole abrupt completion. Absent private
descriptors are omitted; the public adapter alone materializes present snapshots.
Present fresh descriptor
objects are retained before CreateDataPropertyOrThrow updates the result.
String and Symbol keys retain identity and ordering. No public Object/Reflect
lookup, linear-memory carrier or tagged scalar pair participates.

The historical 2026-08-28 source-equivalent ownership checkpoint describes the
previous representation. It gives this rewrite no executable acceptance.
Existing semantic controls and the complete descriptor family are still
required. The obsolete spelling guard is retired; compilation and execution
wait for the entire dry source pass and the confirmed 4096 MiB aggregate cap.
Published conformance counts remain unchanged.
