//! Support for implementing the [`RuntimeLinearMemory`] trait in terms of a
//! platform mmap primitive.

use crate::prelude::*;
use crate::runtime::vm::memory::{MemoryPagePolicy, RuntimeLinearMemory};
use crate::runtime::vm::{HostAlignedByteCount, Mmap, mmap::AlignedLength};
use alloc::sync::Arc;
use wasmtime_environ::MemoryTunables;

use super::MemoryBase;

/// A linear memory instance.
#[derive(Debug)]
pub struct MmapMemory {
    // The underlying allocation.
    mmap: Arc<Mmap<AlignedLength>>,
    // Applied to the complete reservation, including future in-place growth.
    page_policy: MemoryPagePolicy,

    // The current length of this Wasm memory, in bytes.
    //
    // This region starts at `pre_guard_size` offset from the base of `mmap`. It
    // is always accessible, which means that if the Wasm page size is smaller
    // than the host page size, there may be some trailing region in the `mmap`
    // that is accessible but should not be accessed. (We rely on explicit
    // bounds checks in the compiled code to protect this region.)
    len: usize,

    // The optional maximum accessible size, in bytes, for this linear memory.
    //
    // Note that this maximum does not factor in guard pages, so this isn't the
    // maximum size of the linear address space reservation for this memory.
    //
    // This is *not* always a multiple of the host page size, and
    // `self.accessible()` may go past `self.maximum` when Wasm is using a small
    // custom page size due to `self.accessible()`'s rounding up to the host
    // page size.
    maximum: Option<usize>,

    // The amount of extra bytes to reserve whenever memory grows. This is
    // specified so that the cost of repeated growth is amortized.
    extra_to_reserve_on_growth: HostAlignedByteCount,

    // Size in bytes of extra guard pages before the start and after the end to
    // optimize loads and stores with constant offsets.
    pre_guard_size: HostAlignedByteCount,
    offset_guard_size: HostAlignedByteCount,
}

impl MmapMemory {
    /// Create a new linear memory instance with specified minimum and maximum
    /// number of wasm pages.
    pub fn new(
        ty: &wasmtime_environ::Memory,
        memory_tunables: &MemoryTunables<'_>,
        minimum: usize,
        maximum: Option<usize>,
    ) -> Result<Self> {
        // It's a programmer error for these two configuration values to exceed
        // the host available address space, so panic if such a configuration is
        // found (mostly an issue for hypothetical 32-bit hosts).
        //
        // Also be sure to round up to the host page size for this value.
        let offset_guard_bytes =
            HostAlignedByteCount::new_rounded_up_u64(memory_tunables.guard_size())
                .context("tunable.memory_guard_size overflows")?;
        let pre_guard_bytes = if memory_tunables.tunables().guard_before_linear_memory {
            offset_guard_bytes
        } else {
            HostAlignedByteCount::ZERO
        };

        // Calculate how much is going to be allocated for this linear memory in
        // addition to how much extra space we're reserving to grow into.
        //
        // If the minimum size of this linear memory fits within the initial
        // allocation (tunables.memory_reservation) then that's how many bytes
        // are going to be allocated. If the maximum size of linear memory
        // additionally fits within the entire allocation then there's no need
        // to reserve any extra for growth.
        //
        // If the minimum size doesn't fit within this linear memory.
        let mut alloc_bytes = memory_tunables.reservation();
        let mut extra_to_reserve_on_growth = memory_tunables.reservation_for_growth();
        let minimum_u64 = u64::try_from(minimum).unwrap();
        if minimum_u64 <= alloc_bytes {
            if let Ok(max) = ty.maximum_byte_size() {
                if max <= alloc_bytes {
                    extra_to_reserve_on_growth = 0;
                }
            }
        } else {
            alloc_bytes = minimum_u64.saturating_add(extra_to_reserve_on_growth);
        }

        // Convert `alloc_bytes` and `extra_to_reserve_on_growth` to
        // page-aligned `usize` values.
        let alloc_bytes = HostAlignedByteCount::new_rounded_up_u64(alloc_bytes)
            .context("tunables.memory_reservation overflows")?;
        let extra_to_reserve_on_growth =
            HostAlignedByteCount::new_rounded_up_u64(extra_to_reserve_on_growth)
                .context("tunables.memory_reservation_for_growth overflows")?;

        let request_bytes = pre_guard_bytes
            .checked_add(alloc_bytes)
            .and_then(|i| i.checked_add(offset_guard_bytes))
            .with_context(|| format!("cannot allocate {minimum} with guard regions"))?;

        let page_policy = MemoryPagePolicy::SystemDefault;
        let mmap = Self::reserve_mapping(request_bytes, page_policy)?;

        if minimum > 0 {
            let accessible = HostAlignedByteCount::new_rounded_up(minimum)?;
            // SAFETY: mmap is not in use right now so it's safe to make it accessible.
            unsafe {
                mmap.make_accessible(pre_guard_bytes, accessible)?;
            }
        }

        Ok(Self {
            mmap: try_new::<Arc<_>>(mmap)?,
            page_policy,
            len: minimum,
            maximum,
            pre_guard_size: pre_guard_bytes,
            offset_guard_size: offset_guard_bytes,
            extra_to_reserve_on_growth,
        })
    }

    /// Every replacement reservation receives its policy before any bytes are
    /// copied into it. Applying to inaccessible growth space is intentional.
    fn reserve_mapping(
        bytes: HostAlignedByteCount,
        policy: MemoryPagePolicy,
    ) -> Result<Mmap<AlignedLength>> {
        let mmap = Mmap::accessible_reserved(HostAlignedByteCount::ZERO, bytes)?;
        Self::apply_page_policy(&mmap, policy);
        Ok(mmap)
    }

    fn apply_page_policy(mmap: &Mmap<AlignedLength>, policy: MemoryPagePolicy) {
        match policy {
            MemoryPagePolicy::SystemDefault => {}
            MemoryPagePolicy::PreferBasePages => {
                // This is only an OS hint. Refusal cannot invalidate a valid
                // backing; keep the preference so later mappings retry it.
                if let Err(error) = mmap.prefer_base_pages() {
                    log::debug!("GC base-page preference could not be applied: {error:#}");
                }
            }
        }
    }

    /// Get the length of the accessible portion of the underlying `mmap`. This
    /// is the same region as `self.len` but rounded up to a multiple of the
    /// host page size.
    fn accessible(&self) -> HostAlignedByteCount {
        let accessible = HostAlignedByteCount::new_rounded_up(self.len)
            .expect("accessible region always fits in usize");
        debug_assert!(accessible <= self.current_capacity());
        accessible
    }

    /// Get the amount to which this memory can grow.
    fn current_capacity(&self) -> HostAlignedByteCount {
        let mmap_len = self.mmap.len_aligned();
        mmap_len
            .checked_sub(self.offset_guard_size)
            .and_then(|i| i.checked_sub(self.pre_guard_size))
            .expect("guard regions fit in mmap.len")
    }
}

impl RuntimeLinearMemory for MmapMemory {
    fn byte_size(&self) -> usize {
        self.len
    }

    fn byte_capacity(&self) -> usize {
        self.current_capacity().byte_count()
    }

    fn configure_page_policy(&mut self, policy: MemoryPagePolicy) -> Result<()> {
        ensure!(
            self.page_policy == MemoryPagePolicy::SystemDefault || self.page_policy == policy,
            "a configured mmap page policy cannot be reverted"
        );
        Self::apply_page_policy(&self.mmap, policy);
        self.page_policy = policy;
        Ok(())
    }

    fn grow_to(&mut self, new_size: usize) -> Result<()> {
        let new_accessible = HostAlignedByteCount::new_rounded_up(new_size)?;
        let current_capacity = self.current_capacity();
        if new_accessible > current_capacity {
            // If the new size of this heap exceeds the current size of the
            // allocation we have, then this must be a dynamic heap. Use
            // `new_size` to calculate a new size of an allocation, allocate it,
            // and then copy over the memory from before.
            let request_bytes = self
                .pre_guard_size
                .checked_add(new_accessible)
                .and_then(|s| s.checked_add(self.extra_to_reserve_on_growth))
                .and_then(|s| s.checked_add(self.offset_guard_size))
                .context("overflow calculating size of memory allocation")?;

            let mut new_mmap = Self::reserve_mapping(request_bytes, self.page_policy)?;
            // SAFETY: new_mmap is not in use right now so it's safe to make it
            // accessible.
            unsafe {
                new_mmap.make_accessible(self.pre_guard_size, new_accessible)?;
            }

            // This method has an exclusive reference to `self.mmap` and just
            // created `new_mmap` so it should be safe to acquire references
            // into both of them and copy between them.
            unsafe {
                let range =
                    self.pre_guard_size.byte_count()..(self.pre_guard_size.byte_count() + self.len);
                let src = self.mmap.slice(range.clone());
                let dst = new_mmap.slice_mut(range);
                dst.copy_from_slice(src);
            }

            self.mmap = try_new::<Arc<_>>(new_mmap)?;
        } else {
            // If the new size of this heap fits within the existing allocation
            // then all we need to do is to make the new pages accessible. This
            // can happen either for "static" heaps which always hit this case,
            // or "dynamic" heaps which have some space reserved after the
            // initial allocation to grow into before the heap is moved in
            // memory.
            assert!(new_size <= current_capacity.byte_count());
            assert!(self.maximum.map_or(true, |max| new_size <= max));

            // If the Wasm memory's page size is smaller than the host's page
            // size, then we might not need to actually change permissions,
            // since we are forced to round our accessible range up to the
            // host's page size.
            if let Ok(difference) = new_accessible.checked_sub(self.accessible()) {
                // SAFETY: the difference was previously inaccessible so we
                // never handed out any references to within it.
                unsafe {
                    self.mmap.make_accessible(
                        self.pre_guard_size
                            .checked_add(self.accessible())
                            .context("overflow calculating new accessible region")?,
                        difference,
                    )?;
                }
            }
        }

        self.len = new_size;

        Ok(())
    }

    unsafe fn discard_unused_pages(&mut self, range: core::ops::Range<usize>) -> Result<bool> {
        assert!(range.start <= range.end && range.end <= self.len);
        // Keep boundary pages intact even when semi-spaces are only aligned
        // to GC objects rather than host pages.
        let start = HostAlignedByteCount::new_rounded_up(range.start)?;
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let end = HostAlignedByteCount::new(range.end - range.end % page)?;
        if start >= end {
            return Ok(false);
        }
        let offset = self
            .pre_guard_size
            .checked_add(start)
            .context("discard range offset overflow")?;
        let length = end.checked_sub(start)?;
        // SAFETY: the range is inside the accessible anonymous allocation and
        // the caller has finished using its contents. Account for pre-guards.
        unsafe { self.mmap.discard_unused_pages(offset, length) }
    }

    fn set_byte_size(&mut self, len: usize) {
        self.len = len;
    }

    fn base(&self) -> MemoryBase {
        MemoryBase::Mmap(
            self.mmap
                .offset(self.pre_guard_size)
                .expect("pre_guard_size is in bounds"),
        )
    }

    fn vmmemory(&self) -> crate::vm::VMMemoryDefinition {
        let pre_guard_size = self.pre_guard_size.byte_count();
        assert!(pre_guard_size <= self.mmap.len());
        let pre_guard_size = isize::try_from(pre_guard_size).unwrap();
        let mmap = self.mmap.as_non_null();
        let base = unsafe {
            // Safety: `pre_guard_size` is within the mmap allocation.
            mmap.offset(pre_guard_size)
        };

        crate::vm::VMMemoryDefinition {
            base: base.into(),
            current_length: self.len.into(),
        }
    }
}

#[cfg(all(test, target_os = "linux", feature = "std", not(miri)))]
mod discard_tests {
    use super::*;
    use wasmtime_environ::{IndexType, Limits, MemoryKind, Tunables};

    fn memory(bytes: usize) -> MmapMemory {
        memory_with_reservation(bytes, 0)
    }

    fn memory_with_reservation(bytes: usize, reservation: usize) -> MmapMemory {
        let ty = wasmtime_environ::Memory {
            idx_type: IndexType::I32,
            limits: Limits { min: 0, max: None },
            shared: false,
            page_size_log2: 0,
        };
        let tunables = Tunables {
            memory_reservation: reservation as u64,
            memory_reservation_for_growth: 0,
            memory_guard_size: HostAlignedByteCount::host_page_size().byte_count() as u64,
            guard_before_linear_memory: true,
            memory_init_cow: false,
            ..Tunables::default_miri()
        };
        MmapMemory::new(
            &ty,
            &MemoryTunables::new(&tunables, MemoryKind::LinearMemory),
            bytes,
            None,
        )
        .unwrap()
    }

    /// Check the kernel's VMA preference, independently of THP availability,
    /// physical residency, RSS and the requested policy stored in Rust.
    fn has_no_huge_page_preference(address: *mut u8) -> bool {
        let address = address as usize;
        let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
        let mut contains_address = false;
        for line in smaps.lines() {
            if let Some((start, end)) = line
                .split_whitespace()
                .next()
                .and_then(|word| word.split_once('-'))
            {
                if let (Ok(start), Ok(end)) = (
                    usize::from_str_radix(start, 16),
                    usize::from_str_radix(end, 16),
                ) {
                    contains_address = start <= address && address < end;
                }
            }
            if contains_address {
                if let Some(flags) = line.strip_prefix("VmFlags:") {
                    return flags.split_whitespace().any(|flag| flag == "nh");
                }
            }
        }
        panic!("the owned mapping must have a kernel VMA");
    }

    #[test]
    fn gc_page_preference_survives_growth_and_relocation_without_changing_bytes() {
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let mut memory = memory_with_reservation(page, page * 4);
        memory
            .configure_page_policy(MemoryPagePolicy::for_kind(MemoryKind::GcHeap))
            .unwrap();
        assert_eq!(memory.page_policy, MemoryPagePolicy::PreferBasePages);
        let base = memory.base().as_mut_ptr();
        assert!(has_no_huge_page_preference(base));
        // No advice operation writes contents, and future reserved pages carry
        // the same preference before becoming accessible.
        assert!(
            unsafe { core::slice::from_raw_parts(base, page) }
                .iter()
                .all(|byte| *byte == 0)
        );
        unsafe {
            base.write_bytes(0x35, page);
        }
        memory.grow_to(page * 3).unwrap();
        assert_eq!(
            memory.base().as_mut_ptr(),
            base,
            "growth fits the reservation"
        );
        assert!(has_no_huge_page_preference(unsafe { base.add(page * 2) }));
        assert!(
            unsafe { core::slice::from_raw_parts(base, page) }
                .iter()
                .all(|byte| *byte == 0x35)
        );
        assert!(
            unsafe { core::slice::from_raw_parts(base.add(page), page * 2) }
                .iter()
                .all(|byte| *byte == 0)
        );
        unsafe {
            base.add(page).write_bytes(0x72, page * 2);
        }
        // Exceeding the old reservation must allocate a different mapping.
        // The policy is applied before grow_to copies the existing bytes.
        memory.grow_to(page * 7).unwrap();
        let relocated = memory.base().as_mut_ptr();
        assert_ne!(relocated, base);
        assert_eq!(memory.page_policy, MemoryPagePolicy::PreferBasePages);
        assert!(has_no_huge_page_preference(relocated));
        assert!(has_no_huge_page_preference(unsafe {
            relocated.add(page * 6)
        }));
        assert!(
            unsafe { core::slice::from_raw_parts(relocated, page) }
                .iter()
                .all(|byte| *byte == 0x35)
        );
        assert!(
            unsafe { core::slice::from_raw_parts(relocated.add(page), page * 2) }
                .iter()
                .all(|byte| *byte == 0x72)
        );
        assert!(
            unsafe { core::slice::from_raw_parts(relocated.add(page * 3), page * 4) }
                .iter()
                .all(|byte| *byte == 0)
        );
        unsafe {
            relocated.add(page * 6).write(19);
        }
        assert_eq!(unsafe { relocated.add(page * 6).read() }, 19);
    }

    #[test]
    fn base_page_advice_preserves_existing_contents_and_rw_access() {
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let memory = memory(page * 2);
        let base = memory.base().as_mut_ptr();
        unsafe {
            base.write_bytes(0xa3, page * 2);
        }
        // Advice is separate from dead-page discard: even already written
        // bytes retain their values and remain accessible.
        memory.mmap.prefer_base_pages().unwrap();
        assert!(has_no_huge_page_preference(base));
        assert!(
            unsafe { core::slice::from_raw_parts(base, page * 2) }
                .iter()
                .all(|byte| *byte == 0xa3)
        );
        unsafe {
            base.add(page).write(41);
        }
        assert_eq!(unsafe { base.add(page).read() }, 41);
    }

    #[test]
    fn ordinary_linear_memory_keeps_system_page_preference() {
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let mut memory = memory(page);
        memory
            .configure_page_policy(MemoryPagePolicy::for_kind(MemoryKind::LinearMemory))
            .unwrap();
        assert_eq!(memory.page_policy, MemoryPagePolicy::SystemDefault);
        assert!(!has_no_huge_page_preference(memory.base().as_mut_ptr()));
        memory.grow_to(page * 3).unwrap();
        assert_eq!(memory.page_policy, MemoryPagePolicy::SystemDefault);
        assert!(!has_no_huge_page_preference(memory.base().as_mut_ptr()));
    }

    #[test]
    fn reclaim_rounds_inward_preserves_boundaries_and_is_rw_zero_on_reuse() {
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let mut memory = memory(page * 4);
        let base = memory.base().as_mut_ptr();
        for _ in 0..4 {
            // No live typed data or Rust references inhabit this test memory.
            unsafe {
                base.write_bytes(0xa5, page * 4);
            }
            assert!(unsafe { memory.discard_unused_pages(1..page * 3 + 1) }.unwrap());
            // This observes actual residency, not RSS or an allocator counter.
            // Check before reading discarded bytes, which would refault them.
            let mut resident = [0u8; 4];
            assert_eq!(
                unsafe { libc::mincore(base.cast(), page * 4, resident.as_mut_ptr()) },
                0
            );
            assert_eq!(resident[1] & 1, 0);
            assert_eq!(resident[2] & 1, 0);
            let bytes = unsafe { core::slice::from_raw_parts(base, page * 4) };
            assert!(bytes[..page].iter().all(|b| *b == 0xa5));
            assert!(bytes[page..page * 3].iter().all(|b| *b == 0));
            assert!(bytes[page * 3..].iter().all(|b| *b == 0xa5));
            assert_eq!(memory.byte_size(), page * 4);
        }
        // Reclamation does not need recommit before writes or later growth.
        memory.grow_to(page * 7).unwrap();
        let base = memory.base().as_mut_ptr();
        unsafe {
            base.write_bytes(0x5a, page * 7);
        }
        assert!(unsafe { memory.discard_unused_pages(0..page * 7) }.unwrap());
        assert!(
            unsafe { core::slice::from_raw_parts(base, page * 7) }
                .iter()
                .all(|b| *b == 0)
        );
        unsafe {
            base.add(page * 6).write(42);
        }
        assert_eq!(unsafe { base.add(page * 6).read() }, 42);
    }

    #[test]
    fn partial_empty_ranges_do_not_discard_and_custom_backing_requires_opt_in() {
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let mut memory = memory(page);
        let base = memory.base().as_mut_ptr();
        unsafe {
            base.write_bytes(0x7b, page);
        }
        for range in [0..0, 0..page - 1, 1..page, page..page] {
            assert!(!unsafe { memory.discard_unused_pages(range) }.unwrap());
        }
        // Even a custom implementation returning an mmap base must not be
        // classified by its pointer. Only its own explicit capability counts.
        struct Custom(MmapMemory);
        impl RuntimeLinearMemory for Custom {
            fn byte_size(&self) -> usize {
                self.0.byte_size()
            }
            fn byte_capacity(&self) -> usize {
                self.0.byte_capacity()
            }
            fn grow_to(&mut self, size: usize) -> Result<()> {
                self.0.grow_to(size)
            }
            fn base(&self) -> MemoryBase {
                self.0.base()
            }
            fn vmmemory(&self) -> crate::vm::VMMemoryDefinition {
                self.0.vmmemory()
            }
        }
        let mut custom = Custom(memory);
        custom
            .configure_page_policy(MemoryPagePolicy::for_kind(MemoryKind::GcHeap))
            .unwrap();
        assert_eq!(custom.0.page_policy, MemoryPagePolicy::SystemDefault);
        assert!(!has_no_huge_page_preference(base));
        assert!(!unsafe { custom.discard_unused_pages(0..page) }.unwrap());
        assert!(
            unsafe { core::slice::from_raw_parts(base, page) }
                .iter()
                .all(|b| *b == 0x7b)
        );
    }

    #[test]
    #[should_panic]
    fn reclaim_rejects_out_of_bounds_before_alignment() {
        let page = HostAlignedByteCount::host_page_size().byte_count();
        let mut memory = memory(page);
        let _ = unsafe { memory.discard_unused_pages(0..page + 1) };
    }
}
