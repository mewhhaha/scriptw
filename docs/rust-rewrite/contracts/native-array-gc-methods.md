# Array method ownership

Array methods use whole rooted values and complete completions through the
shared object, callable and iterator owners. Source calls perform observable
property Get and actual Call; static receiver shape does not bypass overrides.
The current indexed representation is described in
[Array storage](gc-array-indexed-storage.md).

Generic traversal captures LengthOfArrayLike once, then performs the specified
live property observations in order. Callback validation, original receiver,
argument order and abrupt completions remain observable. Map preserves holes
and creates properties on its species result; Filter skips holes and retains
selected original values until result publication. Every/Some stop at the first
decisive result. Find/FindIndex and their reverse variants Get holes as well as
present elements and return the original value or index, respectively.

Includes uses SameValueZero and reads holes; IndexOf/LastIndexOf use HasProperty
before Get and strict equality. Empty input returns before fromIndex coercion.
LastIndexOf distinguishes an omitted fromIndex from explicit undefined. At
converts its index and checks range before truncating it into an element index.

Concat creates its species result, gets Symbol.isConcatSpreadable before a
spread operand's length, and uses IsArray only when that property is undefined.
Other spreadability values use ToBoolean. Live HasProperty/Get, safe-index
admission and the final strict length Set remain in order. A TypedArray explicitly
marked spreadable follows that same ordinary property protocol.

Flat retains strong traversal frames containing source, captured length, index,
depth and parent. Each parent advances once before observable descent; absent
properties are skipped and infinite depth stays infinite. FlatMap calls its
mapper only on present outer elements. Only actual Array exotics are flattened;
a TypedArray result is not made an Array by its storage shape.

Reverse reads both sides' presence/value before mutation, then applies the exact
strict Set/Delete case. CopyWithin chooses overlap-safe direction and performs
live HasProperty/Get and strict Set/Delete. Splice validates normalized lengths
before species creation, preserves deleted holes, moves properties in the safe
direction and ends with strict length Set. ToSpliced/ToReversed/With create dense
new Arrays; With does not Get the replacement slot.

Sort collects the required values before comparisons, preserves stability,
writes the receiver and deletes trailing properties. ToSorted reads through
holes into a new Array. Undefined values are ordered without invoking the
comparator. Default comparisons perform ordered ToString and full UTF-16
comparison. TypedArray numeric/default sorting retains its separate owner.

Join converts its separator before traversal. Locale formatting performs element
Get, method Get, Call with the original element receiver and locales/options,
then ToString, preserving complete abrupt completions. The private non-derived
`ToLocaleStringReceiverKind` separates generic LengthOfArrayLike from the actual
TypedArray read-view admission; all four join/locale entries choose it explicitly
and consumers match exhaustively. Its focused structure control is
`cargo test -p scriptw-aot-wasm --test structure_builtins -- to_locale_string_receiver_kind_structure::`.
Array and TypedArray toString share a canonical callable that performs ToObject,
Get(join), callability admission and Call, falling back to Object.prototype.toString.

Standalone constructor/of/from/isArray use the actual constructor, iterator and
object owners. TypedArray traversal, species and buffer revalidation belong to
[TypedArray methods](gc-typed-array-native-methods.md). Existing strict/sloppy
native controls live in aot_gc_native_array_entries.rs; published checkpoints
carry their own verification identity, not an ongoing unverified-migration status.
