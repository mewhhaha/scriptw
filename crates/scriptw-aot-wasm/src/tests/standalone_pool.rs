use super::*;

#[test]
fn standalone_primitive_expressions_validate_without_runtime_or_linear_images() {
    for source in [
        "1;",
        "-0;",
        "0 / 0;",
        "1 / 0;",
        "-1 / 0;",
        "void 3;",
        "null;",
        "false;",
        "(1 + 2) * 3 % 4;",
        "true ? -0 : 7;",
        "(false && 3, true || 4);",
        "'';",
        "'a\\0b';",
        "'\\u{1f600}';",
        "'\\ud800x\\udfff';",
        "'\\u{f0000}D800';",
        "'' ? 'wrong' : '\\ud800';",
        "('ignored', '\\0');",
        "!!'\\0';",
    ] {
        let artifact = emit_script(source).expect(source);
        expect_valid_module(&artifact, 0);
        assert!(artifact.runtime().is_none(), "{source}");
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            match payload.expect("standalone module decodes") {
                Payload::MemorySection(_)
                | Payload::DataSection(_)
                | Payload::DataCountSection { .. } => {
                    panic!("standalone primitive must not carry a linear image: {source}");
                }
                Payload::ImportSection(imports) => {
                    for import in imports.into_imports() {
                        let import = import.unwrap();
                        assert_ne!(import.module, crate::RUNTIME_IMPORT_NAMESPACE, "{source}");
                        assert!(
                            !matches!(import.ty, wasmparser::TypeRef::Memory(_)),
                            "{source}"
                        );
                    }
                }
                Payload::ExportSection(exports) => {
                    for export in exports {
                        assert_ne!(
                            export.unwrap().kind,
                            wasmparser::ExternalKind::Memory,
                            "{source}"
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

#[test]
fn runtime_dependent_and_rooted_programs_retain_the_exact_runtime_pool() {
    let baseline = emit_script("({ value: 1 });").unwrap();
    let runtime = baseline.runtime().expect("object literal requires R");
    for source in ["let value = 1; value;", "'text'.length;", "print(1);"] {
        let artifact = emit_script(source).expect(source);
        expect_valid_module(&artifact, 0);
        assert!(
            std::sync::Arc::ptr_eq(
                runtime,
                artifact
                    .runtime()
                    .expect("runtime-dependent source retains R")
            ),
            "{source}"
        );
    }
    let parsed = parse("'\\ud800';", ParseOptions::script()).unwrap();
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let rooted = emit_with_rooted_snapshot(
        &program,
        PromiseRejectionPolicy::default(),
        &scriptw_intl::IntlCompilationProfile::Minimal,
    )
    .unwrap();
    expect_valid_module(&rooted, 0);
    assert!(std::sync::Arc::ptr_eq(runtime, rooted.runtime().unwrap()));
    assert_eq!(wire_data_segment_counts(runtime.bytes()), (4, 4));
    assert_eq!(wire_data_segment_counts(&rooted.bytes), (4, 0));
}
