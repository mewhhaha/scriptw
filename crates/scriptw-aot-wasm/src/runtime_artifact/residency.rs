//! Bounded ownership of completed raw runtime modules. Active callers own
//! their artifacts independently; weak slot references only join in-flight
//! emissions and never keep an old profile or a failed result alive.

use super::{EmitError, RuntimeArtifact};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock, Weak};

type RuntimeCell = OnceLock<Result<Arc<RuntimeArtifact>, EmitError>>;
type RuntimeSlot = Arc<RuntimeCell>;
const MAX_ENTRIES: usize = 2;
const MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Default)]
pub(super) struct RuntimeResidency {
    slots: HashMap<[u8; 32], Weak<RuntimeCell>>,
    recent: VecDeque<([u8; 32], RuntimeSlot)>,
    bytes: usize,
}

impl RuntimeResidency {
    pub(super) fn slot(&mut self, digest: [u8; 32]) -> RuntimeSlot {
        self.slots.retain(|_, slot| slot.strong_count() != 0);
        if let Some(slot) = self.slots.get(&digest).and_then(Weak::upgrade) {
            return slot;
        }
        let slot = Arc::new(OnceLock::new());
        self.slots.insert(digest, Arc::downgrade(&slot));
        slot
    }

    pub(super) fn retain(&mut self, digest: [u8; 32], slot: &RuntimeSlot) {
        self.retain_with_limits(digest, slot, MAX_ENTRIES, MAX_BYTES);
    }

    fn retain_with_limits(
        &mut self,
        digest: [u8; 32],
        slot: &RuntimeSlot,
        max_entries: usize,
        max_bytes: usize,
    ) {
        let Some(Ok(runtime)) = slot.get() else {
            return;
        };
        let bytes = runtime.bytes().len();
        if let Some(position) = self.recent.iter().position(|(key, _)| *key == digest) {
            self.remove(position);
        }
        if max_entries == 0 || bytes > max_bytes {
            return;
        }
        while self.recent.len() >= max_entries || self.bytes > max_bytes - bytes {
            self.remove(0);
        }
        self.bytes += bytes;
        self.recent.push_back((digest, Arc::clone(slot)));
    }

    fn remove(&mut self, position: usize) {
        let (_, slot) = self.recent.remove(position).expect("resident cache entry");
        let runtime = slot.get().and_then(|result| result.as_ref().ok());
        self.bytes -= runtime
            .expect("only initialized successes are retained")
            .bytes()
            .len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PoolBoundary;
    use crate::runtime_artifact::RuntimeLayout;

    fn initialize(slot: &RuntimeSlot, id: u8, bytes: usize) -> Arc<RuntimeArtifact> {
        let runtime = Arc::new(RuntimeArtifact::unadmitted_fixture(
            vec![id; bytes],
            RuntimeLayout::new(
                0,
                vec![],
                vec![],
                PoolBoundary::from_runtime_cache(0, 0, 1).unwrap(),
            ),
        ));
        slot.set(Ok(Arc::clone(&runtime))).unwrap();
        runtime
    }

    #[test]
    fn profile_churn_releases_cache_owners_but_preserves_active_artifacts() {
        let mut cache = RuntimeResidency::default();
        let first = cache.slot([1; 32]);
        let artifact = initialize(&first, 1, 8);
        cache.retain_with_limits([1; 32], &first, 2, 16);
        let first_lease = Arc::downgrade(&first);
        drop(first);
        for id in 2..=4 {
            let slot = cache.slot([id; 32]);
            initialize(&slot, id, 8);
            cache.retain_with_limits([id; 32], &slot, 2, 16);
        }
        assert!(first_lease.upgrade().is_none());
        assert_eq!(artifact.bytes(), &[1; 8]);
        assert_eq!(cache.bytes, 16);
        assert_eq!(cache.recent.len(), 2);
        // Acquisition prunes dead profile metadata as well as raw bytes.
        cache.slot([4; 32]);
        assert_eq!(cache.slots.len(), 2);
    }

    #[test]
    fn in_flight_callers_share_initialization_even_when_other_profiles_churn() {
        let mut cache = RuntimeResidency::default();
        let pending = cache.slot([1; 32]);
        for id in 2..=4 {
            let slot = cache.slot([id; 32]);
            initialize(&slot, id, 8);
            cache.retain_with_limits([id; 32], &slot, 1, 8);
        }
        let joined = cache.slot([1; 32]);
        assert!(Arc::ptr_eq(&pending, &joined));
        initialize(&pending, 1, 8);
        assert!(Arc::ptr_eq(
            pending.get().unwrap().as_ref().unwrap(),
            joined.get().unwrap().as_ref().unwrap(),
        ));
    }

    #[test]
    fn oversized_and_failed_profiles_are_not_sticky_cache_results() {
        let mut cache = RuntimeResidency::default();
        let oversized = cache.slot([1; 32]);
        initialize(&oversized, 1, 9);
        cache.retain_with_limits([1; 32], &oversized, 2, 8);
        let weak = Arc::downgrade(&oversized);
        drop(oversized);
        assert!(weak.upgrade().is_none());
        let failed = cache.slot([2; 32]);
        failed
            .set(Err(EmitError::unsupported("transient cache fixture")))
            .unwrap();
        cache.retain([2; 32], &failed);
        drop(failed);
        assert!(cache.slot([2; 32]).get().is_none());
        assert_eq!(cache.bytes, 0);
        assert!(cache.recent.is_empty());
    }
}
