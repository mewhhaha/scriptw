# Throw-routing capability boundary

The current product path uses rooted whole `CompletionLocals`. The consumed
Proxy dispatcher is `functions/call_dispatch.rs::emit_proxy_call_dispatch`;
its caller selects actual propagation through the closed `PropagateCallThrow`
domain. Indirect operations retain the completion for their enclosing handler,
so user `catch`/`finally` and arbitrary thrown values remain observable.

After a checked Throw branch unconditionally leaves for its actual handler or
epilogue, straight-line fallthrough proves that this exact kind local is not
Throw. A later identical guard can be omitted while that fact remains valid.
This does not prove Normal or change the five-part Completion ABI.

The Wasm sink owns every emitted instruction and invalidates the fact on writes
to the local, local retirement or reuse, and structured control boundaries. No
fact crosses a loop header, control-flow join or exception handler. Calls cannot write caller
Wasm locals; storing a call's completion invalidates the fact. Original Throw
values, cleanup order, Realm selection and branch destinations retain their
existing owners. Encoded controls check invalidation; Engine regressions cover
arbitrary throws, callbacks, forced GC, finalizers, loops and IteratorClose.
