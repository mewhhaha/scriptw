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

The same validated internal definition consumer handles CreateDataProperty,
compiler-created data/accessor definitions and trap-absent Proxy forwarding.
Concrete ordinary targets enter their canonical definition directly; data and
runtime-partial definitions also admit Arrays. Known accessors retain Array
dispatch in the fallback to avoid copying Array code into each source getter or
setter. Other targets materialize the private descriptor carrier and retain their
existing dispatch. Array length coercion, live length/writability and indexed publication
remain owned by ArrayDefineOwnProperty. Forwarded descriptors keep their existing
validation witness and runtime presence bits; public ToPropertyDescriptor
conversion remains observable.

Completed merges retain omitted getter/setter fields as their existing immutable
StoredValue edges. Only a supplied field allocates a replacement; a descriptor
kind change resets both edges to canonical undefined before applying fields.
The containing descriptor remains a fresh mutable record. Snapshot independence
and deferred PropertyValue identity do not depend on copying immutable wrappers.

Object.defineProperties retains these same converted partial descriptors in
private immutable GC records. Absent fields remain absent; conversion does not
complete the descriptor or expose a JavaScript carrier. Complete
PropertyDefinition records are written before the private list count advances.
The collecting owner is consumed into the completed owner only after all
observable conversion work. The apply phase accepts and consumes only that
completed owner, preventing interleaved conversion and target mutation.

Emission controls inspect Array/ordinary data/accessor branches and the retained
exotic fallback. Engine regressions cover inherited setters, value identity,
accessor merging, Proxy presence/order, length conversion/rollback, foreign error
realms, silent refusal and iterator closing.
Run them under the verification resource limits in CONTRIBUTING.md.
