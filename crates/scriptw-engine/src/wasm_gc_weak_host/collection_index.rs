//! One Store's weak collection entries, found by stable identities.
//!
//! An entry is a collector ephemeron owned by its WeakMap or WeakSet. The
//! index only finds it: it holds no reference, keeps nothing alive and needs
//! no rebuilding when objects move. Compiled code mints one stable nonzero
//! identity per object or Symbol (`KEY_HASH_ID`) before it stores an entry;
//! the pair of identities is the hash, and the collector's own owner and key
//! decide a hit, so two objects that share an identity are still two keys.
use super::*;
use wasmtime::AsContext;

/// One operation's collection and key: the raw references of the running
/// host call, and the hash of their identities.
#[derive(Clone, Copy)]
pub(super) struct Operands {
    owner: u32,
    key: u32,
    hash: u64,
}

impl Operands {
    /// Compiled code answers a lookup itself when either object has no
    /// identity yet, and mints both before it stores.
    pub(super) fn new(owner: u32, key: u32, owner_id: i64, key_id: i64) -> wasmtime::Result<Self> {
        if owner_id == 0 || key_id == 0 {
            return Err(wasmtime::Error::msg("weak operand has no identity"));
        }
        // The murmur3 finalizer over both identities: they are small
        // counters, and the low bits pick the cell.
        let mut hash = (owner_id as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .rotate_left(32)
            ^ key_id as u64;
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        Ok(Self {
            owner,
            key,
            hash: hash ^ (hash >> 33),
        })
    }
}

#[derive(Clone, Copy, Default)]
struct Cell {
    hash: u64,
    /// `None` is an empty cell. An entry stays after the collector retires
    /// it (its owner or key died, or it was deleted) until the next rebuild:
    /// a retired handle matches nothing, so it only lengthens a probe.
    entry: Option<GcEphemeron>,
}

/// Where a missing key's entry would go.
#[derive(Clone, Copy)]
pub(super) enum Vacancy {
    /// The table has no cells yet.
    Unallocated,
    Empty(usize),
    /// A retired entry of the same hash, replaced in place.
    Retired(usize),
}

/// A key's entry, or where one would go. Valid only until the Store can next
/// collect or the index next changes.
pub(super) enum Found {
    Entry(GcEphemeron),
    Missing(Vacancy),
}

impl Found {
    pub(super) fn entry(self) -> Option<GcEphemeron> {
        match self {
            Self::Entry(entry) => Some(entry),
            Self::Missing(_) => None,
        }
    }
}

/// Open addressing with linear probing over a power-of-two table that always
/// has an empty cell.
#[derive(Default)]
pub(super) struct Entries {
    cells: Vec<Cell>,
    /// Occupied cells, live or retired.
    used: usize,
}

const MINIMUM_CELLS: usize = 16;

impl Entries {
    /// Finds the entry under a shared Store borrow, which cannot collect, so
    /// the raw references stay the objects' identities throughout.
    pub(super) fn find(&self, store: impl AsContext, at: Operands) -> wasmtime::Result<Found> {
        let store = store.as_context();
        let Some(mask) = self.cells.len().checked_sub(1) else {
            return Ok(Found::Missing(Vacancy::Unallocated));
        };
        let mut index = at.hash as usize & mask;
        let mut retired = None;
        loop {
            let cell = self.cells[index];
            let Some(entry) = cell.entry else {
                return Ok(Found::Missing(
                    retired.map_or(Vacancy::Empty(index), Vacancy::Retired),
                ));
            };
            if cell.hash == at.hash {
                if entry.matches_raw(&store, at.owner, at.key)? {
                    return Ok(Found::Entry(entry));
                }
                if retired.is_none() && !entry.is_live(&store)? {
                    retired = Some(index);
                }
            }
            index = (index + 1) & mask;
        }
    }

    /// Whether one more cell may be occupied: at most five eighths are.
    fn has_room(&self) -> bool {
        (self.used + 1) * 8 <= self.cells.len() * 5
    }

    /// Moves the live entries to a fresh table, which drops the retired
    /// ones. The table doubles when more than three eighths of its cells
    /// hold a live entry, so at least a quarter of them can be filled before
    /// the next rebuild, and gives up half its cells when fewer than a
    /// sixteenth do: entries that die together at each collection then keep
    /// one capacity instead of shrinking and growing again. Nothing changes
    /// when an allocation fails.
    fn rebuild(&mut self, store: impl AsContext) -> wasmtime::Result<()> {
        let store = store.as_context();
        let mut live = Vec::new();
        live.try_reserve_exact(self.used)
            .map_err(wasmtime::Error::new)?;
        for cell in &self.cells {
            if cell.entry.map(|entry| entry.is_live(&store)).transpose()? == Some(true) {
                live.push(*cell);
            }
        }
        let needed = live
            .len()
            .checked_add(1)
            .and_then(|cells| cells.checked_mul(2))
            .and_then(usize::checked_next_power_of_two)
            .ok_or_else(|| wasmtime::Error::msg("weak collection index is too large"))?
            .max(MINIMUM_CELLS);
        let current = self.cells.len();
        let capacity = if live.len() < current / 16 {
            needed.max(current / 2)
        } else if live.len() > current / 8 * 3 {
            needed.max(current.saturating_mul(2))
        } else {
            needed.max(current)
        };
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(capacity)
            .map_err(wasmtime::Error::new)?;
        cells.resize(capacity, Cell::default());
        for cell in &live {
            let empty = Self::first_empty(&cells, cell.hash);
            cells[empty] = *cell;
        }
        self.cells = cells;
        self.used = live.len();
        Ok(())
    }

    fn first_empty(cells: &[Cell], hash: u64) -> usize {
        let mask = cells.len() - 1;
        let mut index = hash as usize & mask;
        while cells[index].entry.is_some() {
            index = (index + 1) & mask;
        }
        index
    }

    /// Stores `value` under the operands' key, replacing the value of an
    /// existing entry. A zero `value` is a WeakSet's membership.
    ///
    /// # Safety
    ///
    /// The operands and a nonzero `value` are references that compiled code
    /// passed to the host call now running in `caller`'s Store, which has not
    /// collected since.
    pub(super) unsafe fn put(
        caller: &mut WasmtimeCaller<'_, WasmHostState>,
        at: Operands,
        value: u32,
    ) -> wasmtime::Result<()> {
        let vacancy = match caller.data().weak.index.find(&*caller, at)? {
            // SAFETY: the caller's contract; nothing above collects.
            Found::Entry(entry) => return unsafe { entry.set_value_raw(&mut *caller, value) },
            Found::Missing(vacancy) => vacancy,
        };
        // Make room before registering the conditional edge, so a failed
        // allocation cannot leave a value-retaining slot the index does not
        // know. Registration cannot collect, so the references stay current.
        let cell = match vacancy {
            Vacancy::Retired(cell) => cell,
            Vacancy::Empty(cell) if caller.data().weak.index.has_room() => cell,
            Vacancy::Empty(_) | Vacancy::Unallocated => {
                let mut index = std::mem::take(&mut caller.data_mut().weak.index);
                let rebuilt = index.rebuild(&*caller);
                caller.data_mut().weak.index = index;
                rebuilt?;
                Self::first_empty(&caller.data().weak.index.cells, at.hash)
            }
        };
        // SAFETY: the caller's contract; nothing above collects.
        let entry = unsafe { GcEphemeron::new_raw(&mut *caller, at.owner, at.key, value)? };
        let index = &mut caller.data_mut().weak.index;
        index.used += usize::from(index.cells[cell].entry.is_none());
        index.cells[cell] = Cell {
            hash: at.hash,
            entry: Some(entry),
        };
        Ok(())
    }
}
