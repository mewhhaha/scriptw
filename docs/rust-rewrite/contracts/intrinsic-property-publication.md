# Shared intrinsic property publication

Intrinsic installers retain target/key evaluation, eager callable creation,
defining-Realm ownership and installation order. Two private Wasm helpers own
complete descriptor construction and the existing ordinary-property append
algorithm. Neither helper performs a JavaScript call, a public property lookup,
descriptor validation for arbitrary user input, or deferred function resolution.

The data helper accepts an ordinary header, actual String/Symbol key, immutable
PropertyValue and three separate attribute booleans. Eager installation captures
one StoredValue before the helper call; deferred method aliases retain their
existing shared recipe. All booleans normalize before creating the closed Data
flag set. Every call allocates a distinct mutable descriptor, with canonical
undefined getter/setter edges.

The accessor helper accepts a header, key, two StoredValue edges and enumerable/
configurable booleans. The GC object owner projects the nonempty getter/setter
role witness and captures each supplied value in the original order. Missing
slots use canonical undefined. Present undefined remains legal, including
RegExp legacy statics; the role witness is not a FunctionObject proof. Getter
and setter callability/materialization remain with their original caller.
The helper constructs one fresh accessor descriptor with canonical undefined
value, then uses the same append/storage owner as the data helper.

The context-free helper registry, exhaustive body routing and module assembly
register both physical owners. Their bodies use raw typed descriptor construction
and OrdinaryPropertyAppend, never the installer facade that calls them. This
prevents code-generation recursion. Function/prototype caches and eager Realm
graph construction keep their existing update order. No accessor function is
made lazy by this change.

Deferred method reads keep the balanced exact-ID lookup, but select rooted code,
name, native source and length before one common function-construction tail.
The existing typed ExecutableCode constructor remains in each leaf. A private
validated metadata witness admits only the uniform strict, ordinary,
nonconstructable standard-builtin domain; a future incompatible planned entry
fails compiler construction explicitly. Defining-Realm context and function
prototypes remain caller-owned. The shared tail closes ACTIVE_FUNCTION and
publishes metadata before the recipe caches its immutable completed value.

This shares emitted construction code without adding a per-builtin helper or
runtime metadata ABI. Existing physical Wasm-local reuse remains intact; local
declaration counts are not used to claim native stack or memory savings.

Runtime layout 26 records the new private publication helper ABI. Actual runtime identities,
native images and compiled programs must agree. There is one Wasm GC object
model and no compatibility representation.

The completed source-bound batch passed 4,874 native cases across 19 targets
and 2,365 exact original pinned Test262 executions. One excluded optional-import
inspection fixture was repaired; the original failed attempt is retained.
The matched 17-workload comparison remains 1.3–1.7× Scriptc process RSS, so parity
is still open. Static owned-image helper sizes confirm smaller code bodies;
these measurements do not isolate execution or RSS effects of individual changes.
See [verification and measurements](../memory-intrinsic-publication-20261027.md).
