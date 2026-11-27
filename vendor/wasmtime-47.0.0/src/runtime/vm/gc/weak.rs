//! Collector-owned weak edges. No entry is added to the ordinary root census.
use crate::prelude::*;
use crate::vm::VMGcRef;
use crate::{ArrayType, AsContextMut, GcEphemeron, StorageType, ValType};
use wasmtime_environ::VMGcKind;

/// Reserved header bit marking an array whose elements are weak `[key, value]`
/// ephemeron pairs. Copied with the header across evacuations.
pub(crate) const WEAK_ARRAY_ELEMENTS: u32 = 1 << 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WeakGcSlotId {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

pub(crate) enum WeakGcEdge {
    Target {
        owner: VMGcRef,
        target: Option<VMGcRef>,
    },
    Ephemeron {
        owner: VMGcRef,
        key: Option<VMGcRef>,
        value: Option<VMGcRef>,
    },
}

impl WeakGcEdge {
    /// Only compares identities. Neither raw operand is dereferenced.
    #[inline]
    pub(crate) fn matches_ephemeron_raw(&self, owner: u32, key: u32) -> bool {
        self.ephemeron_value_if_matches_raw(owner, key).is_some()
    }

    /// The value slot of a matching ephemeron, which may contain null.
    #[inline]
    pub(crate) fn ephemeron_value_if_matches_raw(
        &self,
        owner: u32,
        key: u32,
    ) -> Option<&Option<VMGcRef>> {
        match self {
            Self::Ephemeron {
                owner: edge_owner,
                key: Some(edge_key),
                value,
            } if edge_owner.as_raw_u32() == owner && edge_key.as_raw_u32() == key => Some(value),
            Self::Ephemeron { .. } => None,
            Self::Target { .. } => unreachable!("typed ephemeron handle cannot name target"),
        }
    }
}

pub(crate) struct WeakGcSlot {
    generation: u32,
    /// Whether `touched` lists this slot.
    touched: bool,
    pub(crate) edge: Option<WeakGcEdge>,
}

#[derive(Default)]
pub(crate) struct WeakGcRegistry {
    pub(crate) slots: Vec<WeakGcSlot>,
    /// Arrays of alternating weak keys and conditional values. The collector
    /// relocates this registration only if the array is independently live.
    pub(crate) table_arrays: Vec<VMGcRef>,
    free: Vec<u32>,
    pub(crate) epoch: u64,
    /// Slots whose edges were set since the last collection. Every other edge
    /// names only objects that collection left in the old generation, so a
    /// minor collection processes just these.
    pub(crate) touched: Vec<u32>,
}

impl GcEphemeron {
    /// Register a Wasm array as an ephemeron table in a copying-GC Store.
    ///
    /// The array must have an even number of mutable `eqref` elements, with
    /// each key followed by its value, and must be freshly allocated / empty
    /// (containing no pre-existing un-remembered entries). The array and key
    /// must both be live before the collector marks a value. A dead key clears
    /// both elements. Registration neither roots the array nor allocates a GC
    /// object.
    ///
    /// # Safety
    ///
    /// `raw` must be a reference passed by Wasm to the currently running host
    /// call, and the Store must not have collected since the call began.
    pub unsafe fn register_table_raw(mut store: impl AsContextMut, raw: u32) -> Result<()> {
        let store = store.as_context_mut().0;
        let array = VMGcRef::from_raw_u32(raw)
            .filter(|reference| !reference.is_i31())
            .ok_or_else(|| format_err!("ephemeron table requires an array reference"))?;
        let gc = store.unwrap_gc_store();
        ensure!(
            gc.gc_heap.supports_weak_refs(),
            "selected collector has no weak/ephemeron facility"
        );
        let header = gc.header(&array)?;
        ensure!(
            header.kind().matches(VMGcKind::ArrayRef),
            "ephemeron table requires an array reference"
        );
        let index = header
            .ty()
            .ok_or_else(|| format_err!("ephemeron table array has no type"))?;
        let already_registered = header.reserved_u26() & WEAK_ARRAY_ELEMENTS != 0;
        let ty = ArrayType::from_shared_type_index(store.engine(), index);
        ensure!(
            ty.mutability().is_var()
                && StorageType::eq(&ty.element_type(), &StorageType::ValType(ValType::EQREF)),
            "ephemeron table requires mutable nullable eqref elements"
        );
        ensure!(
            gc.array_len(array.as_arrayref_unchecked())? % 2 == 0,
            "ephemeron table requires an even number of elements"
        );
        ensure!(!already_registered, "ephemeron table already registered");
        let gc_store = store.unwrap_gc_store_mut();
        gc_store.weak_gc.table_arrays.try_reserve(1)?;
        let header = gc_store.gc_heap.header_mut(&array)?;
        let reserved = header.reserved_u26();
        header.set_reserved_u26(reserved | WEAK_ARRAY_ELEMENTS);
        gc_store.weak_gc.table_arrays.push(array);
        Ok(())
    }
}

impl WeakGcRegistry {
    pub(crate) fn advance_epoch(&mut self) -> Result<()> {
        let next = self
            .epoch
            .checked_add(1)
            .ok_or_else(|| format_err!("weak heap epoch exhausted"))?;
        // Weak update may retire every occupied slot; it must not allocate
        // while the copying heap is in a half-collected state.
        self.free.try_reserve(self.slots.len() - self.free.len())?;
        self.epoch = next;
        Ok(())
    }

    pub(crate) fn insert(&mut self, edge: WeakGcEdge) -> Result<WeakGcSlotId> {
        if let Some(&index) = self.free.last() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.edge.is_none());
            Self::touch_slot(&mut self.touched, &mut slot.touched, index)?;
            self.free.pop();
            slot.edge = Some(edge);
            return Ok(WeakGcSlotId {
                index,
                generation: slot.generation,
            });
        }

        let index = u32::try_from(self.slots.len())
            .map_err(|_| format_err!("weak slot index space exhausted"))?;
        self.touched.try_reserve(1)?;
        self.slots.try_reserve(1)?;
        let id = WeakGcSlotId {
            index,
            generation: 1,
        };
        self.touched.push(index);
        self.slots.push(WeakGcSlot {
            generation: id.generation,
            touched: true,
            edge: Some(edge),
        });
        Ok(id)
    }

    pub(crate) fn get(&self, id: WeakGcSlotId) -> Result<&WeakGcEdge> {
        match self.slots.get(id.index as usize) {
            Some(slot) if slot.generation == id.generation => slot
                .edge
                .as_ref()
                .ok_or_else(|| format_err!("weak slot has been retired")),
            _ => Err(format_err!("stale weak slot generation")),
        }
    }

    /// The edge of a live slot, for changing it: the slot counts as touched.
    pub(crate) fn get_mut(&mut self, id: WeakGcSlotId) -> Result<&mut WeakGcEdge> {
        let slot = self
            .slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .ok_or_else(|| format_err!("stale weak slot generation"))?;
        let edge = slot
            .edge
            .as_mut()
            .ok_or_else(|| format_err!("weak slot has been retired"))?;
        Self::touch_slot(&mut self.touched, &mut slot.touched, id.index)?;
        Ok(edge)
    }

    /// Returns `None` for a retired slot and otherwise whether both operands
    /// matched and the value was replaced.
    #[inline]
    pub(crate) fn set_ephemeron_value_if_matches_raw(
        &mut self,
        id: WeakGcSlotId,
        owner: u32,
        key: u32,
        value: u32,
    ) -> Result<Option<bool>> {
        let Some(slot) = self
            .slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
        else {
            return Ok(None);
        };
        let Some(edge) = slot.edge.as_mut() else {
            return Ok(None);
        };
        match edge {
            WeakGcEdge::Ephemeron {
                owner: edge_owner,
                key: Some(edge_key),
                value: current,
            } if edge_owner.as_raw_u32() == owner && edge_key.as_raw_u32() == key => {
                Self::touch_slot(&mut self.touched, &mut slot.touched, id.index)?;
                *current = VMGcRef::from_raw_u32(value);
                Ok(Some(true))
            }
            WeakGcEdge::Ephemeron { .. } => Ok(Some(false)),
            WeakGcEdge::Target { .. } => unreachable!("typed ephemeron handle cannot name target"),
        }
    }

    /// Remember a new or changed edge for the next minor collection before
    /// it can install a nursery reference. On failure the list is unchanged.
    #[inline]
    fn touch_slot(list: &mut Vec<u32>, touched: &mut bool, index: u32) -> Result<()> {
        if !*touched {
            list.try_reserve(1)?;
            list.push(index);
            *touched = true;
        }
        Ok(())
    }

    /// The edge of a live slot; `None` for a stale generation or a retired
    /// slot. Unlike `get`, a miss allocates no error.
    #[inline]
    pub(crate) fn live(&self, id: WeakGcSlotId) -> Option<&WeakGcEdge> {
        match self.slots.get(id.index as usize) {
            Some(slot) if slot.generation == id.generation => slot.edge.as_ref(),
            _ => None,
        }
    }

    pub(crate) fn contains(&self, id: WeakGcSlotId) -> bool {
        self.live(id).is_some()
    }

    pub(crate) fn remove(&mut self, id: WeakGcSlotId) -> Result<()> {
        self.get(id)?;
        self.retire(id.index as usize);
        Ok(())
    }

    /// After a collection, no edge names a young object any more.
    pub(crate) fn clear_touched(&mut self) {
        for index in self.touched.drain(..) {
            if let Some(slot) = self.slots.get_mut(index as usize) {
                slot.touched = false;
            }
        }
    }

    pub(crate) fn retire(&mut self, index: usize) {
        let slot = &mut self.slots[index];
        if slot.edge.take().is_none() {
            return;
        }
        // Retire exhausted generations permanently; never permit ABA.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(index as u32);
        }
    }
}
