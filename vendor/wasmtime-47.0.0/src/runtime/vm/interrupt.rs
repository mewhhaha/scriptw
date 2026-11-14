//! Store interruption without checks in compiled code.
//!
//! A requested interrupt removes access to the store's GC heap and to the
//! linear memories registered for it. Compiled code then faults at its next
//! access to one of them, and the fault handler unwinds it with
//! `Trap::Interrupt`. Host code that reaches that memory in the meantime gets
//! access back and continues, so the requester removes access again until the
//! store reports that the interrupt was delivered or it withdraws the request.
//!
//! Code that reaches none of that memory is not interrupted by this. Producers
//! of such code must make it store to the GC heap wherever it can run
//! unbounded.

use crate::runtime::vm::{SendSyncPtr, VMMemoryDefinition};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::vec::Vec;

/// The ranges a requester may have made inaccessible.
///
/// The fault handler reads this, so it is a fixed table of atomics rather
/// than a locked map. A slot packs its page-aligned base and length into one
/// word, which makes every read a consistent pair.
const SLOTS: usize = 64;
const PAGE_BITS: u32 = 12;
const LENGTH_PAGE_BITS: u32 = 21;

struct Slot {
    /// Zero when free, otherwise the owning `StoreInterrupt`'s address.
    owner: AtomicUsize,
    /// Zero when no range is published.
    range: AtomicU64,
}

static ARMED: [Slot; SLOTS] = [const {
    Slot {
        owner: AtomicUsize::new(0),
        range: AtomicU64::new(0),
    }
}; SLOTS];

fn pack(base: usize, len: usize) -> Option<u64> {
    let page = 1usize << PAGE_BITS;
    let pages = len >> PAGE_BITS;
    let fits = base != 0
        && len != 0
        && base % page == 0
        && len % page == 0
        && pages < 1 << LENGTH_PAGE_BITS
        && (base as u64) >> PAGE_BITS < 1 << (64 - LENGTH_PAGE_BITS);
    fits.then(|| ((base as u64 >> PAGE_BITS) << LENGTH_PAGE_BITS) | pages as u64)
}

fn unpack(range: u64) -> (usize, usize) {
    let pages = range & ((1 << LENGTH_PAGE_BITS) - 1);
    let base = (range >> LENGTH_PAGE_BITS) << PAGE_BITS;
    (base as usize, (pages as usize) << PAGE_BITS)
}

#[cfg(unix)]
unsafe fn set_access(base: usize, len: usize, accessible: bool) {
    use rustix::mm::{MprotectFlags, mprotect};
    let flags = if accessible {
        MprotectFlags::READ | MprotectFlags::WRITE
    } else {
        MprotectFlags::empty()
    };
    // The range was accessible when it was published and is unmapped only
    // after its slot is cleared, so this cannot fail in a way that leaves a
    // different mapping affected.
    let _ = unsafe { mprotect(base as *mut _, len, flags) };
}

/// Interrupt state shared by a store and its `InterruptHandle`s.
pub struct StoreInterrupt {
    pending: AtomicBool,
    heap: Mutex<Heap>,
}

struct Heap {
    /// The store's GC heap definition and its registered linear memories,
    /// each with the slot that publishes its range and the length of the
    /// prefix that stays inaccessible regardless. Empty once the store is
    /// gone.
    memories: Vec<(SendSyncPtr<VMMemoryDefinition>, Option<usize>, usize)>,
}

impl StoreInterrupt {
    pub(crate) fn new() -> Self {
        Self {
            pending: AtomicBool::new(false),
            heap: Mutex::new(Heap {
                memories: Vec::new(),
            }),
        }
    }

    /// Adds a memory definition whose address stays valid until `retire`:
    /// the store's GC heap once the store has its final address, or one of
    /// the store's own unshared linear memories. Its first `inaccessible`
    /// bytes (a GC heap's null guard) are never made accessible, so they are
    /// not published either: a fault there is not an interrupt.
    pub(crate) fn watch(&self, memory: SendSyncPtr<VMMemoryDefinition>, inaccessible: usize) {
        let mut heap = self.lock();
        if !heap
            .memories
            .iter()
            .any(|(known, _, _)| known.as_ptr() == memory.as_ptr())
        {
            heap.memories.push((memory, None, inaccessible));
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Heap> {
        self.heap.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Requests an interrupt and removes access to the watched memory.
    /// Repeating the call removes access again after host code restored it.
    pub fn interrupt(&self) {
        self.pending.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        {
            let mut heap = self.lock();
            if !self.pending.load(Ordering::SeqCst) {
                return;
            }
            let owner = self as *const Self as usize;
            for (memory, slot, inaccessible) in heap.memories.iter_mut() {
                // SAFETY: the list is cleared under this lock before the
                // definitions are freed. These are racy word reads of fields
                // the store's own thread writes; any value read was a real
                // mapping of this store.
                let (base, len) = unsafe {
                    let memory = memory.as_ptr();
                    (
                        (*memory).base.as_ptr() as usize,
                        (*memory).current_length.load(Ordering::Relaxed),
                    )
                };
                let Some(len) = len.checked_sub(*inaccessible) else {
                    continue;
                };
                let base = base + *inaccessible;
                let Some(range) = pack(base, len) else {
                    continue;
                };
                if slot.is_none() {
                    *slot = ARMED.iter().position(|slot| {
                        slot.owner
                            .compare_exchange(0, owner, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                    });
                }
                let Some(slot) = *slot else {
                    continue;
                };
                // Publish before removing access: a fault must find its range.
                ARMED[slot].range.store(range, Ordering::SeqCst);
                unsafe { set_access(base, len, false) };
            }
        }
    }

    /// True while a request is neither delivered nor withdrawn.
    pub fn is_pending(&self) -> bool {
        self.pending.load(Ordering::SeqCst)
    }

    /// Withdraws an undelivered request and restores access.
    pub fn cancel(&self) {
        let mut heap = self.lock();
        self.pending.store(false, Ordering::SeqCst);
        Self::release(&mut heap);
    }

    /// Restores access and forgets the store's memory. The store calls this
    /// before it frees its memories or its context.
    pub(crate) fn retire(&self) {
        let mut heap = self.lock();
        Self::release(&mut heap);
        heap.memories.clear();
    }

    fn release(heap: &mut Heap) {
        for (_, slot, _) in heap.memories.iter_mut() {
            let Some(slot) = slot.take() else {
                continue;
            };
            let range = ARMED[slot].range.swap(0, Ordering::SeqCst);
            #[cfg(unix)]
            if range != 0 {
                let (base, len) = unpack(range);
                unsafe { set_access(base, len, true) };
            }
            let _ = range;
            ARMED[slot].owner.store(0, Ordering::SeqCst);
        }
    }

    /// Consumes a pending request. The caller raises `Trap::Interrupt`.
    pub(crate) fn take_pending(&self) -> bool {
        self.pending.swap(false, Ordering::SeqCst)
    }
}

/// Restores access when `addr` lies in a range an interrupt request made
/// inaccessible, and returns the owning store's state.
///
/// Runs in the fault handler: it reads atomics and makes one `mprotect` call.
///
/// # Safety
///
/// The returned reference is valid while the fault is handled: a slot's owner
/// is retired only by the thread that owns the store, which is the faulting
/// thread or one that is not running the store.
#[cfg(unix)]
pub(crate) unsafe fn restore_access<'a>(addr: usize) -> Option<&'a StoreInterrupt> {
    for slot in &ARMED {
        let range = slot.range.load(Ordering::SeqCst);
        if range == 0 {
            continue;
        }
        let (base, len) = unpack(range);
        if addr.wrapping_sub(base) < len {
            unsafe { set_access(base, len, true) };
            let owner = slot.owner.load(Ordering::SeqCst) as *const StoreInterrupt;
            return unsafe { owner.as_ref() };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_round_trip_and_reject_unrepresentable_inputs() {
        let page = 1usize << PAGE_BITS;
        for (base, len) in [
            (page, page),
            (0x7fff_ffff_f000, page),
            (0x5555_0000_0000, 16 << 20),
            (0x1000_0000, (1usize << 32) - page),
        ] {
            assert_eq!(unpack(pack(base, len).unwrap()), (base, len));
        }
        for (base, len) in [
            (0, page),
            (page, 0),
            (page + 1, page),
            (page, page + 1),
            (page, 1usize << 33),
        ] {
            assert!(pack(base, len).is_none());
        }
    }
}
