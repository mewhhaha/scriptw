use super::*;
use crate::wasm_native_source_compiler::CallbackTrace;
use scriptw_ir::RuntimeFunctionProgram;
use wasmtime::{AsContextMut, FuncType, HeapType, StructType, ValType};

pub(super) fn instantiate_and_create(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    request: &ConvertedRequest,
    program: &RuntimeFunctionProgram,
    signature: &NativeFunctionSignature,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<[Val; 5]> {
    let emit = trace.map(|trace| trace.phase(Phase::Emit));
    let runtime = context.validated_runtime()?;
    let artifact = scriptw_aot_wasm::emit_runtime_function_with_validated_runtime(program, runtime);
    context.check_budget()?;
    let artifact = artifact.map_err(|error| {
        wasmtime::Error::new(EngineError::from_wasm_emit_error(program.program(), error))
    })?;
    drop(emit);
    let checks = trace.map(|trace| trace.phase(Phase::ArtifactChecks));
    // The artifact producer retains this exact immutable runtime owner. Identity
    // proves the same key, layout and bytes without rescanning R on every call.
    if !Arc::ptr_eq(artifact.runtime(), runtime.runtime()) {
        return Err(abi_error(
            "fresh P was emitted against a different actual runtime",
        ));
    }
    context.check_budget()?;
    // Uses the existing bounded native compilation pool and the same Engine.
    // Only this P reaches Module::new; the admitted R is never compiled here.
    drop(checks);
    let canonicalize = trace.map(|trace| trace.phase(Phase::TemplateCanonicalize));
    let template = scriptw_aot_wasm::NativeProgramTemplate::for_function(&artifact)
        .map_err(|e| wasmtime::Error::new(EngineError::new(e.to_string())))?;
    context.check_budget()?;
    drop(canonicalize);
    let instance = crate::wasm_native_program_template::instantiate(
        &mut *store,
        engine,
        context,
        &template,
        trace,
    )?;
    context.check_budget()?;
    let _invoke = trace.map(|trace| trace.phase(Phase::FactoryInvoke));
    let initialize = instance
        .get_func(&mut *store, artifact.program_initialize_export())
        .ok_or_else(|| abi_error("P has no declared program initializer export"))?;
    let factory = instance
        .get_func(&mut *store, artifact.factory_export())
        .ok_or_else(|| abi_error("P has no declared function factory export"))?;
    let initialization = crate::wasm_native_program_template::initializer_arguments(
        &mut *store,
        &template,
        &initialize,
        &signature.program,
    )?;
    if initialization.is_empty() {
        validate_initializer_type(&initialize.ty(&*store), signature)?;
    }
    validate_factory_type(&factory.ty(&*store), signature)?;
    let mut owner = [Val::AnyRef(None)];
    initialize.call(&mut *store, &initialization, &mut owner)?;
    context.check_budget()?;
    let Val::AnyRef(Some(reference)) = owner[0] else {
        return Err(abi_error("program initializer returned no program owner"));
    };
    let reference = reference
        .as_struct(&*store)?
        .ok_or_else(|| abi_error("program owner is not a struct"))?;
    if !StructType::eq(&reference.ty(&*store)?, &signature.program) {
        return Err(abi_error(
            "program initializer returned an incompatible GC schema",
        ));
    }
    let [kind, tag, scalar, target, sources, constructor, environment] = request.operands;
    let arguments = [
        owner[0],
        kind,
        tag,
        scalar,
        target,
        sources,
        constructor,
        environment,
    ];
    let mut output = [Val::I32(0); 6];
    factory.call(&mut *store, &arguments, &mut output)?;
    context.check_budget()?;
    let [tag, scalar, reference, kind, target, Val::I32(1)] = output else {
        return Err(abi_error(
            "compiled request factory did not match its actual source row",
        ));
    };
    Ok([tag, scalar, reference, kind, target])
}

fn program_reference(value: &ValType, signature: &NativeFunctionSignature) -> bool {
    matches!(value, ValType::Ref(reference) if !reference.is_nullable()
        && matches!(reference.heap_type(), HeapType::ConcreteStruct(ty) if StructType::eq(ty, &signature.program)))
}

pub(super) fn validate_initializer_type(
    ty: &FuncType,
    signature: &NativeFunctionSignature,
) -> wasmtime::Result<()> {
    let parameters: Vec<_> = ty.params().collect();
    let results: Vec<_> = ty.results().collect();
    if !parameters.is_empty()
        || !matches!(results.as_slice(), [owner] if program_reference(owner, signature))
    {
        return Err(abi_error(
            "program initializer has an incompatible typed signature",
        ));
    }
    Ok(())
}

pub(super) fn validate_factory_type(
    ty: &FuncType,
    signature: &NativeFunctionSignature,
) -> wasmtime::Result<()> {
    let parameters: Vec<_> = ty.params().collect();
    let results: Vec<_> = ty.results().collect();
    if parameters.len() != signature.arguments.len() + 1
        || !program_reference(&parameters[0], signature)
        || !parameters[1..]
            .iter()
            .zip(&signature.arguments)
            .all(|(actual, expected)| ValType::eq(actual, expected))
        || results.len() != signature.completion.len() + 1
        || !results[..signature.completion.len()]
            .iter()
            .zip(&signature.completion)
            .all(|(actual, expected)| ValType::eq(actual, expected))
        || !matches!(results.last(), Some(ValType::I32))
    {
        return Err(abi_error(
            "native function factory has an incompatible typed signature",
        ));
    }
    Ok(())
}
