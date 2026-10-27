use super::*;
pub(crate) fn instantiate(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    template: &NativeProgramTemplate,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<Instance> {
    context.check_budget()?;
    if !WasmtimeEngine::same(engine, context.linker.engine())
        || !Arc::ptr_eq(template.runtime(), context.runtime())
    {
        return Err(wasmtime::Error::msg(
            "native template has a different actual Engine or R owner",
        ));
    }
    let key = wasm_module_memory_cache_key(
        WasmModuleSource::Program(template.bytes()),
        context.native_compilation_mode,
    );
    let reusable = template.permits_instance_reuse();
    if reusable {
        if let Some(instance) = store
            .as_context_mut()
            .data_mut()
            .native_template_instances
            .get(&key, template.bytes(), engine, &context.instance_authority)
        {
            context.check_budget()?;
            return Ok(instance);
        }
    }
    // Use the existing bounded trace only. The default path does not acquire
    // the cache or read its counters. These are process-wide deltas: concurrent
    // compilations may contribute, so they do not claim exclusive attribution.
    let function_cache = trace.and_then(|_| cranelift_function_cache());
    let function_before = function_cache.as_ref().map(|cache| cache.counters());
    let lookup = trace.map(|trace| trace.phase(Phase::CompiledModuleLookup));
    let (module, hit) = cache::module(engine, context.native_compilation_mode, template.bytes())
        .map_err(wasmtime::Error::new)?;
    context.check_budget()?;
    drop(lookup);
    if let Some(trace) = trace {
        let delta = function_cache
            .as_ref()
            .zip(function_before)
            .map(|(cache, before)| {
                let after = cache.counters();
                (
                    after.0.saturating_sub(before.0),
                    after.1.saturating_sub(before.1),
                )
            });
        trace.compiled_module_cache(hit, delta);
    }
    let bind = trace.map(|trace| trace.phase(Phase::BindImports));
    let mut imports = Vec::new();
    for import in module.imports() {
        let value = if import.module() == NATIVE_PROGRAM_DATA_MODULE {
            bindings::data_import(&mut *store, context, template, &import)?
        } else {
            context
                .resolve_import(&mut *store, import.module(), import.name())
                .map_err(|error| {
                    wasmtime::Error::msg(format!(
                        "native template lacks actual import {}.{}: {error}",
                        import.module(),
                        import.name()
                    ))
                })?
        };
        imports.push(value);
    }
    context.check_budget()?;
    drop(bind);
    let instantiate = trace.map(|trace| trace.phase(Phase::Instantiate));
    let instance = Instance::new(&mut *store, &module, &imports)?;
    context.check_budget()?;
    drop(instantiate);
    if reusable {
        store
            .as_context_mut()
            .data_mut()
            .native_template_instances
            .retain(
                key,
                template.bytes(),
                module,
                instance,
                Arc::clone(&context.instance_authority),
            );
    }
    if std::env::var_os("SCRIPTW_WASM_TRACE").is_some() {
        eprintln!(
            "scriptw native template: cache={} code_bytes={} imports={} images={} factories={}",
            if hit { "hit" } else { "miss" },
            template.bytes().len(),
            imports.len(),
            template.data_images().len(),
            template.array_factories().len()
        );
    }
    Ok(instance)
}
