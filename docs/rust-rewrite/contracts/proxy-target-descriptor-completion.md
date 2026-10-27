# Proxy target descriptor completion

Status: the cloud continuation's joined all-feature/all-target type check and
all three native descriptor functions pass, including strict/sloppy snapshot
mutation controls. The original global-loop and exhaustive Float16 deadline
controls also pass without changing their limits. Complete broad verification
is pending; this remains a shared invariant correction within T11.

The 2026-10-27 Array extension is implemented with authored regression controls;
its native and pinned-case verification is pending.

Actual `OrdinaryObject` references now acquire a fresh completed GC descriptor
directly from their ordinary own-property table. The concrete reference type
test excludes every exotic object, including objects with ordinary headers;
the registry declares this struct final with no subtypes. This path invokes no
user code and retains the same semantic String/Symbol lookup and absence rule.
It removes the private native callable and JavaScript descriptor-object round
trip from ordinary Has/Get/Set consumers without caching a binding or skipping
any environment Reference operation.

The copied `PropertyDescriptor` record owns its flags and mutable value/getter/
setter fields independently from the stored property. Its `StoredValue` edges
can be shared because their tag, scalar and reference fields are immutable.
Returning the live stored descriptor is forbidden: later descriptor-conversion
getters can redefine the target after acquisition, while the current consumer
must retain the earlier descriptor. Each later acquisition scans the current
table again and observes those changes. Getter/setter invocation, receiver
identity, target extensibility and Realm-sensitive errors remain owned by the
existing consumers.

Concrete Array references use a typed Array-only storage descriptor lookup
for indexed entries, named and Symbol properties, and virtual length. The Array
indexed/length tail is shared with the public non-Proxy kernel; unrelated exotic
branches are not emitted beneath this concrete type proof. They then
use the same fresh-record copier as ordinary targets. A Proxy wrapping an Array
fails the concrete reference test and retains its trap path. No public builtin
FunctionObject, argument vector or JavaScript descriptor transport object is
constructed for these internal Array acquisitions. Absent entries remain absent,
and later consumers retain ownership of inherited Set behavior and definition.

Every other concrete target type, including namespace, arguments,
TypedArray, boxed primitive, Function and Proxy, retains the existing native
acquisition path. The actual Object.getOwnPropertyDescriptor owner handles
recursive target hooks and descriptor validation. The private
completed descriptor object is never exposed to user code. Its own data fields
supply the existing Fact, Get or Set projection without invoking property values
or adding another descriptor algorithm. The same target-descriptor factory
continues to serve the public Proxy descriptor algorithm's compatibility check.

Each of the four outer invariant consumers acquires a completed projection;
a reserved raw projection cannot enter that consumer boundary. A normal Get
trap result reaches target GetOwnProperty before SameValue/missing-getter checks.
Set false returns before lookup; Set true reaches the target descriptor before
frozen-data/missing-setter checks. Has true and Delete false return before lookup.
Has false and Delete true retain presence independently from descriptor bits;
an absent descriptor skips outer IsExtensible, while a present nonconfigurable
property fails before that query. Original target throws leave the outer
constraints unobserved and preserve the original completion.

These ordering requirements follow
[ECMAScript2026 Proxy clauses10.5.7–10](https://tc39.es/ecma262/2026/multipage/ordinary-and-exotic-objects-behaviours.html#sec-proxy-object-internal-methods-and-internal-slots-hasproperty-p).
Literal global declaration descriptor acquisition retains its existing direct
Fact path and declaration policy.

Actual callers use the existing trusted Proxy execution or set-path Realm
projection. The native fallback's builtin entry and private input encoder carry
that projection into the real descriptor call. A lexical environment is never
interpreted as a native Realm record. Planning retains the descriptor builtin
for every emitted caller, including source operators and outlined helpers.
Nested descriptor errors and outer Get/Delete invariant errors use the executing
operation's intrinsic TypeError. Inline source Delete propagates generated
errors into the active catch/finally handler; builtin/helper completions retain
their existing return behavior. Existing original user throws remain unchanged.

The additional strict/sloppy ordinary snapshot control redefines data and
accessor properties during observable trap-result descriptor conversion, for
both String and Symbol keys. The current compatibility check retains the earlier
descriptor while later Get/Set and descriptor acquisitions see the new frozen
attributes, values and accessor identities. This control specifically rejects
an aliased mutable descriptor and a cached descriptor reused on later reads.

The Array controls additionally force both packed-number and full mutable indexed
storage. A Proxy trap-result value getter freezes and changes the target after
its internal descriptor was acquired. The current compatibility check observes
the captured record; a subsequent query sees the frozen property and rejects
the same report. Adjacent controls cover inherited setters, reentry, accessor
edges, present Undefined, length failures, nonextensibility, index boundaries,
revocation, foreign error realms and public-intrinsic/prototype poisoning.

The strict/sloppy Engine controls require actual WasmAot, normal Number262 and
one final print. They cover all four operation orders, String/Symbol key identity,
original target and outer-trap throws, early Boolean outcomes and missing
properties, frozen/accessor and SameValue NaN/signed-zero constraints. Both
borrowed Reflect Realm directions and source operators retain native error
prototypes after public foreign intrinsic bindings are replaced. These controls
passed within the [4,549-case native checkpoint](../memory-array-native-input-20261027.md), including
all 209 GC-entry cases and 1,092 original selected Test262 executions. Current scalar layouts, semantic GC migration,
full descriptor-lattice obligations and complete T11/Test262 acceptance remain
separate open work.
