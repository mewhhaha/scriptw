use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, StatementIr};

fn guarded_body_bytes(condition: &str, repetitions: usize) -> u32 {
    let condition = condition.to_string();
    std::thread::Builder::new()
        .name(format!("constant-number-guard-{repetitions}"))
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let guard = format!("if ({condition}) {{ new Failure('shift: ' + (1 << 0)); }}\n");
            let source = format!(
                "function Failure(message) {{ this.message = message; }}\n\
                 function checks(value) {{ {} return 1; }} checks(1);",
                guard.repeat(repetitions)
            );
            let parsed = parse(&source, ParseOptions::script()).expect("ordinary guards parse");
            let lowered = lower(&parsed);
            assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
            let checks = lowered
                .script
                .as_ref()
                .expect("script IR")
                .functions
                .iter()
                .find(|function| function.name == "checks")
                .expect("checks function");
            assert_eq!(
                checks
                    .body
                    .statements
                    .iter()
                    .filter(|statement| matches!(statement, StatementIr::If { .. }))
                    .count(),
                repetitions,
                "all branches must reach lowering and planning"
            );
            let artifact = emit(&lowered).expect("ordinary guards emit real Wasm");
            artifact
                .function_sizes
                .iter()
                .filter(|body| body.name.starts_with("js::checks#"))
                .map(|body| body.body_bytes.bytes())
                .max()
                .expect("checks function emitted")
        })
        .expect("compiler worker starts")
        .join()
        .expect("guard emission does not panic")
}

#[test]
fn unreachable_numeric_guards_do_not_repeat_constructor_machinery() {
    let single = guarded_body_bytes("-2147483649 >> 33 !== 1073741823", 1);
    let repeated = guarded_body_bytes("-2147483649 >> 33 !== 1073741823", 257);
    let growth = repeated
        .checked_sub(single)
        .expect("statement completion seeds remain");
    assert!(
        growth < 256 * 128,
        "256 unreachable guards added {growth} bytes ({single} -> {repeated})"
    );
}

#[test]
fn selected_and_mutable_guards_retain_their_constructor_calls() {
    for condition in ["1 << 0 === 1", "value << 0 !== 1"] {
        let single = guarded_body_bytes(condition, 1);
        let repeated = guarded_body_bytes(condition, 9);
        assert!(
            repeated > single + 8 * 128,
            "{condition}: reachable constructor work disappeared ({single} -> {repeated})"
        );
    }
}

#[test]
fn unreachable_numeric_guards_do_not_hide_early_errors() {
    for source in [
        "if (1 << 0 !== 1) { let duplicate; let duplicate; }",
        "if (1 << 0 !== 1) { break missing; }",
        "'use strict'; if (1 << 0 !== 1) { delete identifier; }",
    ] {
        assert!(parse(source, ParseOptions::script()).is_err(), "{source}");
    }
}
