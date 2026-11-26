//! Collector-owned weak edges. No entry is added to the ordinary root census.
use crate::prelude::*;
use crate::vm::VMGcRef;

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
        match self {
            Self::Ephemeron {
                owner: edge_owner,
                key: Some(edge_key),
                ..
            } => edge_owner.as_raw_u32() == owner && edge_key.as_raw_u32() == key,
            Self::Ephemeron { key: None, .. } => false,
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
    free: Vec<u32>,
    pub(crate) epoch: u64,
    /// Slots whose edges were set since the last collection. Every other edge
    /// names only objects that collection left in the old generation, so a
    /// minor collection processes just these.
    pub(crate) touched: Vec<u32>,
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
        if !edge.matches_ephemeron_raw(owner, key) {
            return Ok(Some(false));
        }
        let WeakGcEdge::Ephemeron { value: current, .. } = edge else {
            unreachable!("matched edge is an ephemeron")
        };
        Self::touch_slot(&mut self.touched, &mut slot.touched, id.index)?;
        *current = VMGcRef::from_raw_u32(value);
        Ok(Some(true))
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
