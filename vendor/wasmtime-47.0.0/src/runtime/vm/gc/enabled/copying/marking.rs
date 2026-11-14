//! Collection-local incidence of independently live owners and keys.
use crate::prelude::*;
use crate::vm::{VMGcRef, WeakGcEdge, WeakGcRegistry};

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
    /// Each conditional edge's registry slot and its open gates.
    entries: Vec<(usize, u8)>,
    ready: Vec<usize>,
}

/// The bucket an original index starts probing from. Objects are `ALIGN`
/// aligned, so the low bits carry no information.
#[inline]
fn bucket(original: u32, mask: usize) -> usize {
    ((original >> super::ALIGN.trailing_zeros()).wrapping_mul(0x9e37_79b1) as usize) & mask
}

impl EphemeronMarking {
    /// `live(raw)` says which original indices are live before anything is
    /// copied: none for a full collection, the old objects for a minor one.
    /// A gate whose object is already live opens at once.
    /// Only the registry slots `candidates` yields are considered.
    pub(super) fn new(
        registry: &WeakGcRegistry,
        candidates: impl ExactSizeIterator<Item = usize>,
        live: impl Fn(u32) -> bool,
    ) -> Result<Self> {
        // Storage for every candidate, whether or not it is a conditional
        // edge, so that no notification allocates.
        let count = candidates.len();
        if count == 0 {
            return Ok(Self::default());
        }
        let overflow = || format_err!("ephemeron dependency count overflow");
        let mut marking = Self::default();
        let dependencies = count.checked_mul(2).ok_or_else(overflow)?;
        if u32::try_from(dependencies).is_err() {
            return Err(overflow());
        }
        marking.dependencies.try_reserve_exact(dependencies)?;
        marking.entries.try_reserve_exact(count)?;
        marking.ready.try_reserve_exact(count)?;
        for slot in candidates {
            if let Some(WeakGcEdge::Ephemeron {
                owner,
                key: Some(key),
                value: Some(_),
            }) = registry.slots[slot].edge.as_ref()
            {
                // `entries` is no longer than `dependencies`, which fits a
                // `u32`.
                let entry = marking.entries.len() as u32;
                let mut gates = 0;
                for (original, gate) in [(owner.as_raw_u32(), 1), (key.as_raw_u32(), 2)] {
                    if live(original) {
                        gates |= gate;
                    } else {
                        marking.dependencies.push(Dependency {
                            original,
                            entry,
                            gate,
                            next: 0,
                        });
                    }
                }
                marking.entries.push((slot, gates));
                if gates == 3 {
                    marking.ready.push(slot);
                }
            }
        }
        if marking.dependencies.is_empty() {
            // Every gate is already open.
            return Ok(marking);
        }
        let buckets = (marking.dependencies.len() * 2).next_power_of_two();
        marking.buckets.try_reserve_exact(buckets)?;
        marking.buckets.resize(buckets, 0);
        let mask = buckets - 1;
        for index in 0..marking.dependencies.len() {
            let original = marking.dependencies[index].original;
            let mut at = bucket(original, mask);
            // Fewer dependencies than buckets: an empty bucket always exists.
            loop {
                let head = marking.buckets[at];
                if head == 0 {
                    marking.buckets[at] = index as u32 + 1;
                    break;
                }
                if marking.dependencies[head as usize - 1].original == original {
                    marking.dependencies[index].next = head;
                    marking.buckets[at] = index as u32 + 1;
                    break;
                }
                at = (at + 1) & mask;
            }
        }
        Ok(marking)
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

    pub(super) fn pop(&mut self) -> Option<usize> {
        self.ready.pop()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ready.is_empty()
    }
}
