use scriptw_aot_wasm::{emit, GcHostImport};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{Parser, Payload, Validator, WasmFeatures};

fn assert_system_zone_import(source: &'static str, policy: HostSurfacePolicy) {
    std::thread::Builder::new()
        .name("system-time-zone-imports".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let parsed = parse(source, ParseOptions::script()).expect("fixture should parse");
            let program = lower_with_host_surface_policy(&parsed, policy);
            let artifact = emit(&program).expect("configured-zone caller should emit");
            let runtime = artifact.runtime().expect("configured-zone caller links R");
            // R imports the fixed host set, so it declares the snapshot import
            // once; P keeps only host functions its own bodies call, and the
            // zone's callers are R's. The Intl data identity is R's alone.
            for (module, bytes, expected_imports, expected_identities) in [
                ("R", &runtime.bytes()[..], 1, 1),
                ("P", artifact.bytes.as_slice(), 0, 0),
            ] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .expect("the fixed host import set must preserve type and function indices");
                let mut system_imports = 0;
                let mut identities = 0;
                for payload in Parser::new(0).parse_all(bytes) {
                    match payload.expect("section should decode") {
                        Payload::ImportSection(section) => {
                            for import in section.into_imports() {
                                let import = import.expect("import should decode");
                                system_imports += usize::from(
                                    import.module == GcHostImport::SystemTimeZoneSnapshot.module()
                                        && import.name
                                            == GcHostImport::SystemTimeZoneSnapshot.name(),
                                );
                            }
                        }
                        Payload::CustomSection(section)
                            if section.name()
                                == scriptw_intl::INTL_ARTIFACT_IDENTITY_CUSTOM_SECTION =>
                        {
                            identities += 1;
                            assert_eq!(
                                section.data(),
                                scriptw_intl::embedded_intl_data_identity()
                                    .unwrap()
                                    .artifact_identity()
                                    .as_bytes(),
                            );
                        }
                        _ => {}
                    }
                }
                assert_eq!(system_imports, expected_imports, "{module}: {source}");
                assert_eq!(identities, expected_identities, "{module}: {source}");
            }
        })
        .expect("compiler worker should spawn")
        .join()
        .expect("compiler worker must not panic");
}

#[test]
fn date_local_and_omitted_now_consumers_preserve_optional_import_indices() {
    assert_system_zone_import(
        "var d=new Date(2020,0,1); d.getHours(); Temporal.Now.timeZoneId(); Math.atan2(1,1); Math.random(); Date.now();",
        HostSurfacePolicy::default(),
    );
}

#[test]
fn detached_created_realm_local_consumers_retain_the_primitive_and_pin() {
    assert_system_zone_import(
        "var foreign=__scriptwCreateRealm().global; var getter=foreign.Date.prototype.getHours; getter.call(new Date(0)); new foreign.Intl.DateTimeFormat('en').resolvedOptions();",
        HostSurfacePolicy::Test262,
    );
}
