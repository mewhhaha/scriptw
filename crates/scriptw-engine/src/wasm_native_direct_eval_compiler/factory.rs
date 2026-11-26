use super::*;
use crate::wasm_native_source_compiler::CallbackTrace;
use scriptw_ir::{PreparedScriptKind, PreparedScriptOutcome, RuntimeDirectEvalProgram};
use wasmtime::{AsContextMut, FuncType, HeapType, StructType, ValType};
pub(super) fn instantiate_and_execute(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    request: &ConvertedRequest,
    program: &RuntimeDirectEvalProgram,
    signature: &NativeDirectEvalSignature,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<[Val; 6]> {
    require_exact_program(&request.caller, &request.source, program)?;
    context.check_budget()?;
    if let Some(proof) = program.empty_completion() {
        return super::empty::execute(store, engine, context, &proof, signature, trace);
    }
    if let Some(proof) = program.primitive_literal() {
        return super::primitive::execute(store, engine, context, &proof, signature, trace);
    }
    // Inert preparation only adds dead declarative links above this actual
    // chain. A null caller chain retains the ordinary prepared-script route.
    if matches!(request.operands[3], Val::AnyRef(Some(_))) {
        if let Some(proof) = program.static_regexp_literal() {
            if let Some(output) =
                super::regexp_literal::execute(store, context, &proof, request, signature)?
            {
                return Ok(output);
            }
        }
    }
    let emit = trace.map(|trace| trace.phase(Phase::Emit));
    let runtime = context.validated_runtime()?;
    let artifact =
        scriptw_aot_wasm::emit_runtime_direct_eval_with_validated_runtime(program, runtime);
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
            "new direct eval P is not linked to the exact admitted R",
        ));
    }
    if artifact.caller() != &request.caller || artifact.source() != request.source.as_slice() {
        return Err(abi_error(
            "P2 artifact differs from the complete caller/source witness",
        ));
    }
    context.check_budget()?;
    drop(checks);
    let canonicalize = trace.map(|trace| trace.phase(Phase::TemplateCanonicalize));
    let template = scriptw_aot_wasm::NativeProgramTemplate::for_direct_eval(&artifact)
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
        .ok_or_else(|| abi_error("P2 has no declared direct eval factory"))?;
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
    let mut arguments = Vec::with_capacity(signature.arguments.len() + 1);
    arguments.push(owner[0]);
    arguments.extend(request.operands);
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
pub(super) fn require_exact_program(
    caller: &scriptw_ir::RuntimeDirectEvalCaller,
    source: &[u16],
    program: &RuntimeDirectEvalProgram,
) -> wasmtime::Result<()> {
    let kind = PreparedScriptKind::DirectEval(caller.context().clone());
    let root = program
        .program()
        .script
        .as_ref()
        .and_then(|s| s.prepared_scripts.first())
        .ok_or_else(|| abi_error("typed Script witness has no root source row"))?;
    if program.caller() != caller
        || root.kind != kind
        || root.source != scriptw_front::encode_js_string_utf16(source)
        || !matches!(&root.outcome, PreparedScriptOutcome::Executable(unit)
            if unit.id == program.script_id() && unit.kind == kind)
    {
        return Err(abi_error(
            "native request and compiled source/kind/root witness differ",
        ));
    }
    Ok(())
}
fn program_reference(value: &ValType, signature: &NativeDirectEvalSignature) -> bool {
    matches!(value, ValType::Ref(r) if !r.is_nullable()
        && matches!(r.heap_type(), HeapType::ConcreteStruct(t) if StructType::eq(t, &signature.program)))
}
pub(super) fn validate_initializer_type(
    ty: &FuncType,
    signature: &NativeDirectEvalSignature,
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
    signature: &NativeDirectEvalSignature,
) -> wasmtime::Result<()> {
    let arguments = ty.params().collect::<Vec<_>>();
    let results = ty.results().collect::<Vec<_>>();
    if arguments.len() != signature.arguments.len() + 1
        || !program_reference(&arguments[0], signature)
        || !arguments[1..]
            .iter()
            .zip(&signature.arguments)
            .all(|(a, e)| ValType::eq(a, e))
        || results.len() != 6
        || !results[..5]
            .iter()
            .zip(&signature.completion)
            .all(|(a, e)| ValType::eq(a, e))
        || !matches!(results[5], ValType::I32)
    {
        return Err(abi_error(
            "direct-eval factory type differs from actual P/caller records/Completion/status types",
        ));
    }
    Ok(())
}
