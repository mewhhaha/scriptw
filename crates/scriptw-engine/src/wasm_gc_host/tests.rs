//! Actual import domains and unchanged present-import admission/late arming.
use super::*;

fn leb(mut value: usize, bytes: &mut Vec<u8>) {
    loop {
        let next = (value & 127) as u8;
        value >>= 7;
        bytes.push(next | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}

fn imported(namespace: &str, name: &str, descriptor: &[u8], copies: usize) -> Vec<u8> {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    // One () -> () type: deliberately invalid for every native compiler ABI.
    bytes.extend_from_slice(&[1, 4, 1, 0x60, 0, 0]);
    let mut imports = Vec::new();
    leb(copies, &mut imports);
    for _ in 0..copies {
        for label in [namespace, name] {
            leb(label.len(), &mut imports);
            imports.extend_from_slice(label.as_bytes());
        }
        imports.extend_from_slice(descriptor);
    }
    bytes.push(2);
    leb(imports.len(), &mut bytes);
    bytes.extend(imports);
    bytes
}

#[test]
fn emitted_portable_and_native_r_keep_actual_compiler_import_domains() {
    run_on_sized_stack(|| {
        let engine = shared_wasm_engine().unwrap();
        let selection = IntlDataSelection::new(IntlCompilationProfile::Minimal);
        for (profile, expected) in [
            (
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::Portable,
                false,
            ),
            (
                scriptw_aot_wasm::RuntimeSourceCompilationProfile::NativeHost,
                true,
            ),
        ] {
            let runtime = scriptw_aot_wasm::runtime_artifact_with_inputs(
                &selection,
                scriptw_aot_wasm::RuntimeArtifactInputs::new(None)
                    .with_source_compilation_profile(profile),
            )
            .unwrap();
            let module = compile_wasm_module(&engine, runtime.bytes()).unwrap();
            assert_eq!(imports_native_source_compiler(&module), expected);
            for row in SOURCE_COMPILER_IMPORTS {
                assert_eq!(
                    module.imports().any(|import| {
                        import.module() == row.module() && import.name() == row.name()
                    }),
                    expected,
                    "the emitted {profile:?} runtime owns the actual {row:?} import domain"
                );
            }
            let mut linker = WasmtimeLinker::new(&engine);
            link(&mut linker, &module).unwrap();
        }
        Ok::<_, EngineError>(())
    })
    .unwrap();
}

#[test]
fn present_compiler_names_preserve_wrong_signature_kind_and_duplicate_rejection() {
    let engine = shared_wasm_engine().unwrap();
    for (row, family) in SOURCE_COMPILER_IMPORTS.into_iter().zip([
        "native Function compiler",
        "native global Script compiler",
        "native direct eval compiler",
    ]) {
        for (descriptor, copies, expected) in [
            (&[0, 0][..], 1, format!("{family} signature:")),
            (
                &[2, 0, 1][..],
                1,
                format!("{family} import is not a function"),
            ),
            (&[0, 0][..], 2, format!("duplicate {family} import")),
        ] {
            let module = compile_wasm_module(
                &engine,
                &imported(row.module(), row.name(), descriptor, copies),
            )
            .unwrap();
            assert!(imports_native_source_compiler(&module));
            let mut linker = WasmtimeLinker::new(&engine);
            let error = link(&mut linker, &module).unwrap_err();
            assert!(error.to_string().contains(&expected), "{error}");
        }
        let unrelated =
            compile_wasm_module(&engine, &imported("unrelated", row.name(), &[0, 0], 1)).unwrap();
        assert!(!imports_native_source_compiler(&unrelated));
        link(&mut WasmtimeLinker::new(&engine), &unrelated).unwrap();
    }
}

#[test]
fn native_compilers_still_arm_after_linking_before_all_three_dynamic_calls() {
    let function =
        Arc::new(wasm_native_function_compiler::NativeFunctionCompilationObserver::default());
    let global = Arc::new(
        wasm_native_global_script_compiler::NativeGlobalScriptCompilationObserver::default(),
    );
    let direct =
        Arc::new(wasm_native_direct_eval_compiler::NativeDirectEvalCompilationObserver::default());
    let mut engine = Engine::new(RealmBuilder::new().build());
    engine.native_function_compilation_observer = Some(Arc::clone(&function));
    engine.native_global_script_compilation_observer = Some(Arc::clone(&global));
    engine.native_direct_eval_compilation_observer = Some(Arc::clone(&direct));
    let outcome = engine.observe_script(
        r#"
var token = 'late_arm_' + Date.now();
var compiled = Function('return 23; /* ' + token + ' */');
var indirect = eval;
indirect('globalThis["' + token + '"] = 31;');
function local(text) { let value = 47; return eval(text); }
if (compiled() !== 23 || globalThis[token] !== 31 || local('value + 1; /* ' + token + ' */') !== 48) throw new Error('native compiler authority changed');
1;
"#,
        CompileOptions::default(),
        RunOptions { timeout_ms: Some(60_000), ..RunOptions::default() },
    ).unwrap();
    assert_eq!(
        outcome.completion,
        ObservedCompletion::Normal(ObservedJsValue::Number(ObservedNumber::from_f64(1.0)))
    );
    assert!(function.snapshot().requests > 0 && function.snapshot().completed_factories > 0);
    assert!(global.snapshot().requests > 0 && global.snapshot().completed_factories > 0);
    assert!(direct.snapshot().requests > 0 && direct.snapshot().completed_factories > 0);
}
