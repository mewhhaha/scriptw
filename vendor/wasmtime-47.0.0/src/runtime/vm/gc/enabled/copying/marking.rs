//! Collection-local incidence of independently live owners and keys.
use super::weak::{WeakArrayEntry, WeakArrays};
use crate::prelude::*;
use crate::vm::{VMGcRef, WeakGcEdge, WeakGcRegistry};
use core::mem::size_of;

/// Limit the retained scratch in each buffer.
const MAX_RETAINED_BUFFER_BYTES: usize = 256 * 1024;

pub(super) fn reset_buffer<T>(buffer: &mut Vec<T>) {
    if buffer.capacity() > MAX_RETAINED_BUFFER_BYTES / size_of::<T>() {
        *buffer = Vec::new();
    } else {
        buffer.clear();
    }
}

#[derive(Clone, Copy)]
pub(super) enum ConditionalEdge {
    Registry(u32),
    Array(u32),
}

/// One gate of one conditional edge: the owner or key it waits for, the
/// edge's index in `entries`, and the next dependency on the same object.
struct Dependency {
    original: u32,
    entry: u32,
    gate: u8,
    /// One more than the index of the next dependency on `original`; zero
    /// ends the chain.
    next: u32,
}

/// Raw old-space indices never participate in the root census. All storage is
/// reserved before the heap flips, and first-copy notifications cannot allocate.
#[derive(Default)]
pub(super) struct EphemeronMarking {
    dependencies: Vec<Dependency>,
    /// Open-addressing table from an original index to its chain of
    /// dependencies: one more than the index of the chain's first
    /// dependency, or zero for an empty bucket. Its length is a power of two
    /// at least twice the number of dependencies, so a first copy of an object
    /// no edge depends on usually costs one probe.
    buckets: Vec<u32>,
    /// Each conditional edge and its open gates.
    entries: Vec<(ConditionalEdge, u8)>,
    ready: Vec<ConditionalEdge>,
    pub(super) arrays: WeakArrays,
}

/// The bucket an original index starts probing from. Objects are `ALIGN`
/// aligned, so the low bits carry no information.
#[inline]
fn bucket(original: u32, mask: usize) -> usize {
    ((original >> super::ALIGN.trailing_zeros()).wrapping_mul(0x9e37_79b1) as usize) & mask
}

impl EphemeronMarking {
    /// Forget all collection-local indices and discard allocations above the
    /// retention budget. This must also work after an interrupted preparation.
    pub(super) fn reset(&mut self) {
        reset_buffer(&mut self.dependencies);
        reset_buffer(&mut self.buckets);
        reset_buffer(&mut self.entries);
        reset_buffer(&mut self.ready);
        self.arrays.reset();
    }

    /// `live(raw)` says which original indices are live before anything is
    /// copied: none for a full collection, the old objects for a minor one.
    /// A gate whose object is already live opens at once.
    /// Only the registry slots `candidates` yields and the prepared array
    /// pairs are considered.
    pub(super) fn prepare(
        &mut self,
        registry: &WeakGcRegistry,
        candidates: impl ExactSizeIterator<Item = usize>,
        live: impl Fn(u32) -> bool,
        read_pair: impl Fn(WeakArrayEntry) -> Result<[u32; 2]>,
    ) -> Result<()> {
        // Storage for every candidate, whether or not it is a conditional
        // edge, so that no notification allocates.
        let overflow = || format_err!("ephemeron dependency count overflow");
        let count = candidates
            .len()
            .checked_add(self.arrays.entries.len())
            .ok_or_else(overflow)?;
        if count == 0 {
            return Ok(());
        }
        let dependencies = count.checked_mul(2).ok_or_else(overflow)?;
        if u32::try_from(dependencies).is_err() {
            return Err(overflow());
        }
        self.dependencies.try_reserve_exact(dependencies)?;
        self.entries.try_reserve_exact(count)?;
        self.ready.try_reserve_exact(count)?;
        for slot in candidates {
            if let Some(WeakGcEdge::Ephemeron {
                owner,
                key: Some(key),
                value: Some(_),
            }) = registry.slots[slot].edge.as_ref()
            {
                self.add_edge(
                    ConditionalEdge::Registry(slot as u32),
                    owner.as_raw_u32(),
                    key.as_raw_u32(),
                    &live,
                );
            }
        }
        for index in 0..self.arrays.entries.len() {
            let slot = self.arrays.entries[index];
            let [key, value] = read_pair(slot)?;
            if key != 0 && value != 0 && !live(value) {
                self.add_edge(ConditionalEdge::Array(index as u32), slot.array, key, &live);
            }
        }
        if self.dependencies.is_empty() {
            // Every gate is already open.
            return Ok(());
        }
        let buckets = self
            .dependencies
            .len()
            .checked_mul(2)
            .and_then(usize::checked_next_power_of_two)
            .ok_or_else(overflow)?;
        self.buckets.try_reserve_exact(buckets)?;
        self.buckets.resize(buckets, 0);
        let mask = buckets - 1;
        for index in 0..self.dependencies.len() {
            let original = self.dependencies[index].original;
            let mut at = bucket(original, mask);
            // Fewer dependencies than buckets: an empty bucket always exists.
            loop {
                let head = self.buckets[at];
                if head == 0 {
                    self.buckets[at] = index as u32 + 1;
                    break;
                }
                if self.dependencies[head as usize - 1].original == original {
                    self.dependencies[index].next = head;
                    self.buckets[at] = index as u32 + 1;
                    break;
                }
                at = (at + 1) & mask;
            }
        }
        Ok(())
    }

    fn add_edge(&mut self, edge: ConditionalEdge, owner: u32, key: u32, live: impl Fn(u32) -> bool) {
        // The reserved dependency capacity bounds the index to u32.
        let entry = self.entries.len() as u32;
        let mut gates = 0;
        for (original, gate) in [(owner, 1), (key, 2)] {
            if live(original) {
                gates |= gate;
            } else {
                self.dependencies.push(Dependency {
                    original,
                    entry,
                    gate,
                    next: 0,
                });
            }
        }
        self.entries.push((edge, gates));
        if gates == 3 {
            self.ready.push(edge);
        }
    }

    /// Called exactly when an original-space object is first copied. Equal
    /// owner/key identities satisfy both gates, while duplicate slots remain
    /// independent. Each conditional edge enters the queue at most once.
    #[inline]
    pub(super) fn copied(&mut self, original: &VMGcRef) {
        if self.buckets.is_empty() {
            return;
        }
        let raw = original.as_raw_u32();
        let mask = self.buckets.len() - 1;
        let mut at = bucket(raw, mask);
        let mut link = loop {
            let head = self.buckets[at];
            if head == 0 {
                return;
            }
            if self.dependencies[head as usize - 1].original == raw {
                break head;
            }
            at = (at + 1) & mask;
        };
        while link != 0 {
            let dependency = &self.dependencies[link as usize - 1];
            link = dependency.next;
            let (slot, gates) = &mut self.entries[dependency.entry as usize];
            let previous = *gates;
            *gates |= dependency.gate;
            if previous != 3 && *gates == 3 {
                debug_assert!(self.ready.len() < self.ready.capacity());
                self.ready.push(*slot);
            }
        }
    }

    pub(super) fn pop(&mut self) -> Option<ConditionalEdge> {
        self.ready.pop()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ready.is_empty()
    }
}
