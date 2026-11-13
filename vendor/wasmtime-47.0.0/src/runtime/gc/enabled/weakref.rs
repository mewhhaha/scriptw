//! Store-scoped handles into collector-owned weak edges.
use crate::prelude::*;
use crate::store::{AutoAssertNoGc, StoreId, StoreOpaque};
use crate::vm::{VMGcRef, WeakGcEdge, WeakGcSlotId};
use crate::{AnyRef, AsContext, AsContextMut, Rooted};
use core::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy)]
struct WeakHandle {
    store: StoreId,
    slot: WeakGcSlotId,
}

impl WeakHandle {
    fn check(&self, store: &StoreOpaque) -> Result<()> {
        ensure!(
            self.store == store.id(),
            "weak handle used with wrong Store"
        );
        Ok(())
    }

    /// The live edge this handle names, or `None` once it has been retired.
    /// Readers take a shared Store borrow, which cannot collect, so they need
    /// no no-GC scope and allocate nothing on a miss.
    fn live<'a>(&self, store: &'a StoreOpaque) -> Result<Option<&'a WeakGcEdge>> {
        self.check(store)?;
        Ok(store.unwrap_gc_store().weak_gc.live(self.slot))
    }
}

fn reference(store: &StoreOpaque, reference: &AnyRef) -> Result<VMGcRef> {
    ensure!(
        reference.inner.comes_from_same_store(store),
        "GC reference used with wrong Store"
    );
    Ok(reference.inner.try_gc_ref(store)?.unchecked_copy())
}

fn heap_reference(store: &StoreOpaque, value: &AnyRef) -> Result<VMGcRef> {
    let reference = reference(store, value)?;
    ensure!(
        !reference.is_i31(),
        "weak owners and targets require heap objects"
    );
    Ok(reference)
}

fn insert(store: &mut AutoAssertNoGc<'_>, edge: WeakGcEdge) -> Result<WeakHandle> {
    let id = store.id();
    let gc = store.unwrap_gc_store_mut();
    ensure!(
        gc.gc_heap.supports_weak_refs(),
        "selected collector has no weak/ephemeron facility"
    );
    Ok(WeakHandle {
        store: id,
        slot: gc.weak_gc.insert(edge)?,
    })
}

/// A weak target whose entry is retired when its owner dies.
/// The handle never exposes an unrooted reference. Upgrades are atomic with
/// respect to collection and return a scoped strong root in the given Store.
/// This copyable handle does not deregister on drop. Call `remove` when deleting
/// a registration; collection retires it when its owner becomes unreachable.
#[derive(Debug, Clone, Copy)]
pub struct WeakAnyRef {
    handle: WeakHandle,
}

impl WeakAnyRef {
    /// Register a target without retaining either target or owner strongly.
    /// Requires a collector that implements weak reachability.
    pub fn new(mut store: impl AsContextMut, owner: &AnyRef, target: &AnyRef) -> Result<Self> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        let owner = heap_reference(&guard, owner)?;
        let target = heap_reference(&guard, target)?;
        Ok(Self {
            handle: insert(
                &mut guard,
                WeakGcEdge::Target {
                    owner,
                    target: Some(target),
                },
            )?,
        })
    }

    /// Upgrade a surviving target to a strong root. A cleared or retired target
    /// returns None. A handle used with a different Store returns an error.
    pub fn upgrade(&self, mut store: impl AsContextMut) -> Result<Option<Rooted<AnyRef>>> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        let value = match self.handle.live(&guard)? {
            None => return Ok(None),
            Some(WeakGcEdge::Target { target, .. }) => target.as_ref().map(VMGcRef::unchecked_copy),
            Some(WeakGcEdge::Ephemeron { .. }) => {
                unreachable!("typed weak handle cannot name ephemeron")
            }
        };
        Ok(value.map(|value| {
            let value = guard.clone_gc_ref(&value);
            Rooted::new(&mut guard, value)
        }))
    }

    /// Remove the weak entry. Repeated removal and a collected owner are safe.
    pub fn remove(&self, mut store: impl AsContextMut) -> Result<()> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        self.handle.check(&guard)?;
        let gc = guard.unwrap_gc_store_mut();
        if gc.weak_gc.contains(self.handle.slot) {
            gc.weak_gc.remove(self.handle.slot)?;
        }
        Ok(())
    }
}

/// A conditional strong edge whose owner and key are both weak.
/// Values are marked only after owner and key become live independently, then
/// participate in the collector's ephemeron fixed point.
/// This copyable handle does not deregister on drop. Call `remove` when deleting
/// an entry; collection retires it when its owner or key becomes unreachable.
#[derive(Debug, Clone, Copy)]
pub struct GcEphemeron {
    handle: WeakHandle,
}

impl GcEphemeron {
    /// Whether the entry still has a surviving owner and key after collection.
    pub fn is_live(&self, store: impl AsContext) -> Result<bool> {
        Ok(self.handle.live(store.as_context().0)?.is_some())
    }

    /// Register a conditional value without rooting owner or key.
    pub fn new(
        mut store: impl AsContextMut,
        owner: &AnyRef,
        key: &AnyRef,
        value: Option<&AnyRef>,
    ) -> Result<Self> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        let owner = heap_reference(&guard, owner)?;
        let key = heap_reference(&guard, key)?;
        let value = value.map(|value| reference(&guard, value)).transpose()?;
        Ok(Self {
            handle: insert(
                &mut guard,
                WeakGcEdge::Ephemeron {
                    owner,
                    key: Some(key),
                    value,
                },
            )?,
        })
    }

    /// Test actual key identity without exposing or rooting a weak key.
    pub fn matches_key(&self, store: impl AsContext, key: &AnyRef) -> Result<bool> {
        let store = store.as_context().0;
        self.handle.check(store)?;
        let wanted = heap_reference(store, key)?;
        match self.handle.live(store)? {
            None => Ok(false),
            Some(WeakGcEdge::Ephemeron { key, .. }) => {
                Ok(key.as_ref().is_some_and(|key| key == &wanted))
            }
            Some(WeakGcEdge::Target { .. }) => {
                unreachable!("typed ephemeron handle cannot name target")
            }
        }
    }

    /// Upgrade the surviving value to a scoped strong root.
    pub fn value(&self, mut store: impl AsContextMut) -> Result<Option<Rooted<AnyRef>>> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        let value = match self.handle.live(&guard)? {
            None => return Ok(None),
            Some(WeakGcEdge::Ephemeron {
                key: Some(_),
                value,
                ..
            }) => value.as_ref().map(VMGcRef::unchecked_copy),
            Some(WeakGcEdge::Ephemeron { key: None, .. }) => None,
            Some(WeakGcEdge::Target { .. }) => {
                unreachable!("typed ephemeron handle cannot name target")
            }
        };
        Ok(value.map(|value| {
            let value = guard.clone_gc_ref(&value);
            Rooted::new(&mut guard, value)
        }))
    }

    /// Replace only the conditional value of a live entry.
    pub fn set_value(&self, mut store: impl AsContextMut, value: Option<&AnyRef>) -> Result<()> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        self.handle.check(&guard)?;
        let value = value.map(|value| reference(&guard, value)).transpose()?;
        match guard
            .unwrap_gc_store_mut()
            .weak_gc
            .get_mut(self.handle.slot)?
        {
            WeakGcEdge::Ephemeron {
                key: Some(_),
                value: slot,
                ..
            } => *slot = value,
            WeakGcEdge::Ephemeron { key: None, .. } => bail!("ephemeron key has been cleared"),
            WeakGcEdge::Target { .. } => unreachable!("typed ephemeron handle cannot name target"),
        }
        Ok(())
    }

    /// Remove the conditional edge; stale generations cannot remove new edges.
    pub fn remove(&self, mut store: impl AsContextMut) -> Result<()> {
        let mut guard = AutoAssertNoGc::new(store.as_context_mut().0);
        self.handle.check(&guard)?;
        let gc = guard.unwrap_gc_store_mut();
        if gc.weak_gc.contains(self.handle.slot) {
            gc.weak_gc.remove(self.handle.slot)?;
        }
        Ok(())
    }
}

/// A copying-heap identity epoch, scoped to one Store. Every copying cycle
/// invalidates it before any object moves, including cycles without weak edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GcWeakHeapEpoch {
    store: StoreId,
    collection: u64,
}

impl GcWeakHeapEpoch {
    fn checked(store: &StoreOpaque) -> Result<Self> {
        let id = store.id();
        let gc = store
            .optional_gc_store()
            .ok_or_else(|| format_err!("weak heap epoch requires an initialized GC heap"))?;
        ensure!(
            gc.gc_heap.supports_weak_key_fingerprints(),
            "selected collector has no epoch-scoped weak key identity"
        );
        Ok(Self {
            store: id,
            collection: gc.weak_gc.epoch,
        })
    }

    /// Current collector identity epoch. This token retains no GC object.
    pub fn current(store: impl AsContext) -> Result<Self> {
        Self::checked(store.as_context().0)
    }
}

/// An opaque, non-rooting lookup token for a heap object in one collection
/// epoch. It must never be used as a permanent or cross-Store object identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcWeakKeyFingerprint {
    epoch: GcWeakHeapEpoch,
    index: u32,
}

/// Hashes only the heap index. Equal tokens have equal indices, so this agrees
/// with `Eq`; a table holds tokens of one epoch, where the index alone already
/// distinguishes keys. The hasher sees exactly one `write_u32`.
impl Hash for GcWeakKeyFingerprint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(self.index);
    }
}

impl GcWeakKeyFingerprint {
    /// Capture the object's current token without retaining it strongly.
    /// A shared Store borrow cannot collect while the token is taken.
    pub fn new(store: impl AsContext, key: &AnyRef) -> Result<Self> {
        let store = store.as_context().0;
        let key = heap_reference(store, key)?;
        Ok(Self {
            epoch: GcWeakHeapEpoch::checked(store)?,
            index: key.as_raw_u32(),
        })
    }

    /// The token is valid only while this exact heap epoch remains current.
    pub fn epoch(self) -> GcWeakHeapEpoch {
        self.epoch
    }
}

impl GcEphemeron {
    /// Current non-rooting token for a surviving key; retired handles return
    /// None. No weak key is upgraded or exposed as an unrooted reference.
    pub fn key_fingerprint(&self, store: impl AsContext) -> Result<Option<GcWeakKeyFingerprint>> {
        let store = store.as_context().0;
        self.handle.check(store)?;
        let epoch = GcWeakHeapEpoch::checked(store)?;
        match self.handle.live(store)? {
            None => Ok(None),
            Some(WeakGcEdge::Ephemeron { key, .. }) => {
                Ok(key.as_ref().map(|key| GcWeakKeyFingerprint {
                    epoch,
                    index: key.as_raw_u32(),
                }))
            }
            Some(WeakGcEdge::Target { .. }) => {
                unreachable!("typed ephemeron handle cannot name target")
            }
        }
    }
}
