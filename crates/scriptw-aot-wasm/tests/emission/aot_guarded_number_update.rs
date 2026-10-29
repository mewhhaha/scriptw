use super::linked_bodies::{linked_bodies, runtime_function_imports};
use scriptw_aot_wasm::{emit, WasmRuntimeValueTag};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, ExprIr, NumericUpdateValueKind, StatementIr};
use wasmparser::{Operator, Parser, Payload, TypeRef, Validator, WasmFeatures};

fn body_operators(bytes: &[u8], index: u32) -> Vec<Operator<'_>> {
    let mut next_function = 0;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("guarded update module decodes") {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    if matches!(
                        import.expect("function import decodes").ty,
                        TypeRef::Func(_) | TypeRef::FuncExact(_)
                    ) {
                        next_function += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                if next_function == index {
                    return body
                        .get_operators_reader()
                        .expect("guarded update body opens")
                        .into_iter()
                        .map(|operator| operator.expect("guarded update operator decodes"))
                        .collect();
                }
                next_function += 1;
            }
            _ => {}
        }
    }
    panic!("missing encoded body {index}");
}

fn assert_number_bypasses_conversion(body: &[Operator<'_>], conversion: u32) {
    let calls = body
        .iter()
        .enumerate()
        .filter_map(|(position, operator)| {
            matches!(operator, Operator::Call { function_index } if *function_index == conversion)
                .then_some(position)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls.len(),
        1,
        "one complete fallback conversion is encoded"
    );
    let call = calls[0];
    let guards = body
        .windows(4)
        .enumerate()
        .filter_map(|(position, operators)| match operators {
            [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Ne, Operator::If { .. }]
                if *value == WasmRuntimeValueTag::Number as i32 => Some(position + 3),
            _ => None,
        })
        .filter(|&guard| guard < call)
        .collect::<Vec<_>>();
    let mut found = false;
    for guard in guards {
        let mut depth = 1;
        for (position, operator) in body.iter().enumerate().skip(guard + 1) {
            match operator {
                Operator::Block { .. }
                | Operator::Loop { .. }
                | Operator::If { .. }
                | Operator::TryTable { .. } => depth += 1,
                Operator::Else if depth == 1 => {
                    assert!(position > call, "conversion must be in the non-Number arm");
                    break;
                }
                Operator::End => {
                    depth -= 1;
                    if depth == 0 {
                        if position > call {
                            assert!(
                                body[position + 1..].windows(4).any(|operators| matches!(
                                    operators,
                                    [Operator::LocalGet { .. }, Operator::F64ReinterpretI64,
                                     Operator::F64Const { value }, Operator::F64Add]
                                        if f64::from_bits(value.bits()) == 1.0
                                )),
                                "the acquired numeric payload is incremented after the guard rejoins"
                            );
                            found = true;
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    assert!(
        found,
        "P must branch around the actual conversion call for an acquired Number tag"
    );
}

#[test]
fn program_number_tag_guards_bypass_the_actual_runtime_update_conversions() {
    std::thread::Builder::new()
        .name("guarded-number-update-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Publishing the functions retains their unspecialized source
            // bodies. The primitive result kind deliberately still needs
            // conversion; the Number result kind alone cannot skip it.
            let source = r#"
function number() { let value = -0; return value++; }
function primitive() { let value = '4'; return value++; }
function dynamic(value) { return value++; }
function property(record) { return record.value++; }
[number, primitive, dynamic, property];
"#;
            let parsed = parse(source, ParseOptions::script()).expect("update fixtures parse");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program
                .script
                .as_ref()
                .expect("update fixtures own script IR");
            for (name, expected) in [
                ("number", NumericUpdateValueKind::Number),
                ("primitive", NumericUpdateValueKind::Number),
                ("dynamic", NumericUpdateValueKind::Dynamic),
            ] {
                let functions = script
                    .functions
                    .iter()
                    .filter(|function| function.name == name)
                    .collect::<Vec<_>>();
                assert_eq!(functions.len(), 1, "one unspecialized {name} body");
                assert!(
                    functions[0]
                        .body
                        .statements
                        .iter()
                        .any(|statement| matches!(
                            statement,
                            StatementIr::Return(value)
                                if matches!(&value.expr, ExprIr::UpdateIdentifier { value_kind, .. }
                                    if *value_kind == expected)
                        )),
                    "{name} must exercise the intended numeric-update conversion seam"
                );
            }
            let artifact = emit(&program).expect("guarded updates emit actual compiled Wasm");
            Validator::new_with_features(WasmFeatures::all())
                .validate_all(&artifact.bytes)
                .expect("the physical guarded program validates");
            let runtime = artifact.runtime().expect("update conversions link R");
            let linked = linked_bodies(&artifact);
            let imports = runtime_function_imports(&artifact.bytes, runtime.bytes());
            for (name, helper) in [
                ("number", "helper::value_to_number"),
                ("primitive", "helper::value_to_number"),
                ("dynamic", "helper::value_to_numeric"),
                ("property", "helper::value_to_numeric"),
            ] {
                let definition = linked.unique(helper).index;
                let targets = imports
                    .iter()
                    .filter_map(|(&import, &target)| (target == definition).then_some(import))
                    .collect::<Vec<_>>();
                assert_eq!(
                    targets.len(),
                    1,
                    "P imports the actual {helper} definition once"
                );
                let prefix = format!("js::{name}#");
                let bodies = linked
                    .program
                    .iter()
                    .filter(|body| body.name.starts_with(&prefix))
                    .collect::<Vec<_>>();
                assert_eq!(bodies.len(), 1, "one actual source definition for {name}");
                let body = body_operators(&artifact.bytes, bodies[0].index);
                assert_number_bypasses_conversion(&body, targets[0]);
            }
        })
        .expect("emission worker starts")
        .join()
        .expect("guarded numeric update emission does not panic");
}
