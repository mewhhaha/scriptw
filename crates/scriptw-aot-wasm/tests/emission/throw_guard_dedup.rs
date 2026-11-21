//! Actual P retains fresh abrupt checks while omitting checked straight-line repeats.
//! Each potentially throwing operation keeps one escaping Throw guard, on its
//! own result or on the function completion.
use std::collections::{BTreeMap, BTreeSet};

use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

use super::property_read_plans::Module;

/// The body's own Completion kind local: the epilogue returns tag, scalar,
/// reference, kind and target from the Completion's locals. (Its prologue
/// initialization stores only Wasm defaults into locals nothing wrote yet,
/// so `Function::store_constant` elides it.)
fn completion_kind(body: &[Operator<'_>]) -> u32 {
    match &body[body.len() - 6..] {
        [Operator::LocalGet { .. }, Operator::LocalGet { .. }, Operator::LocalGet { .. }, Operator::LocalGet { local_index }, Operator::LocalGet { .. }, Operator::End] => {
            *local_index
        }
        _ => panic!("source body returns one complete entry Completion"),
    }
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
    let mut all_guards = 0;
    for (index, window) in body.windows(4).enumerate() {
        let [Operator::LocalGet { local_index }, Operator::I32Const { value: 1 }, Operator::I32Eq, Operator::If { .. }] =
            window
        else {
            continue;
        };
        if else_arms.contains(&(index + 3)) {
            continue;
        }
        let end = ends[&(index + 3)];
        if matches!(body[end - 1], Operator::Br { relative_depth } if relative_depth > 0)
            || matches!(body[end - 1], Operator::Return | Operator::Unreachable)
        {
            // An operation's own result escapes its Throw before joining;
            // only the function's completion is re-tested after joins.
            all_guards += 1;
            if *local_index == kind {
                guards.insert(index, end);
            }
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
    all_guards
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
