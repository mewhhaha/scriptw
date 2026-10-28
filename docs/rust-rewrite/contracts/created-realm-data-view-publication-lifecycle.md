# Created-Realm DataView publication lifecycle

## Current atomic GC source — 2026-10-05

The common completed GC bootstrap owns DataView constructor/prototype/member
publication for both entry and created Realms. NativeHost consumes it through
`emit_created_realm_global_object`, rather than a separate raw DataView
publication loop. Whole values, typed FunctionContext capture and the actual
Realm intrinsic table supply identity, descriptors and defining-Realm errors.

The maintained `created_realm_data_view_publication_structure` target retains
the existing CLI fixture witness. Its old exact token/borrow/hash/call-count
publication pins below are historical. Native GC binary-data controls are
separately authored; see [DataView access](data-view-access-owner.md).

All source, types and controls for the atomic batch remain uncompiled and
unexecuted. Final representation/helper/guard composition also remains pending.
Earlier verification commands and results below retain their original source
scope; they are historical records, not instructions to run during the full-task
dry-source pass. Later verification follows the [batch workflow](../../../CONTRIBUTING.md#verification)
with a confirmed aggregate 4096 MiB cap, swap zero and serial execution.

## Historical predecessor record

Status: normative and focused-verified for the Wasm-AOT created-Realm DataView
prototype publication plan in Batch AK.

## Boundary

Created realms publish the complete implemented DataView prototype from one
ordered constant: three accessors, twenty-two methods and `@@toStringTag`.
`CreatedRealmDataViewPrototypePublication` owns whether a row is callable or
the tag property. Each callable row carries one
`CreatedRealmDataViewPropertyKind::{Accessor, Method}` selection, so property
name derivation and descriptor emission cannot select those roles separately.

Batch AK gives the plan one move-only publication lifecycle. The twenty-six
publication rows are consumed by the sole installer loop. A callable row moves
out its one property kind, and two borrowed property-kind decisions use that
same value in order: the first derives the accessor or method property name;
the second emits the accessor or data descriptor. Clone, copy, debug, default,
comparison, hashing and ordering capabilities are absent from both private
domains.

The strengthened structure guard pins the exact two-variant domains, three
type mentions for each authority, the `25/1` callable/tag and `3/22`
accessor/method producer splits, one consuming publication match, two borrowed
property-kind matches, and the complete publication order shared with the
main-Realm installer. It also pins the absence of aliases, clones,
dereferences, mutable borrows and additional consumer loops.

## Source equivalence

The attribute-free property-kind and publication domains remain
`05b1434c91f260120b859796e4559a91dd99b700175a46795a5939ec2658076f`
and
`e233d91ed76449afd337efb3f62fd5bb53e8f786cb210d707ca3bdea98158250`.
The exact twenty-six-row plan remains
`3f6cf64e59462fa7274fceba69b9edd226b899bd8dfa13c9806fe8c80844d933`.
The borrowed installer loop is
`097601ce49d1ce65170cc10ce4306e78b577d4bb12b75a364f51dccc4c154202`;
its whitespace-normalized form is
`8a3362c4160a67ba86c812101fac126e16fbb50a1feb5f3e8ce363b693fad01b`.
Erasing only the two borrow markers produces
`bbb73df09787395a302b040e64d27359e7a17cc391cf7eceaef29bde128f291d`
and the frozen semantic fingerprint `(2516, 0x608ef2c4a91e5569)`.

No callable allocation, Realm capture, native-name lookup, property key,
descriptor attributes, publication order or emitted instruction changes.

## Evidence and nonclaims

At the 2026-08-28 Batch AK checkpoint, `cargo xc` is green, the four-test
structure target passes `4/4`, and the exact created-Realm DataView CLI fixture
passes `1/1`. The focused DataView accessor, method-name and `@@toStringTag`
Test262 leaves pass all `6/6` Wasm-AOT variants with every failure bucket at
zero. No semantic golden was required or run.

This invariant does not change the main-Realm DataView installer, add methods,
prove complete DataView or resizable-buffer semantics, retire a Test262
rewrite, or change published conformance counts.
