# Execution hot paths

This batch reduces work in emitted JavaScript execution while keeping the
experimental Wasmtime backend, private layout 20 and native policy unchanged.

## Private Array sorting

Array `sort` and `toSorted` collect the same private value snapshot before
comparing anything. Stable bottom-up merging moves existing immutable
`StoredValue` references rather than constructing a new wrapper for each move.
Unordered input needs one scratch array of references and O(n log n)
comparisons. An adjacent scan handles ordered input with n-1 comparisons and
no scratch array. Equal comparisons, including comparator NaN, take the left
element first. Source property reads, holes, undefined ordering, comparator
exceptions and receiver writeback retain the ordinary algorithms.

Comparison order is implementation-defined by `SortIndexedProperties`;
permutation, stable order and immediate propagation of abrupt comparisons
remain required. TypedArray sorting already uses stable merging and is unchanged.
This is a comparison/movement bound, not a bound on arbitrary user callbacks.

## Private string construction

Array and TypedArray `join` / `toLocaleString` append into unpublished UTF-16
storage, grow it geometrically and publish an exact-length immutable string.
This replaces repeated copying of the accumulated prefix. Appends occur at the
original concatenation points, preserving live element reads, separator
conversion, locale calls, reentrancy, exceptions and unsigned-length overflow
ordering. Construction consumes its private storage before publication; it
cannot mutate an existing JavaScript string. Final publication copies the used
prefix, so small-input allocation costs require separate controls.

## Scalar consumers and initialization

Number comparison predicates feed Wasm control flow directly. Value-producing
comparisons share that comparison emitter and materialize a Boolean only when
a JavaScript value is needed. Proven Number local operands load their scalar
payload after the same lexical initialization check used by ordinary reads.
Operand evaluation remains ordered and floating-point operations are not
reassociated.

Ordinary source relations retain general comparison IR, so the predicate
consumer handles that real path as well as explicit Number comparison IR.
Only a certificate for an actual emitted Number result can bypass the shared
runtime comparison. Singleton hints on local/property/environment reads or
calls do not supply that certificate; those comparisons retain runtime tag
handling and ordered primitive conversion.

Checked lexical/captured initialization, initialized hoisted vars and private
temporaries are distinct states. Only constructors that publish an initial
value omit the initialization flag. A captured implicit-arguments snapshot
keeps its source initialization state. Captured environments, dynamic names,
lexical `this`, coercive Number/BigInt cases and generic value boundaries retain
their existing paths.

## String addition and folding

Exact literal String operands fold by decoding their semantic UTF-16 units,
concatenating and re-encoding once. This preserves lone surrogates and pairs
formed across the operand boundary. A 4-KiB encoded-input budget per fold
bounds copying; larger and effectful expressions retain runtime evaluation.

The shared runtime `CoerciveAdd` helper retains ordinary `ToPrimitive` for both
evaluated operands. Within its string-result branch, an actual String tag
elides that operand's identity `ToString`; other primitives retain conversion
and error behavior in left-to-right order. The converted values reuse their
rooted operand locals, then enter one shared concatenation body. The copy
implementation stays in the shared runtime rather than being duplicated at
source addition sites. This does not apply algebraic identities to
floating-point expressions.

## Verification and measurement

The native regressions cover stable sort and comparison scaling, snapshot and
writeback effects, abrupt/reentrant callbacks, UTF-16, GC identity, resized
TypedArrays, Number edge cases, TDZ and captured bindings. Emitted-Wasm checks
cover direct predicates and removal of literal-concatenation helper calls.

`crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths/manifest.json` defines
checked tuning workloads and controls. Measure actual compiled Wasm against
the same sources in V8, retaining compilation separately from execution,
generated code size, process RSS and GC capacity. These memory observations
are not live JavaScript bytes or allocation counts. Full-suite conformance and
historical performance charts retain their own measured compiler identities.
