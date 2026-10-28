# T22 named-zone consumer batch

This document describes the prepared complete source batch. Rust compilation,
Wasm validation, native product tests, pinned replay and broad verification
remain pending. Source audits and independent reference observations establish
no product pass or published conformance result. T22 and T23 remain in progress.

## Observable behavior and actual owners

An available IANA name retains its zone identity through construction,
projection and each later calendar operation. A supplied numeric offset cannot
replace that name. Offset-at-instant, possible-instant lookup and strict
next/previous transition queries share the pinned provider's actual selector.
Aliases retain their observable Identifier; PrimaryIdentifier serves equality.
UTC and numeric fixed-offset paths retain their pure behavior.

| Source owner | Responsibility in the complete batch |
| --- | --- |
| `scriptw-intl` named-time-zone provider, exact query and gap-topology modules; Engine `intl_time_zone_host.rs` | Data-only exact offsets, complete variable candidate lists and transitions from pinned TZif/POSIX data; checked capacity/span protocol and an enforced catalogue gap certificate. |
| `scriptw-aot-wasm` `temporal_zone_provider` and its identity/exact/policy/responses/relative children | Brand, epoch, zone and calendar proofs; one-pass property/option conversion; all-candidate Instant checks; offset matching, disambiguation and distinct GetStartOfDay; validated allocation inputs. |
| `temporal_zoned_arithmetic.rs`, `rounding.rs` and `difference.rs` | Retained-zone calendar addition, exact elapsed addition, raw date differences, string rounding and correlated day rounding without replacing calendar days with fixed elapsed hours. |
| `temporal_duration_methods.rs`, `temporal_duration_relative.rs` and arithmetic `relative.rs` | Zoned relativeTo origin/zone/calendar retention, actual calendar-window inverse queries, RoundRelativeDuration/Total/Nudge/Bubble and shared once-rounded exact rational totals. |
| ZonedDateTime methods/with/day/format leaves and Instant/PlainDate/PlainDateTime conversion leaves | Real projection, getters, transitions, field replacement, day operations, differences, formatting and zone conversion callers; private context guards keep inactive relativeTo branches from exposing proofs. |
| `intl_datetimeformat` initialization/provider-render/zoned-locale seam and the ZonedDateTime format caller | The original locales/options are observed in Wasm; receiver zone and calendar are forced into the shared intrinsic formatter, with forbidden supplied timeZone behavior and exact negative-fraction floor milliseconds. |

The host accepts primitive data records. It does not receive JavaScript objects
or source, decide property order, run disambiguation policy, or construct JS
results. Strict proof ownership and LIFO scratch release connect each accepted
value to its actual validation and prevent raw local IDs from becoming epochs,
zones or allocations by relabelling.

The native provider retains raw inverse candidates even outside the Instant
interval. Wasm performs the prescribed check on every candidate before choosing
one; a selected valid candidate cannot hide an invalid neighbor. Certified gap
endpoints implement globally nearest nonempty local records; Wasm still checks
their exact epochs, shifts the local record and queries again. GetStartOfDay is
separate from compatible midnight interpretation. Rounding selects its new
exact epoch before requesting the corresponding offset.

PlainDate.toZonedDateTime has one argument and observes zone before plainTime.
Absent time uses GetStartOfDay; explicit time completes ToTemporalTime and the
prescribed ISO limits before compatible inverse selection. PlainDateTime
retains its actual Disambiguation option. RelativeTo bags read calendar first,
then the calendar-sensitive alphabetical fields once, including era/eraYear
where required; they never call another bag converter after those reads.

ZonedDateTime.toLocaleString keeps the exact Instant, resolved zone and actual
calendar instead of delegating through a PlainDateTime. User options retain
their specified order and abrupt completions. A nonundefined timeZone option
is rejected at its prescribed read. Negative fractional epochs use floor
milliseconds, including −1 nanosecond; the formatter uses the zone at that
formatted epoch and retains locale/calendar compatibility rules.

## Limits and failure ownership

The admitted Temporal calendars remain `iso8601`, `gregory`, `buddhist`, `roc`
and `japanese`. Their closed arithmetic domain uses stored ISO records with
proleptic Gregorian month/day arithmetic and the supported year/era mapping.
Additional Temporal calendar algorithms remain open. T23's wider Intl calendar
or locale data does not establish their Temporal arithmetic.

T23 owns pinned locale/data services and their service/profile limits. This
batch connects the existing DateTimeFormat boundary; it does not complete
ECMA-402 or additional Intl service families. This named-zone-only checkpoint
kept SystemTimeZoneIdentifier at UTC and did not add configured defaults or
Date local consumers. The later authored
[configured-zone batch](contracts/date-system-time-zone.md) now supplies those
actual consumers; its compilation and execution remain pending. The existing
deterministic HostClock remains the clock authority.

Once the complete graph is integrated, ordinary available named zones no
longer take the historical lookup-only `TemporalNamedTimeZone` blocker.
Unknown names still throw the called function Realm's intrinsic RangeError.
The old wire-6 diagnostic keeps its historical meaning; it is not relabelled.

The remaining narrow diagnostic is
`RuntimeSemanticGap::TemporalZonedRoundingWindow`, T22 wire 8. Actual contextual
nudge code emits it for a zero or wrong-direction window, or a destination
still outside the final bracket after the prescribed additional-shift
recomputation. [Upstream issue 3310](https://github.com/tc39/proposal-temporal/issues/3310)
records the legal skipped-day ambiguity; authored Apia −1-day and −25-hour
controls retain it explicitly. No extra retry or fabricated ratio is supplied.
This fatal semantic rejection occurs outside JavaScript completion. It cannot
be caught, satisfy a negative JavaScript test or be counted as a runtime crash.
Positive adjacent controls and ordinary named-zone operations remain separate.

Full pinned Date/Temporal, `intl402/Temporal`, calendar and Intl conformance
remain open. Every failure retains an owner and reason. Fake-suite green,
reference-engine results and source checks cannot establish full Test262 green.

## Clause-specific authority

The immutable [Stage4 integration PR3966 source](https://github.com/ptomato/ecma262/tree/3d4a6e7124a6878cb5af3132af7e01e01a88317f/temporal)
supplies the reviewed timezone/conversion/arithmetic algorithms. The consensus
corrections for [issue 3312](https://github.com/tc39/proposal-temporal/issues/3312)
and [issue 3316](https://github.com/tc39/proposal-temporal/issues/3316)
are included; a zero start window means the entire DateDuration is empty, not
merely that its rounded unit field is zero. ECMA-402's reviewed
[immutable PR1044 source](https://github.com/ptomato/ecma402/tree/d026068ed653130a37d79a95c0122ecf5319f1a4)
defines the forced-zone locale bridge and shared DateTimeFormat initialization.

Creation bounds have a specific authority exception. The approved Stage4
proposal at immutable commit `e8cc03fc970a65a3359e8870e3b35e687ac94e55`
requires [ISODateTimeWithinLimits](https://github.com/tc39/proposal-temporal/blob/e8cc03fc970a65a3359e8870e3b35e687ac94e55/spec/plaindatetime.html)
to admit absolute ISO epoch days at most 100000001, followed by the exclusive
Instant±day nanosecond interval.
[ISODateWithinLimits](https://github.com/tc39/proposal-temporal/blob/e8cc03fc970a65a3359e8870e3b35e687ac94e55/spec/plaindate.html)
evaluates that operation at noon. PR3966's narrower first day step contradicts
its non-throwing creation after a valid Instant is projected through any zone.
A transcription defect is the supported inference, not a confirmed upstream
ruling. Existing PlainDate/PDT bounds, the plain Duration normalized contextual
guard and positive extreme-offset materialization controls are preserved.

Inverse ValidateISODaysRange is a distinct inclusive ±100000000-day check at
the prescribed inverse/offset-interpretation steps. It does not replace
creation bounds or add a universal shadow check to contextual projections,
raw date arithmetic or inverse probes. Exact source/history/hash receipts for
both snapshots are retained in the preparation record.

## Verification and refresh

Provisional verification: the complete batch has not yet passed its canonical
build, focused native controls, unchanged pinned selections and broad workspace
checkpoint. Replace this paragraph only with dated actual commands, source and
binary identities, observed outcomes and owned remaining failures. Preserve
earlier evidence in the [lookup-only authority contract](contracts/temporal-named-zone-authority.md)
and historical task notes; it is not evidence for this later consumer graph.

After the complete source batch is final, follow the
[batch verification workflow](../../CONTRIBUTING.md#verification), generating/checking the pinned
catalogue before the single coordinated compile. Focused native refreshes include:

```sh
python3 scripts/generate-intl-named-time-zones.py --check
cargo check --workspace --all-targets
cargo test -p scriptw-intl provider::named_time_zones -- --test-threads=1
cargo test -p scriptw-engine --lib intl_time_zone_host -- --test-threads=1
cargo test -p scriptw-engine --test aot_intl -- aot_intl_named_time_zones:: --test-threads=1
cargo test -p scriptw-engine --test aot_temporal -- aot_temporal_named_arithmetic:: aot_temporal_named_zdt_leaves:: --test-threads=1
cargo test -p scriptw-engine --test aot_temporal -- aot_temporal_duration_zoned_relative:: aot_temporal_named_conversions:: aot_temporal_relative_bag:: aot_temporal_zoned_locale:: --test-threads=1
```

After the focused gates, run the broad checkpoint from the batch workflow,
then build the canonical release CLI. Preserve that exact binary and source
receipt for pinned refreshes and publication. Pinned refreshes retain exact
source, flags, harness metadata and execution modes. Store scratch results
outside tracked status artifacts:

```sh
cargo build --release --locked -j 2 -p scriptw-cli
./target/release/scriptw --jobs 1 test262 run built-ins/Temporal --suite-root test262/vendor/test262 --execution-backend wasm-aot --threads 2 --timeout-ms 240000 --snapshot-dir target/test262-scratch/t22 --snapshot-name t22-temporal
./target/release/scriptw --jobs 1 test262 run intl402/Temporal --suite-root test262/vendor/test262 --execution-backend wasm-aot --threads 2 --timeout-ms 240000 --snapshot-dir target/test262-scratch/t22 --snapshot-name t22-intl-consumers
```

Publish a fresh complete real matrix only through the guarded canonical
publisher, using the frozen tested release binary. The generated README count
block changes only after verified
matrix completion; do not insert local/native/fake results into it:

```sh
SCRIPTW_BIN=./target/release/scriptw THREADS=2 JOBS=1 ISOLATE_CASES=1 ./scripts/publish-real-status-low-ram.sh wasm-aot t22-named-consumers
```

The snapshot name is a refresh namespace, not evidence that these commands
ran. Changing compiler/source identity requires a fresh namespace rather than
resuming a prior compiler's matrix.
