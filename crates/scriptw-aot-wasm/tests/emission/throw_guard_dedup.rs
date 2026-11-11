//! Actual P retains fresh abrupt checks while omitting checked straight-line repeats.
use std::collections::{BTreeMap, BTreeSet};

use scriptw_aot_wasm::emit;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

use super::property_read_plans::Module;

fn completion_kind(body: &[Operator<'_>]) -> u32 {
    body.windows(10)
        .find_map(|window| match window {
            [
                Operator::I32Const { value: 0 },
                Operator::LocalSet { .. },
                Operator::I64Const { value: 0 },
                Operator::LocalSet { .. },
                Operator::RefNull { .. },
                Operator::LocalSet { .. },
                Operator::I32Const { value: 0 },
                Operator::LocalSet { local_index },
                Operator::I32Const { value: 0 },
                Operator::LocalSet { .. },
            ] => Some(*local_index),
            _ => None,
        })
        .expect("source body initializes one complete entry Completion")
}

fn check_guards(body: &[Operator<'_>]) -> usize {
    let kind = completion_kind(body);
    let mut stack = Vec::new();
    let mut ends = BTreeMap::new();
    let mut else_arms = BTreeSet::new();
    for (index, operator) in body.iter().enumerate() {
        match operator {
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::TryTable { .. }
            | Operator::Try { .. } => stack.push(index),
            Operator::Else => {
                else_arms.insert(*stack.last().unwrap());
            }
            Operator::End => {
                if let Some(start) = stack.pop() {
                    ends.insert(start, index);
                }
            }
            _ => {}
        }
    }
    assert!(stack.is_empty());
    let mut guards = BTreeMap::new();
    for (index, window) in body.windows(4).enumerate() {
        let [
            Operator::LocalGet { local_index },
            Operator::I32Const { value: 1 },
            Operator::I32Eq,
            Operator::If { .. },
        ] = window
        else {
            continue;
        };
        if *local_index != kind || else_arms.contains(&(index + 3)) {
            continue;
        }
        let end = ends[&(index + 3)];
        if matches!(body[end - 1], Operator::Br { relative_depth } if relative_depth > 0)
            || matches!(body[end - 1], Operator::Return | Operator::Unreachable)
        {
            guards.insert(index, end);
        }
    }
    let checked_ends = guards.values().copied().collect::<BTreeSet<_>>();
    let mut excluded = false;
    for (index, operator) in body.iter().enumerate() {
        if guards.contains_key(&index) {
            assert!(
                !excluded,
                "redundant escaping Throw guard at operator {index}"
            );
        }
        match operator {
            Operator::LocalSet { local_index } | Operator::LocalTee { local_index }
                if *local_index == kind =>
            {
                excluded = false
            }
            Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::If { .. }
            | Operator::Else
            | Operator::End
            | Operator::TryTable { .. }
            | Operator::Try { .. }
            | Operator::Catch { .. }
            | Operator::CatchAll
            | Operator::Delegate { .. }
            | Operator::Br { .. }
            | Operator::BrIf { .. }
            | Operator::BrTable { .. }
            | Operator::BrOnNull { .. }
            | Operator::BrOnNonNull { .. }
            | Operator::BrOnCast { .. }
            | Operator::BrOnCastFail { .. }
            | Operator::Return
            | Operator::ReturnCall { .. }
            | Operator::ReturnCallRef { .. }
            | Operator::ReturnCallIndirect { .. }
            | Operator::Throw { .. }
            | Operator::ThrowRef
            | Operator::Rethrow { .. }
            | Operator::Unreachable => excluded = false,
            _ => {}
        }
        if checked_ends.contains(&index) {
            excluded = true;
        }
    }
    guards.len()
}

#[test]
fn source_completion_guards_survive_writes_and_joins_without_straight_line_repeats() {
    let artifact = std::thread::Builder::new()
        .name("throw-guard-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Three formals retain the canonical vector entry. Dynamic keys
            // keep two independent potentially throwing property operations.
            let source = r#"
                function straight(receiver, first, second) {
                    receiver[first];
                    return receiver[second];
                }
                function joined(receiver, key, choose) {
                    if (choose) { receiver[key]; } else { receiver[key]; }
                    return receiver[key];
                }
                function looped(receiver, key, count) {
                    while (count-- > 0) { receiver[key]; }
                    return receiver[key];
                }
                function handled(receiver, key, unused) {
                    try { return receiver[key]; }
                    catch (error) { return receiver.other; }
                }
                globalThis.retained = [straight, joined, looped, handled];
            "#;
            let parsed = parse(source, ParseOptions::script()).unwrap();
            let lowered = lower(&parsed);
            assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
            emit(&lowered).expect("checked escape proof holds throughout actual R/P emission")
        })
        .unwrap()
        .join()
        .unwrap();
    for bytes in [
        artifact.bytes.as_slice(),
        artifact.runtime().unwrap().bytes(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("guard omission preserves all real Wasm control targets");
    }
    let program = Module::read(&artifact.bytes, true);
    for name in ["straight", "joined", "looped", "handled"] {
        let (_, body) = program.named(&format!("js::{name}#"));
        assert!(
            check_guards(body) >= 2,
            "{name} retains checks for independently throwing operations and handler/join entries"
        );
    }
}
