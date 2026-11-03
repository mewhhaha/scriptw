# Immutable native compiler pool

One exact `Arc<RuntimeArtifact>` owns a bounded `OnceLock` containing its
compiler-owned pool. Only `ValidatedRuntimeArtifact` can initialize it after
admitting that R's current layout and physical Intl profile. Initial collection
checks the complete pool boundary and compares both canonical data images with
the actual R: passive UTF-16 followed by active static bytes at the declared
memory offset. An initialization failure remains attached to that R.

Freezing moves the byte images and ordered string map into immutable `Arc`
storage. A program shares those owners and starts with empty appendices. It
retains the original virtual static/UTF-16 offsets and literal ordinals; lookup
deduplicates against R before adding a P-local entry. Read-only Intl, Unicode,
Temporal and RegExp metadata resolve through the retained prefix.

The existing checked raw APIs preserve their diagnostics, exact source/caller
witnesses and layout/profile checks. Function, global Script and direct eval
pass the same admitted proof into actual emission. The public proof APIs keep
their signatures. P publication reads only its own appended images and literal
descriptors. No GC layout or R/P wire version changes.

After collection, P may compact its passive UTF-16 suffix by sharing aligned,
overlapping slices. Literal ordinals, WTF-8 metadata addresses and R's immutable
prefix stay fixed. Every descriptor retains its exact length and code units;
String construction still uses the existing GC representation. Search work is
bounded, and exhaustion appends the original bytes. Packing is published only
when the physical image shrinks.

Authored controls compare P images and ordinals against fresh collection,
assert actual prefix storage identity and isolation between P appendices,
reject changed same-size R data, and verify that multiple proofs and all three
native source kinds retain the one R-owned prefix. These controls require the
coherent batch's Rust and native verification; source checks alone do not
establish performance or conformance results.
