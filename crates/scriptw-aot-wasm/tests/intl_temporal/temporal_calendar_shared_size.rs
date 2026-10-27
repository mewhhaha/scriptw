//! Calendar consumers call the same bounded conversion and year algorithms.

use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use std::collections::{BTreeMap, BTreeSet};
use wasmparser::{Validator, WasmFeatures};

use crate::linked_bodies;
use linked_bodies::linked_bodies;

const HELPERS: [&str; 9] = [
    "helper::temporal_chinese_year",
    "helper::temporal_dangi_year",
    "helper::temporal_umalqura_year",
    "helper::temporal_umalqura_epoch",
    "helper::temporal_calendar_project_date",
    "helper::temporal_calendar_fields_to_iso",
    "helper::temporal_calendar_days_in_month",
    "helper::temporal_calendar_balance_year_month",
    "helper::temporal_calendar_difference_date",
];
const COMPOSITE_HELPERS: [&str; 2] = [
    "helper::temporal_calendar_project_date",
    "helper::temporal_calendar_fields_to_iso",
];
const YEAR_HELPERS: [&str; 4] = [
    "helper::temporal_chinese_year",
    "helper::temporal_dangi_year",
    "helper::temporal_umalqura_year",
    "helper::temporal_umalqura_epoch",
];
const ADDITION_HELPERS: [&str; 2] = [
    "helper::temporal_calendar_days_in_month",
    "helper::temporal_calendar_balance_year_month",
];
const DIFFERENCE_HELPER: &str = "helper::temporal_calendar_difference_date";
const CONVERSION_HELPERS: [&str; 2] = [
    "helper::temporal_zoned_date_time_convert",
    "helper::temporal_plain_date_convert",
];

// The original repeated year expansions produced 6.96 MB / 5.56 MB /
// 3.11 MB Duration bodies and 2.85 MB ZonedDateTime difference bodies.
// These limits bound each native compilation unit as well as requiring calls
// to the shared algorithms below; an unused helper cannot satisfy the control.
const CONSUMERS: [(&str, usize); 5] = [
    ("builtin::Temporal.Duration.prototype.round", 1024 * 1024),
    ("builtin::Temporal.Duration.prototype.total", 1024 * 1024),
    ("builtin::Temporal.Duration.compare", 1024 * 1024),
    (
        "builtin::Temporal.ZonedDateTime.prototype.until",
        1024 * 1024,
    ),
    (
        "builtin::Temporal.ZonedDateTime.prototype.since",
        1024 * 1024,
    ),
];

const SOURCE: &str = r#"
const duration = new Temporal.Duration(1, 2, 0, 3);
const relativeTo = new Temporal.PlainDate(2024, 2, 10, 'chinese');
duration.round({largestUnit: 'year', smallestUnit: 'month', relativeTo});
duration.total({unit: 'month', relativeTo});
Temporal.Duration.compare(duration, {months: 14}, {relativeTo});
const start = new Temporal.ZonedDateTime(0n, 'UTC', 'dangi');
const end = new Temporal.ZonedDateTime(3456000000000000n, 'UTC', 'dangi');
start.until(end, {largestUnit: 'year'});
end.since(start, {largestUnit: 'year'});
"#;

/// Validates the linked runtime and program modules together.
fn validate_linked(artifact: &scriptw_aot_wasm::WasmArtifact, message: &str) {
    for bytes in [
        &artifact
            .runtime()
            .expect("Temporal consumers link R")
            .bytes()[..],
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect(message);
    }
}

#[test]
fn duration_and_zoned_difference_share_bounded_calendar_year_helpers() {
    let artifact = std::thread::Builder::new()
        .name("temporal-calendar-shared-size".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(SOURCE, ParseOptions::script()).expect("Temporal source parses");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let artifact = scriptw_aot_wasm::emit(&program).expect("Temporal consumers emit");
            eprintln!(
                "calendar artifact: {} program bytes, {} runtime bytes",
                artifact.bytes.len(),
                artifact
                    .runtime()
                    .expect("Temporal consumers link R")
                    .bytes()
                    .len()
            );
            validate_linked(
                &artifact,
                "shared scalar and whole-Completion results and callers validate",
            );
            artifact
        })
        .expect("compiler worker starts")
        .join()
        .expect("compiler worker completes");

    // Every calendar algorithm is an R body; only `main` is the program's.
    let bodies = linked_bodies(&artifact);
    for body in bodies.all() {
        if body.name == "scriptw::main"
            || HELPERS.contains(&body.name.as_str())
            || CONVERSION_HELPERS.contains(&body.name.as_str())
            || CONSUMERS.iter().any(|(name, _)| *name == body.name)
        {
            eprintln!("{}: {} bytes", body.name, body.bytes());
        }
    }
    let mut selected = BTreeSet::new();
    let helpers = HELPERS.map(|name| {
        let body = bodies.unique(name);
        assert!(
            (128..=64 * 1024).contains(&body.bytes()),
            "{name} must have a real bounded body, got {} bytes",
            body.bytes()
        );
        selected.insert(body.index);
        (name, body.index)
    });
    for name in CONVERSION_HELPERS {
        let body = bodies.unique(name);
        assert!(
            (128..=1024 * 1024).contains(&body.bytes()),
            "{name} must have one real bounded conversion body, got {} bytes",
            body.bytes()
        );
        selected.insert(body.index);
    }
    for (name, limit) in CONSUMERS {
        let body = bodies.unique(name);
        assert!(
            body.bytes() <= limit,
            "{name} is {} bytes; per-body limit is {limit}",
            body.bytes()
        );
        selected.insert(body.index);
    }

    // Every selected helper/consumer belongs to R. Keep P indices out of
    // this map: compact P has its own independent function namespace.
    let calls = bodies
        .runtime
        .iter()
        .filter(|body| selected.contains(&body.index))
        .map(|body| (body.index, body.calls()))
        .collect::<BTreeMap<u32, BTreeSet<u32>>>();
    let index_of = |name: &str| bodies.unique(name).index;
    for (name, _) in CONSUMERS {
        let body_calls = calls.get(&index_of(name)).expect("consumer body exists");
        assert!(
            body_calls.contains(&index_of(CONVERSION_HELPERS[0])),
            "{name} must call the shared complete ZonedDateTime conversion"
        );
        if name.starts_with("builtin::Temporal.Duration") {
            assert!(
                body_calls.contains(&index_of(CONVERSION_HELPERS[1])),
                "{name} must call the shared complete PlainDate conversion"
            );
        }
        for helper in COMPOSITE_HELPERS.into_iter().chain(ADDITION_HELPERS) {
            assert!(
                body_calls.contains(&index_of(helper)),
                "{name} must call {helper}"
            );
        }
        if name != "builtin::Temporal.Duration.compare" {
            assert!(
                body_calls.contains(&index_of(DIFFERENCE_HELPER)),
                "{name} must call the shared complete calendar difference"
            );
        }
    }
    for (helper, index) in helpers {
        if YEAR_HELPERS.contains(&helper) {
            assert!(
                COMPOSITE_HELPERS.iter().any(|name| {
                    calls
                        .get(&index_of(name))
                        .expect("calendar composite body exists")
                        .contains(&index)
                }),
                "the shared calendar conversions must call {helper}"
            );
        }
    }
    let difference_calls = calls
        .get(&index_of(DIFFERENCE_HELPER))
        .expect("complete calendar difference body exists");
    for helper in COMPOSITE_HELPERS
        .into_iter()
        .chain(["helper::temporal_calendar_days_in_month"])
    {
        assert!(
            difference_calls.contains(&index_of(helper)),
            "the shared calendar difference must call {helper}"
        );
    }
}

#[test]
fn plain_date_only_source_collects_shared_converter_dependencies() {
    std::thread::Builder::new()
        .name("temporal-shared-converter-dependencies".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Every program links the one runtime module, so a source naming
            // only PlainDate still finds both complete converters in R.
            let parsed = parse(
                "Temporal.PlainDate.from('2024-02-29').year;",
                ParseOptions::script(),
            )
            .expect("PlainDate source parses");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let artifact = scriptw_aot_wasm::emit(&program)
                .expect("shared converters are emitted with the runtime module");
            validate_linked(&artifact, "all emitted shared conversion bodies validate");
            let bodies = linked_bodies(&artifact);
            for helper in CONVERSION_HELPERS {
                let body = bodies.unique(helper);
                assert!(
                    bodies
                        .runtime
                        .iter()
                        .any(|candidate| candidate.index == body.index)
                        && body.bytes() > 20,
                    "shared converter must have a real runtime body: {helper}"
                );
            }
        })
        .expect("compiler worker starts")
        .join()
        .expect("compiler worker completes");
}

#[test]
fn partial_date_constructors_collect_shared_converter_host_dependencies() {
    std::thread::Builder::new()
        .name("temporal-shared-converter-imports".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Neither constructor declares a direct named-zone provider call.
            // The emitted shared converter owns that import in the first pass.
            let parsed = parse(
                "new Temporal.PlainMonthDay(2, 29); new Temporal.PlainYearMonth(2024, 2);",
                ParseOptions::script(),
            )
            .expect("partial-date source parses");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let artifact = scriptw_aot_wasm::emit(&program)
                .expect("emitted converter owns its checked host import");
            Validator::new_with_features(WasmFeatures::all())
                .validate_all(&artifact.bytes)
                .expect("shared converter and imported host signatures validate");
        })
        .expect("compiler worker starts")
        .join()
        .expect("compiler worker completes");
}
