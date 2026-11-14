//! Conditional marking and weak updates belong to the copying collection.
use super::*;
use crate::vm::WeakGcEdge;

impl CopyingCollection<'_, '_> {
    pub(super) fn process_ephemerons_increment(&mut self) -> Result<GcProgress> {
        let mut evacuation = Evacuation::new(self.heap);
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
        let total = if minor {
            self.trace_state.weak_gc.touched.len()
        } else {
            self.trace_state.weak_gc.slots.len()
        };
        let end = (self.weak_cursor + OBJECTS_PER_INCREMENT).min(total);
        let evacuation = Evacuation::new(self.heap);
        let forwarded = |reference: &VMGcRef| -> Result<Option<VMGcRef>> {
            Ok(evacuation
                .forwarded(reference.as_raw_u32())?
                .and_then(VMGcRef::from_raw_u32))
        };
        while self.weak_cursor < end {
            let index = if minor {
                self.trace_state.weak_gc.touched[self.weak_cursor] as usize
            } else {
                self.weak_cursor
            };
            self.weak_cursor += 1;
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
            self.trace_state.weak_gc.clear_touched();
            self.phase = CopyingCollectionPhase::SweepExternRefs;
        }
        Ok(GcProgress::Continue)
    }
}
