//! Conditional marking and weak updates belong to the copying collection.
use super::*;
use crate::vm::WeakGcEdge;

impl CopyingHeap {
    fn weak_forwarded(&self, reference: &VMGcRef) -> Result<Option<VMGcRef>> {
        // Objects the collection does not move are live: copies in the active
        // semi-space, and old objects during a minor collection.
        if reference.is_i31() || !self.is_in_from_space(reference.heap_index()?.get()) {
            return Ok(Some(reference.unchecked_copy()));
        }
        self.index(header_and_forwarding_ref(reference))?
            .forwarding_ref()
    }
}

impl CopyingCollection<'_, '_> {
    pub(super) fn process_ephemerons_increment(&mut self) -> Result<GcProgress> {
        for _ in 0..OBJECTS_PER_INCREMENT {
            let Some(index) = self.marking.pop() else {
                break;
            };
            let Some(WeakGcEdge::Ephemeron {
                owner,
                key: Some(key),
                value: Some(value),
            }) = self.trace_state.weak_gc.slots[index].edge.as_ref()
            else {
                unreachable!("conditional edges are stable until weak update");
            };
            debug_assert!(self.heap.weak_forwarded(owner)?.is_some());
            debug_assert!(self.heap.weak_forwarded(key)?.is_some());
            let value = value.unchecked_copy();
            let forwarded = match self.heap.weak_forwarded(&value)? {
                Some(forwarded) => forwarded,
                None => self.heap.forward(&value, &mut self.marking)?,
            };
            let Some(WeakGcEdge::Ephemeron { value, .. }) =
                self.trace_state.weak_gc.slots[index].edge.as_mut()
            else {
                unreachable!();
            };
            *value = Some(forwarded);
        }
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
        let end =
            (self.weak_cursor + OBJECTS_PER_INCREMENT).min(self.trace_state.weak_gc.slots.len());
        while self.weak_cursor < end {
            let index = self.weak_cursor;
            self.weak_cursor += 1;
            let Some(edge) = self.trace_state.weak_gc.slots[index].edge.as_mut() else {
                continue;
            };
            let owner = match edge {
                WeakGcEdge::Target { owner, .. } | WeakGcEdge::Ephemeron { owner, .. } => owner,
            };
            let Some(forwarded) = self.heap.weak_forwarded(owner)? else {
                self.trace_state.weak_gc.retire(index);
                continue;
            };
            *owner = forwarded;
            match edge {
                WeakGcEdge::Target { target, .. } => {
                    *target = target
                        .as_ref()
                        .map(|r| self.heap.weak_forwarded(r))
                        .transpose()?
                        .flatten();
                }
                WeakGcEdge::Ephemeron { key, value, .. } => {
                    *key = key
                        .as_ref()
                        .map(|r| self.heap.weak_forwarded(r))
                        .transpose()?
                        .flatten();
                    if key.is_none() {
                        self.trace_state.weak_gc.retire(index);
                    } else {
                        *value = value
                            .as_ref()
                            .map(|r| self.heap.weak_forwarded(r))
                            .transpose()?
                            .flatten();
                    }
                }
            }
        }
        if self.weak_cursor == self.trace_state.weak_gc.slots.len() {
            self.phase = CopyingCollectionPhase::SweepExternRefs;
        }
        Ok(GcProgress::Continue)
    }
}
