# JSON.stringify preparation and Get ownership

The dry implementation on 2026-10-03 repairs the two existing shared JSON
preparation owners in `builtins/json.rs`. These owners are consumed by the
actual stringify builtin and its six root/Array/Proxy-Array/object property
paths. No new value representation or invocation authority is introduced.

Replacer preparation checks IsCallable before IsArray. A callable Proxy
remains the selected replacer even after revocation; the root's toJSON step
precedes the attempted replacer Call. Noncallable Proxy-array revocation
still throws during IsArray. An Array replacer reads length once, converts
that retained value with ToLength, and uses the ordinary Proxy-aware Get
owner for each increasing index. The previous direct Array index reader
only examined own elements and missed inherited values/getters.

The existing normalization retains the original replacer and current item
across hooks, converts accepted Number/String wrappers with a String hint,
then compares resulting strings and appends only their first occurrence.
The final private property list replaces the replacer carrier after the
whole loop. Length changes during a getter do not change the iteration
bound, and no normalized entry invokes the original wrapper again.

The shared toJSON owner uses the existing Object-kind predicate, covering
ordinary Object, Array, Function and Arguments tags, together with the
existing immediate/heap BigInt route. Every non-BigInt lookup uses the
shared property-read owner with the exact value as receiver. The former
Array named-data scan omitted accessors and inheritance. The existing
primitive BigInt prototype/Realm route is unchanged. Getter/call failure
propagates the actual thrown value before replacer Call; successful toJSON
precedes the existing replacer call and subsequent wrapper conversion.

The authored Engine target `aot_json_stringify_preparation` checks exact
ordinary/strict traces for inherited indexed getters, Proxy length and Get
order, hook conversion and duplicate removal, mutation after the length
snapshot, all four Object representation tags, inherited Array toJSON,
nested callable values, receiver/key identity, and abrupt hooks. These
sources are pending execution. Compilation, emitted-Wasm validation,
focused regressions and broad JSON acceptance remain pending; no suite
counts or task acceptance are changed by this source packet.

Stringify and the canonical parse-reviver walk share the same enumerable-key
snapshot owner. It snapshots own keys first, skips Symbols, then reads live
presence/enumerability in order through `emit_own_property_enumerability`.
Ordinary compact, intrinsic and dictionary storage supply these scalars without
materializing a descriptor or reading its value/getter/setter fields. Exotic
objects retain their descriptor traps, invariant checks and original thrown
values. The complete enumerable String-key list exists before any descendant
getter or reviver runs; descendant reads still observe current property values.
Emission regressions cover both JSON owners, and exact execution traces cover
Proxy descriptor mutations, vanished keys, Symbol omission, callback mutation,
fresh reviver descendants and abrupt descriptor traps.

Primary specifications: [JSON.stringify](https://tc39.es/ecma262/2026/multipage/structured-data.html#sec-json.stringify)
and [SerializeJSONProperty](https://tc39.es/ecma262/2026/multipage/structured-data.html#sec-serializejsonproperty).
