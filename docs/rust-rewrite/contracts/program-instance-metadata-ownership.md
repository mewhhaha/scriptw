# ProgramInstance metadata ownership

This ABI6 component is prepared and unapplied. It has not been compiled or run
through native execution, schema validation or the two-program acceptance gates.
It must join the explicit entry owner, typed hooks, literal producer, host
compiler, retained jobs and module ownership work before source integration.

P metadata is copied from its own passive segment into the ProgramInstance's
immutable GC ByteArray before Realm or source execution. `ProgramMetadataSpan`
is minted against the actual StringPool runtime boundary and final image size.
It excludes the R prefix, uses P-relative offset/length and rejects overflow and
out-of-image extents. Compile-time collection locators may retain absolute
storage addresses, but executable P readers cannot use those as linear pointers.

`ProgramMetadataView` roots the actual P byte owner and validates the span with
unsigned i64 arithmetic before reading. Byte and little-endian u32/u64 reads
check each requested extent against that view and use actual GC array reads.
The wrapper exposes neither shared-memory loads nor writes to published bytes.

Numeric call rows bind a view of their complete table. They still perform the
original live identifier Reference lookup before each argument row, preserve
receiver/coercion order and propagate the original whole call completion. Raw
Bytes Module contents similarly read their exact bytes from the defining
program view while allocating the ECMAScript buffer in its existing module Realm.

RegExp candidate records retain the exhaustive Program/Rejected/Unsupported
outcome domain and value comparisons. Their five u64 words are source literal
operand, flags literal operand, relative descriptor payload, outcome and explicit
Program descriptor domain. A Program row cannot select an arbitrary R or P
linear address. If compilation deduplicates a descriptor to R's prefix, the P
writer copies that small validated descriptor into P metadata before encoding
the row. R's original bytes and fixed Unicode/folding tables remain unchanged.
Publication checks the descriptor domain and span, then copies the actual P GC
bytes into the existing unpublished RegExpProgram construction. Rejected and
unsupported entries keep their distinct syntax-error/fallback behavior.

Direct RegExp literals already publish a GC ImmutableByteArray from their exact
validated bytes with `array.fixed`; that path has no P static-memory pointer and
is preserved. Serialized descriptor internals remain relative to their own
descriptor; they do not embed program pool or linear-memory string addresses.

Embedded source/flags and compact-array strings now encode a closed R/P domain
in the high word and a table-relative ordinal in the low word. Runtime reads
validate the domain and bounds, select R's stable prefix or the actual P's
literal table, resolving an unpublished P slot through that P's own resolver,
and require a non-null String. A P operand never indexes the old
combined mutable R+P table. Compact-array recipes retain their Program-owned
passive-to-GC path, complete pure-walk admission, exact scalar bits and source
order. Their helper gains a required explicit caller ProgramInstance operand.

Removal of P `memory.init` and the P-specific shared scratch-cursor reset belongs
to the module/bootstrap owner. It is allowed only after these readers and the
sealed literal/entry joins are present together and P metadata is published
before any consuming source path. Keep R's active static prefix, runtime Unicode
and calendar images, and the transient runtime scratch allocator. This receipt
does not itself remove or accept that writer change.

Pending tests include the new Rust typed-span/domain and R-descriptor-copy
controls, all existing numeric row/compact array/module/RegExp regressions and
the unchanged two-P conflict/transport controls. Native acceptance must collide
P offsets and literal ordinals, execute P1 after P2 initialization and main,
reverse order, reenter through native callbacks and collect GC while retaining
old functions, modules and resumptions. Source-only format/wiring checks do not
establish that semantic result.
