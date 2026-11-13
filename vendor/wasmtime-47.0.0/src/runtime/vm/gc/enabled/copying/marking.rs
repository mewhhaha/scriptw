//! Collection-local incidence of independently live owners and keys.
use crate::prelude::*;
use crate::vm::{VMGcRef, WeakGcEdge, WeakGcRegistry};

/// Original-space indices covered by one filter bit. Objects are `ALIGN`
/// aligned, so distinct objects never share a granule; the filter only has to
/// be conservative, never exact.
const GRANULE_SHIFT: u32 = super::ALIGN.trailing_zeros();

struct Dependency {
    original: u32,
    slot: usize,
    gate: u8,
}

/// Raw old-space indices never participate in the root census. All storage is
/// reserved before the heap flips, and first-copy notifications cannot allocate.
#[derive(Default)]
pub(super) struct EphemeronMarking {
    dependencies: Vec<Dependency>,
    live_gates: Vec<u8>,
    ready: Vec<usize>,
    /// First dependency index; granule `g` covers indices from
    /// `granule_base + (g << GRANULE_SHIFT)`.
    granule_base: u32,
    /// One bit per granule that holds some dependency's owner or key. Every
    /// first copy consults it before searching `dependencies`, so a copy of an
    /// object no edge depends on costs one bit test instead of a binary search.
    granules: Vec<u64>,
}

impl EphemeronMarking {
    pub(super) fn new(registry: &WeakGcRegistry) -> Result<Self> {
        let count = registry
            .slots
            .iter()
            .filter(|slot| {
                matches!(
                    slot.edge,
                    Some(WeakGcEdge::Ephemeron {
                        key: Some(_),
                        value: Some(_),
                        ..
                    })
                )
            })
            .count();
        if count == 0 {
            return Ok(Self::default());
        }
        let mut marking = Self::default();
        marking.dependencies.try_reserve_exact(
            count
                .checked_mul(2)
                .ok_or_else(|| format_err!("ephemeron dependency count overflow"))?,
        )?;
        marking.live_gates.try_reserve_exact(registry.slots.len())?;
        marking.live_gates.resize(registry.slots.len(), 0);
        marking.ready.try_reserve_exact(count)?;
        for (slot, entry) in registry.slots.iter().enumerate() {
            if let Some(WeakGcEdge::Ephemeron {
                owner,
                key: Some(key),
                value: Some(_),
            }) = entry.edge.as_ref()
            {
                marking.dependencies.push(Dependency {
                    original: owner.as_raw_u32(),
                    slot,
                    gate: 1,
                });
                marking.dependencies.push(Dependency {
                    original: key.as_raw_u32(),
                    slot,
                    gate: 2,
                });
            }
        }
        marking
            .dependencies
            .sort_unstable_by_key(|dependency| dependency.original);
        let (Some(first), Some(last)) = (marking.dependencies.first(), marking.dependencies.last())
        else {
            unreachable!("a counted edge records two dependencies")
        };
        let base = first.original;
        let span = usize::try_from((last.original - base) >> GRANULE_SHIFT)? + 1;
        marking.granules.try_reserve_exact(span.div_ceil(64))?;
        marking.granules.resize(span.div_ceil(64), 0);
        for dependency in &marking.dependencies {
            let granule = ((dependency.original - base) >> GRANULE_SHIFT) as usize;
            marking.granules[granule / 64] |= 1 << (granule % 64);
        }
        marking.granule_base = base;
        Ok(marking)
    }

    /// False only for an index that no dependency records. Indices below the
    /// base wrap far past the filter's end.
    #[inline]
    fn may_depend(&self, raw: u32) -> bool {
        let granule = (raw.wrapping_sub(self.granule_base) >> GRANULE_SHIFT) as usize;
        self.granules
            .get(granule / 64)
            .is_some_and(|word| word & (1 << (granule % 64)) != 0)
    }

    /// Called exactly when an original-space object is first copied. Equal
    /// owner/key identities satisfy both gates, while duplicate slots remain
    /// independent. Each conditional edge enters the queue at most once.
    #[inline]
    pub(super) fn copied(&mut self, original: &VMGcRef) {
        let raw = original.as_raw_u32();
        if !self.may_depend(raw) {
            return;
        }
        let first = self
            .dependencies
            .partition_point(|dependency| dependency.original < raw);
        for dependency in self.dependencies[first..]
            .iter()
            .take_while(|dependency| dependency.original == raw)
        {
            let gates = &mut self.live_gates[dependency.slot];
            let previous = *gates;
            *gates |= dependency.gate;
            if previous != 3 && *gates == 3 {
                debug_assert!(self.ready.len() < self.ready.capacity());
                self.ready.push(dependency.slot);
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
