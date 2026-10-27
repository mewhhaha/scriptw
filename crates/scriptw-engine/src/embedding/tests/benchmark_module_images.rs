use super::*;

fn assert_actual(observed: &BenchmarkModuleImage, actual: &wasmtime::Module) {
    let image = actual.image_range();
    let text = actual.text().as_ptr_range();
    assert_eq!(observed.image_range(), image.start.addr()..image.end.addr());
    assert_eq!(observed.text_range(), text.start.addr()..text.end.addr());
    assert!(observed.image_range().start < observed.image_range().end);
    assert!(observed.text_range().start < observed.text_range().end);
}

#[test]
fn diagnostic_images_match_actual_program_runtime_and_survive_store_drop() {
    let artifact = artifact("print('owned-images');", ForeignModuleRegistry::default());
    let runtime = EmbeddedRuntime::compile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let before = runtime.benchmark_module_images();
    assert_actual(before.program(), &runtime.modules.program);
    let actual_runtime = runtime
        .modules
        .runtime
        .as_ref()
        .expect("ordinary Portable program owns R");
    assert_actual(
        before.runtime().expect("actual R is reported"),
        actual_runtime,
    );
    assert_ne!(
        before.program().image_range(),
        before.runtime().unwrap().image_range()
    );
    {
        let mut store = store(&runtime, 0.0);
        let mut linker = linker(&runtime);
        runtime.register(&mut store, &mut linker).unwrap();
        let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
        assert_output(instance.evaluate(&mut store).unwrap(), "owned-images");
    }
    assert_eq!(runtime.benchmark_module_images(), before);
    assert_actual(before.program(), &runtime.modules.program);
    assert_actual(
        before.runtime().unwrap(),
        runtime.modules.runtime.as_ref().unwrap(),
    );
}
