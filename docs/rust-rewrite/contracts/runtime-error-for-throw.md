# Shared native error preparation

Messageful compiler throws share one runtime-owned allocation and diagnostic
kernel. Its argument is the existing closed `NativeErrorKind` enum, followed by
the caller-selected prototype and a rooted, nonnull message String. A private
parameter marker compares only enum variants. A program string ordinal cannot
be passed as a runtime error name, and an invalid wire kind traps.

The caller retains execution/defining/explicit Realm selection and resolves its
message through the original R or P string domain. The order remains prototype
selection, message resolution, fresh error allocation, diagnostic publication,
then caller completion and branching. The supplied message stays rooted and is
reused instead of looking it up a second time.

The shared kernel calls the allocation-only `RuntimeErrorObject` helper first.
After allocation it clears only the Name and Message diagnostics, selects a name
from the nine enum-derived R string seeds, and publishes that name and the
supplied message. It returns the same fresh error object. ConstructorName is
unchanged, and completion/control flow stays outside this kernel.

Normal cancellation still uses the allocation-only helper. Message-less
TypeErrors and native-parser SyntaxError paths retain their existing emission.
The GC object model, descriptors, prototype identity and source ownership remain
unchanged. The additional helper ABI requires runtime layout 32; incompatible
runtime artifacts are rejected.

Native verification and a fresh compiler-disabled Scriptc comparison are
pending. Removing repeated emitted code does not itself establish resident-memory
or execution-time savings.
