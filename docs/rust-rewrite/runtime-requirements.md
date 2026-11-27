# Wasm runtime requirements

Scriptw's tested execution path is its Rust CLI/library with the project's
vendored **Wasmtime 47.0.0**. The host policy is defined in
[wasmtime_policy.rs](../../crates/scriptw-engine/src/wasmtime_policy.rs).
The same proposal and collector policy applies to both native compilation modes.

## Required feature set

| Wasm capability | Required by the backend |
| --- | --- |
| GC structs and arrays | JavaScript objects and the semantic heap |
| Reference types and typed function references | Values, closures and typed calls across modules |
| Exception handling | JavaScript abrupt completion and exception flow |
| Tail calls | Enabled as part of the required backend target |
| Multi-memory | Program/runtime memory arrangements |
| Threads and shared memory | Required proposal support and host shared-memory allocation |

The native adapter explicitly selects Wasmtime's copying collector and verifies
GC, reference types, typed function references, exceptions, threads and shared
memory before admitting its engine. Multi-memory and tail calls are enabled
in the same policy. This does not claim that Scriptw supplies browser Workers
or Node's worker APIs. SIMD is not an additional requirement of this policy.

## Host compatibility

The embedding API's `configure_wasm_runtime` takes a `NativeCompilationPolicy`:
`Speed` (the default; Cranelift `OptLevel::Speed` with the backtracking
register allocator) or `Fast` (`OptLevel::None`, single-pass), and the
process's `CompilationJobs` (by default two, or one on a single-CPU machine),
which decides whether Wasmtime compiles functions in parallel on Scriptw's
bounded pool. Scriptw's own
CLI and Test262 execution choose their native mode internally, as before
(`Fast` for one-shot runs), and do not use this default. The precompiled runtime image R keeps its own fixed policy
and loads under either.

Rust embedding entry runs synchronously on the caller thread using a guarded
64-MiB Wasmtime fiber stack. Synchronous host callbacks retain that thread's
affinity. Reentrant entry receives an independent 64-MiB scoped-thread stack;
the Wasm stack limit remains 32 MiB.

The GC heap reserves a fixed 4-GiB virtual address range with a 32-MiB guard.
Its initial accessible backing is 32 MiB (a 4-MiB nursery and two nominal
14-MiB old semi-spaces); the Store limiter still caps each backing memory at
1 GiB. Reserving addresses does not commit 4 GiB of RAM. On the supported
64-bit Wasmtime baseline this permits protected-page checks and a stable heap
base; Wasm array bounds and null-reference traps remain. Native images bind
these settings through configuration schema 9. Custom pooling allocators
require matching GC and linear-memory tunables; ordinary Scriptw uses on-demand
allocation.

Execution timeouts need Unix signal handling. Compiled code carries no
deadline checks: a deadline removes access to the Store's GC heap and private
linear memory, and the fault stops the code. Both stay at a fixed address for
that reason, and epoch interruption is off. Engine rejects a requested timeout
on a host without this support. See
[execution deadlines](contracts/execution-deadlines.md).

An emitted program is linked to Scriptw's runtime module and host imports,
including the `scriptw_host` namespace. Matching runtime bytes, Intl data and
host adapters are part of the execution environment.

- **Scriptw CLI / Rust API:** the supported integration with vendored Wasmtime
  47.0.0, including the copying collector and project host bindings.
- **Browsers, Node's Wasm API and other engines:** no compatible Scriptw adapter
  or end-to-end portability result is established. An engine implementing the
  same Wasm proposals still needs the host ABI, data and collector capabilities.
- **Bare WASI / a standalone Wasmtime CLI:** WASI does not provide Scriptw's
  JavaScript runtime or its custom host imports. The tested path is Scriptw's
  embedding, not loading the emitted file as an arbitrary WASI command.

These are requirements of the current backend, not a promise of support for
every engine or a fallback implementation for engines missing Wasm GC.

On Linux the embedded native runtime is mapped straight from the running
executable, which needs readable `/proc/self/exe` and `/proc/self/maps` (or
`/proc/self/auxv` and `/proc/self/mem`) and an executable on a filesystem that
permits executable mappings. Without them, and on other hosts, the same image
is copied into anonymous memory: slower to start and private to each process,
with identical behavior. The
[byte-ownership contract](contracts/runtime-byte-ownership.md#native-backing-storage)
states what admits that image.

## Runtime ABI

The runtime ABI is **96**. Portable bundles and native images built under an
earlier ABI must be rebuilt. ABIs 58 to 77 added per-site read, write,
`instanceof` and global read records, a dictionary position hint on Strings,
further native call entries, one JSON output accumulator, compact shape
transitions, a computed-key write kernel, static site records kept in P
globals and chained per read site, weak collection entries found by identity,
String concatenation helpers, leaf kernels over code units and bytes, a
branch hinting section in P and R and a Symbol row count in intrinsic
layouts. ABIs 78 to 82 make the Error family with DOMException, twenty-six
constructors and the ordinary global functions demand-built, and publish a new
global object's bindings in one call
([contract](contracts/lazy-runtime-values.md)). ABI 83 adds the native
recursion depth global that bounds helper recursion (see
[implementation limits](#implementation-limits)). ABI 84 changes no
interface: it re-emits R with fewer null refinements, stores and copies
(see [locals and GC roots](#locals-and-gc-roots)), so R's bytes and key
differ. ABI 85 adds the per-Realm protector record (each intrinsic
prototype and the intrinsic storage it held when the Realm's intrinsics were
complete; a fast path checks that identity against the live object), the
`instanceof` site record's terminal `%Object.prototype%`, a cached
`{value, done}` iterator-result shape and the plain `JSON.stringify` helper.
ABI 86 adds the builtin fact epoch global, a watched mark on a dictionary
entry, and the epoch's advances in R, which let a builtin entry guard keep
its whole proof between edits. ABI 87 keeps the dictionary entry record in
each legacy `caller`/`arguments` state, so a sloppy activation checks the
property without a lookup
([contract](contracts/active-legacy-function-reflection.md)). ABI 88 drops the
unread WTF-8 copy of each pooled string from R's static image and P's metadata
image, so every later static table and P metadata span moves. ABI 89 builds P
literals on first use: a ProgramInstance starts with an all-null literal table
and keeps its descriptor rows and a resolver hook, and a source function keeps
its text as a literal ordinal until `Function.prototype.toString` asks for it
([contract](contracts/program-literal-root-ownership.md)). ABI 90 has R define
its private linear memory instead of importing a host-created one, so its data
segments map copy-on-write from a memory image
([contract](contracts/runtime-byte-ownership.md)). ABI 91 adds
`DeferredLegacyArguments` (lazy sloppy-function `.arguments` materialization
with recipe inputs cleared upon observation), widens `InlineCompactDataStorage`
from two to four inline fields, adds the immutable `CompactDataShape.LACKS_TO_JSON`
witness, expands direct local-argument native builtin entries to 55 builtins,
and switches `CollectionHashTable` sizing to power-of-two bitmasking. ABI 92
defers 51 additional Realm intrinsics and namespaces (`DemandPrototype`,
`DemandDynamicFunction`, `DemandBoxedPrototype`, `DemandHostFunction`,
`DemandNamespace`, and `Object`/`Function`/`String` constructors via
`DemandGlobalRecipe` and `RealmEnsureConstructor`, leaving 1 eager constructor
and 5 eager `IntrinsicPropertyStorage` layouts at Realm birth), passes
compile-time precomputed property-key hashes into `FreshDataPropertiesPublish`,
adds the `StringConcatStringNumber` leaf helper, and lowers guarded inline
2-arg `Math.min`/`Math.max`, 1-arg `String`/`Number`, and within-capacity 1-arg
`Array.prototype.push` directly in P. ABI 93 compacts `StringValue` to 32 bytes
(`RAW_KEY_HASH` first, `CODE_UNITS` at field 1, `ENTRY_HINT` at field 2), defers
`WellKnownSymbolTable`, `RegisteredSymbolTable`, `ArrayConstructor`,
`%ThrowTypeError%`, `RegExpLegacyState`, and `ProgramInstance` native payloads
until first use (removing 78 eager GC allocations per Store + Realm +
`ProgramInstance`), caches per-entry `ExecutableCode` references in P globals
when not emitting a reusable `NativeFactory` template, adds in-place monotonic
numeric subtraction sort (`BinaryNumericCallback::SUBTRACTION_ADMITTED`), fused
multi-stage `.split().join()` replacement and prefix projection helpers,
4-record compact property-read caching, direct own-descriptor predicates,
compact `for…in`, zero-copy non-shared `TextDecoder.decode`, an execution-only
runtime receipt for warm cache hits, and bounded caller-thread `FiberStack`
caching (at most 2 idle stacks process-wide), alongside Cranelift local GC
safepoint retention and Wasmtime copying-GC barrier/initialization and
ephemeron improvements. ABI 94 publishes global dictionary rows directly and
makes well-known symbols, Array callables, RegExp legacy state and native
payloads demand-owned. ABI 95 replaces eager global builtin publication with
`GlobalBuiltinBacking` (immutable builtin rows, a 128-bit deleted mask, sparse
lazy original entries and a nullable append dictionary, plus
`scriptw_snapshot_global_builtin_row` for rooted snapshots), adds a 6-lane
zero-argv strict `arguments` local entry (`WideLocalArgumentsFunction`), narrows
immutable host `print(...)` calls to a standalone proof that drops the 42 MiB
runtime link on simple scripts, restores the Portable `StringPool` prefix from
the built-in runtime receipt during `scriptw build wasm`, optimizes
Promise/await, byte streams, UTF-8 codecs, sealed array callbacks, compact
JSON/for-in/`Object.keys`/`Object.assign`, packed joins, literal concat-length
folding and single-word bitmap push, raises the initial copying-GC heap to
32 MiB (`4 MiB` nursery) with near-limit growth fallback, and adds prepared
`InstancePre` late-bound `SharedMemory` and positional R-to-P module linkage,
single-entry cold-call GC root sinking, and `MAX_SPLITS_PER_SPILLSET = 4` in
vendored Wasmtime 47 / Cranelift / Regalloc2. ABI 96 (configuration schema 9,
`SCRWEP04`) makes precompiled `.swep` images native-first with a 126-byte footer
and Unix `O_NONBLOCK` file-backed `mmap` deserialization (with `noexec` copy
fallback), adds selective host-import registration, lazy remembered-slot
allocation and tighter WeakMap/module metadata in Wasmtime 47, extends Cranelift
cold-guard GC root sinking, adds straight-line own + closed-tail absent inline
property reads, P-local `for…in` cursors with `hasOwnProperty`/computed hints,
`%Object.prototype%` computed-write absence filters, 5-field direct compact
literals, packed-array `StoredValue` reuse, plain `JSON.stringify` traversal,
mixed ASCII + 2-byte UTF-8 codec loops, Promise/Streams/await fast paths,
`instanceof` ladder reuse, lean direct local-argument calls, and shared empty
locals / null-reference argument-projection sentinels. `LAYOUT_VERSION` in
`crates/scriptw-aot-wasm/src/runtime_artifact.rs` records each step.

### Locals and GC roots

The emitter assumes that a Wasm local retains nothing once nothing reads it.
Wasmtime compiles each local to Cranelift SSA values and finds GC roots
through stack maps of the values live across each safepoint, so a reference
left in a dead local is not a root. A released pooled reference local
therefore keeps its last value until its next holder writes it, with no
store of null (the code sink checks, in test and debug builds, that every
holder writes before it reads). An engine that roots Wasm locals for their
whole activation, rather than by liveness, would retain those dead
references longer; this is part of the Wasmtime baseline, not a claim
about other engines.

Two other elisions are exact on any engine and rest on no such assumption.
A body's epilogue ends its holders without storing null
(`Function::begin_exit`): nothing runs in the activation after its final
`end`. A constant store is elided (`Function::store_constant`) where the
local already holds the value on every path, or where the local was never
written, no loop is open and the value is the Wasm default the local starts
with: in structured Wasm only a loop's back edge reaches an earlier point of
the same activation.

A reference the emitter's own types prove non-null (a non-null holder in a
nullable pooled local, or a runtime root published before program code
runs) is not refined again before an instruction that traps on null
itself, and is stored into a nullable local without one. Such a reference
is null only by a compiler bug, and for a correct program nothing changes;
under such a bug the trap moves to the next instruction, or, after a store
into a nullable local, to that local's next refined read (or is lost if
nothing refines it again).

## Implementation limits

ECMA-262 bounds none of these; each limit throws a catchable error where the
alternative was a Wasm trap that ends the program, and each is derived from a
real constraint of the product Store (a 1 GiB GC heap whose copying collector
keeps half idle, so at most 512 MiB is live, and a 32 MiB Wasm stack):

| Limit | Value | Error | Constraint |
| --- | --- | --- | --- |
| String length | 2^27 - 6 * 2^20 code units (127,926,272) | RangeError | A concatenation keeps its operands live beside its result, four bytes per result unit, in the 512 MiB live semi-space less 24 MiB for the runtime's own data (measured at about 17 MiB) (`MAX_STRING_LENGTH`). V8's 2^29 - 24 cannot be allocated in this heap. |
| BigInt size | `MAX_STRING_LENGTH` - 1 bits | RangeError | ToString of a BigInt cannot throw; its radix-2 form, with a sign, must fit the String limit (`MAX_BIGINT_BITS`; V8: 2^30). A power whose size provably exceeds it throws before computing: the check bounds e log2\|base\| from below to within e / 2^20 + e / 2^30 + 1 bits (exactly for a power-of-two base), so small bases (`3n ** 100000000n`) throw at once. A power that fits, or one within that margin, still computes with quadratic multiplication (hours near the limit). |
| Bound function nesting | 4,096 levels | RangeError | Each level keeps its own `name`, "bound " plus its target's, so a chain's names grow quadratically; an unbounded chain trapped on heap exhaustion by 12,000 levels (measured before the limit). Callable Proxies between binds forward the name and count as levels. |
| Nested helper recursion | 4,096 executing calls | RangeError | Proxy forwarding, JSON.stringify and helper-to-built-in calls recurse with the program's data; 20,000 such calls ran in the 32 MiB Wasm stack, so 4,096 stay under a quarter of it. |
| Source nesting | 1,000 levels | SyntaxError | The parser and the compiler phases recurse per level on the 64 MiB compiler stack (about 18 KB per level on nested array literals; the deepest measured, a ladder of precedence levels with two operators each per parenthesis, takes about 35 KB, 33.7 MB at 950 parentheses), so the bound is on the tree's height (`boa_parser::parser::NestingCharge`; the error names the construct that exceeded it): an `else if` costs two thirds of a level (about 1,500 compile); a property access, call, tagged template or optional chain link, and each item of an optional chain, two levels (about 500), a weight set for the native compilation of one property access chain, which is quadratic in its length (it bounds one chain, not a function body's total; see [compile memory](#compile-memory)); each of a method call's two links half a level (about 1,000 chained calls; with the Speed policy and one compilation job, `SCRIPTW_JOBS=1`, the longest, 950 calls, compiles natively in about 0.7 GB); and a binary, logical, `??` or comma operator, which the compiler walks without recursion, a two-thousandth (about two million fit; a 100,000-term chain compiles and runs). Lowering folds those chains, and a template literal's parts, into IR at most about 256 levels above their operands (about two above one much deeper operand, about log2(k) + 1 above k comparably deep ones), so the IR's depth follows the tree's height; a flat template of 100,000 substitutions lowers to bounded IR, but from about 14,000 substitutions in one function its body exceeds the Wasm body size limit, a compile error (see [current work](current-inventory.md)). In a generator or async function body, a chain of which an operand contains `await` or `yield` costs half a level per operator (about 2,000); others cost the same as anywhere. An ES module's top level is charged as such a body, and eight levels deep (`boa_parser::parser::MODULE_BODY_ENCLOSING_LEVELS`): module lowering parses it again inside generated scaffolding, so a module's body may nest eight levels less than a Script (`var x = (…)` nests 998 parentheses in a Script, 990 in a module). The forms that reparse rewrites are charged as rewritten: `export var`, an anonymous `export default` function or class, and `import(…)`, which becomes a call of a generated function, also in a Script. Not a memory bound: see [compile memory](#compile-memory). |
| Formal parameters | 65,535 per function | SyntaxError | ECMA-262 sets no limit; Node 24's V8 accepts at most 65,525, although its SyntaxError says 65,534 are allowed (`boa_parser::parser::MAX_FORMAL_PARAMETERS`). |
| Regular expression nesting | 1,000 groups and `v`-mode classes | SyntaxError for a literal | The pattern parsers recurse per group and nested `v`-mode class. A deeper literal is a SyntaxError (`lexer::regex::MAX_PATTERN_NESTING_DEPTH`). Scriptw's compile-time pattern compiler applies the same scan to every other pattern (a `RegExp` call's string argument, the runtime candidate table's string literals) and leaves a deeper one to the runtime compiler, which keeps its frames on the heap; V8 compiles about 10,000. |
| Active source bodies | 128 | RangeError | Simultaneously active source function bodies, resumable bodies included while they run (`MAX_ACTIVE_SOURCE_BODIES`): a conservative bound on the 32 MiB Wasm stack, which no Wasm instruction measures. V8 allows about 10,000 frames. |

A value within these limits can still exhaust a heap that other live values
fill; that is a Store resource trap. The String and BigInt limits assume the
1 GiB heap cap of the CLI, pooled Engines and the Store's own limiter
(`limits_mut`); an embedder that attaches a smaller limiter sees such values
trap instead of throwing.

### Compile memory

Compilation has no memory budget. Native compilation of one Wasm function
was quadratic in memory where Cranelift's SSA builder kept a dense map over
every block for every variable and the frame-reload pass a known set per
block that grew with nesting (both fixed in the vendored crates: a
2,000-term top-level `+` chain fell from 2.6 GB to 387 MB with the Fast
policy, 1,000 nested `else if` branches from 3.3 GB to 392 MB). Lowering,
emission and the remaining native compilation still grow faster than the
source in some shapes, and the time of the single-pass allocator does too
(see [current work](current-inventory.md)). The parser's `else if` and member
weights were set for the native compilation cost of one such chain; they bound
one chain, not the total of a function body that holds several. One Wasm function body
is limited to 7,654,321 bytes by the Wasm validator, so a single very large
body is a compile error. **An embedder that compiles untrusted source must
run compilation under an OS memory limit** (a cgroup, `ulimit -v` or a
container limit): a few kilobytes of nested source can still use gigabytes.
Compile time is unbounded too, so such an embedder also needs a deadline:
every conditionally reached expression (each right operand of `||`, `&&`
and `??`, each conditional branch) copies the flow facts of the bindings in
scope, so n object-valued globals and n logical operators take time
quadratic in n (2,000 of each: 34 s), and native compilation of one large
function grows superlinearly (a 10,000-term top-level `+` chain in a module:
493 s and 2.9 GB with the Fast policy). Native compilation memory follows the
size of the whole function body, which the nesting limit does not bound: with
the Speed policy, the embedding default, and one compilation job
(`SCRIPTW_JOBS=1`), 2,700 `o.m();` statements in one function took 1.5 GB,
and three chains of 900 method calls 1.9 GB, though each chain alone fits in
about 0.5 GB. These are one job's figures: with more jobs, the default,
separate functions compile concurrently and their peaks add up (two separate
900-call functions peaked at 1.2 GB with one job, since part of one body's
memory outlives its compilation).

Parsing costs about 256 bytes per statement: a boxed 224-byte `Statement`
(464 before the for-in/of loops were boxed), its list entry and its span. A
script of four million `{}` blocks (Test262's
`staging/sm/regress/regress-610026.js`, through `eval`) peaks at 1.2 GB,
nearly all of it that tree. Lowering shares heap shapes between the facts
that hold them, so a function's `this` fact no longer copies the global
object's shape (about 1 MB with the builtins); 1,000 methods in one object
literal (`staging/sm/class/methDefn.js`) fell from 2.2 GB to under 0.5 GB.
A write, `delete` or call that invalidates shape facts decides each edit from
the shared shape and copies only the shapes it changes: 200 bindings of one
500-property object and a `delete` elsewhere peaked at 312 MiB above the
start of lowering while each binding's shape was copied first, and at 27 MiB
shared.

## Dynamic source: eval and Function constructors

Dynamic code generation is **limited**, rather than wholly absent:

- The compiler can prepare finite known `eval` and Function-constructor sources
  ahead of time, with dispatch that checks the source and execution context.
- The native host can parse, lower and compile additional supported runtime
  strings into Wasm. See the actual
  [direct-eval compiler](../../crates/scriptw-engine/src/wasm_native_direct_eval_compiler.rs)
  and [Function compiler](../../crates/scriptw-engine/src/wasm_native_function_compiler.rs).
- A portable Wasm artifact does not bundle a parser, interpreter or JavaScript
  VM. Runtime strings outside its prepared sources require a suitable native
  compiler host; unsupported syntax, lowering or contexts remain capability gaps.

This does not establish full ECMAScript `eval`, `new Function` or cross-realm
dynamic-source compatibility. See the [current inventory](current-inventory.md)
for verification and remaining acceptance work.

## Out-of-scope tests

Standalone Wasm dynamic-source cases that would require bundling a JavaScript
parser, interpreter or VM into the emitted artifact are **outside product
scope**, rather than features planned for implementation. This includes such
uses of `eval`, `new Function` and cross-realm Function constructors.

The chart now classifies a conservative, audited subset by each test's primary
specification target: the two exact PerformEval ESIDs and eight individually
reviewed Function-constructor source-parser cases. This is **221 source files /
429 mode-specific executions** at the published suite pin: 413 eval and 16 Function
executions. It excludes the general dynamic-source promise of standalone Wasm,
even where finite prepared fixtures or the native compiler host can pass a test.
It does not remove supported prepared-source/native-host code or declare it broken.

The four direct/indirect non-string eval pass-through tests remain in scope: they
return their input without compiling source. Ordinary API-property tests, Function call/apply/bind, constructors used only to
set up another behavior, and incidental source mentions remain in the ordinary
measured categories. This is a known audited subset, not an exhaustive inventory
of every dynamic-source use. Parser/lowering failures in supported paths remain
failures; an Unsupported failure kind cannot decide scope. No excluded execution
is an existing failure or unimplemented result. The four remaining original
failures stay red.

[The original scope audit](test262-chart-reconciliation-20261021.json) binds every
selected source hash, execution ID, reason and ESID to the verified vendored tree.
The [current complete publication](test262-current.md) retains that scope inventory
and all original outcomes. Blue describes product scope; it does not claim those
native-host executions failed or remove them from the complete result.

## Refreshing the README chart

After a complete canonical publication,
`python3 -B scripts/generate-test-graph.py --refresh-evidence --full-evidence`
refreshes the README caption and [chart](assets/test262-status.svg) from its
portable native snapshots. `--check` verifies the evidence and generated outputs
without writing. The generator checks compiler/pins, exact disjoint execution IDs,
outcomes and the unchanged scope inventory. A scope decision cannot hide a
non-success; reviewed errata snapshots are never inputs.

Green is in-scope Success, red is Bug plus Crash, gray is NotImplemented, and blue
is the audited standalone dynamic-source scope subset. The graph uses the
[complete 2026-10-23 run](test262-current.md), with every execution bound to its
recorded compiler. Earlier baseline/replay reconciliation remains historical
evidence. A new compiler requires a new complete publication before its counts
can replace this result. Unrun tests do not become unimplemented tests, and no
scope decision silently skips execution.
