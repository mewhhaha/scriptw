# Proxy OwnPropertyKeys result ownership

## Internal List boundary

`ObjectOwnKeys` is the sole shared runtime kernel for accepted own-key Lists.
Its typed arguments retain the target Value, executing Realm, Environment and
Program. Its result is the whole five-part Completion followed by a nullable
`PropertyKeyTable` GC reference. The table is null on Throw. The caller roots
both outputs before propagating the original Throw; only Normal completion
admits a nonnull table as `CompletedOwnPropertyKeys`. Every kernel throw branches
to the extended-result exit rather than returning the JavaScript-call ABI.

Proxy fallback and target validation call this same helper recursively. The
kernel is emitted once, with no Rust emitter recursion. ForIn, CopyDataProperties,
JSON and Object consumers retain the accepted List directly. Public own-key
array producers materialize their specified Arrays after successful acquisition;
internal consumers do not manufacture a Reflect callable, a result Array or its
property descriptors merely to reconstruct the private List.

The existing Proxy acquisition, duplicate checks, target observation order,
descriptor checks, arbitrary abrupt identity and executing-Realm errors are
unchanged. ForIn still snapshots lazily per prototype and checks live descriptors
before marking a String visited. This introduces no new heap representation.

Runtime layout 49 rejects layout 48 caches and build packages because the typed
helper changes the shared function/type layout. Raw cache keys also include the
compiler identity and layout version; the runtime key hashes its bytes and
layout version. `own_keys_helper` checks emitted R/P signatures, recursive calls
and internal consumers; existing Proxy and ForIn semantic cohorts remain the
runtime correctness gates. Verification of this change is pending.

## Completed list authority — 2026-10-03 dry source

The existing typed pending trap result now feeds the actual private
`objects/proxy_own_keys_list.rs` factory. It first rejects a non-object result,
performs one ordinary Get of length and ToLength, and captures an ascending
ordinary indexed Get for each position. Actual Arrays use the same observable
Get path, so accessors and inherited indices run. Each value is immediately
checked as String or Symbol, and abrupt completion propagates before later work.
No public result is the trap's mutable object.

The complete private snapshot precedes duplicate checking. String keys compare
content and Symbols compare identity. Thus a later indexed getter's abrupt
completion precedes every duplicate error, while an invalid earlier key prevents
later Gets. Target validation follows the completed-list phase described below.

Only successful completion of acquisition, key and duplicate validation and
the completed target checks mints the private non-Copy
`CompletedProxyOwnKeysList`. Its length and storage cannot escape this module.
Reflect publication consumes it directly. Object names/symbols filtering reads
its snapshot and consumes it in final publication. Factory scratch releases
in reverse, retaining only the owner pair; publication releases that pair before
the filtered caller's earlier scratch. The existing three public producer
signatures and distinct target/trap/pending-result roles are unchanged.

Native list-validation errors use the called builtin's defining TypeError Realm,
and fresh snapshot/filtered result Arrays use that builtin's defining Array
Realm. This reuses the real current-function Realm helpers and immutable
intrinsics; mutable globals and public constructor properties supply no fallback.

The new paired strict/sloppy WasmAot/Test262-host Engine controls observe live
accessor/inherited/Proxy Gets, length coercion, captured-key mutation, immediate
type checking, delayed duplicate checking and arbitrary foreign abrupt identity.
They retain existing ordinary/nonextensible/nonconfigurable target controls and
check borrowed foreign native errors and result Array prototypes. Normal262 and
the sole fixture print are required. They are authored controls, not executed
evidence. Existing structure guards follow actual owner relocation without new
guard families. No compilation, parser/runtime, Wasm validation, Test262 suite
or status refresh ran for this batch. Full T11 and semantic GC remain open.

## Completed target authority — 2026-10-04 dry source

After duplicate validation, the target phase calls the existing Proxy-aware
IsExtensible operation once and retains its result. It then calls the real
shared ObjectOwnKeys helper directly for target OwnPropertyKeys and the real
Object.getOwnPropertyDescriptor implementation for each captured target key.
These calls consume the existing metadata dependency fixpoint and outer trusted
Realm context; mutable public Reflect/Object properties are not operation
providers. Existing exotic and nested Proxy operation owners supply their real
keys and descriptors. The former raw ordinary/Function/Array and namespace-only
constraint branches are retired, including the callerless namespace helper.

Every descriptor lookup must complete before any missing-key constraint. A
later descriptor's thrown value therefore wins over an earlier missing
nonconfigurable key. A missing descriptor is classified with configurable keys,
as the specification requires. Only the full descriptor pass can mint the
private non-Copy classified-target owner, which retains the initial
extensibility result, actual target-key snapshot and nonconfigurable-key list.
Its consuming constraint phase compares String value and Symbol identity,
requires all nonconfigurable keys, and requires the exact captured target set
when the captured extensibility result is false. Descriptor mutation neither
rechecks extensibility nor rebuilds target keys.

The existing outlined IsExtensible helper receives the trusted outer execution
Realm context in its previously unused parameter6. Its body installs that
context in the existing current-environment local. The existing closed Proxy
execution and object-read error source projections recognize that helper;
recursive extensibility calls, handler Get and trap dispatch forward the same
context. Revocation and generated noncallable/inconsistent-result errors select
that Realm and retain active-handler completion routing. Arbitrary trap/getter
throws propagate without replacement. Target validation retains the original
Realm authority and semantic object representation.

Two additional finite strict/sloppy Engine fixtures cover ordered target
operations and all-descriptor abrupt precedence, captured extensibility and key
lists across mutation, boxed String/Function/Array/TypedArray targets, and all
three consumer methods in both borrowed Realm directions. Mutable foreign
globals are replaced after saving intrinsic identities. Generated native error
prototypes, arbitrary foreign thrown identity, prior assignment/finally effects
and successful result Array prototypes are asserted. The existing three list
fixtures are retained. Source review and Rustfmt are source checks; these
controls and compilation remain unexecuted. Full T11, semantic GC and unrelated
Proxy trap families remain open.

The ordering follows [ECMA-262 2026 Proxy OwnPropertyKeys](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-proxy-object-internal-methods-and-internal-slots-ownpropertykeys).

## Authority

The prospective Proxy `[[OwnPropertyKeys]]` trap and its call-result destination
are distinct, non-copyable `ProxyOwnKeysTrapLocals` and
`ProxyOwnKeysTrapResultLocals` roles. Previously both adjacent arguments were
`TaggedLocals`, so transposing them compiled: trap lookup and invocation could
write into the wrong scratch pair while post-trap validation read the other
pair.

Each of the three Object/Reflect producers (`Object.getOwnPropertyNames`,
`Object.getOwnPropertySymbols` and `Reflect.ownKeys`) now gives the result
authority to the single acquisition emitter, receives that same authority back,
and consumes it once in the corresponding post-trap validator. `Object.keys`,
`Object.values` and `Object.entries` share one EnumerableOwnProperties owner
that calls the `ObjectOwnKeys` helper and so acquires no trap of its own.
Validators also accept the existing distinct `ProxyTargetLocals` instead of
adjacent raw payload/tag arguments. Trap, target, handler, and result roles
therefore cannot be transposed at these boundaries.

## Durable evidence

`proxy_own_keys_handler_protocol_structure` uses a Rust lexical identifier
census that excludes comments and ordinary, raw, byte, C-string, character,
and byte-character literals. It pins both exact role types, their lack of Copy
or Clone, the sole acquisition, all three producers, the returned ownership
transition, and exactly one typed validator consumption per producer.

On 2026-08-27, its seven focused structure tests passed, as did
`cargo check -p scriptw-aot-wasm --lib`. The exact `wasm_proxy_own_keys.js` and
`wasm_proxy_own_keys_handler_protocol.js` CLI witnesses each passed `1/1` on
the Wasm-AOT backend. Rustfmt's check mode and `git diff --check` also passed
for the scoped source, test, task, and contract files.

The typed trap, target and result roles preserve Proxy lookup order, fallback,
revocation, calls and validation. This contract does not claim complete
Proxy/Reflect conformance.
