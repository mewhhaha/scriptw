use super::*;

#[test]
fn standalone_literal_units_copy_one_exact_passive_image_into_a_fresh_array() {
    use std::fmt::Write;

    let prefix = [
        0, 0xd800, 0x0078, 0xdfff, 0xd83d, 0xde00, 0xdb80, 0xdc00, 0x0044, 0x0038, 0x0030, 0x0030,
    ];
    let empty_size = emit_script("'';").unwrap().bytes.len();
    for length in [0, 1, 4096, 16_383, 16_384, 65_536] {
        let mut units = Vec::with_capacity(length);
        units.extend_from_slice(&prefix[..length.min(prefix.len())]);
        for index in units.len()..length {
            units.push((index * 257) as u16);
        }
        let mut source = String::from("'");
        for unit in &units {
            write!(&mut source, "\\u{unit:04x}").unwrap();
        }
        source.push_str("';");
        let artifact = emit_script(&source).unwrap();
        expect_valid_module(&artifact, 0);
        assert!(artifact.runtime().is_none(), "{length}");
        let main = function_names(&artifact)
            .into_iter()
            .find_map(|(index, name)| (name == "scriptw::main").then_some(index))
            .expect("actual main function");
        let mut index = imported_function_count(&artifact);
        let mut copies = Vec::new();
        let mut images = Vec::new();
        let mut body_bytes = None;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            match payload.unwrap() {
                Payload::MemorySection(_) => {
                    panic!("standalone literal needs no linear memory: {length}")
                }
                Payload::DataSection(section) => {
                    for segment in section {
                        let segment = segment.unwrap();
                        assert!(matches!(segment.kind, wasmparser::DataKind::Passive));
                        images.push(segment.data.to_vec());
                    }
                }
                Payload::DataCountSection { count, .. } => assert_eq!(count, 1),
                Payload::CodeSectionEntry(body) => {
                    if index == main {
                        body_bytes = Some(body.range().len());
                        let operators = body
                            .get_operators_reader()
                            .unwrap()
                            .into_iter()
                            .map(Result::unwrap)
                            .collect::<Vec<_>>();
                        for window in operators.windows(3) {
                            if let [Operator::I32Const { value: offset }, Operator::I32Const { value: extent }, Operator::ArrayNewData {
                                array_data_index, ..
                            }] = window
                            {
                                copies.push((*offset, *extent, *array_data_index));
                            }
                        }
                        for operator in operators {
                            match operator {
                                Operator::ArrayNewFixed { .. } | Operator::DataDrop { .. } => {
                                    panic!(
                                        "literal must copy its retained passive image: {length}"
                                    );
                                }
                                Operator::ArrayNew { .. }
                                | Operator::ArrayNewDefault { .. }
                                | Operator::ArraySet { .. } => {
                                    panic!("literal backing must be initialized once: {length}");
                                }
                                _ => {}
                            }
                        }
                    }
                    index += 1;
                }
                _ => {}
            }
        }
        assert_eq!(copies, [(0, length as i32, 0)]);
        let expected = units
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(images, [expected]);
        // Only offset and length constants grow; copying the passive image
        // does not emit an instruction or native store per UTF-16 unit.
        assert!(body_bytes.unwrap() <= 512, "{length}");
        assert!(
            artifact.bytes.len() <= empty_size + 2 * length + 128,
            "{length}"
        );
    }
    let ordered = emit_script("'z'; 'a'; 'z';").unwrap();
    expect_valid_module(&ordered, 0);
    assert!(ordered.runtime().is_none());
    assert_eq!(wire_data_segment_counts(&ordered.bytes), (1, 0));
    assert_eq!(data_segment_at(&ordered.bytes, 0), b"a\0z\0");
}

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
        let mut data_segments = 0;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            match payload.expect("standalone module decodes") {
                Payload::MemorySection(_) => {
                    panic!("standalone primitive needs no linear memory: {source}")
                }
                Payload::DataSection(section) => {
                    for segment in section {
                        let segment = segment.unwrap();
                        assert!(matches!(segment.kind, wasmparser::DataKind::Passive));
                        assert_eq!(segment.data.len() % 2, 0);
                        data_segments += 1;
                    }
                }
                Payload::DataCountSection { count, .. } => assert_eq!(count, 1),
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
        assert!(data_segments <= 1, "{source}");
        if !source.contains('\'') {
            assert_eq!(
                data_segments, 0,
                "numeric-only standalone has no data: {source}"
            );
        }
    }
}

#[test]
fn runtime_dependent_and_rooted_programs_retain_the_exact_runtime_pool() {
    let baseline = emit_script("({ value: 1 });").unwrap();
    let runtime = baseline.runtime().expect("object literal requires R");
    for source in [
        "let value = 1; value;",
        "'text'.length;",
        "print(1);",
        "'literal'; ({});",
        "('literal', print(1));",
    ] {
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
    // Even without the script-level host marker, the closed expression proof
    // must reject the call after seeing an earlier literal.
    let parsed = parse("'literal'; print(1);", ParseOptions::script()).unwrap();
    let mut partial = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let script = partial.script.as_mut().unwrap();
    script.host_builtins.clear();
    assert!(crate::emit::runtime_requirement::StandaloneAdmission::new(script).is_none());
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
