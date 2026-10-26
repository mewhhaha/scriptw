//! Bounded reuse of idle compiler scratch, independent of compiled Modules.
use super::CompilerContext;
use cranelift_codegen::ir;

// Retain useful serial/small parallel reuse without keeping one arena per past
// worker. These are work-size admission limits, not an allocator-byte census.
const MAX_IDLE_CONTEXTS: usize = 2;
const MAX_INSTRUCTIONS: usize = 4096;
const MAX_VALUES: usize = 8192;
const MAX_BLOCKS: usize = 1024;
const MAX_LOCALS: u32 = 2048;
const MAX_NATIVE_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub(super) struct ScratchExtent {
    instructions: usize,
    values: usize,
    blocks: usize,
    locals: u32,
    native_bytes: usize,
}

impl ScratchExtent {
    pub(super) fn observe_ir(&mut self, func: &ir::Function) {
        self.instructions = self.instructions.max(func.dfg.num_insts());
        self.values = self.values.max(func.dfg.num_values());
        self.blocks = self.blocks.max(func.dfg.num_blocks());
    }

    pub(super) fn observe_locals(&mut self, locals: u32) {
        self.locals = self.locals.max(locals);
    }

    fn reusable(&self) -> bool {
        self.instructions <= MAX_INSTRUCTIONS
            && self.values <= MAX_VALUES
            && self.blocks <= MAX_BLOCKS
            && self.locals <= MAX_LOCALS
            && self.native_bytes <= MAX_NATIVE_BYTES
    }
}

#[derive(Default)]
pub(super) struct ContextPool {
    idle: Vec<CompilerContext>,
    retired_hits: usize,
    retired_cached: usize,
}

impl ContextPool {
    pub(super) fn pop(&mut self) -> Option<CompilerContext> {
        self.idle.pop()
    }

    pub(super) fn recycle(&mut self, mut context: CompilerContext, native_bytes: usize) {
        context
            .scratch_extent
            .observe_ir(&context.codegen_context.func);
        context.scratch_extent.native_bytes = context.scratch_extent.native_bytes.max(native_bytes);
        if self.idle.len() < MAX_IDLE_CONTEXTS && context.scratch_extent.reusable() {
            // Native output already has its own owner. Clear final CLIF now,
            // rather than keeping it until a later compilation checks this out.
            context.codegen_context.clear();
            context.scratch_extent = ScratchExtent::default();
            self.idle.push(context);
        } else if let Some(cache) = &context.incremental_cache_ctx {
            self.retired_hits += cache.num_hits;
            self.retired_cached += cache.num_cached;
        }
        // Rejected contexts drop here, releasing all opaque translator,
        // validator and Cranelift capacities together. Active jobs are absent.
    }

    pub(super) fn finish_module(&mut self) {
        let mut retained = 0;
        self.idle.retain_mut(|context| {
            context
                .scratch_extent
                .observe_ir(&context.codegen_context.func);
            if retained < MAX_IDLE_CONTEXTS && context.scratch_extent.reusable() {
                context.codegen_context.clear();
                context.scratch_extent = ScratchExtent::default();
                retained += 1;
                true
            } else {
                if let Some(cache) = &context.incremental_cache_ctx {
                    self.retired_hits += cache.num_hits;
                    self.retired_cached += cache.num_cached;
                }
                false
            }
        });
    }

    pub(super) fn cache_counts(&self) -> (usize, usize) {
        self.idle
            .iter()
            .filter_map(|context| context.incremental_cache_ctx.as_ref())
            .fold(
                (self.retired_hits, self.retired_cached),
                |(hits, cached), context| (hits + context.num_hits, cached + context.num_cached),
            )
    }
}

#[cfg(test)]
mod tests;
