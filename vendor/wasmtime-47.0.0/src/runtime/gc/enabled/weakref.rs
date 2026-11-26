//! Store-scoped handles into collector-owned weak edges.
use crate::prelude::*;
use crate::store::{AutoAssertNoGc, StoreId, StoreOpaque};
use crate::vm::{GcStore, VMGcRef, WeakGcEdge, WeakGcSlotId};
use crate::{AnyRef, AsContext, AsContextMut, Rooted};
use core::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy)]
struct WeakHandle {
    store: StoreId,
    slot: WeakGcSlotId,
}

impl WeakHandle {
    #[inline]
    fn check(&self, store: &StoreOpaque) -> Result<()> {
        self.check_store_id(store.id())
    }

    #[inline]
    fn check_store_id(&self, store: StoreId) -> Result<()> {
        ensure!(self.store == store, "weak handle used with wrong Store");
        Ok(())
    }

    /// The live edge this handle names, or `None` once it has been retired.
    /// Readers take a shared Store borrow, which cannot collect, so they need
    /// no no-GC scope and allocate nothing on a miss.
    #[inline]
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

/// The heap object a raw reference names. Null and `i31` have no identity a
/// weak edge can follow.
#[inline]
fn raw_heap_reference(raw: u32) -> Result<VMGcRef> {
    match VMGcRef::from_raw_u32(raw) {
        Some(reference) if !reference.is_i31() => Ok(reference),
        _ => bail!("weak owners and targets require heap objects"),
    }
}

/// Registration allocates no GC object and cannot collect.
fn insert(store: &mut StoreOpaque, edge: WeakGcEdge) -> Result<WeakHandle> {
    let id = store.id();
    insert_in_gc(id, store.unwrap_gc_store_mut(), edge)
}

fn insert_in_gc(store: StoreId, gc: &mut GcStore, edge: WeakGcEdge) -> Result<WeakHandle> {
    ensure!(
        gc.gc_heap.supports_weak_refs(),
        "selected collector has no weak/ephemeron facility"
    );
    Ok(WeakHandle {
        store,
        slot: gc.weak_gc.insert(edge)?,
    })
}

fn raw_ephemeron(owner: u32, key: u32, value: u32) -> Result<WeakGcEdge> {
    Ok(WeakGcEdge::Ephemeron {
        owner: raw_heap_reference(owner)?,
        key: Some(raw_heap_reference(key)?),
        value: VMGcRef::from_raw_u32(value),
    })
}

#[inline]
fn expose_ephemeron_value(gc: &mut GcStore, value: Option<VMGcRef>) -> Result<u32> {
    let Some(value) = value else {
        return Ok(0);
    };
    let value = gc.clone_gc_ref(&value);
    Ok(gc.expose_gc_ref_to_wasm(value)?.get())
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
    /// Borrow the Store's host data and raw ephemeron operations together.
    ///
    /// The ephemeron access cannot collect or reenter Wasm. This lets a host
    /// index probe, read, or update an edge while the raw references stay valid.
    #[inline]
    pub fn with_raw_access<T: 'static, R>(
        mut store: impl AsContextMut<Data = T>,
        f: impl FnOnce(&mut T, &mut RawGcEphemerons<'_>) -> R,
    ) -> R {
        let mut store = store.as_context_mut();
        let (data, store, gc) = store.data_and_gc_store();
        f(data, &mut RawGcEphemerons { store, gc })
    }

    /// Whether the entry still has a surviving owner and key after collection.
    #[inline]
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

    /// Test key identity against a token taken for that key in the current
    /// epoch, without resolving the key's root again. Equal to `matches_key`
    /// for the token's key; a token from another Store or epoch is an error.
    pub fn matches_fingerprint(
        &self,
        store: impl AsContext,
        fingerprint: GcWeakKeyFingerprint,
    ) -> Result<bool> {
        let store = store.as_context().0;
        ensure!(
            fingerprint.epoch == GcWeakHeapEpoch::checked(store)?,
            "weak key token is not from this Store's current heap epoch"
        );
        match self.handle.live(store)? {
            None => Ok(false),
            Some(WeakGcEdge::Ephemeron { key, .. }) => {
                Ok(key.as_ref().is_some_and(|key| key.as_raw_u32() == fingerprint.index))
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

/// Unrooted operands, for host functions made with `Func::new_unchecked`.
///
/// Such a function receives each GC reference as the raw `u32` of a `ValRaw`
/// (`ValRaw::get_anyref`). Until the call returns or the Store collects, that
/// value is the reference itself, so an operation that cannot collect needs
/// no root for it. None of these operations allocates a GC object or
/// collects, and none enters a no-GC scope.
impl GcEphemeron {
    /// [`GcEphemeron::new`] for raw operands; a zero `value` is no value.
    ///
    /// # Safety
    ///
    /// `owner`, `key` and a nonzero `value` must be references that Wasm
    /// passed to the host call now running in this Store, which must not have
    /// collected since it was entered.
    pub unsafe fn new_raw(
        mut store: impl AsContextMut,
        owner: u32,
        key: u32,
        value: u32,
    ) -> Result<Self> {
        Self::register_raw(store.as_context_mut().0, owner, key, value)
    }

    /// Not generic, so that the registry's small accessors inline here.
    fn register_raw(store: &mut StoreOpaque, owner: u32, key: u32, value: u32) -> Result<Self> {
        Ok(Self {
            handle: insert(store, raw_ephemeron(owner, key, value)?)?,
        })
    }

    /// Whether this entry is live and its owner and key are exactly the
    /// objects the raw references name. Only identities are compared; no
    /// reference is followed.
    #[inline]
    pub fn matches_raw(&self, store: impl AsContext, owner: u32, key: u32) -> Result<bool> {
        Ok(self
            .handle
            .live(store.as_context().0)?
            .is_some_and(|edge| edge.matches_ephemeron_raw(owner, key)))
    }

    /// The surviving value as a raw reference a host function may return to
    /// Wasm at once, or zero for none or a retired entry.
    pub fn value_raw(&self, mut store: impl AsContextMut) -> Result<u32> {
        self.exposed_value(store.as_context_mut().0)
    }

    /// Not generic, for the same reason as `register_raw`.
    fn exposed_value(&self, store: &mut StoreOpaque) -> Result<u32> {
        self.handle.check(store)?;
        let gc = store.unwrap_gc_store_mut();
        let value = match gc.weak_gc.live(self.handle.slot) {
            Some(WeakGcEdge::Ephemeron {
                key: Some(_),
                value: Some(value),
                ..
            }) => value.unchecked_copy(),
            None | Some(WeakGcEdge::Ephemeron { .. }) => return Ok(0),
            Some(WeakGcEdge::Target { .. }) => {
                unreachable!("typed ephemeron handle cannot name target")
            }
        };
        expose_ephemeron_value(gc, Some(value))
    }

    /// [`GcEphemeron::set_value`] for a raw operand; zero clears the value.
    ///
    /// # Safety
    ///
    /// As for [`GcEphemeron::new_raw`].
    pub unsafe fn set_value_raw(&self, mut store: impl AsContextMut, value: u32) -> Result<()> {
        let store: &mut StoreOpaque = store.as_context_mut().0;
        self.handle.check(store)?;
        match store.unwrap_gc_store_mut().weak_gc.get_mut(self.handle.slot)? {
            WeakGcEdge::Ephemeron {
                key: Some(_),
                value: slot,
                ..
            } => *slot = VMGcRef::from_raw_u32(value),
            WeakGcEdge::Ephemeron { key: None, .. } => bail!("ephemeron key has been cleared"),
            WeakGcEdge::Target { .. } => unreachable!("typed ephemeron handle cannot name target"),
        }
        Ok(())
    }
}

/// Raw ephemeron operations during a disjoint borrow of a Store's host data.
///
/// Obtain this through [`GcEphemeron::with_raw_access`].
pub struct RawGcEphemerons<'a> {
    store: StoreId,
    gc: Option<&'a mut GcStore>,
}

/// The result of trying to update an ephemeron by raw owner and key identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcEphemeronUpdate {
    /// Both operands matched and the value was replaced.
    Updated,
    /// The live slot belongs to a different owner or key, or its key is cleared.
    Mismatch,
    /// The slot has been retired or now belongs to another generation.
    Retired,
}

impl RawGcEphemerons<'_> {
    /// Check whether an entry still has a live slot.
    #[inline]
    pub fn is_live(&self, entry: GcEphemeron) -> Result<bool> {
        entry.handle.check_store_id(self.store)?;
        Ok(self
            .gc
            .as_deref()
            .is_some_and(|gc| gc.weak_gc.contains(entry.handle.slot)))
    }

    /// Return the value to pass directly to Wasm if the live owner and key
    /// match. `Some(0)` is a matching entry with a null value.
    ///
    /// The result is unrooted and valid only until the Store next collects.
    /// The operands are only compared, never dereferenced.
    #[inline]
    pub fn value_if_matches_raw(
        &mut self,
        entry: GcEphemeron,
        owner: u32,
        key: u32,
    ) -> Result<Option<u32>> {
        entry.handle.check_store_id(self.store)?;
        let Some(gc) = self.gc.as_deref_mut() else {
            return Ok(None);
        };
        let Some(edge) = gc.weak_gc.live(entry.handle.slot) else {
            return Ok(None);
        };
        if !edge.matches_ephemeron_raw(owner, key) {
            return Ok(None);
        }
        let WeakGcEdge::Ephemeron { value, .. } = edge else {
            unreachable!("matched edge is an ephemeron")
        };
        let value = value.as_ref().map(VMGcRef::unchecked_copy);
        Ok(Some(expose_ephemeron_value(gc, value)?))
    }

    /// Replace a conditional value if the live owner and key match. A zero
    /// value clears the entry's value while retaining membership.
    ///
    /// # Safety
    ///
    /// A nonzero value must be a valid GC reference in the Store that owns
    /// this access, taken since its last collection. Owner and key are only
    /// compared and are never dereferenced.
    #[inline]
    pub unsafe fn set_value_if_matches_raw(
        &mut self,
        entry: GcEphemeron,
        owner: u32,
        key: u32,
        value: u32,
    ) -> Result<GcEphemeronUpdate> {
        entry.handle.check_store_id(self.store)?;
        let Some(gc) = self.gc.as_deref_mut() else {
            return Ok(GcEphemeronUpdate::Retired);
        };
        let registry = &mut gc.weak_gc;
        let updated =
            registry.set_ephemeron_value_if_matches_raw(entry.handle.slot, owner, key, value)?;
        Ok(match updated {
            Some(true) => GcEphemeronUpdate::Updated,
            Some(false) => GcEphemeronUpdate::Mismatch,
            None => GcEphemeronUpdate::Retired,
        })
    }

    /// Register an ephemeron without rooting its operands. Null and i31 are
    /// invalid as owners or keys. A zero value registers membership alone.
    ///
    /// # Safety
    ///
    /// As for [`GcEphemeron::new_raw`].
    #[inline]
    pub unsafe fn insert_raw(&mut self, owner: u32, key: u32, value: u32) -> Result<GcEphemeron> {
        let edge = raw_ephemeron(owner, key, value)?;
        let gc = self
            .gc
            .as_deref_mut()
            .ok_or_else(|| format_err!("weak registration requires an initialized GC heap"))?;
        Ok(GcEphemeron {
            handle: insert_in_gc(self.store, gc, edge)?,
        })
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
