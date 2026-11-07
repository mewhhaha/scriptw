# Ordinary generator ForIn native enumeration

The checked ordinary-generator carrier retains four distinct actual invocation
allocations: the completed raw source, one compiler-private native enumeration
edge, the selected String key and StatementList value V. Its source head,
advance, body and exit ranges remain the sole continuation authority. The
complete original per-key binding or Reference/pattern initialization is eager
and runs once after advancement; a resumed body never repeats it.

One `ForInEnumerationRecord` in the existing Wasm GC schema owns a boxed current
object/prototype StoredValue, a nullable accepted PropertyKeyTable snapshot, an
i32 next index and an immutable strong edge to a private canonical SetObject.
Only String keys enter its hashed storage. Null current means completion;
absent remaining keys means this prototype level has not observed OwnKeys yet.
There is no JavaScript iterator or separate object model. Nullish sources are
completed cursors; other primitives are boxed once after source evaluation.

The actual native create/advance methods are shared by the three existing eager
ForIn variants and the complete ordinary-generator consumer. OwnKeys is lazy
per prototype. The next index is committed before descriptors or a selected
body can run. Symbols and visited strings are skipped. A missing descriptor
does not shadow the prototype; an existing nonenumerable descriptor does.
Visited publication follows descriptor existence and precedes enumeration.
Getters of ordinary properties are not invoked. Prototype lookup occurs only
after this level's snapshot drains and retains the original whole Throw.
CompletedOwnPropertyKeys transfers its accepted table into the cursor; its
original key reader also serves retained snapshots, with no arbitrary
reattachment constructor that can manufacture a completed-list proof.

The cursor's only persistent root is the appended nullable BindingCell field
in the original InvocationFrame.INVOCATION_ENVIRONMENT. With and nested lexical
records cannot redirect this storage. Ordinary Yield and delegated done:false
retain the same cursor; matching Continue keeps it. Normal exhaustion, Break
and outward whole completions retire it. ForIn performs no IteratorClose.
Array-pattern head assignment continues to use its own ordinary iterator
acquisition/close semantics; those do not turn enumeration into an iterator.

The original head TDZ environment is allocated once or reattached before any
resumed injected completion. It is left and the restored outer environment is
saved before cursor creation. Each selected key enters its original per-key
environment; a resumed body reattaches that same record before injected
Return/Throw. A whole cleanup scope exists first, so nested finalizers finish
before outer transfer and the original outer environment is saved on exit.

The persistent statement-value owner has a closed Switch/Iteration purpose.
Only the complete source owner can supply its retained cells. Head and per-key
initialization do not publish fictitious body results. Actual body statements
checkpoint and restore V through existing sequence and Empty-source-item
wrappers; nested scopes retain their original completion behavior. Normal body
completion and matching Continue meet before the next advance.

Encoded controls consume the paired source semantic cohorts. They follow the
actual 13-field BindingCell edge to the four-field cursor, accepted key table,
and canonical Set entries containing StoredValues. They require real cursor
creation, current/snapshot/index transitions, an immutable visited-Set edge,
and invocation-edge publication, reload and retirement. The complete
[resumable family](resumable-for-in-family.md) shares this native owner.
