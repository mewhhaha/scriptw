# TypedArray native mutation, copying and sorting

TypedArray algorithms retain one Wasm GC object model and the shared buffer,
element-access, callable and Realm owners. The GC schema supplies the actual
receiver brand; public properties cannot forge it.

The six actual standard entries select an exhaustive TypedArrayNativeMethod.
The private receiver constructor admits only a concrete TypedArrayObject and
owns its initial length and closed element kind. A Proxy cannot supply a brand
through observable Get. Mutating reverse, copyWithin and sort consume the sole
Objects write-view validation before coercion, including length zero. Copying
methods consume read validation. Sort/toSorted check comparator callability
before receiver admission. Internal copying and sort walkers accept their own
closed two-case domains; a different method cannot enter those walkers.

Reverse performs two whole element reads before either write. ToReversed and
With construct through the saved defining-Realm intrinsic constructor and the
actual whole TypedArrayCreateSameType operation; constructor/species properties
on the source are not read. With resolves its negative index using the original
length, then converts the replacement value, then checks the current buffer.
It retains ToBigInt/ToNumber's original Throw and uses the original output length
when a coercion grows the receiver. All element conversion and bounds semantics
stay in Objects' single typed-array element owner.

CopyWithin converts target, start and end once, in order, against its original
length. Positive count triggers a fresh bounds witness and clips the count to
the longest applicable prefix. The consumed byte-copy operation uses the existing
ElementAccess owner in private Uint8 mode, copies in physical overlap direction,
and preserves source bytes including NaN payloads. It performs one unordered
Uint8 access per byte on shared backing, rather than widening the operation to
an integer-element atomic access. Zero count adds no late bounds check.

Sort and ToSorted snapshot every whole source value into one real GC ValueArray
before the first comparator call. A second GC ValueArray holds merge scratch.
Stable bottom-up merging is O(n log n), choosing the earlier left value on
comparison equality. Default comparison uses exact BigInt Compare or Number
ordering with NaNs last and negative zero before positive zero. A supplied
comparator uses actual Call with undefined this, then actual ToNumber; NaN is
canonical comparison equality. Every comparison completes before writeback, so
callbacks may mutate/detach the original buffer without changing the captured
list, and comparison Throws retain the original value. Detached in-place writes
use the existing specified suppressed-write behavior. Temporary lists contain
GC references only. Normal return clears their roots; an abrupt exit releases
the remaining temporary roots with the builtin Wasm activation.

The immutable-buffer write admission follows the already admitted
[Stage 2.7 immutable ArrayBuffer proposal](https://tc39.es/proposal-immutable-arraybuffer/).
Current core coercion/copying/sorting algorithms follow
[ECMA-262 TypedArray methods](https://tc39.es/ecma262/multipage/indexed-collections.html#sec-properties-of-the-%typedarray%-prototype-object).
The current core's post-coercion prefix clipping is retained; older proposal
text is used only for its added immutable access policy. Integer-index [[Set]]
and descriptor immutable policies remain a coupled Objects source obligation
until their real owning paths are updated; this method port does not claim them
complete. The runtime target is experimental Wasmtime with Wasm GC, reference
and declared shared-resource support, with no second heap representation.

Traversal methods take their initial typed length/read-view admission and use
live element operations. Join converts the separator before element reads;
locale formatting preserves element/method Get, original receiver, locale/options,
Call and ToString. Every/Some stop at the first decisive callback result.
Search retains each method's range, direction, equality and presence rules.

Map creates and write-validates its species result before callbacks. Filter
retains selected original values until callbacks finish, then creates and
write-validates its result, including empty output. Slice also validates empty
species results; a nonempty copy revalidates the source after species effects,
then uses ascending same-kind byte copies or ordered unlike-kind conversion.

The focused native controls cover element kinds, exact bytes/overlap,
coercion/resize, intrinsic construction, sorting, GC/Throw, borrowed Realms and
immutable admission. They live with the engine's GC entry tests. Current
[verification guidance](../../../CONTRIBUTING.md#verification) and source-bound
published checkpoints govern execution; this contract carries no independent
conformance count.
