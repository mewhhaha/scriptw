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

pub(crate) struct WeakGcSlot {
    generation: u32,
    pub(crate) edge: Option<WeakGcEdge>,
}

#[derive(Default)]
pub(crate) struct WeakGcRegistry {
    pub(crate) slots: Vec<WeakGcSlot>,
    free: Vec<u32>,
    pub(crate) epoch: u64,
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
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                let index = u32::try_from(self.slots.len())
                    .map_err(|_| format_err!("weak slot index space exhausted"))?;
                self.slots.try_reserve(1)?;
                self.slots.push(WeakGcSlot {
                    generation: 1,
                    edge: None,
                });
                index
            }
        };
        let slot = &mut self.slots[index as usize];
        debug_assert!(slot.edge.is_none());
        slot.edge = Some(edge);
        Ok(WeakGcSlotId {
            index,
            generation: slot.generation,
        })
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

    pub(crate) fn get_mut(&mut self, id: WeakGcSlotId) -> Result<&mut WeakGcEdge> {
        match self.slots.get_mut(id.index as usize) {
            Some(slot) if slot.generation == id.generation => slot
                .edge
                .as_mut()
                .ok_or_else(|| format_err!("weak slot has been retired")),
            _ => Err(format_err!("stale weak slot generation")),
        }
    }

    /// The edge of a live slot; `None` for a stale generation or a retired
    /// slot. Unlike `get`, a miss allocates no error.
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
