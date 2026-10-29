#[cfg(feature = "benchmark-module-images")]
mod benchmark_module_images;
use super::*;
mod realm_async;
mod transfer_retained;
use scriptw_ir::foreign::{
    ForeignErrorTag, ForeignFunctionDeclaration, ForeignModuleDeclaration, ForeignResultType,
    ForeignValueType,
};

mod async_operations;
mod compact_records;
mod reentrant_execution;

fn registry(error_tag: bool) -> ForeignModuleRegistry {
    let mut registry = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "dd:host",
        vec![
            ForeignFunctionDeclaration::new(
                "add",
                "dd",
                "add",
                vec![ForeignValueType::Number; 2],
                ForeignResultType::Number,
            )
            .unwrap(),
            ForeignFunctionDeclaration::new(
                "record",
                "dd",
                "record",
                vec![ForeignValueType::Number],
                ForeignResultType::Void,
            )
            .unwrap(),
        ],
    )
    .unwrap()])
    .unwrap();
    if error_tag {
        registry = registry.with_error_tag(ForeignErrorTag::code_v1());
    }
    registry
}

fn artifact(source: &str, declarations: ForeignModuleRegistry) -> Artifact {
    configure_compilation_jobs(1).unwrap();
    let compiler = Engine::new(RealmBuilder::new().build());
    let unit = compiler
        .compile_module(
            source,
            CompileOptions {
                foreign_modules: declarations,
                ..CompileOptions::default()
            },
        )
        .expect("declared module compiles without a filesystem native-module source");
    compiler
        .emit_wasm(&unit)
        .expect("foreign calls emit real Portable P/R")
}

#[derive(Default)]
struct UserData {
    offset: f64,
    calls: usize,
    values: Vec<f64>,
}

fn store(runtime: &EmbeddedRuntime, offset: f64) -> WasmtimeStore<WasmStoreState> {
    let state = runtime
        .new_state(
            RealmBuilder::new().build(),
            UserData {
                offset,
                ..Default::default()
            },
        )
        .unwrap();
    let mut store = WasmtimeStore::new(&runtime.engine, state);
    store.limiter(|state| state.limits_mut());
    store.set_epoch_deadline(u64::MAX / 2);
    store
}

fn linker(runtime: &EmbeddedRuntime) -> WasmtimeLinker<WasmStoreState> {
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    linker
        .func_wrap(
            "dd",
            "add",
            |mut caller: WasmtimeCaller<'_, WasmStoreState>, a: f64, b: f64| {
                let state = caller.data_mut().user_data_mut::<UserData>().unwrap();
                state.calls += 1;
                let value = a + b;
                if state.offset == 0.0 {
                    value
                } else {
                    value + state.offset
                }
            },
        )
        .unwrap();
    linker
        .func_wrap(
            "dd",
            "record",
            |mut caller: WasmtimeCaller<'_, WasmStoreState>, value: f64| {
                caller
                    .data_mut()
                    .user_data_mut::<UserData>()
                    .unwrap()
                    .values
                    .push(value);
            },
        )
        .unwrap();
    linker
}

fn assert_output(outcome: ObservedRunOutcome, expected: &str) {
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(
        matches!(outcome.completion, ObservedCompletion::Normal(_)),
        "{outcome:?}"
    );
    assert_eq!(
        outcome.output_events,
        [HostOutputEvent::PrintLine(expected.into())]
    );
}

#[test]
fn ordinary_linker_numeric_functions_keep_first_class_call_and_strict_number_semantics() {
    let artifact = artifact(
        r#"
import { add, record } from 'dd:host';
if (add.name !== 'add' || add.length !== 2) throw 'callable metadata';
if (Object.hasOwn(add, 'prototype')) throw 'foreign function is constructable';
try { new add(1, 2); throw 'constructed'; } catch (error) { if (!(error instanceof TypeError)) throw error; }
var alias = add, bound = add.bind({ignored: true}, 20);
function through(fn) { return () => fn.call(null, 20, 22); }
if (alias(20,22) !== 42 || bound(22) !== 42 || through(add)() !== 42 || add.apply({}, [20,22]) !== 42) throw 'first class';
var events = [];
try { add({ valueOf() { throw 'must not coerce'; } }, (events.push(1), 2), (events.push(2), 3)); throw 'accepted object'; }
catch (error) { if (!(error instanceof TypeError)) throw error; }
if (events.join(',') !== '1,2') throw 'argument evaluation order';
for (var value of [undefined, null, false, '1', 1n, Symbol(), {}]) {
  try { add(value, 2); throw 'accepted non-number'; } catch (error) { if (!(error instanceof TypeError)) throw error; }
}
try { add(1); throw 'accepted missing'; } catch (error) { if (!(error instanceof TypeError)) throw error; }
if (!Number.isNaN(add(NaN, 1)) || add(Infinity, 1) !== Infinity || 1/add(-0,-0) !== -Infinity) throw 'number bits';
try { record('bad'); throw 'accepted void argument'; } catch (error) { if (!(error instanceof TypeError)) throw error; }
if (record(42) !== undefined) throw 'void result';
print('foreign-ok');
"#,
        registry(false),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut store = store(&runtime, 0.0);
    let mut linker = linker(&runtime);
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    assert_output(instance.evaluate(&mut store).unwrap(), "foreign-ok");
    let state = store.data().user_data::<UserData>().unwrap();
    assert_eq!(
        state.calls, 7,
        "only valid calls may enter native implementations"
    );
    assert_eq!(state.values, [42.0]);
    assert!(store.data().user_data::<String>().is_none());
}

#[test]
fn persisted_artifact_and_two_owned_stores_keep_their_own_userdata_and_realms() {
    let artifact = artifact(
        "import {add} from 'dd:host'; if (globalThis.seen) throw 'shared Realm'; globalThis.seen=true; print(add(20,22));",
        registry(false),
    );
    let bundle = artifact
        .encode_portable_bundle(&IntlCompilationProfile::Minimal)
        .unwrap();
    let loaded = Engine::new(RealmBuilder::new().build())
        .load_portable_bundle(&bundle)
        .unwrap();
    assert!(
        loaded.compilation_policy.is_none(),
        "loading needs no source compiler policy or closure registry"
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &loaded).unwrap();
    for (offset, expected) in [(0.0, "42"), (100.0, "142")] {
        let mut store = store(&runtime, offset);
        let mut linker = linker(&runtime);
        runtime.register(&mut store, &mut linker).unwrap();
        let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        assert_output(instance.evaluate(&mut store).unwrap(), expected);
        assert_eq!(store.data().user_data::<UserData>().unwrap().calls, 1);
        #[cfg(feature = "native-source-compiler")]
        assert!(store.data().native_source_compiler.is_none());
    }
}

#[test]
fn missing_foreign_module_function_and_wrong_signature_fail_before_runtime_instantiation() {
    let artifact = artifact(
        "import {add} from 'dd:host'; print(add(1,2));",
        registry(false),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    for wrong in [false, true] {
        let mut store = store(&runtime, 0.0);
        let mut linker = WasmtimeLinker::new(&runtime.engine);
        if wrong {
            linker
                .func_wrap("dd", "add", |a: i32, b: i32| a + b)
                .unwrap();
        }
        runtime.register(&mut store, &mut linker).unwrap();
        let error = runtime
            .instantiate(&mut store, &mut linker)
            .err()
            .expect("foreign linkage must fail");
        assert!(
            error.to_string().contains(if wrong {
                "type mismatch"
            } else {
                "missing declared foreign import"
            }),
            "{error}"
        );
        assert!(store.data().embedding.as_ref().unwrap().phase == StorePhase::Failed);
        assert_eq!(store.data().user_data::<UserData>().unwrap().calls, 0);
    }
    let mut store = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    linker
        .func_wrap("dd", "another_name", |a: f64, b: f64| a + b)
        .unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    assert!(runtime
        .instantiate(&mut store, &mut linker)
        .err()
        .unwrap()
        .to_string()
        .contains("dd.add"));
}

#[test]
fn declared_operation_error_is_a_js_error_and_unrelated_host_failure_stays_fatal() {
    let source = "import {add,record} from 'dd:host'; try { add(1,2); throw 'returned'; } catch(error) { if (!(error instanceof Error) || error.code !== -2147483648) throw error; var descriptor=Object.getOwnPropertyDescriptor(error,'code'); if(!descriptor.writable || !descriptor.enumerable || !descriptor.configurable) throw 'error code attributes'; } try { record(42); throw 'void returned'; } catch(error) { if (!(error instanceof Error) || error.code !== 42) throw error; } print('caught-code');";
    let artifact = artifact(source, registry(true));
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut expected_store = store(&runtime, 0.0);
    let mut expected_linker = WasmtimeLinker::new(&runtime.engine);
    expected_linker
        .func_wrap(
            "dd",
            "add",
            |caller: WasmtimeCaller<'_, WasmStoreState>, _: f64, _: f64| -> wasmtime::Result<f64> {
                throw_foreign_error(caller, i32::MIN)
            },
        )
        .unwrap();
    expected_linker
        .func_wrap(
            "dd",
            "record",
            |caller: WasmtimeCaller<'_, WasmStoreState>, _: f64| -> wasmtime::Result<()> {
                throw_foreign_error(caller, 42)
            },
        )
        .unwrap();
    runtime
        .register(&mut expected_store, &mut expected_linker)
        .unwrap();
    let instance = runtime
        .instantiate(&mut expected_store, &mut expected_linker)
        .unwrap();
    assert_output(
        instance.evaluate(&mut expected_store).unwrap(),
        "caught-code",
    );

    let mut fatal_store = store(&runtime, 0.0);
    let mut fatal_linker = WasmtimeLinker::new(&runtime.engine);
    fatal_linker
        .func_wrap("dd", "add", |_: f64, _: f64| -> wasmtime::Result<f64> {
            Err(wasmtime::Error::msg("fatal-foreign-host"))
        })
        .unwrap();
    fatal_linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
    runtime
        .register(&mut fatal_store, &mut fatal_linker)
        .unwrap();
    let instance = runtime
        .instantiate(&mut fatal_store, &mut fatal_linker)
        .unwrap();
    let failure = instance.evaluate(&mut fatal_store).unwrap_err();
    assert_eq!(
        failure.wasm_execution_failure_kind(),
        Some(WasmExecutionFailureKind::Trap)
    );
    assert!(failure.to_string().contains("fatal-foreign-host"));

    // The error signature alone is insufficient: the host throw helper and
    // the wrapper's catch must refer to the same nominal Store-owned tag.
    let mut replaced_store = store(&runtime, 0.0);
    let mut replaced_linker = linker(&runtime);
    runtime
        .register(&mut replaced_store, &mut replaced_linker)
        .unwrap();
    let exception = ExnType::new(&runtime.engine, [ValType::I32]).unwrap();
    let different_tag = wasmtime::Tag::new(&mut replaced_store, &exception.tag_type()).unwrap();
    replaced_linker.allow_shadowing(true);
    let declared = ForeignErrorTag::code_v1();
    replaced_linker
        .define(
            &replaced_store,
            declared.module(),
            declared.name(),
            different_tag,
        )
        .unwrap();
    assert!(runtime
        .instantiate(&mut replaced_store, &mut replaced_linker)
        .err()
        .unwrap()
        .to_string()
        .contains("differs from the registered Store tag"));
}

#[test]
fn typed_semantic_rejections_and_classified_host_errors_keep_their_categories() {
    // Even with the operation-error tag declared, a Rust host rejection is
    // not that Wasm exception and must not be caught as a JS operation error.
    let artifact = artifact(
        "import {add} from 'dd:host'; try { add(1,2); } catch(error) { print('must-not-catch'); }",
        registry(true),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    for preclassified in [false, true] {
        let mut store = store(&runtime, 0.0);
        let mut linker = WasmtimeLinker::new(&runtime.engine);
        linker
            .func_wrap(
                "dd",
                "add",
                move |_: f64, _: f64| -> wasmtime::Result<f64> {
                    if preclassified {
                        Err(wasmtime::Error::new(
                            EngineError::from_runtime_semantic_rejection(
                                RuntimeSemanticRejection::Gap(
                                    RuntimeSemanticGap::RegExpRuntimePatternCompilation,
                                ),
                            ),
                        ))
                    } else {
                        Err(wasmtime::Error::new(
                            RuntimeSemanticRejection::DynamicSource(
                                DynamicSourceRuntimeOperation::Eval,
                            ),
                        ))
                    }
                },
            )
            .unwrap();
        linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
        runtime.register(&mut store, &mut linker).unwrap();
        let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        let failure = instance.evaluate(&mut store).unwrap_err();
        if preclassified {
            assert_eq!(
                failure.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::SemanticGap)
            );
            assert_eq!(
                failure.runtime_semantic_gaps(),
                [RuntimeSemanticGap::RegExpRuntimePatternCompilation]
            );
        } else {
            assert_eq!(
                failure.wasm_execution_failure_kind(),
                Some(WasmExecutionFailureKind::DynamicSource)
            );
            assert_eq!(
                failure.runtime_dynamic_source_operations(),
                [DynamicSourceRuntimeOperation::Eval]
            );
        }
        assert!(
            store.data().output_events.take().is_empty(),
            "JS catch must not run"
        );
    }
}

#[test]
fn undeclared_error_tags_and_foreign_store_engine_lifecycles_are_not_authority() {
    let artifact = artifact(
        "import {add} from 'dd:host'; try { add(1,2); } catch(error) { print('must-not-catch'); }",
        registry(false),
    );
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut first = store(&runtime, 0.0);
    let mut linker = WasmtimeLinker::new(&runtime.engine);
    linker
        .func_wrap(
            "dd",
            "add",
            |caller: WasmtimeCaller<'_, WasmStoreState>, _: f64, _: f64| -> wasmtime::Result<f64> {
                throw_foreign_error(caller, 5)
            },
        )
        .unwrap();
    linker.func_wrap("dd", "record", |_: f64| {}).unwrap();
    runtime.register(&mut first, &mut linker).unwrap();
    assert!(
        runtime.register(&mut first, &mut linker).is_err(),
        "registration is one-shot"
    );
    let instance = runtime.instantiate(&mut first, &mut linker).unwrap();
    assert!(instance
        .evaluate(&mut first)
        .unwrap_err()
        .to_string()
        .contains("not explicitly declared"));
    let mut second = store(&runtime, 0.0);
    let mut second_linker = super::tests::linker(&runtime);
    runtime.register(&mut second, &mut second_linker).unwrap();
    let instance = runtime
        .instantiate(&mut second, &mut second_linker)
        .unwrap();
    let mut other = store(&runtime, 0.0);
    assert!(instance
        .evaluate(&mut other)
        .unwrap_err()
        .to_string()
        .contains("different Store"));
    let mut config = WasmtimeConfig::new();
    configure_wasm_runtime(&mut config);
    let foreign_engine = WasmtimeEngine::new(&config).unwrap();
    let mut wrong_store = WasmtimeStore::new(
        &foreign_engine,
        runtime.new_state(RealmBuilder::new().build(), ()).unwrap(),
    );
    let mut wrong_linker = WasmtimeLinker::new(&foreign_engine);
    assert!(runtime
        .register(&mut wrong_store, &mut wrong_linker)
        .is_err());
}

#[test]
fn embedder_fuel_limit_is_not_reset_and_different_foreign_programs_share_unchanged_r() {
    let first = artifact(
        "import {add} from 'dd:host'; print(add(20,22));",
        registry(false),
    );
    let other_declarations = ForeignModuleRegistry::new(vec![ForeignModuleDeclaration::new(
        "tools:native",
        vec![ForeignFunctionDeclaration::new(
            "multiply",
            "tools",
            "multiply",
            vec![ForeignValueType::Number; 2],
            ForeignResultType::Number,
        )
        .unwrap()],
    )
    .unwrap()])
    .unwrap();
    let second = artifact(
        "import {multiply} from 'tools:native'; print(multiply(6,7));",
        other_declarations,
    );
    assert_eq!(
        first.runtime.as_ref().unwrap().key(),
        second.runtime.as_ref().unwrap().key()
    );
    assert_ne!(first.bytes, second.bytes);
    let second_runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &second).unwrap();
    let mut second_store = store(&second_runtime, 0.0);
    let mut second_linker = WasmtimeLinker::new(&second_runtime.engine);
    second_linker
        .func_wrap("tools", "multiply", |a: f64, b: f64| a * b)
        .unwrap();
    second_runtime
        .register(&mut second_store, &mut second_linker)
        .unwrap();
    let instance = second_runtime
        .instantiate(&mut second_store, &mut second_linker)
        .unwrap();
    assert_output(instance.evaluate(&mut second_store).unwrap(), "42");

    // Fuel changes native-image compatibility. A real standalone scalar P is
    // enough to prove ownership of the caller's fuel, without compiling a new
    // full R merely for an instrumentation configuration control.
    let compiler = Engine::new(RealmBuilder::new().build());
    let scalar = compiler
        .compile_script("40 + 2;", CompileOptions::default())
        .unwrap();
    let scalar = compiler.emit_wasm(&scalar).unwrap();
    assert!(scalar.runtime.is_none());
    let mut config = WasmtimeConfig::new();
    configure_wasm_runtime(&mut config);
    config.consume_fuel(true);
    let engine = WasmtimeEngine::new(&config).unwrap();
    let runtime = EmbeddedRuntime::compile(&engine, &scalar).unwrap();
    let mut exhausted = store(&runtime, 0.0);
    exhausted.set_fuel(0).unwrap();
    let mut exhausted_linker = linker(&runtime);
    runtime
        .register(&mut exhausted, &mut exhausted_linker)
        .unwrap();
    assert_eq!(exhausted.get_fuel().unwrap(), 0);
    assert!(runtime
        .instantiate(&mut exhausted, &mut exhausted_linker)
        .err()
        .unwrap()
        .to_string()
        .contains("fuel"));
    assert_eq!(exhausted.get_fuel().unwrap(), 0);
    let mut store = store(&runtime, 0.0);
    store.set_fuel(1_000_000).unwrap();
    let mut linker = linker(&runtime);
    let initial_fuel = store.get_fuel().unwrap();
    runtime.register(&mut store, &mut linker).unwrap();
    assert_eq!(store.get_fuel().unwrap(), initial_fuel);
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    let remaining = store.get_fuel().unwrap();
    assert!(remaining > 0 && remaining < initial_fuel);
    // Instantiation executes compiler-owned initialization and consumes fuel.
    // Exhaust the caller's remaining budget before explicit program entry.
    store.set_fuel(0).unwrap();
    let failure = instance.evaluate(&mut store).unwrap_err();
    assert_eq!(
        failure.wasm_execution_failure_kind(),
        Some(WasmExecutionFailureKind::Trap)
    );
    assert_eq!(store.get_fuel().unwrap(), 0);
    assert_eq!(store.data().user_data::<UserData>().unwrap().calls, 0);
}
