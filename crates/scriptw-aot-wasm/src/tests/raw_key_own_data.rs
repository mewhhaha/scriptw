//! Inspect actual GetV consumers, including their conversion-free hit and saved-operand miss.
use super::*;

fn runtime_imports(artifact: &WasmArtifact) -> BTreeMap<u32, String> {
    let runtime = artifact.runtime().expect("property program links R");
    let names = function_names_in_bytes(runtime.bytes());
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(section) = payload.unwrap() {
            for export in section {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func {
                    if let Some(name) = names.get(&export.index) {
                        exports.insert(export.name, name.clone());
                    }
                }
            }
        }
    }
    let mut imports = BTreeMap::new();
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(section) = payload.unwrap() {
            for import in section.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE {
                        if let Some(name) = exports.get(import.name) {
                            imports.insert(index, name.clone());
                        }
                    }
                    index += 1;
                }
            }
        }
    }
    imports
}

fn branch_end(operators: &[Operator<'_>], start: usize) -> usize {
    let mut depth = 0;
    for (offset, operator) in operators[start..].iter().enumerate() {
        match operator {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::End if depth == 0 => return start + offset,
            Operator::End => depth -= 1,
            _ => {}
        }
    }
    panic!("guard has a complete structured branch");
}

#[test]
fn computed_getv_hits_leave_before_boxing_conversion_and_full_lookup() {
    // Unknown keys must retain both actual-tag arms. The three reads match
    // rows[i & 63][keys[i & 3]], including a dynamically produced String key.
    for (source, consumer, packed_reads, string_reads) in [
        (
            "function read(value,key){return value[key];} read;",
            "read",
            1,
            1,
        ),
        (
            "var rows,keys;function run(i){return rows[i&63][keys[i&3]];} run;",
            "run",
            3,
            1,
        ),
    ] {
        let artifact = emit_script(source).expect("actual computed consumer emits");
        Validator::new_with_features(crate::runtime_artifact::program_validation_features())
            .validate_all(&artifact.bytes)
            .expect("raw-key guards preserve typed GC roots");
        let imports = runtime_imports(&artifact);
        let prohibited = imports
            .iter()
            .filter_map(|(index, name)| {
                matches!(
                    name.as_str(),
                    "helper::value_to_object"
                        | "helper::value_to_property_key"
                        | "helper::object_read"
                )
                .then_some(*index)
            })
            .collect::<Vec<_>>();
        assert_eq!(prohibited.len(), 3, "complete fallback remains linked");
        let names = function_names(&artifact);
        let mut index = imported_function_count(&artifact);
        let mut checked = 0;
        for payload in Parser::new(0).parse_all(&artifact.bytes) {
            let Payload::CodeSectionEntry(body) = payload.unwrap() else {
                continue;
            };
            if names
                .get(&index)
                .is_some_and(|name| name.starts_with(&format!("js::{consumer}#")))
            {
                let operators = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>();
                let mut packed = 0;
                let mut strings = 0;
                for (guard, window) in operators.windows(4).enumerate() {
                    let [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Eq, Operator::If { .. }] =
                        window
                    else {
                        continue;
                    };
                    let end = branch_end(&operators, guard + 4);
                    // A raw-key probe returns found directly to the property's
                    // exit branch. Inner representation guards have other tails.
                    if !matches!(
                        operators.get(end + 1..end + 3),
                        Some([Operator::LocalGet { .. }, Operator::BrIf { .. }])
                    ) {
                        continue;
                    }
                    let branch = &operators[guard + 4..end];
                    if *value == WasmRuntimeValueTag::Number as i32 {
                        assert!(branch.iter().any(|op| matches!(op, Operator::F64Trunc)));
                        assert!(branch
                            .iter()
                            .any(|op| matches!(op, Operator::ArrayGet { .. })));
                        packed += 1;
                    } else if *value == WasmRuntimeValueTag::String as i32 {
                        assert!(branch
                            .iter()
                            .any(|op| matches!(op, Operator::StructGet { .. })));
                        strings += 1;
                    } else {
                        continue;
                    }
                    assert!(!branch.iter().any(|op| matches!(op, Operator::Call { function_index } if prohibited.contains(function_index))), "own-data hits bypass conversions and full lookup");
                }
                assert_eq!(
                    packed, packed_reads,
                    "each numeric consumer has its own guarded packed read"
                );
                assert_eq!(
                    strings, string_reads,
                    "each computed String consumer has its own guarded own-data read"
                );
                for helper in &prohibited {
                    assert!(operators.iter().any(|op| matches!(op, Operator::Call { function_index } if function_index == helper)), "miss retains original semantic helper");
                }
                checked += 1;
            }
            index += 1;
        }
        assert!(checked > 0, "inspect actual source variants");
    }
}
