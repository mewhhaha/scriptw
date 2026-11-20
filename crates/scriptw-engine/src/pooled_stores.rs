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
/// P could define. Scriptw's own R and P define none (they import host-created
/// memories, which Wasmtime always allocates on demand), so in practice a
/// Store takes one slot; the rest are reserved address space only. A
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
/// entry. Apply it with [`configure_pooled_stores`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PooledStores {
    max_concurrent: NonZeroU32,
    keep_resident_bytes: usize,
}

impl PooledStores {
    /// At most `max_concurrent` Stores of the Engine may be alive at once
    /// (`1..=MAX_POOLED_STORES`); creating another fails with
    /// [`EngineError::is_store_pool_exhausted`], never by falling back to
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
}

/// How a pooled Engine resets a GC heap its Store returned, chosen once by
/// [`configure_pooled_stores`] for this host.
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

/// Make `config` (already set up by
/// [`configure_wasm_runtime`](crate::configure_wasm_runtime) or
/// [`configure_precompiled_wasm_runtime`](crate::configure_precompiled_wasm_runtime),
/// which replace the whole configuration) take GC heaps from a pool of
/// `pool.max_concurrent()` slots. Returns how this host resets them.
///
/// The pool needs the linear-memory and GC-heap reservations to match, as
/// every Scriptw configuration sets them. Store limits still cap each heap at
/// 1 GiB, and the heaps never move, so execution deadlines are unchanged. Each
/// warm slot keeps up to `keep_resident_bytes` resident after its Store is
/// dropped, so at most `max_concurrent × keep_resident_bytes` stay resident;
/// the pool's address space is reserved while the Engine lives.
pub fn configure_pooled_stores(
    config: &mut wasmtime::Config,
    pool: &PooledStores,
) -> PooledStoreReset {
    configure_with_pagemap(
        config,
        pool,
        wasmtime::PoolingAllocationConfig::is_pagemap_scan_available(),
    )
}

pub(crate) fn configure_with_pagemap(
    config: &mut wasmtime::Config,
    pool: &PooledStores,
    pagemap_scan_available: bool,
) -> PooledStoreReset {
    let stores = pool.max_concurrent.get();
    let reset = if pagemap_scan_available {
        PooledStoreReset::KeepDirtyPagesResident {
            bytes: pool.keep_resident_bytes,
        }
    } else {
        PooledStoreReset::ReleaseAll
    };
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

/// Report a Store pool's capacity limit as a typed error.
pub(crate) fn classify_allocation_error(error: wasmtime::Error, context: &str) -> EngineError {
    if error.is::<wasmtime::PoolConcurrencyLimitError>() {
        EngineError::from_store_pool_exhaustion(format!(
            "{context}: the Store pool's capacity is reached: {error:#}"
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
            let reset = configure_with_pagemap(&mut config, &PooledStores::sequential(), available);
            assert_eq!(reset, expected);
            if available && !wasmtime::PoolingAllocationConfig::is_pagemap_scan_available() {
                continue;
            }
            // The pool accepts the product's equal heap and memory
            // reservations; a mismatch fails here, at Engine creation.
            wasmtime::Engine::new(&config).unwrap();
        }
    }
}
