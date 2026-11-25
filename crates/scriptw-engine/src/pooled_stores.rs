//! Pooled, kept-resident GC heaps for hosts that create a fresh Store per entry.
//!
//! Every Store owns a GC heap of 4 GiB reserved address space. On-demand allocation maps a fresh reservation per Store, so
//! each first touch of a heap page faults. A pooled Engine takes GC heaps from
//! Wasmtime's memory pool instead: a returned heap's dirty pages are zeroed
//! and stay resident for the next Store, and its mapping is reused.
use crate::wasmtime_config::{WASM_GC_HEAP_INITIAL_BYTES, WASM_STORE_MEMORY_CAP_BYTES};
use crate::EngineError;
use std::num::NonZeroU32;

/// The most Stores one pooled Engine serves at once. Each costs about
/// three memory slots of 4.06 GiB reserved (never committed) address space,
/// so 1,024 Stores reserve about 12.5 TiB of the 128 TiB user space.
pub const MAX_POOLED_STORES: u32 = 1024;

/// A pool keeps at most the GC heap's initial size resident per warm slot.
pub const MAX_POOLED_KEEP_RESIDENT_BYTES: usize = WASM_GC_HEAP_INITIAL_BYTES;

/// Kept-resident sizes are whole Wasm pages, a multiple of every host page.
const KEEP_RESIDENT_GRANULE_BYTES: usize = 64 * 1024;

/// Memory slots one Store may use: its GC heap, and one memory each that R or
/// P could define. Scriptw's R defines its private memory, so a Store takes
/// two slots; P imports R's and the third is reserved address space only (a
/// profile-collecting P defines its profile memory there). The shared memory
/// is host-created, which Wasmtime always allocates on demand. R's slot keeps
/// R's copy-on-write data image mapped: Wasmtime's reset writes the image's
/// bytes back into the kept-resident pages a Store dirtied and lets the rest
/// fall back to the image, so the next Store starts from R's pristine data. A
/// non-GC module that defines its memory (the benchmark WASI adapter's
/// program) takes one slot for it, and the same keep-resident reset applies.
const MEMORY_SLOTS_PER_STORE: u32 = 3;
/// Core instances one Store may use: R and P.
const CORE_INSTANCES_PER_STORE: u32 = 2;
/// Wasmtime's per-instance metadata (`VMContext`) limit. The pool allocates
/// nothing up front for it, so this only rejects absurd programs; the default
/// 1 MiB could reject a large P with tens of thousands of functions or globals.
const MAX_CORE_INSTANCE_BYTES: usize = 64 * 1024 * 1024;

/// A validated Store pool policy for an Engine that creates a fresh Store per
/// entry. [`PooledStores::engine`] creates such an Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PooledStores {
    max_concurrent: NonZeroU32,
    keep_resident_bytes: usize,
}

impl PooledStores {
    /// At most `max_concurrent` Stores of the Engine may be alive at once
    /// (`1..=MAX_POOLED_STORES`); creating another fails with
    /// [`EngineError::is_store_pool_limit_reached`], never by falling back to
    /// on-demand allocation. Up to `keep_resident_bytes` of each returned GC
    /// heap's dirty pages stay resident for the next Store: a multiple of
    /// 64 KiB, at most [`MAX_POOLED_KEEP_RESIDENT_BYTES`]. Zero keeps nothing
    /// resident and still reuses mappings.
    pub fn new(
        max_concurrent: NonZeroU32,
        keep_resident_bytes: usize,
    ) -> Result<Self, EngineError> {
        if max_concurrent.get() > MAX_POOLED_STORES {
            return Err(EngineError::new(format!(
                "a Store pool serves at most {MAX_POOLED_STORES} concurrent Stores, not {max_concurrent}"
            )));
        }
        if keep_resident_bytes > MAX_POOLED_KEEP_RESIDENT_BYTES
            || keep_resident_bytes % KEEP_RESIDENT_GRANULE_BYTES != 0
        {
            return Err(EngineError::new(format!(
                "a Store pool keeps a multiple of 64 KiB up to {MAX_POOLED_KEEP_RESIDENT_BYTES} bytes resident, not {keep_resident_bytes}"
            )));
        }
        Ok(Self {
            max_concurrent,
            keep_resident_bytes,
        })
    }

    /// One Store at a time, keeping the GC heap's whole initial size resident:
    /// a host that runs one fresh Store after another on one thread.
    pub fn sequential() -> Self {
        Self {
            max_concurrent: NonZeroU32::MIN,
            keep_resident_bytes: MAX_POOLED_KEEP_RESIDENT_BYTES,
        }
    }

    pub const fn max_concurrent(&self) -> NonZeroU32 {
        self.max_concurrent
    }

    /// The requested bytes; [`PooledStoreReset`] says what the host applies.
    pub const fn keep_resident_bytes(&self) -> usize {
        self.keep_resident_bytes
    }

    /// How a pooled Engine on this host resets a GC heap its Store returned.
    pub fn reset(&self) -> PooledStoreReset {
        self.reset_with_pagemap(wasmtime::PoolingAllocationConfig::is_pagemap_scan_available())
    }

    fn reset_with_pagemap(&self, pagemap_scan_available: bool) -> PooledStoreReset {
        if pagemap_scan_available {
            PooledStoreReset::KeepDirtyPagesResident {
                bytes: self.keep_resident_bytes,
            }
        } else {
            PooledStoreReset::ReleaseAll
        }
    }

    /// An Engine for `config` that takes GC heaps from a pool of
    /// `max_concurrent()` slots, and how this host resets them.
    ///
    /// `config` is complete: set up by
    /// [`configure_wasm_runtime`](crate::configure_wasm_runtime) or
    /// [`configure_precompiled_wasm_runtime`](crate::configure_precompiled_wasm_runtime),
    /// which replace the whole configuration, and any settings applied after
    /// them. The pool is applied to a copy as the Engine is created, so no
    /// later configuration call can replace it and leave a reset policy that
    /// describes an on-demand Engine.
    ///
    /// The pool needs the linear-memory and GC-heap reservations to match, as
    /// every Scriptw configuration sets them. Store limits still cap each heap
    /// at 1 GiB, and the heaps never move, so execution deadlines are
    /// unchanged. Each warm slot keeps up to `keep_resident_bytes` resident
    /// after its Store is dropped, so at most `max_concurrent ×
    /// keep_resident_bytes` stay resident; the pool's address space is
    /// reserved while the Engine lives.
    pub fn engine(
        &self,
        config: &wasmtime::Config,
    ) -> Result<(wasmtime::Engine, PooledStoreReset), EngineError> {
        let mut config = config.clone();
        let reset = configure_with_pagemap(
            &mut config,
            self,
            wasmtime::PoolingAllocationConfig::is_pagemap_scan_available(),
        );
        let engine = wasmtime::Engine::new(&config)
            .map_err(|error| EngineError::new(format!("pooled Wasmtime Engine: {error:#}")))?;
        Ok((engine, reset))
    }
}

/// How a pooled Engine resets a GC heap its Store returned, chosen once for
/// this host ([`PooledStores::reset`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PooledStoreReset {
    /// Linux's `PAGEMAP_SCAN` (6.7 and later) finds the dirty pages. Up to
    /// `bytes` of them are zeroed in place and stay resident; the rest are
    /// released.
    KeepDirtyPagesResident { bytes: usize },
    /// The host lacks `PAGEMAP_SCAN`. Without it, keeping memory resident
    /// zeroes the whole kept size at every Store drop, about 1.5–3 ms for
    /// 16 MiB, which costs small entries more than their faults. Every page
    /// is released instead, as without a pool; mappings are still reused.
    ReleaseAll,
}

pub(crate) fn configure_with_pagemap(
    config: &mut wasmtime::Config,
    pool: &PooledStores,
    pagemap_scan_available: bool,
) -> PooledStoreReset {
    let stores = pool.max_concurrent.get();
    let reset = pool.reset_with_pagemap(pagemap_scan_available);
    let mut pooling = wasmtime::PoolingAllocationConfig::new();
    pooling
        .total_gc_heaps(stores)
        .total_memories(stores * MEMORY_SLOTS_PER_STORE)
        .max_memories_per_module(1)
        .total_core_instances(stores * CORE_INSTANCES_PER_STORE)
        .max_core_instance_size(MAX_CORE_INSTANCE_BYTES)
        .total_tables(stores * CORE_INSTANCES_PER_STORE)
        .max_tables_per_module(1)
        .total_stacks(stores)
        .max_memory_size(WASM_STORE_MEMORY_CAP_BYTES)
        // Only the slots this many Stores leave behind stay warm (Wasmtime's
        // default of 100 could keep 1.6 GiB resident).
        .max_unused_warm_slots(stores);
    match reset {
        PooledStoreReset::KeepDirtyPagesResident { bytes } => {
            pooling
                .linear_memory_keep_resident(bytes)
                .pagemap_scan(wasmtime::Enabled::Yes);
        }
        PooledStoreReset::ReleaseAll => {
            pooling
                .linear_memory_keep_resident(0)
                .pagemap_scan(wasmtime::Enabled::No);
        }
    }
    config.allocation_strategy(wasmtime::InstanceAllocationStrategy::Pooling(pooling));
    reset
}

/// Report a Store pool limit as a typed error.
///
/// Wasmtime reports every pool total the same way, and a total is reached for
/// one of two reasons that this error cannot tell apart: the pool's other live
/// Stores hold their slots (dropping one frees them), or this Store needs
/// more core instances, memories (its GC heap included) or tables than the
/// per-Store share configured above while the others hold theirs. A Store
/// that alone needs more than the whole pool fails even with no other Store
/// alive, and no drop helps it.
pub(crate) fn classify_allocation_error(error: wasmtime::Error, context: &str) -> EngineError {
    if error.is::<wasmtime::PoolConcurrencyLimitError>() {
        EngineError::from_store_pool_limit(format!(
            "{context}: a Store pool limit is reached: {error:#}. Either the pool's other \
             live Stores hold its slots (dropping one frees them), or this Store needs more \
             than its share (1 GC heap, {CORE_INSTANCES_PER_STORE} core instances, \
             {MEMORY_SLOTS_PER_STORE} memories with its GC heap, {CORE_INSTANCES_PER_STORE} \
             tables, 1 stack) while the others use theirs; no drop helps a Store that needs \
             more than the whole pool"
        ))
    } else {
        EngineError::new(format!("{context}: {error:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_rejects_unbounded_pools_and_unaligned_or_oversized_residency() {
        let one = NonZeroU32::MIN;
        assert!(PooledStores::new(NonZeroU32::new(MAX_POOLED_STORES).unwrap(), 0).is_ok());
        assert!(PooledStores::new(NonZeroU32::new(MAX_POOLED_STORES + 1).unwrap(), 0).is_err());
        assert!(PooledStores::new(one, MAX_POOLED_KEEP_RESIDENT_BYTES).is_ok());
        assert!(PooledStores::new(one, MAX_POOLED_KEEP_RESIDENT_BYTES + 65536).is_err());
        assert!(PooledStores::new(one, 4096).is_err());
        assert_eq!(
            PooledStores::sequential(),
            PooledStores::new(one, 16 * 1024 * 1024).unwrap()
        );
    }

    #[test]
    fn a_host_without_pagemap_scan_releases_every_page() {
        for (available, expected) in [
            (
                true,
                PooledStoreReset::KeepDirtyPagesResident {
                    bytes: MAX_POOLED_KEEP_RESIDENT_BYTES,
                },
            ),
            (false, PooledStoreReset::ReleaseAll),
        ] {
            let mut config = wasmtime::Config::new();
            crate::configure_wasm_runtime(
                &mut config,
                crate::NativeCompilationPolicy::Fast,
                crate::CompilationJobs::current(),
            );
            let pool = PooledStores::sequential();
            let reset = configure_with_pagemap(&mut config, &pool, available);
            assert_eq!(reset, expected);
            assert_eq!(pool.reset_with_pagemap(available), expected);
            if available && !wasmtime::PoolingAllocationConfig::is_pagemap_scan_available() {
                continue;
            }
            // The pool accepts the product's equal heap and memory
            // reservations; a mismatch fails here, at Engine creation.
            wasmtime::Engine::new(&config).unwrap();
        }
    }

    #[test]
    fn a_pooled_engine_keeps_its_pool_whatever_configuration_came_before() {
        let mut config = wasmtime::Config::new();
        crate::configure_wasm_runtime(
            &mut config,
            crate::NativeCompilationPolicy::Fast,
            crate::CompilationJobs::current(),
        );
        let pool = PooledStores::sequential();
        let (engine, reset) = pool.engine(&config).unwrap();
        assert_eq!(reset, pool.reset());
        // The Engine was created with the pool: there is no configuration
        // step after which it could be lost.
        assert!(engine.pooling_allocator_metrics().is_some());
        // The caller's Config is untouched and still makes on-demand Engines.
        let on_demand = wasmtime::Engine::new(&config).unwrap();
        assert!(on_demand.pooling_allocator_metrics().is_none());
    }

    /// The reset a host without `PAGEMAP_SCAN` uses, run on any host: a
    /// reused slot reads zeros after every page was released.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn the_release_all_reset_hands_the_next_store_a_zeroed_slot() {
        let mut config = wasmtime::Config::new();
        crate::configure_wasm_runtime(
            &mut config,
            crate::NativeCompilationPolicy::Fast,
            crate::CompilationJobs::current(),
        );
        let reset = configure_with_pagemap(&mut config, &PooledStores::sequential(), false);
        assert_eq!(reset, PooledStoreReset::ReleaseAll);
        let engine = wasmtime::Engine::new(&config).unwrap();
        // (module (memory (export "memory") 2))
        let module = wasmtime::Module::new(
            &engine,
            b"\0asm\x01\0\0\0\x05\x03\x01\x00\x02\x07\x0a\x01\x06memory\x02\x00",
        )
        .unwrap();
        let mut base = None;
        for round in 0..3u8 {
            let mut store = wasmtime::Store::new(&engine, ());
            let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
            let memory = instance.get_memory(&mut store, "memory").unwrap();
            let address = memory.data_ptr(&store) as usize;
            assert_eq!(*base.get_or_insert(address), address, "the slot is reused");
            assert!(
                memory.data(&store).iter().all(|byte| *byte == 0),
                "round {round} reads an earlier Store's bytes"
            );
            memory.data_mut(&mut store).fill(round + 1);
        }
    }
}
