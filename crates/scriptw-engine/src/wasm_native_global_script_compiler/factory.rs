use super::*;
use crate::wasm_native_source_compiler::CallbackTrace;
use scriptw_ir::{
    PreparedScriptKind, PreparedScriptOutcome, RuntimeGlobalScriptKind, RuntimeGlobalScriptProgram,
};
use wasmtime::{AsContextMut, FuncType, HeapType, StructType, ValType};

pub(super) fn instantiate_and_execute(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    request: &ConvertedRequest,
    program: &RuntimeGlobalScriptProgram,
    signature: &NativeGlobalScriptSignature,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<[Val; 6]> {
    require_exact_request(request, program)?;
    let emit = trace.map(|trace| trace.phase(Phase::Emit));
    let runtime = context.validated_runtime()?;
    let artifact =
        scriptw_aot_wasm::emit_runtime_global_script_with_validated_runtime(program, runtime);
    context.check_budget()?;
    let artifact = artifact.map_err(|e| {
        wasmtime::Error::new(EngineError::from_wasm_emit_error(program.program(), e))
    })?;
    drop(emit);
    let checks = trace.map(|trace| trace.phase(Phase::ArtifactChecks));
    // The artifact producer retains this exact immutable runtime owner. Identity
    // proves the same key, layout and bytes without rescanning R on every call.
    if !Arc::ptr_eq(artifact.runtime(), runtime.runtime()) {
        return Err(abi_error(
            "new global Script P is not linked to the exact admitted R",
        ));
    }
    for payload in WasmParser::new(0).parse_all(artifact.bytes()) {
        if matches!(
            payload.map_err(abi_error)?,
            WasmPayload::StartSection { .. }
        ) {
            return Err(abi_error(
                "native global Script P may not execute a module start function",
            ));
        }
    }
    context.check_budget()?;
    drop(checks);
    let canonicalize = trace.map(|trace| trace.phase(Phase::TemplateCanonicalize));
    let template = scriptw_aot_wasm::NativeProgramTemplate::for_global_script(&artifact)
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
        .ok_or_else(|| abi_error("P2 has no declared initializer"))?;
    let factory = instance
        .get_func(&mut *store, artifact.factory_export())
        .ok_or_else(|| abi_error("P2 has no declared global Script factory"))?;
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
        return Err(abi_error("P2 initializer returned no owner"));
    };
    let reference = reference
        .as_struct(&*store)?
        .ok_or_else(|| abi_error("P2 owner is not a core struct"))?;
    if !StructType::eq(&reference.ty(&*store)?, &signature.program) {
        return Err(abi_error(
            "P2 owner differs from the actual calling core schema",
        ));
    }
    let [kind, source, realm, _calling_program] = request.operands;
    let arguments = [owner[0], kind, source, realm];
    let mut output = [Val::I32(0); 6];
    factory.call(&mut *store, &arguments, &mut output)?;
    context.check_budget()?;
    if !matches!(output[5], Val::I32(2)) {
        return Err(abi_error(
            "owned P2 factory did not enter its exact witnessed Script root",
        ));
    }
    Ok(output)
}

fn require_exact_request(
    request: &ConvertedRequest,
    program: &RuntimeGlobalScriptProgram,
) -> wasmtime::Result<()> {
    let kind = match request.kind {
        RuntimeGlobalScriptKind::IndirectEval => PreparedScriptKind::IndirectEval,
        RuntimeGlobalScriptKind::RealmScript => PreparedScriptKind::RealmScript,
    };
    let root = program
        .program()
        .script
        .as_ref()
        .and_then(|s| s.prepared_scripts.first())
        .ok_or_else(|| abi_error("typed Script witness has no root source row"))?;
    if program.kind() != request.kind
        || root.kind != kind
        || root.source != scriptw_front::encode_js_string_utf16(&request.source)
        || !matches!(&root.outcome, PreparedScriptOutcome::Executable(unit)
            if unit.id == program.script_id() && unit.kind == kind)
    {
        return Err(abi_error(
            "native request and compiled source/kind/root witness differ",
        ));
    }
    Ok(())
}

fn program_reference(value: &ValType, signature: &NativeGlobalScriptSignature) -> bool {
    matches!(value, ValType::Ref(r) if !r.is_nullable()
        && matches!(r.heap_type(), HeapType::ConcreteStruct(t) if StructType::eq(t, &signature.program)))
}
pub(super) fn validate_initializer_type(
    ty: &FuncType,
    signature: &NativeGlobalScriptSignature,
) -> wasmtime::Result<()> {
    let arguments = ty.params().collect::<Vec<_>>();
    let results = ty.results().collect::<Vec<_>>();
    if !arguments.is_empty()
        || !matches!(results.as_slice(), [owner] if program_reference(owner, signature))
    {
        return Err(abi_error("initializer type is not [] -> actual non-null P"));
    }
    Ok(())
}
pub(super) fn validate_factory_type(
    ty: &FuncType,
    signature: &NativeGlobalScriptSignature,
) -> wasmtime::Result<()> {
    let arguments = ty.params().collect::<Vec<_>>();
    let results = ty.results().collect::<Vec<_>>();
    if arguments.len() != 4
        || !program_reference(&arguments[0], signature)
        || !arguments[1..]
            .iter()
            .zip(&signature.arguments[..3])
            .all(|(a, e)| ValType::eq(a, e))
        || results.len() != 6
        || !results[..5]
            .iter()
            .zip(&signature.completion)
            .all(|(a, e)| ValType::eq(a, e))
        || !matches!(results[5], ValType::I32)
    {
        return Err(abi_error(
            "global factory type differs from actual P/kind/String/Realm/Completion/status types",
        ));
    }
    Ok(())
}
