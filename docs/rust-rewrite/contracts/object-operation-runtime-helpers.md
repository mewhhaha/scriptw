# Shared object-operation runtime helpers

The public object emitters consume typed runtime helpers for
Get, GetPrototypeOf, SetPrototypeOf, Delete, IsExtensible and PreventExtensions.

The six physical kernels are private methods in `objects.rs`. Their typed
operation facades and registered compiler entries live together in its private
`objects/runtime_helpers.rs` child, so
other compiler modules cannot enter a physical kernel. Each compiler begins its
actual registered helper body, loads that row's typed parameters and enters
the kernel directly. Ordinary callers enter a facade that emits a typed helper
call. No facade flag, caller-selected inline path or new runtime representation
is needed.

The subsequent [ordinary allocation and explicit Realm ToObject closure](ordinary-allocation-and-to-object-runtime-helpers.md)
outlines the remaining shared header allocation and primitive-boxing operations
consumed inside these kernels. Realm selection and caller completion routing
remain at their original operation boundaries.

| Helper | Original operands | Result |
| --- | --- | --- |
| ObjectRead | whole target, whole receiver, property key, caller Environment | whole Completion |
| ObjectGetPrototypeOf | whole target, caller Environment | whole Completion |
| ObjectSetPrototypeOf | whole target, whole prototype, caller Environment | whole Completion |
| ObjectDelete | whole target, property key, caller Environment | whole Completion |
| ObjectIsExtensible | whole target, caller Environment | whole Completion |
| ObjectPreventExtensions | whole target, caller Environment | whole Completion |

Get preserves the distinction between lookup target and accessor receiver.
`emit_object_read_with_throw_routing` calls the same registered ObjectRead
facade, then applies the original closed routing choice: leave the whole result
in the supplied Completion, or copy it into the caller's Completion and route
its Throw. The physical Get kernel does not own a caller's abrupt exit. Proxy
slot validation still precedes trap acquisition, nested targets retain the
receiver and key, and the original descriptor and accessor algorithms remain
inside that kernel.

Actual ordinary objects and FunctionObject headers use one rooted own-property
observation: absent, data value or getter. Compact, intrinsic and dictionary
storage are queried once; getters retain the original receiver, and intrinsic
recipes resolve in their defining Realm. Primitive boxes retain the descriptor
fallback so virtual String indices and length remain visible. Arrays retain
their data fast path; other exotics retain canonical descriptor acquisition.
The same observation drives the shared getter/prototype continuation without
allocating an ordinary descriptor snapshot or resolving an unused setter.

Each helper retains its original nullable caller Environment parameter. Helper
entry installs that exact parameter before the physical operation. Error Realm
selection continues through the original defining Environment/active Realm
authority, and primitive boxing still reads the current runtime Realm. The
repair does not replace a caller's completion with an accessor result or
manufacture a new Realm owner. Parameters remain rooted until the complete
result is published.

The registered ObjectReadProxy row remains present. Its compiler forwards its
target, receiver, key and caller Environment through ObjectRead instead of
emitting another full Get kernel. Proxy fallback recursion and the other object
operations retain their existing typed helper edges; the registered compilers
do not enter their own public facades.

The source controls retain the created-Realm revoked-Proxy witnesses and pin
private physical ownership, row-bound compiler entry, receiver forwarding,
whole-completion publication and the original throw routing. Runtime coverage
should include the existing Proxy helper Realm fixtures, reflection handler
protocols, array deletion and JSON reviver behavior alongside the queued Array
lifecycle cohort. Artifact coverage should require one emitted body for each
of the six helper names and a direct caller outside that body. Existing body
size limits remain unchanged.
