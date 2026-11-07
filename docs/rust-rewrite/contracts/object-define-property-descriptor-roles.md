# Object.defineProperty descriptor roles

The native entry rejects a primitive target before ToPropertyKey and descriptor
conversion. It consumes the sole ToPropertyDescriptor owner, whose six field
presence bits and whole Value/get/set roots produce the validated borrowed
WasmDescriptor accepted by the shared DefineOwnProperty owner. Normal false
becomes a current-execution-Realm TypeError; the original whole Throw propagates
unchanged. Normal true returns the original target.

OrdinarySet receiver updates carry a validated value-only descriptor; creation
carries value plus true writable/enumerable/configurable flags. Actual ordinary
objects and Arrays consume those normalized fields directly through canonical
definition. Ordinary writes do not construct a JavaScript descriptor carrier or
call Reflect.defineProperty to decode it again. Exotic receivers retain their
definition protocol, including observable Proxy descriptor objects and partial
field presence. Refusal remains a Boolean for the original strictness owner.

Object.defineProperties retains these same converted partial descriptors in
private immutable GC records. Absent fields remain absent; conversion does not
complete the descriptor or expose a JavaScript carrier. Complete
PropertyDefinition records are written before the private list count advances.
The collecting owner is consumed into the completed owner only after all
observable conversion work. The apply phase accepts and consumes only that
completed owner, preventing interleaved conversion and target mutation.

The previous scalar TaggedLocals spelling guard is retired. Existing semantic
controls and new GC Object controls remain required. No compilation or runtime
verification occurred during this dry rewrite; final acceptance requires the
complete source pass and the confirmed 4096 MiB aggregate cgroup budget.
