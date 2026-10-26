use super::super::{Compiler, FunctionCompiler, IncrementalCacheContext};
use super::*;
use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, UserFuncName, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use std::borrow::Cow;
use std::sync::Arc;
use wasmtime_environ::{CacheStore, ObjectKind, Tunables};

fn fill_ir(context: &mut CompilerContext, instructions: usize) -> *const ir::InstructionData {
    let function = &mut context.codegen_context.func;
    let block = function.dfg.make_block();
    function.layout.append_block(block);
    let mut cursor = FuncCursor::new(function);
    cursor.goto_bottom(block);
    for value in 0..instructions {
        cursor.ins().iconst(types::I64, value as i64);
    }
    let first = function.layout.first_inst(block).unwrap();
    &function.dfg.insts[first]
}

fn assert_empty(context: &CompilerContext) {
    assert_eq!(context.codegen_context.func.dfg.num_insts(), 0);
    assert_eq!(context.codegen_context.func.dfg.num_values(), 0);
    assert_eq!(context.codegen_context.func.dfg.num_blocks(), 0);
    assert!(context.codegen_context.compiled_code().is_none());
}

fn compiler() -> Compiler {
    let mut flags = settings::builder();
    flags.set("enable_verifier", "true").unwrap();
    flags.set("opt_level", "none").unwrap();
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();
    Compiler::new(
        Tunables::default_host(),
        isa,
        None,
        false,
        crate::builder::LinkOptions::default(),
        None,
        false,
    )
}

fn constant_function(compiler: &Compiler, valid: bool) -> FunctionCompiler<'_> {
    let mut function = compiler.function_compiler();
    let mut signature = Signature::new(CallConv::triple_default(compiler.isa.triple()));
    signature.returns.push(AbiParam::new(types::I64));
    let func = &mut function.cx.codegen_context.func;
    *func = ir::Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let block = func.dfg.make_block();
    func.layout.append_block(block);
    let mut cursor = FuncCursor::new(func);
    cursor.goto_bottom(block);
    let value = cursor.ins().iconst(types::I64, 42);
    if valid {
        cursor.ins().return_(&[value]);
    } else {
        // Verifier failure after checkout must drop this owner, not publish
        // partially compiled IR into the reusable pool.
        cursor.ins().return_(&[]);
    }
    function
}

#[test]
fn small_context_is_cleared_and_reuses_its_actual_instruction_allocation() {
    let mut pool = ContextPool::default();
    let mut context = CompilerContext::default();
    let allocation = fill_ir(&mut context, 16);
    pool.recycle(context, 32);
    assert_eq!(pool.idle.len(), 1);
    let mut context = pool.pop().unwrap();
    assert_empty(&context);
    assert_eq!(fill_ir(&mut context, 16), allocation);
    pool.recycle(context, 32);
    pool.finish_module();
    assert_empty(&pool.pop().unwrap());
}

#[test]
fn oversized_ir_locals_and_native_extents_retire_the_entire_owner() {
    let mut contexts = Vec::new();
    let mut instructions = CompilerContext::default();
    fill_ir(&mut instructions, MAX_INSTRUCTIONS + 1);
    contexts.push((instructions, 0));
    let mut values = CompilerContext::default();
    let block = values.codegen_context.func.dfg.make_block();
    for _ in 0..=MAX_VALUES {
        values
            .codegen_context
            .func
            .dfg
            .append_block_param(block, types::I64);
    }
    contexts.push((values, 0));
    let mut blocks = CompilerContext::default();
    for _ in 0..=MAX_BLOCKS {
        blocks.codegen_context.func.dfg.make_block();
    }
    contexts.push((blocks, 0));
    let mut locals = CompilerContext::default();
    locals.scratch_extent.observe_locals(MAX_LOCALS + 1);
    contexts.push((locals, 0));
    contexts.push((CompilerContext::default(), MAX_NATIVE_BYTES + 1));
    let mut pool = ContextPool::default();
    for (context, native_bytes) in contexts {
        pool.recycle(context, native_bytes);
        assert!(pool.pop().is_none());
    }
}

#[test]
fn preoptimization_peak_cannot_be_hidden_by_a_later_small_function() {
    let mut context = CompilerContext::default();
    fill_ir(&mut context, MAX_INSTRUCTIONS + 1);
    context
        .scratch_extent
        .observe_ir(&context.codegen_context.func);
    context.codegen_context.clear();
    fill_ir(&mut context, 1);
    let mut pool = ContextPool::default();
    pool.recycle(context, 1);
    assert!(pool.idle.is_empty());
}

#[test]
fn module_boundary_cleans_idle_contexts_and_never_touches_an_active_job() {
    let compiler = compiler();
    let mut active = compiler.function_compiler();
    fill_ir(&mut active.cx, 7);
    {
        let mut pool = compiler.contexts.lock().unwrap();
        for _ in 0..MAX_IDLE_CONTEXTS + 1 {
            let mut context = CompilerContext::default();
            fill_ir(&mut context, 3);
            pool.idle.push(context);
        }
        let mut oversized = CompilerContext::default();
        fill_ir(&mut oversized, MAX_INSTRUCTIONS + 1);
        pool.idle.insert(0, oversized);
    }
    let mut object = wasmtime_environ::Compiler::object(&compiler, ObjectKind::Module).unwrap();
    wasmtime_environ::Compiler::append_code(&compiler, &mut object, &[], &|_, _| unreachable!())
        .unwrap();
    let pool = compiler.contexts.lock().unwrap();
    assert_eq!(pool.idle.len(), MAX_IDLE_CONTEXTS);
    for context in &pool.idle {
        assert_empty(context);
    }
    assert_eq!(active.cx.codegen_context.func.dfg.num_insts(), 7);
}

#[test]
fn returned_jobs_obey_the_pool_bound_and_keep_compiled_output_independent() {
    let compiler = compiler();
    let jobs: Vec<_> = (0..MAX_IDLE_CONTEXTS + 2)
        .map(|_| constant_function(&compiler, true))
        .collect();
    let mut expected = None;
    for job in jobs {
        let compiled = job.finish("scratch_reuse").unwrap();
        assert!(!compiled.buffer.data().is_empty());
        let bytes = compiled.buffer.data().to_vec();
        if let Some(expected) = &expected {
            assert_eq!(&bytes, expected);
        } else {
            expected = Some(bytes);
        }
    }
    assert_eq!(
        compiler.contexts.lock().unwrap().idle.len(),
        MAX_IDLE_CONTEXTS
    );
    let compiled = constant_function(&compiler, true)
        .finish("scratch_reuse")
        .unwrap();
    assert_eq!(compiled.buffer.data(), expected.unwrap());
    assert_empty(compiler.contexts.lock().unwrap().idle.last().unwrap());
}

#[test]
fn failed_compilation_drops_its_owner_and_does_not_poison_small_reuse() {
    let compiler = compiler();
    let first = constant_function(&compiler, true)
        .finish("scratch_error")
        .unwrap();
    assert_eq!(compiler.contexts.lock().unwrap().idle.len(), 1);
    assert!(
        constant_function(&compiler, false)
            .finish("scratch_error")
            .is_err()
    );
    assert!(compiler.contexts.lock().unwrap().idle.is_empty());
    let next = constant_function(&compiler, true)
        .finish("scratch_error")
        .unwrap();
    assert_eq!(first.buffer.data(), next.buffer.data());
    assert_eq!(compiler.contexts.lock().unwrap().idle.len(), 1);
}

#[derive(Debug)]
struct EmptyCache;
impl CacheStore for EmptyCache {
    fn get(&self, _: &[u8]) -> Option<Cow<'_, [u8]>> {
        None
    }
    fn insert(&self, _: &[u8], _: Vec<u8>) -> bool {
        false
    }
}

#[test]
fn retiring_contexts_preserves_incremental_cache_diagnostics() {
    let cached = |hits, cached| CompilerContext {
        incremental_cache_ctx: Some(IncrementalCacheContext {
            cache_store: Arc::new(EmptyCache),
            num_hits: hits,
            num_cached: cached,
        }),
        ..Default::default()
    };
    let mut pool = ContextPool::default();
    pool.recycle(cached(7, 3), MAX_NATIVE_BYTES + 1);
    pool.recycle(cached(5, 2), 0);
    pool.recycle(cached(11, 13), 0);
    pool.recycle(cached(17, 19), 0);
    assert_eq!(pool.idle.len(), MAX_IDLE_CONTEXTS);
    assert_eq!(pool.cache_counts(), (40, 37));
    pool.finish_module();
    assert_eq!(pool.cache_counts(), (40, 37));
    let context = pool.pop().unwrap();
    pool.recycle(context, 0);
    assert_eq!(pool.cache_counts(), (40, 37));
}
