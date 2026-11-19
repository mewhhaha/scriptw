# Tolong Siki numbering in NumberFormat and DateTimeFormat

The `tols` addition is an actual shared digit/pattern contribution to both
formatting services. The source package preserves all CLDR47 files and all 77
existing digit alphabets. It adds the exact CLDR48 numeric declaration and BCP47
type for Tolong Siki, together with all six declared root aliases for symbols,
decimal/scientific/percent/currency formats and miscellaneous patterns.
These aliases resolve through the existing locale inheritance machinery;
there is no handwritten Latin fallback.

The immutable authority is Unicode CLDR48 commit
[`acd6d88ae493633240e19a87a721076a8a75c310`](https://github.com/unicode-org/cldr/tree/acd6d88ae493633240e19a87a721076a8a75c310):
[digit declaration](https://github.com/unicode-org/cldr/blob/acd6d88ae493633240e19a87a721076a8a75c310/common/supplemental/numberingSystems.xml),
[BCP47 type](https://github.com/unicode-org/cldr/blob/acd6d88ae493633240e19a87a721076a8a75c310/common/bcp47/number.xml)
and [actual aliases](https://github.com/unicode-org/cldr/blob/acd6d88ae493633240e19a87a721076a8a75c310/common/main/root.xml).
[Unicode17 UnicodeData](https://www.unicode.org/Public/17.0.0/ucd/UnicodeData.txt)
defines U+11DE0–U+11DE9 as decimal numbers with values zero through nine.
The complete four primary files have fixed SHA-256 pins; the shared generator
constructor checks every source and the closed one-system/six-alias recipe.
Unicode data retains its [Unicode licence](../../crates/scriptw-intl/data/datetime-cldr-47/LICENSE).

NumberFormat appends only those ten genuine `Nd` points to its baseline Unicode16
spacing properties before evaluating the existing CLDR UnicodeSet rules.
DateTimeFormat consumes the same alphabet and actual decimal/minus symbols.
Both renderers produce localized primitive parts, which the Wasm consumers copy
without substituting digits in currency text, names or literals. Supplementary
digits remain complete UTF-16 pairs. The checked enumeration catalogue publishes
`tols` only after the actual NF and DTF profiles admit it without fallback.

The generated profiles retain their base CLDR47 version and add separate
`numbering_supplement` metadata with CLDR48, Unicode17 and the source-manifest
SHA. The NF payload digest binds the added alphabet and spacing ranges; the DTF
kernel digest also binds the shared producer and all supplement sources.
NF/DTF service profiles include this service-specific provenance in their
canonical artifact identity. Locale-only identity bytes and baseline version
labels remain unchanged. No global Unicode17 upgrade or numeric/weight support
for future Collator data is claimed.

Six native controls and four Engine tests (eight sloppy/strict Wasm observations)
are authored. They cover exact BigInt/decimal digits, currency spacing and text,
extension/explicit-option selection, decimal classification, time fractions and
range sources, real Buddhist solar projection, source-recipe rejection and
called-function Realm ownership. They remain unexecuted. Offline generator and
source checks are recorded separately from product evidence.

Required product gates include the complete original NumberFormat253-file/506-mode,
DateTimeFormat248-file/496-mode and enumeration25-file/50-mode pinned cohorts,
with no exclusions, followed by applicable workspace/fake checks and visible
whole-Intl outcomes. The complete fake suite has 190 physical files/191 modes;
its result cannot establish full ECMAScript or Test262 conformance.
RelativeTimeFormat remains an owned missing consumer even though NF/DTF now
share all 78 required simple digit mappings. The remaining calendar/service and
Chinese Temporal debts stay open.

```sh
python3 scripts/generate-intl-numberformat-profile.py --check
python3 scripts/generate-intl-datetime-profile.py --output crates/scriptw-intl/src/provider/datetime/generated/profile.json --report crates/scriptw-intl/src/provider/datetime/generated/report.json --check
python3 scripts/generate-intl-datetime-identity.py --check
python3 -m unittest discover -s scripts/tests -p 'test_intl_positional_numbering.py'
python3 -m unittest discover -s scripts/tests -p 'test_generate_intl_datetime_profile.py'
cargo test --locked -p scriptw-intl tols -- --test-threads=1
cargo test --locked -p scriptw-intl tests::formatting_identity_discloses_only_its_checked_numbering_supplement -- --exact --test-threads=1
cargo test --locked -p scriptw-engine --test aot_intl -- aot_intl_numbering_tols:: --test-threads=1
./scripts/publish-real-status-low-ram.sh wasm-aot <fresh-snapshot-name>
```

The status publisher alone may refresh generated JSON/TXT and the canonical
README numerical block after a verified complete run. This source patch makes
no new native, Engine, pinned or full-conformance PASS claim.

The coherent source join retains the PluralRules schema2 ordinal table and all
535 future NumberFormat profiles and 1,082 locale associations. It inserts the
source-declared Tolong Siki Latin alias into each profile and adds only its ten
Nd points. Serialization uses the actually merged producer. The complete offline
source reproducibility check and all product gates remain separate requirements.
