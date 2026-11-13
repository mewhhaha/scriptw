//! Epoch-scoped non-rooting lookup for one native weak collection resource.
use super::*;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use wasmtime::{AsContext, GcWeakHeapEpoch, GcWeakKeyFingerprint};

/// A fingerprint hashes as one `write_u32` of its heap index. Indices are
/// collector offsets, not caller-supplied bytes, so a keyed SipHash bought
/// nothing per lookup; the murmur3 finalizer is a bijection on `u64` that
/// spreads the aligned indices over hashbrown's bucket and control bits.
#[derive(Default)]
struct HeapIndexHasher(u64);

impl Hasher for HeapIndexHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(byte);
        }
    }

    fn write_u32(&mut self, index: u32) {
        self.0 = self.0.rotate_left(32) ^ u64::from(index);
    }

    fn finish(&self) -> u64 {
        let mut hash = self.0;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        hash ^ (hash >> 33)
    }
}

type Table = HashMap<GcWeakKeyFingerprint, GcEphemeron, BuildHasherDefault<HeapIndexHasher>>;

#[derive(Default)]
pub(super) struct Entries {
    entries: Table,
    epoch: Option<GcWeakHeapEpoch>,
}

/// A key's token in the current epoch and its entry, if the collection has one.
/// Valid only until the Store can next collect.
pub(super) struct Lookup {
    fingerprint: GcWeakKeyFingerprint,
    pub(super) entry: Option<GcEphemeron>,
}

impl Entries {
    /// Finds `key` under a shared Store borrow, which cannot collect, so no
    /// operand is rooted or upgraded here. A hit is confirmed by the
    /// collector's identity check before it is returned.
    pub(super) fn lookup(
        &mut self,
        store: impl AsContext,
        key: &Rooted<AnyRef>,
    ) -> wasmtime::Result<Lookup> {
        let store = store.as_context();
        let wanted = GcWeakKeyFingerprint::new(&store, key)?;
        if self.epoch != Some(wanted.epoch()) {
            // GC updates the registry's weak references before the mutator
            // resumes. Rebuild once from surviving handles, transactionally:
            // allocation/validation failure preserves the existing table.
            let mut next = Table::default();
            next.try_reserve(self.entries.len())
                .map_err(wasmtime::Error::new)?;
            for entry in self.entries.values() {
                if let Some(current) = entry.key_fingerprint(&store)? {
                    if next.insert(current, *entry).is_some() {
                        return Err(wasmtime::Error::msg("duplicate weak collection key"));
                    }
                }
            }
            self.entries = next;
            self.epoch = Some(wanted.epoch());
        }
        let entry = self.entries.get(&wanted).copied();
        if let Some(entry) = entry {
            if !entry.matches_fingerprint(&store, wanted)? {
                return Err(wasmtime::Error::msg(
                    "weak collection key index identity mismatch",
                ));
            }
        }
        Ok(Lookup {
            fingerprint: wanted,
            entry,
        })
    }

    pub(super) fn insert(
        &mut self,
        caller: &mut WasmtimeCaller<'_, WasmHostState>,
        owner: &Rooted<AnyRef>,
        key: &Rooted<AnyRef>,
        value: Option<&Rooted<AnyRef>>,
    ) -> wasmtime::Result<()> {
        let found = self.lookup(&*caller, key)?;
        if let Some(entry) = found.entry {
            return entry.set_value(&mut *caller, value.map(|value| &**value));
        }
        // Reserve before registering a conditional edge, so allocation cannot
        // orphan a value-retaining collector slot. Registration cannot collect,
        // so the token taken by the lookup stays current.
        self.entries.try_reserve(1).map_err(wasmtime::Error::new)?;
        let entry = GcEphemeron::new(&mut *caller, owner, key, value.map(|value| &**value))?;
        let previous = self.entries.insert(found.fingerprint, entry);
        debug_assert!(previous.is_none());
        Ok(())
    }

    /// Removes the entry a lookup found. Removal cannot collect, so the
    /// lookup's token is still the table key.
    pub(super) fn remove(
        &mut self,
        caller: &mut WasmtimeCaller<'_, WasmHostState>,
        found: Lookup,
    ) -> wasmtime::Result<()> {
        if let Some(entry) = found.entry {
            entry.remove(&mut *caller)?;
            self.entries.remove(&found.fingerprint);
        }
        Ok(())
    }
}
