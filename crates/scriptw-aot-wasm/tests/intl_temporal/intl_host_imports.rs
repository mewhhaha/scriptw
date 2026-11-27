use scriptw_aot_wasm::{emit, GcHostImport};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{Parser, Payload, Validator, WasmFeatures};

/// The Intl provider import count of each module of the artifact: R then P
/// for a heap program, P alone for a program that links no runtime.
fn provider_import_counts(
    source: &'static str,
    host_surface_policy: HostSurfacePolicy,
) -> Vec<usize> {
    std::thread::Builder::new()
        .name("intl-host-imports".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let parsed = parse(source, ParseOptions::script()).expect("fixture should parse");
            let program = lower_with_host_surface_policy(&parsed, host_surface_policy);
            let artifact = emit(&program).expect("standalone provider caller must emit");
            let mut features = WasmFeatures::default();
            for feature in [
                WasmFeatures::THREADS,
                WasmFeatures::MULTI_MEMORY,
                WasmFeatures::REFERENCE_TYPES,
                WasmFeatures::FUNCTION_REFERENCES,
                WasmFeatures::GC,
                WasmFeatures::EXCEPTIONS,
                WasmFeatures::TAIL_CALL,
            ] {
                features.set(feature, true);
            }
            let modules = artifact
                .runtime()
                .map(|runtime| &runtime.bytes()[..])
                .into_iter()
                .chain([artifact.bytes.as_slice()]);
            modules
                .map(|bytes| {
                    Validator::new_with_features(features)
                        .validate_all(bytes)
                        .expect("the host import set must preserve R/P function and call indices");
                    let mut count = 0;
                    for payload in Parser::new(0).parse_all(bytes) {
                        if let Payload::ImportSection(section) =
                            payload.expect("section must decode")
                        {
                            for import in section.into_imports() {
                                let import = import.expect("import must decode");
                                count += usize::from(
                                    import.module == GcHostImport::IntlProviderCall.module()
                                        && import.name == GcHostImport::IntlProviderCall.name(),
                                );
                            }
                        }
                    }
                    count
                })
                .collect()
        })
        .expect("compiler worker should spawn")
        .join()
        .expect("compiler worker must not panic")
}

/// A heap program's R imports the provider exactly once: R's host import set
/// is fixed, so its imports do not depend on the source. The provider's
/// callers are R bodies, and P keeps a host function import only when one of
/// its own bodies calls it (final import compaction), so P has none.
fn intl_import_count(source: &'static str, host_surface_policy: HostSurfacePolicy) -> usize {
    let counts = provider_import_counts(source, host_surface_policy);
    assert_eq!(counts.len(), 2, "heap program links R: {source}");
    assert_eq!(counts[1], 0, "P calls no provider itself: {source}");
    counts[0]
}

#[test]
fn locale_construction_declares_its_own_provider_import() {
    assert_eq!(
        intl_import_count("new Intl.Locale('iw');", HostSurfacePolicy::default()),
        1
    );
}

#[test]
fn locale_options_and_optional_imports_preserve_wasm_indices() {
    assert_eq!(
        intl_import_count(
            "var locale = new Intl.Locale('en', {calendar:'islamicc',numeric:true}); locale.calendar; Date.now(); Math.random();",
            HostSurfacePolicy::default(),
        ),
        1
    );
}

#[test]
fn created_realm_locale_dependencies_declare_the_provider_import() {
    assert_eq!(
        intl_import_count(
            "var foreign = __scriptwCreateRealm().global; new foreign.Intl.Locale('iw').language;",
            HostSurfacePolicy::Test262,
        ),
        1
    );
}

#[test]
fn a_program_without_provider_callers_omits_the_intl_import() {
    // Runtime-free IR is one self-contained module with no host surface.
    assert_eq!(
        provider_import_counts("1 + 1;", HostSurfacePolicy::default()),
        [0]
    );
    // A heap program has no provider caller either, yet R still declares the
    // import: it is part of R's fixed host set, not a per-program gate. P
    // drops it, as every host function no P body calls.
    assert_eq!(
        provider_import_counts("print({});", HostSurfacePolicy::default()),
        [1, 0]
    );
}

#[test]
fn likely_subtag_methods_declare_the_provider_and_preserve_wasm_indices() {
    assert_eq!(
        intl_import_count(
            "var locale = new Intl.Locale('en'); locale.maximize().minimize(); Date.now(); Math.random();",
            HostSurfacePolicy::default(),
        ),
        1,
    );
}

#[test]
fn detached_likely_subtag_methods_retain_provider_dependencies() {
    assert_eq!(
        intl_import_count(
            "var foreign = __scriptwCreateRealm().global; var method = foreign.Intl.Locale.prototype.maximize; method.call(new Intl.Locale('en'));",
            HostSurfacePolicy::Test262,
        ),
        1,
    );
}

#[test]
fn number_format_and_both_primitive_locale_consumers_root_the_provider() {
    for source in [
        "new Intl.NumberFormat('en-US').format(123);",
        "(123).toLocaleString('en-US', {minimumFractionDigits:2});",
        "(123n).toLocaleString('en-US', {minimumFractionDigits:2});",
        "var foreign=__scriptwCreateRealm().global; foreign.Intl.NumberFormat.prototype.formatRange.call(new Intl.NumberFormat('en-US'),1,2);",
    ] {
        assert_eq!(intl_import_count(source, HostSurfacePolicy::Test262), 1);
    }
}

#[test]
fn instant_locale_methods_root_their_intrinsic_formatter_dependencies() {
    for (source, host_surface_policy) in [
        (
            "new Temporal.Instant(0n).toLocaleString('en-US', {timeZone:'UTC'});",
            HostSurfacePolicy::default(),
        ),
        (
            "var foreign=__scriptwCreateRealm().global; var method=foreign.Temporal.Instant.prototype.toLocaleString; method.call(new Temporal.Instant(-1n),'en-US',{timeZone:'UTC'});",
            HostSurfacePolicy::Test262,
        ),
    ] {
        assert_eq!(intl_import_count(source, host_surface_policy), 1);
    }
}
