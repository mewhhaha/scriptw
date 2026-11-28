use super::*;
use crate::wasm_native_source_compiler::CallbackTrace;
use scriptw_aot_wasm::NativeProgramTemplate;
use scriptw_ir::RuntimeFunctionProgram;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use wasmtime::{AsContextMut, FuncType, HeapType, StructType, ValType};

const MAX_CACHED_NATIVE_FUNCTIONS: usize = 64;
const MAX_CACHED_TEMPLATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SINGLE_CACHED_TEMPLATE_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct CachedNativeFunction {
    digest: [u8; 32],
    kind: DynamicFunctionKind,
    arguments: Vec<Vec<u16>>,
    policy: WasmCompilationPolicy,
    pub(super) template: NativeProgramTemplate,
    pub(super) program_initialize_export: String,
    pub(super) factory_export: String,
}

#[derive(Default)]
struct NativeFunctionTemplateCache {
    entries: VecDeque<Arc<CachedNativeFunction>>,
    total_bytes: usize,
}

fn template_cache() -> &'static Mutex<NativeFunctionTemplateCache> {
    static CACHE: OnceLock<Mutex<NativeFunctionTemplateCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(NativeFunctionTemplateCache::default()))
}

fn request_digest(kind: DynamicFunctionKind, arguments: &[Vec<u16>]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let kind_byte = match kind {
        DynamicFunctionKind::Ordinary => 0u8,
        DynamicFunctionKind::Generator => 1u8,
        DynamicFunctionKind::Async => 2u8,
        DynamicFunctionKind::AsyncGenerator => 3u8,
    };
    hasher.update([kind_byte]);
    hasher.update((arguments.len() as u64).to_le_bytes());
    for arg in arguments {
        hasher.update((arg.len() as u64).to_le_bytes());
        for &unit in arg {
            hasher.update(unit.to_le_bytes());
        }
    }
    hasher.finalize().into()
}

pub(super) fn lookup_cached_template(
    context: &NativeSourceCompilerContext,
    request: &ConvertedRequest,
) -> wasmtime::Result<Option<Arc<CachedNativeFunction>>> {
    let runtime = context.validated_runtime()?;
    let digest = request_digest(request.kind, &request.arguments);
    let cache = template_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for entry in &cache.entries {
        if entry.digest == digest
            && entry.kind == request.kind
            && entry.policy == context.policy
            && Arc::ptr_eq(entry.template.runtime(), runtime.runtime())
            && entry.arguments == request.arguments
        {
            return Ok(Some(Arc::clone(entry)));
        }
    }
    Ok(None)
}

fn retain_cached_template(entry: Arc<CachedNativeFunction>) {
    let bytes = entry.template.bytes().len();
    if bytes > MAX_SINGLE_CACHED_TEMPLATE_BYTES {
        return;
    }
    let mut cache = template_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache.entries.iter().any(|existing| {
        existing.digest == entry.digest
            && existing.kind == entry.kind
            && existing.policy == entry.policy
            && Arc::ptr_eq(existing.template.runtime(), entry.template.runtime())
            && existing.arguments == entry.arguments
    }) {
        return;
    }
    while cache.entries.len() >= MAX_CACHED_NATIVE_FUNCTIONS
        || cache.total_bytes.saturating_add(bytes) > MAX_CACHED_TEMPLATE_BYTES
    {
        let Some(evicted) = cache.entries.pop_front() else {
            break;
        };
        cache.total_bytes = cache
            .total_bytes
            .saturating_sub(evicted.template.bytes().len());
    }
    cache.total_bytes = cache.total_bytes.saturating_add(bytes);
    cache.entries.push_back(entry);
}

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
    let template = NativeProgramTemplate::for_function(&artifact)
        .map_err(|e| wasmtime::Error::new(EngineError::new(e.to_string())))?;
    context.check_budget()?;
    drop(canonicalize);
    let program_initialize_export = artifact.program_initialize_export().to_string();
    let factory_export = artifact.factory_export().to_string();
    let entry = Arc::new(CachedNativeFunction {
        digest: request_digest(request.kind, &request.arguments),
        kind: request.kind,
        arguments: request.arguments.clone(),
        policy: context.policy.clone(),
        template,
        program_initialize_export,
        factory_export,
    });
    retain_cached_template(Arc::clone(&entry));
    instantiate_and_create_from_template(
        store,
        engine,
        context,
        request,
        &entry.template,
        &entry.program_initialize_export,
        &entry.factory_export,
        signature,
        trace,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn instantiate_and_create_from_template(
    store: &mut impl AsContextMut<Data = WasmHostState>,
    engine: &WasmtimeEngine,
    context: &NativeSourceCompilerContext,
    request: &ConvertedRequest,
    template: &NativeProgramTemplate,
    program_initialize_export: &str,
    factory_export: &str,
    signature: &NativeFunctionSignature,
    trace: Option<&CallbackTrace<'_>>,
) -> wasmtime::Result<[Val; 5]> {
    let instance = crate::wasm_native_program_template::instantiate(
        &mut *store,
        engine,
        context,
        template,
        trace,
    )?;
    context.check_budget()?;
    let _invoke = trace.map(|trace| trace.phase(Phase::FactoryInvoke));
    let initialize = instance
        .get_func(&mut *store, program_initialize_export)
        .ok_or_else(|| abi_error("P has no declared program initializer export"))?;
    let factory = instance
        .get_func(&mut *store, factory_export)
        .ok_or_else(|| abi_error("P has no declared function factory export"))?;
    let initialization = crate::wasm_native_program_template::initializer_arguments(
        &mut *store,
        template,
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
