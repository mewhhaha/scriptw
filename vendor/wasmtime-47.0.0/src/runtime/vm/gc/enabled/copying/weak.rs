//! Conditional marking and weak updates belong to the copying collection.
use super::*;
use super::marking::{ConditionalEdge, reset_buffer};
use crate::vm::WeakGcEdge;
use crate::vm::gc::weak::WEAK_ARRAY_ELEMENTS;

const PAIR_BYTES: u32 = 2 * mem::size_of::<u32>() as u32;

// InlineTraceInfo only uses array bits 1 through 3, and bit 0 is the
// forwarding flag. `WEAK_ARRAY_ELEMENTS` (bit 4) is interpreted only in the
// array scan arm and set once on registration.

impl VMCopyingHeader {
    pub(super) fn has_weak_array_elements(&self) -> bool {
        self.header.reserved_u26() & WEAK_ARRAY_ELEMENTS != 0
    }
}

/// The original array and the pair offset, which is valid in either copy.
/// The source array length is overwritten by forwarding.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct WeakArrayEntry {
    pub(super) array: u32,
    offset: u32,
}

#[derive(Clone, Copy)]
struct WeakArray {
    raw: u32,
    end: u32,
}

impl WeakArray {
    fn start(self) -> u32 {
        self.raw + GC_REF_ARRAY_ELEMS_OFFSET
    }

    fn slot(self, field: u32) -> WeakArrayEntry {
        let element_offset = field - self.start();
        let key_offset = element_offset - element_offset % PAIR_BYTES;
        WeakArrayEntry {
            array: self.raw,
            offset: GC_REF_ARRAY_ELEMS_OFFSET + key_offset,
        }
    }
}

/// Collection-local identities and pairs. The registration is weak, and the
/// private array acts as the owner gate for its pairs.
#[derive(Default)]
pub(super) struct WeakArrays {
    ranges: Vec<WeakArray>,
    pub(super) entries: Vec<WeakArrayEntry>,
    kept: usize,
}

impl WeakArrays {
    pub(super) fn reset(&mut self) {
        reset_buffer(&mut self.ranges);
        reset_buffer(&mut self.entries);
        self.kept = 0;
    }

    fn original(&self, raw: u32) -> Option<WeakArray> {
        let index = self.ranges.binary_search_by_key(&raw, |array| array.raw).ok()?;
        Some(self.ranges[index])
    }

    fn remembered_entry(&self, slot: u32) -> Option<WeakArrayEntry> {
        let next = self.ranges.partition_point(|array| array.raw <= slot);
        let array = self.ranges[next.checked_sub(1)?];
        (array.start() <= slot && slot < array.end).then(|| array.slot(slot))
    }

    pub(super) fn should_skip_remembered(&self, slot: u32) -> bool {
        self.remembered_entry(slot).is_some()
    }

    fn add_slot(&mut self, slot: WeakArrayEntry, evacuation: &Evacuation<'_>) -> Result<()> {
        let [key, value] = evacuation.read_weak_pair(slot.array, slot.offset)?;
        let heap = &evacuation.heap;
        let young_ref = |raw| raw != 0 && raw & 1 == 0 && heap.is_young(raw);
        if [key, value] != [0, 0]
            && (!heap.collecting_minor || key == 0 || young_ref(key) || young_ref(value))
        {
            self.entries.try_reserve(1)?;
            self.entries.push(slot);
        }
        Ok(())
    }

    fn add_array(&mut self, array: WeakArray, evacuation: &Evacuation<'_>) -> Result<()> {
        for field in (array.start()..array.end).step_by(PAIR_BYTES as usize) {
            self.add_slot(array.slot(field), evacuation)?;
        }
        Ok(())
    }

    fn prepare(&mut self, arrays: &[VMGcRef], evacuation: &Evacuation<'_>) -> Result<()> {
        if arrays.is_empty() {
            return Ok(());
        }
        self.ranges.try_reserve_exact(arrays.len())?;
        for array in arrays {
            self.ranges
                .push(evacuation.weak_array(array.as_raw_u32())?);
        }
        self.ranges.sort_unstable_by_key(|array| array.raw);
        debug_assert!(self.ranges.windows(2).all(|pair| pair[0].end <= pair[1].raw));

        let heap = &evacuation.heap;
        // SAFETY: the mutator is stopped and preparation only reads the data.
        let inner = unsafe { &*heap.vmctx_data.inner.get() };
        let scan_old = !heap.collecting_minor || inner.remembered_overflow != 0;
        for index in 0..self.ranges.len() {
            let array = self.ranges[index];
            if scan_old || heap.is_young(array.raw) {
                self.add_array(array, evacuation)?;
            }
        }
        if !scan_old {
            // Directly allocated old arrays can have initializers that
            // bypass the write barrier. After the next collection a barrier
            // records every new young reference into an old array.
            for &raw in &heap.direct_old {
                if let Some(array) = self.original(raw) {
                    self.add_array(array, evacuation)?;
                }
            }
            let count = usize::try_from(inner.remembered_len)?;
            let Some(remembered) = heap.remembered.get(..count) else {
                bail_bug!("remembered slot count exceeds the buffer");
            };
            for &slot in remembered {
                if let Some(entry) = self.remembered_entry(slot) {
                    self.add_slot(entry, evacuation)?;
                }
            }
            // Either half can be remembered many times. Relocate each pair
            // once so weak update always reads its two original indices.
            self.entries.sort_unstable();
            self.entries.dedup();
        }
        Ok(())
    }
}

impl Evacuation<'_> {
    fn weak_array(&self, raw: u32) -> Result<WeakArray> {
        let header = self.object::<VMCopyingArrayHeader>(usize::try_from(raw)?)?;
        if header.header.header.kind() != VMGcKind::ArrayRef
            || !matches!(
                header.header.inline_trace_info(),
                InlineTraceInfo::Array { elems_are_gc_refs: true }
            )
            || header.length % 2 != 0
        {
            bail_bug!("invalid weak reference array");
        }
        let extent = header
            .length
            .checked_mul(4)
            .and_then(|bytes| GC_REF_ARRAY_ELEMS_OFFSET.checked_add(bytes))
            .filter(|&extent| extent <= header.header.object_size());
        let Some(end) = extent.and_then(|extent| raw.checked_add(extent)) else {
            bail_bug!("weak reference array exceeds its object bounds");
        };
        if usize::try_from(end)? > self.len {
            bail_bug!("weak reference array exceeds the heap bounds");
        }
        Ok(WeakArray { raw, end })
    }

    fn weak_pair_address(&self, array: u32, offset: u32) -> Result<*mut [u32; 2]> {
        let field = usize::try_from(array)?.checked_add(usize::try_from(offset)?);
        let Some(field) = field.filter(|field| {
            field
                .checked_add(PAIR_BYTES as usize)
                .is_some_and(|end| end <= self.len)
        }) else {
            bail_bug!("weak reference pair exceeds the heap bounds");
        };
        // SAFETY: the pair range fits in the heap. No safepoint occurs
        // between this address calculation and accessing the pair.
        Ok(unsafe { self.base.add(field).cast() })
    }

    fn read_weak_pair(&self, array: u32, offset: u32) -> Result<[u32; 2]> {
        let pair = self.weak_pair_address(array, offset)?;
        // SAFETY: this range was validated by weak_pair_address.
        Ok(unsafe { pair.read_unaligned() }.map(u32::from_le))
    }

    fn update_weak_pair(&mut self, slot: WeakArrayEntry) -> Result<()> {
        let Some(array) = self.forwarded(slot.array)? else {
            return Ok(());
        };
        let [key, value] = self.read_weak_pair(array, slot.offset)?;
        let pair = match self.forwarded(key)? {
            Some(key) if key != 0 => {
                let Some(value) = self.forwarded(value)? else {
                    bail_bug!("conditional value was not marked for a live weak key");
                };
                [key, value]
            }
            // A null key with a nonzero hash leaves a tombstone in Wasm.
            _ => [0, 0],
        };
        let address = self.weak_pair_address(array, slot.offset)?;
        // SAFETY: this range was validated by weak_pair_address.
        unsafe { address.write_unaligned(pair.map(u32::to_le)) };
        Ok(())
    }
}

impl CopyingCollection<'_, '_> {
    pub(super) fn prepare_weak_marking(&mut self) -> Result<()> {
        let weak = &mut *self.trace_state.weak_gc;
        let mut evacuation = Evacuation::new(self.heap);
        self.marking.arrays.prepare(&weak.table_arrays, &evacuation)?;
        let minor = evacuation.heap.collecting_minor;
        let live = |raw| raw == 0 || raw & 1 != 0 || (minor && raw >= evacuation.nursery_end);
        let read_pair = |slot: WeakArrayEntry| evacuation.read_weak_pair(slot.array, slot.offset);
        if minor {
            self.marking.prepare(
                weak,
                weak.touched.iter().map(|&slot| slot as usize),
                live,
                read_pair,
            )?;
        } else {
            self.marking.prepare(weak, 0..weak.slots.len(), live, read_pair)?;
        }
        weak.advance_epoch()?;
        for array in &self.marking.arrays.ranges {
            // Ranges were checked above. The marker is set on registration and
            // copied with the header; skip writes when already set so minor GC
            // does not dirty clean old-space pages.
            let header =
                evacuation.object_mut::<VMCopyingHeader>(usize::try_from(array.raw)?)?;
            if !header.has_weak_array_elements() {
                let reserved = header.header.reserved_u26();
                header.header.set_reserved_u26(reserved | WEAK_ARRAY_ELEMENTS);
            }
        }
        Ok(())
    }

    pub(super) fn process_ephemerons_increment(&mut self) -> Result<GcProgress> {
        let mut evacuation = Evacuation::new(self.heap);
        for _ in 0..OBJECTS_PER_INCREMENT {
            let Some(edge) = self.marking.pop() else {
                break;
            };
            let index = match edge {
                ConditionalEdge::Registry(index) => index as usize,
                ConditionalEdge::Array(index) => {
                    let slot = self.marking.arrays.entries[index as usize];
                    let Some(array) = evacuation.forwarded(slot.array)? else {
                        bail_bug!("ready ephemeron has no live array");
                    };
                    let [key, value] = evacuation.read_weak_pair(array, slot.offset)?;
                    debug_assert!(key != 0 && evacuation.forwarded(key)?.is_some());
                    if evacuation.forwarded(value)?.is_none() {
                        evacuation.forward(value, &mut self.marking)?;
                    }
                    // Weak update still needs both originals in the pair.
                    continue;
                }
            };
            let Some(WeakGcEdge::Ephemeron {
                owner,
                key: Some(key),
                value: Some(value),
            }) = self.trace_state.weak_gc.slots[index].edge.as_ref()
            else {
                unreachable!("conditional edges are stable until weak update");
            };
            debug_assert!(evacuation.forwarded(owner.as_raw_u32())?.is_some());
            debug_assert!(evacuation.forwarded(key.as_raw_u32())?.is_some());
            let value = value.as_raw_u32();
            let forwarded = match evacuation.forwarded(value)? {
                Some(forwarded) => forwarded,
                None => evacuation.forward(value, &mut self.marking)?,
            };
            let Some(WeakGcEdge::Ephemeron { value, .. }) =
                self.trace_state.weak_gc.slots[index].edge.as_mut()
            else {
                unreachable!();
            };
            *value = VMGcRef::from_raw_u32(forwarded);
        }
        drop(evacuation);
        if self.marking.is_empty() {
            self.weak_cursor = 0;
            self.phase = if self.heap.worklist_ptr < self.heap.bump_ptr() {
                // Stored value cells expose their strong children through the
                // normal worklist; newly copied owners/keys queue only their
                // dependent edges instead of rescanning the whole registry.
                CopyingCollectionPhase::ProcessWorklist
            } else {
                CopyingCollectionPhase::UpdateWeak
            };
        }
        Ok(GcProgress::Continue)
    }

    pub(super) fn update_weak_increment(&mut self) -> Result<GcProgress> {
        // A minor collection updates only the edges set since the last
        // collection; every other edge names old objects alone.
        let minor = self.heap.collecting_minor;
        let registry_slots = if minor {
            self.trace_state.weak_gc.touched.len()
        } else {
            self.trace_state.weak_gc.slots.len()
        };
        let arrays = &mut self.marking.arrays;
        let entries_end = registry_slots + arrays.entries.len();
        let total = entries_end + arrays.ranges.len();
        let end = (self.weak_cursor + OBJECTS_PER_INCREMENT).min(total);
        let mut evacuation = Evacuation::new(self.heap);
        while self.weak_cursor < end {
            let cursor = self.weak_cursor;
            self.weak_cursor += 1;
            if cursor >= entries_end {
                let original = arrays.ranges[cursor - entries_end].raw;
                if let Some(array) = evacuation.forwarded(original)?.and_then(VMGcRef::from_raw_u32)
                {
                    self.trace_state.weak_gc.table_arrays[arrays.kept] = array;
                    arrays.kept += 1;
                }
                continue;
            }
            if cursor >= registry_slots {
                evacuation.update_weak_pair(arrays.entries[cursor - registry_slots])?;
                continue;
            }
            let forwarded = |reference: &VMGcRef| -> Result<Option<VMGcRef>> {
                Ok(evacuation
                    .forwarded(reference.as_raw_u32())?
                    .and_then(VMGcRef::from_raw_u32))
            };
            let index = if minor {
                self.trace_state.weak_gc.touched[cursor] as usize
            } else {
                cursor
            };
            let Some(edge) = self.trace_state.weak_gc.slots[index].edge.as_mut() else {
                continue;
            };
            let owner = match edge {
                WeakGcEdge::Target { owner, .. } | WeakGcEdge::Ephemeron { owner, .. } => owner,
            };
            let Some(owner_forwarded) = forwarded(owner)? else {
                self.trace_state.weak_gc.retire(index);
                continue;
            };
            *owner = owner_forwarded;
            match edge {
                WeakGcEdge::Target { target, .. } => {
                    *target = target
                        .as_ref()
                        .map(forwarded)
                        .transpose()?
                        .flatten();
                }
                WeakGcEdge::Ephemeron { key, value, .. } => {
                    *key = key
                        .as_ref()
                        .map(forwarded)
                        .transpose()?
                        .flatten();
                    if key.is_none() {
                        self.trace_state.weak_gc.retire(index);
                    } else {
                        *value = value
                            .as_ref()
                            .map(forwarded)
                            .transpose()?
                            .flatten();
                    }
                }
            }
        }
        drop(evacuation);
        if self.weak_cursor == total {
            self.trace_state.weak_gc.table_arrays.truncate(arrays.kept);
            self.trace_state.weak_gc.clear_touched();
            self.phase = CopyingCollectionPhase::SweepExternRefs;
        }
        Ok(GcProgress::Continue)
    }
}
