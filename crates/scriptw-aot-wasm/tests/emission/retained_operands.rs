//! Private evaluated operands have no source TDZ; real lexical reads retain it.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::emit;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{ExprIr, StatementIr, lower};
use wasmparser::{Operator, Validator, WasmFeatures};

fn calls(body: &[Operator<'_>], callee: u32) -> usize {
    body.iter()
        .filter(|op| {
            matches!(op,
        Operator::Call { function_index } if *function_index == callee)
        })
        .count()
}

#[test]
fn held_method_receivers_omit_private_tdz_checks_but_keep_source_checks() {
    std::thread::Builder::new()
        .name("retained-operands".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let program = lower(
                &parse(
                    r#"
function reference(receiver,argument,extra) { 'use strict'; return (receiver.method,argument); }
function invoke(receiver,argument,extra) { 'use strict'; return receiver.method(argument); }
function identity(receiver,argument,extra) { 'use strict'; return receiver; }
function lexical(receiver,argument,extra) { 'use strict'; let saved=receiver; return saved; }
[reference,invoke,identity,lexical];
"#,
                    ParseOptions::script(),
                )
                .unwrap(),
            );
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let source = program
                .script
                .as_ref()
                .unwrap()
                .functions
                .iter()
                .find(|function| function.name == "invoke")
                .unwrap();
            let returned = source
                .body
                .statements
                .iter()
                .find_map(|statement| match statement {
                    StatementIr::Return(value) => Some(value),
                    _ => None,
                })
                .unwrap();
            assert!(
                matches!(&returned.expr, ExprIr::MaterializeBinding {name,..}
                if name.starts_with("$call.receiver.")),
                "the actual source call has a compiler-owned receiver binding"
            );
            let artifact = emit(&program).unwrap();
            for bytes in [
                artifact.bytes.as_slice(),
                artifact.runtime().unwrap().bytes(),
            ] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .unwrap();
            }
            let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
            let program = Module::read(&artifact.bytes, true);
            let (definition, _) = runtime.named("helper::lexical_binding_uninitialized");
            let tdz = program.imported_runtime_function(&runtime, definition);
            assert_eq!(
                program.function_type(tdz),
                runtime.function_type(definition)
            );
            let (_, reference) = program.named("js::reference#");
            let (_, invoke) = program.named("js::invoke#");
            assert_eq!(
                calls(reference, tdz),
                0,
                "completed ordinary parameters are initialized before the body"
            );
            assert_eq!(
                calls(invoke, tdz),
                calls(reference, tdz),
                "held callee-base and receiver uses add no private TDZ branches"
            );
            let (_, identity) = program.named("js::identity#");
            let (_, lexical) = program.named("js::lexical#");
            assert_eq!(calls(identity, tdz), 0);
            assert_eq!(
                calls(lexical, tdz),
                1,
                "the initialized source let still owns its TDZ protocol"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
