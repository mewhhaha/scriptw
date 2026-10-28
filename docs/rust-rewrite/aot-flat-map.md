# T16: observable `flatMap` on the Wasm-AOT path

## Scope and baseline

This batch starts from `31c89c4b29b1fe84e3a3eaef55312448b80e3f07`.
It replaces duplicated flatMap policies with existing shared operations, without
adding a host interpreter, changing Test262 inputs or pins, excluding tests, or
updating published full-suite percentages from a focused sample.

The unchanged compiler passed 8 of the 16 original regression programs and failed
8. The failures cover mapper validation before observable length access, species
side effects changing the loop bound, huge lengths trapping during numeric
conversion, TypedArray length overrides, callable Proxies, nested source and
mapped Proxies, and Proxy species constructors. The additional minimal-program
regression guards the shared target-definition builtin dependency.

Reference algorithms: [Array.prototype.flatMap and FlattenIntoArray](https://tc39.es/ecma262/multipage/indexed-collections.html#sec-array.prototype.flatmap),
[LengthOfArrayLike](https://tc39.es/ecma262/multipage/abstract-operations.html#sec-lengthofarraylike),
and [ArraySpeciesCreate](https://tc39.es/ecma262/multipage/abstract-operations.html#sec-arrayspeciescreate).

## Observable operations

The canonical builtin performs ToObject and one LengthOfArrayLike before
IsCallable and ArraySpeciesCreate. Missing mapper arguments do not skip length
getters or coercion. Shared ToLength bounds enormous lengths without an unguarded
Wasm integer conversion. Species effects cannot change the captured source bound.

Source properties use live HasProperty, then Get, then Call with value, index,
the boxed source and the supplied thisArg. Shared callability and invocation
support callable Proxies. IsArray classifies mapped results, but length and
indexed properties retain their original Proxy receiver. Only one level is
flattened; holes are skipped, and arbitrary array-likes, strings and TypedArrays
returned by the mapper remain single values.

One append owner checks the maximum safe integer bound, performs
CreateDataPropertyOrThrow, propagates abrupt completion and then increments.
Species targets receive own data properties rather than inherited setter calls
or a synthetic length write. Planning roots the ObjectDefineProperty dependency
even when the input program never names Object, and the new bound error is
registered in the builtin string pool.

TypedArray receivers now use observable length properties, including own and
inherited overrides. Generic integer-indexed property operations own live buffer
checks after resizing or detachment. Existing direct-call, spread-argument and
TypedArray fixture programs remain unchanged.

The ownership contracts are [algorithm ownership](contracts/native-array-gc-methods.md)
and [TypedArray observation](contracts/native-array-gc-methods.md).

## Verification

The seventeen named engine regressions explicitly use WasmAot. Reference Node
execution verifies expectations only; product evidence requires actual compiled
Wasm execution. The CI workflow checks a nonempty compiled inventory, requires
all its engine tests to execute without failures or ignored tests, runs retained
CLI fixtures and executes the entire pinned real flatMap Test262 subtree.

```sh
cargo test --locked -p scriptw-engine --test aot_builtins -- aot_flat_map:: --test-threads=1
cargo test --locked -p scriptw-aot-wasm --test array_flat_map_algorithm_owner_structure --test array_flat_map_typed_array_witness_structure --test structure_builtins -- array_species_create_operation_evidence_structure::
cargo test --locked -p scriptw-cli --test cli -- array_flat_map --test-threads=2
cargo build --locked -p scriptw-cli
./target/debug/scriptw test262 run built-ins/Array/prototype/flatMap/ --execution-backend wasm --threads 2 --jobs 2 --timeout-ms 60000 --snapshot-dir /tmp/flatmap-test262 --snapshot-name flatmap-review
```

The PR records exact tested revisions and before/after results. These commands
are not themselves proof of a passing run. Temporary focused snapshots are CI
artifacts, not replacements for current-pin full-suite status. The separate
agent CI fix selects the existing pinned-case test by its exact compiled name
and fails if the test is missing, ignored, or not executed.

## Next work toward literal conformance

Obtain a complete current-pin Wasm-AOT snapshot and rank failures by shared
semantic owner. Fake-suite checkpoints and historical spec-exec results are not
a substitute, and a focused flatMap percentage is not the overall Test262 score.

For Array follow-ups, review copied species paths in flat, concat, map and filter,
and generic length observation in find-family methods and toLocaleString.
General-depth flattening and realm-sensitive constructor behavior need separate
execution evidence. This batch does not repair neighboring Array algorithms or
the static call dispatcher's broader property-lookup policy.

Other review priorities include remaining harness materializer shortcuts,
module-cycle/namespace/top-level-await interactions, actual GC and weak-reference
semantics, and default time-zone/provider support. These are separate workstreams,
not features delivered by this patch. Unsupported dynamic source generation must
remain explicit instead of disappearing from the denominator.

Generic-operation performance has not been benchmarked against former private
fast paths. Any later optimization must preserve observable semantics.


## Shared-operation review follow-up

Execution exposed three distinct owner choices: callback invocation must use the
Proxy-aware Call dispatcher, ArraySpeciesCreate must use the Proxy-aware Construct
dispatcher, and source element reads must use the shared indexed-read dispatcher
that owns live TypedArray element access. Ordinary named-property Get remains the
owner for observable length. The ArraySpeciesCreate correction also applies to its
existing slice and splice callers; it does not migrate other copied species paths.
