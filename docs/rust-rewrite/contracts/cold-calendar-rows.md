# Deferred Chinese/Dangi retained-year rows

R retains both immutable 8,357-row calendar tables in passive segment 3.
Their 133,712 bytes keep their canonical reserved linear-memory addresses.
The logical pool, provider rows, signed month prefixes and calendar algorithms
are unchanged. Programs that never read retained rows do not initialize them.

The image's sole executable row-address accessor emits an instance-local
readiness check and `memory.init` before returning the address. Both direct
readers use it inside their retained-data branch: year selection and month
serial binary search. Proleptic paths embed header constants and need no copy.
The readiness flag is published after the complete copy; this sequence cannot
allocate a GC object, coerce a value or call JavaScript. R keeps the passive
segment available. Realms in one R instance share the installed rows; a fresh
instance starts unready, including after another Store has used its Module.

Layout 30 declares four passive images followed by four active fragments.
UTF-16, normalization and Unicode properties retain indices 0, 1 and 2.
P's independent segment 3 remains its compact-array recipe image. Producer
metadata and raw artifact admission require all eight R roles and reconstruct
the exact logical arena in virtual-address order. Partial test pools preserve
the roles with empty spans ordered before the following real region.

The static active-byte reduction is 366,448 to 232,736 bytes for the measured
Portable runtime. It establishes neither resident savings nor Scriptc parity.
Fresh native verification and a matched comparison are pending.
