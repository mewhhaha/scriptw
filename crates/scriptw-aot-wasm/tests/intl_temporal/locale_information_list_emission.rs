use scriptw_aot_wasm::{emit, GcHostImport};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{Parser, Payload, Validator, WasmFeatures};

fn validate_provider_caller(source: &'static str, policy: HostSurfacePolicy) {
    std::thread::Builder::new()
        .name("locale-information-list-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let parsed = parse(source, ParseOptions::script()).expect("fixture should parse");
            let program = lower_with_host_surface_policy(&parsed, policy);
            assert!(program.is_wasm_supported(), "Locale information must lower");
            let artifact = emit(&program).expect("Locale information must emit");
            let runtime = artifact.runtime().expect("Locale information links R");
            // R's fixed host set declares the provider; P drops it, since
            // only R bodies call it (final import compaction).
            for (bytes, expected) in [(runtime.bytes(), 1), (artifact.bytes.as_slice(), 0)] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .expect(
                        "checked list loops and defining-Realm allocation must form valid Wasm",
                    );
                let mut provider_imports = 0;
                for payload in Parser::new(0).parse_all(bytes) {
                    if let Payload::ImportSection(section) = payload.expect("section should decode")
                    {
                        for import in section.into_imports() {
                            let import = import.expect("import should decode");
                            provider_imports += usize::from(
                                import.module == GcHostImport::IntlProviderCall.module()
                                    && import.name == GcHostImport::IntlProviderCall.name(),
                            );
                        }
                    }
                }
                assert_eq!(
                    provider_imports, expected,
                    "each standalone caller roots the provider in R, and P calls none"
                );
            }
        })
        .expect("compiler worker should spawn")
        .join()
        .expect("compiler worker must not panic");
}

#[test]
fn standalone_locale_information_lists_emit_valid_wasm_and_root_the_provider() {
    for source in [
        "new Intl.Locale('th-TH').getCalendars();",
        "new Intl.Locale('de').getCollations();",
        "new Intl.Locale('en-US').getTimeZones();",
    ] {
        validate_provider_caller(source, HostSurfacePolicy::default());
    }
}

#[test]
fn detached_foreign_realm_information_methods_emit_their_own_array_allocation_path() {
    for source in [
        "var foreign = __scriptwCreateRealm().global; foreign.Intl.Locale.prototype.getCalendars.call(new Intl.Locale('en-US'));",
        "var foreign = __scriptwCreateRealm().global; foreign.Intl.Locale.prototype.getCollations.call(new Intl.Locale('de'));",
        "var foreign = __scriptwCreateRealm().global; foreign.Intl.Locale.prototype.getTimeZones.call(new Intl.Locale('en-US'));",
    ] {
        validate_provider_caller(source, HostSurfacePolicy::Test262);
    }
}
