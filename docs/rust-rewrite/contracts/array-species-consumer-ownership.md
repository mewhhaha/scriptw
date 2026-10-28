# Array species consumer ownership

The dry implementation on 2026-10-03 removes the final ArraySpeciesCreate
copies from `Array.prototype.flat` and `concat`. Both now call the existing
shared emitter, alongside map, filter, flatMap, slice and splice. That emitter
owns Proxy-aware IsArray, constructor Get, cross-Realm intrinsic handling,
object-valued @@species Get, null/undefined defaults, IsConstructor rejection,
the current-function-Realm default Array, and Function/Proxy Construct. It
returns the actual Object result rather than assuming an Array.

The old two copies restricted constructor carriers and selected species by
outer Object/Function tags. They used Function-only construction. Flat also
performed a later Proxy-array constructor Get whose result did not select
the target. The migration deletes that duplicate observation and the
constructor/argv scratch locals used only by those copies. Shared emitter
instructions and the other five callers are unchanged.

This change has one Array @@species read authority. Three separate TypedArray
reads remain because TypedArraySpeciesCreate has different default and result
validation requirements. No new value representation, operation catalog row,
interpreter, or compatibility path is added.

The authored semantic sources cover Array-valued constructor carriers, Proxy
species constructors and original newTarget, ordinary Object results,
once-only Proxy-receiver constructor/species lookup, original abrupt values,
nonconstructor rejection, and skipped source getters after a failed creation.
Existing structural assertions are maintained for the real call census and
Concat's conversion/species/spreadability ordering. They are not execution
evidence. Compilation, emitted-Wasm validation and all fixtures are pending.
The eventual focused target is `scriptw-engine --test aot_builtins -- aot_array_species_consumers::`.

The later [forward Flat traversal](native-array-gc-methods.md) source batch
replaces depth conversion, root-length ordering and flattening traversal while
retaining this shared species operation. That complete replacement is integrated
and independently source-reviewed, with compilation and execution pending.
Full Array/TypedArray and cross-Realm conformance, task acceptance and published
suite counts remain unchanged.

Primary specifications: [ArraySpeciesCreate](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-arrayspeciescreate),
[Flat](https://tc39.es/ecma262/2026/multipage/indexed-collections.html#sec-array.prototype.flat),
and [Concat](https://tc39.es/ecma262/2026/multipage/indexed-collections.html#sec-array.prototype.concat).
