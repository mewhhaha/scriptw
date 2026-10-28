use super::*;

fn assert_actual(observed: &BenchmarkModuleImage, actual: &wasmtime::Module) {
    use sha2::{Digest, Sha256};
    let image = actual.image_range();
    let text = actual.text().as_ptr_range();
    assert_eq!(observed.image_range(), image.start.addr()..image.end.addr());
    assert_eq!(observed.text_range(), text.start.addr()..text.end.addr());
    assert!(observed.image_range().start < observed.image_range().end);
    assert!(observed.text_range().start < observed.text_range().end);
    let serialized = actual.serialize().unwrap();
    let sections = observed.sections();
    assert_eq!(sections.serialized_bytes(), serialized.len());
    assert_eq!(
        sections.serialized_sha256(),
        format!("{:x}", Sha256::digest(&serialized))
    );
    assert_eq!(sections.elf_header_range(), 0..64);
    let text_section = sections
        .file_sections()
        .iter()
        .find(|section| section.name() == ".text")
        .unwrap();
    assert_eq!(
        text_section.offset(),
        text.start.addr() - image.start.addr()
    );
    assert_eq!(text_section.bytes(), actual.text().len());
    assert_eq!(text_section.section_type(), 1);
    assert_ne!(text_section.flags() & 4, 0);
    assert_eq!(
        &serialized[text_section.offset()..text_section.offset() + text_section.bytes()],
        actual.text()
    );
    assert!(sections
        .file_sections()
        .iter()
        .all(|section| !section.name().is_empty()
            && section.bytes() > 0
            && section.section_type() != 0
            && section.section_type() != 8));
}

#[test]
fn diagnostic_images_match_actual_program_runtime_and_survive_store_drop() {
    let artifact = artifact("print('owned-images');", ForeignModuleRegistry::default());
    let image = EmbeddedRuntime::precompile(&shared_wasm_engine().unwrap(), &artifact).unwrap();
    let mut config = wasmtime::Config::new();
    configure_precompiled_wasm_runtime(&mut config);
    let engine = wasmtime::Engine::new(&config).unwrap();
    assert!(engine.get_cranelift_opt_level().is_none());
    let runtime = EmbeddedRuntime::from_precompiled(&engine, &image).unwrap();
    let before = runtime.benchmark_module_images().unwrap();
    // The safe producer envelope owns its exact native P immediately before
    // the 32-byte checksum. Post-load serialization must retain those bytes.
    use sha2::{Digest, Sha256};
    let native_end = image.as_bytes().len() - 32;
    let native_start = native_end - image.serialized_program_len();
    assert_eq!(
        before.program().sections().serialized_sha256(),
        format!(
            "{:x}",
            Sha256::digest(&image.as_bytes()[native_start..native_end])
        )
    );
    assert_eq!(
        BenchmarkModuleImage::capture(&runtime.modules.program).unwrap(),
        *before.program()
    );
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
    assert_eq!(runtime.benchmark_module_images().unwrap(), before);
    assert_actual(before.program(), &runtime.modules.program);
    assert_actual(
        before.runtime().unwrap(),
        runtime.modules.runtime.as_ref().unwrap(),
    );
}
