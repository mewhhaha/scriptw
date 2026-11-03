use super::*;

fn with_dispatch_bodies(source: &str, mut check: impl FnMut(&[Operator<'_>])) {
    let artifact = emit_script(source).expect("switch source emits");
    expect_valid_module(&artifact, 1);
    let names = function_names(&artifact)
        .into_iter()
        .filter(|(_, name)| name.starts_with("js::dispatch#"))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty(), "source function has emitted bodies");
    let mut index = imported_function_count(&artifact);
    let mut inspected = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("Wasm parses") {
            if names.contains_key(&index) {
                let operators = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();
                check(&operators);
                inspected += 1;
            }
            index += 1;
        }
    }
    assert_eq!(inspected, names.len(), "all source variants checked");
}

#[test]
fn dense_integer_switches_emit_real_tables_at_branch_workload_sizes() {
    for count in [32, 128, 512] {
        let cases = (0..count)
            .map(|index| format!("case {index}: return {};", index + 1))
            .collect::<String>();
        let source = format!(
            "function dispatch(value) {{ switch(value) {{ {cases} default: return -1; }} }} dispatch(0);"
        );
        with_dispatch_bodies(&source, |operators| {
            let tables = operators
                .iter()
                .filter_map(|operator| match operator {
                    Operator::BrTable { targets } => Some(targets.len()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(tables, [count], "one constant-time case dispatch");
            assert!(operators
                .iter()
                .any(|operator| matches!(operator, Operator::I32TruncSatF64S)));
            assert!(operators
                .iter()
                .any(|operator| matches!(operator, Operator::F64Eq)));
        });
    }
}

#[test]
fn table_entries_keep_first_duplicate_and_middle_default_source_positions() {
    with_dispatch_bodies(
        "function dispatch(value) { switch(value) { case -0: return 0; case 0: return 99; default: return -1; case 1: return 1; case 2: return 2; case 3: return 3; } } dispatch(0);",
        |operators| {
            let tables = operators
                .iter()
                .filter_map(|operator| match operator {
                    Operator::BrTable { targets } => Some((
                        targets.targets().collect::<Result<Vec<_>, _>>().unwrap(),
                        targets.default(),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(tables, [(vec![1, 4, 5, 6], 3)]);
        },
    );
}

#[test]
fn dense_switch_admission_retains_ordered_fallback_for_other_selectors() {
    for cases in [
        "case 0: break; case 1: break; case 2: break; case probe(): break;",
        "case 0: break; case 1: break; case 2: break; case '3': break;",
        "case 0: break; case 1: break; case 2: break; case 3n: break;",
        "case 0: break; case 1: break; case 2: break; case 3.5: break;",
        "case 0: break; case 1: break; case 2: break; case 10000: break;",
        "case 2147483648: break; case 2147483649: break; case 2147483650: break; case 2147483651: break;",
        "case 0: break; case 1: break; case 2: break; case 0 / 0: break;",
        "case 0: break; case 1: break; case 2: break; case 1 / 0: break;",
    ] {
        let source = format!(
            "function probe() {{ return 3; }} function dispatch(value) {{ switch(value) {{ {cases} }} return 7; }} dispatch(0);"
        );
        with_dispatch_bodies(&source, |operators| {
            assert!(
                operators
                    .iter()
                    .all(|operator| !matches!(operator, Operator::BrTable { .. })),
                "nonliteral, sparse, fractional or out-of-range cases retain ordered tests: {cases}"
            );
        });
    }
}
