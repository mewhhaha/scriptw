# Temporal GC values and completed calendar/zone records

## Current atomic source — 2026-10-05

The Temporal source draft uses concrete GC records for Instant, Duration,
PlainDate, PlainDateTime, PlainTime, PlainYearMonth, PlainMonthDay and
ZonedDateTime. Public JavaScript inputs, option values, acquired month-code
Strings and abrupt completions retain their complete tag, scalar and reference
components. ISO coordinates, duration Number bits, epoch parts and native
calendar/zone domains use their actual typed scalar locals. No Temporal object
address or payload offset is a semantic carrier.

Conversion tests the concrete GC record before reading a property bag. Branded
Date/DateTime/YearMonth/MonthDay/Duration conversions therefore ignore poisoned
public getters. A branded ZonedDateTime conversion recovers its retained epoch,
zone and calendar, obtains a completed provider snapshot and reads the resulting
ISO record. The collector keeps those references through Calls, option getters
and explicit collection; record publication uses the defining intrinsic Realm.

Calendar acquisition produces a canonical GC identifier with its native calendar
domain. Field preparation preserves alphabetical Get/coercion order. The era
pair is acquired only for calendars that have eras. Month-code acquisition
performs ToPrimitive with the String hint, requires a String result and validates
UTF16 syntax before the later year Get or overflow option. Calendar suitability
is resolved later. A completed year proof is consumed by month resolution;
the acquired original String and the encoded original month code remain distinct
from the resolved month ordinal. Partial-field merging preserves the receiver
defaults and the exclusion between supplied era/year and month/monthCode fields.

Duration acquisition keeps all ten Number fields and their specified order.
Immediate conversion/integral errors stop the sweep; deferred sign/range
validation follows the completed reads. Compare converts both operands before
relativeTo options. Round and total retain their complete option order and the
real relativeTo record. Calendar arithmetic, elapsed arithmetic and full-range
date rounding use the existing separate day/second/nanosecond scalar kernels.
They are not replaced by a manual semantic heap or an evaluator.

Epoch milliseconds floor negative epochs. Instant formatting first splits the
exact epoch into the appropriate civil second and nonnegative fractional tail;
rounding keeps the existing as-if-positive rule. ISO and duration parsers consume
GC UTF16 Strings directly. Named-zone requests and replies retain their concrete
GC byte arrays; validated provider response owners bound word/String reads and
preserve the existing transition and inverse-disambiguation algorithms. Completed
calendar/zone/epoch proofs are required at their consumers.

Throw-bearing branches participate in the builder's control-frame ledger,
including retained Duration and Date arithmetic. String/JSON dispatch selects
each Plain family's closed native mode; JSON does not accidentally observe the
String method's options. Constructors finish argument/calendar validation before
newTarget.prototype observation. Native failures use the retained defining
Realm, including after nested user Calls; user throws preserve their identity.

The existing Temporal Engine, CLI and pinned-source controls are retained.
The new `aot_gc_temporal_entries` target adds three finite paired strict/sloppy
cohorts for record roots and Duration order, negative epochs and UTF16 abrupt
prefixes, and named-zone transitions and borrowed Realms. These controls and the
atomic source are uncompiled and unexecuted. Source review and isolated Rust
formatting do not establish Wasm validity, runtime correctness or conformance.

Finish the complete task batch before verification. Use the
[batch workflow](../../../CONTRIBUTING.md#verification) and the confirmed 4096 MiB aggregate
cgroup budget, no swap, grouped OOM termination and serial workers. No larger
budget or uncapped fallback is allowed. The separate weak/ephemeron capability
gap remains explicit under the [weak facility boundary](weak-unavailable-runtime-boundary.md).

## Independent scalar constant initialization — recovered source

The fixed-offset parser, ZonedDateTime property-bag defaults and calendar
year/month difference initialize each scalar through its own typed local
set_constant operation. One producer cannot accidentally feed two consuming
LocalSet instructions. I32 uses the same plain initializer for the real RegExp
lastIndex descriptor flags. The GC UTF16 reader already requires explicit
String/index/output locals and retains that interface.

The existing finite zoned cohort now includes positive/negative HH, HHMM and
HH:MM offsets, malformed and out-of-range forms, absent/explicit property-bag
offsets, a year-largest calendar difference and a foreign defining-Realm error.
The wrapper's three paired cohorts and all existing assertions remain. These
new controls and source corrections have not been compiled or executed. The
prior MAIN135 type check does not establish their Wasm validity or runtime
correctness. Recovery re-authors vanished staged controls and does not recreate
or claim historical seal hashes.
