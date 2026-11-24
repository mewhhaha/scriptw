# Active legacy function reflection

This contract describes the runtime ownership rules for active legacy function
reflection. Execution receipts separately record native acceptance, original
pinned-fixture outcomes and retention of earlier passing IDs.

## Policy and specification boundary

Ordinary non-strict function declarations/expressions and corresponding dynamic
ordinary Functions initially publish own data properties `caller` and
`arguments` with `writable: false`, `enumerable: false`, `configurable: true` and
value `null`. Assignment retains its ordinary non-writable behavior. While an
original property remains implementation-owned and unfrozen, `arguments` follows
the innermost invocation's actual mapped or unmapped ArgumentsObject. `caller`
follows the actual ordinary non-strict JavaScript caller, or `null` when the
immediate caller is absent or opaque.

The former policy published these properties as already non-configurable and
non-writable, then changed their values on activation entry and exit. That
contradicts the pinned
[essential internal-method invariants](https://github.com/tc39/ecma262/blob/5345883164f463e87f8b40aca4956157ecba8783/spec.html#L3284).
Once a non-configurable, non-writable data property's value has been observed,
future `[[GetOwnProperty]]` and `[[Get]]` operations must retain its SameValue.
An implementation-selected extension cannot bypass that invariant. The initial
configurability change permits active reflection without claiming that its
changing value is already frozen.

Making either property non-configurable while it remains non-writable fixes its
current value. `Reflect.defineProperty`, `Object.defineProperty`, `Object.seal`
and `Object.freeze` therefore preserve an inactive `null` or an active snapshot.
Nested calls, normal exits, abrupt exits and later activations cannot replace
that value. Freezing a property whose value is an ArgumentsObject fixes the
object's identity; it does not freeze that object's indexed elements or change
its mapped-parameter semantics.

Deletion, data redefinition and accessor conversion leave the user's resulting
descriptor in charge. A successful ordinary definition reconstructs the stored
value record, including SameValue definitions; automatic publication then
retires permanently for that property. Accessor getters are invoked only by
ordinary property reads, never by the activation's ownership checks.

This is an implementation-selected extension. The pinned
[Forbidden Extensions](https://github.com/tc39/ecma262/blob/5345883164f463e87f8b40aca4956157ecba8783/spec.html#L30316)
continue to restrict eligible function forms and automatic caller values.
Strict functions, arrows, methods/accessors, class constructors, generators,
async functions, async generators, bound functions, builtins and
Function.prototype receive no new own properties. Their defining Realm's
canonical ThrowTypeError accessor remains unchanged. Automatic publication
never exposes a strict caller and never skips an opaque immediate caller to
reveal an older ordinary caller.

Actual Function.prototype.call/apply, Reflect.apply and eval intrinsics remain
transparent forwarding operations. Bound dispatch resolves the actual target
before entry. Prepared eval Scripts have environment execution roles and add no
fake FunctionObject activation. User replacements for forwarding properties
remain ordinary source functions. Other native calls and constructors are
opaque boundaries.

## Ownership and exit ordering

The actual FunctionObject owns separate optional legacy-property state records.
Each records the property's dictionary entry record, the original descriptor
publication installed in it and the last StoredValue reference written by
automatic publication. Function allocation creates these states before the
callable escapes; separate closures sharing one source body have separate states.
No JavaScript name, property currently containing the callable, source shape or
estimated call depth selects the owner.

Publication and restoration read the descriptor the recorded entry holds now,
without a key lookup and without calling getters. That read answers what a
fresh own-property lookup would, because of three heap rules:

- A header whose storage is a dictionary keeps that dictionary. Only compact
  and intrinsic storages are replaced: a compact storage by a compact storage
  one key longer or by a dictionary, intrinsic rows by a dictionary. A fresh
  object's first storage is installed once.
- A property keeps its one entry record while it lives. Entry records are
  constructed only by dictionary insertion after the key was proved absent.
  Compaction and table growth move the record and update its position; a
  replacement through the same key writes a new descriptor into the record.
- Deletion, the only way a live entry leaves its table, replaces the record's
  descriptor with a fresh accessor descriptor that no property owns. A later
  definition of the same key creates a new record with a new descriptor.

The `scriptw-aot-wasm` structure test
`structure_language::legacy_reflection_entry_record_structure` pins every
writer of the header's storage, of an entry's descriptor and of a dictionary's
table, and the one entry constructor, so a new rebuild path fails that test
until this contract is reviewed. The global read site record relies on the
same rules.

A write requires that the entry still holds the original descriptor, a data
property which remains configurable or writable, and the same last
automatically written StoredValue record. Generic DefineOwnProperty mutates
descriptor records in place but constructs a fresh StoredValue; the
value-record identity therefore detects redefinition that descriptor identity
alone would miss. Generic property definition, descriptor flags and ordinary
Get behavior need no special case.

On an ownership mismatch or a frozen property, the state clears its entry,
descriptor and stored-value edges. No later activation revives it. An
activation separately roots its saved prior value and state until its matching
exit. Recursive publication updates the shared state's expected token; a normal
unsealed nested return restores the outer value and token. A frozen inner
snapshot survives every enclosing restoration instead. User-owned saved
ArgumentsObjects remain ordinary live values after the source activation ends.

The actual innermost call-boundary FunctionObject and source-depth budget still
have their existing independent owners. Property retirement never disables
their restoration. Entry remains before parameter/instance initializers;
ArgumentsObject publication remains before a source binding can replace
`arguments`. Normal return, uncaught throw, suspension, initializer returns,
finally execution and proper-tail-call ordering retain their existing epilogue.
Mapped cells, duplicate-parameter selection and non-simple unmapped arguments
are unchanged.

## Authored controls and required replay

The authored inactive descriptor expectation now requires configurability, and
its incompatible frozen redefinition explicitly freezes the function first.
The existing recursion/restoration control now tests unsealed properties;
freezing while expecting their data values to keep changing was contradictory.
Its actual argument/caller identities and abrupt-restoration assertions remain.

Four new Engine controls require inactive frozen consistency through active
calls; inner/outer recursive and abrupt frozen snapshots; in-place user data
definitions, accessors, deletion and recreation; and foreign arguments plus the
actual cross-Realm caller across GC and later calls. Two controls cover the
recorded entry: deletion and recreation of both properties inside an active
call, including recreation with the original attributes and value and
deletion in a nested activation, never revives automatic publication; and
compaction and table growth of the function's dictionary while it is active
keep publication and restoration working. Existing strict/non-ordinary
poison, forwarding, private/mapped arguments, actual-caller and exit controls
remain required. No IR source policy changes are involved.

The exact pinned
`built-ins/Object/internals/DefineOwnProperty/consistent-value-function-arguments.js`
and `consistent-value-function-caller.js` fixtures remain unchanged. Required
acceptance is their actual sloppy replay, the new Engine controls, the complete
existing `aot_function_caller` module and broader retained native inventory.
The ten original staging fixture copies still retain all eleven original
execution inputs. Source review, formatting and unapplied-patch checks establish
none of those execution results.
