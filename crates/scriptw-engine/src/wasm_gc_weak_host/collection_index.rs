//! One Store's weak collection entries, found by stable identities.
//!
//! An entry is a collector ephemeron owned by its WeakSet. The
//! index only finds it: it holds no reference, keeps nothing alive and needs
//! no rebuilding when objects move. Compiled code mints one stable nonzero
//! identity per object or Symbol (`KEY_HASH_ID`) before it stores an entry;
//! the pair of identities is the hash, and the collector's own owner and key
//! decide a hit, so two objects that share an identity are still two keys.
use super::*;
use wasmtime::{GcEphemeronUpdate, RawGcEphemerons};

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
    #[inline]
    pub(super) fn new(owner: u32, key: u32, owner_id: i64, key_id: i64) -> wasmtime::Result<Self> {
        if owner_id == 0 || key_id == 0 {
            return Err(wasmtime::Error::msg("weak operand has no identity"));
        }
        // Mix the two allocation counters, then fold the high bits of the
        // product into the low bits that select the cell.
        let pair = (owner_id as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .rotate_left(32)
            ^ key_id as u64;
        let hash = (pair ^ (pair >> 32)).wrapping_mul(0xd6e8_feb8_6659_fd93);
        Ok(Self {
            owner,
            key,
            hash: hash ^ (hash >> 32),
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
enum Vacancy {
    /// The table has no cells yet.
    Unallocated,
    Empty(usize),
    /// A retired entry of the same hash, replaced in place.
    Retired(usize),
}

enum Candidate<T> {
    Match(T),
    Mismatch,
    Retired,
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
    /// Inspects entries with the same hash. Only insertion needs to track
    /// retired cells and record a vacancy.
    #[inline]
    fn probe<T>(
        &self,
        at: Operands,
        vacancy: Option<&mut Vacancy>,
        mut inspect: impl FnMut(GcEphemeron) -> wasmtime::Result<Candidate<T>>,
    ) -> wasmtime::Result<Option<T>> {
        let Some(mask) = self.cells.len().checked_sub(1) else {
            return Ok(None);
        };
        let first = at.hash as usize & mask;
        let (before, after) = self.cells.split_at(first);
        let mut retired = None;
        for (offset, cell) in after.iter().chain(before).enumerate() {
            let Some(entry) = cell.entry else {
                if let Some(vacancy) = vacancy {
                    let empty = (first + offset) & mask;
                    *vacancy = retired.map_or(Vacancy::Empty(empty), Vacancy::Retired);
                }
                return Ok(None);
            };
            if cell.hash == at.hash {
                match inspect(entry)? {
                    Candidate::Match(value) => return Ok(Some(value)),
                    Candidate::Mismatch => {}
                    Candidate::Retired => {
                        retired.get_or_insert((first + offset) & mask);
                    }
                }
            }
        }
        unreachable!("the weak collection index always has an empty cell")
    }

    pub(super) fn find(
        &self,
        store: &WasmtimeCaller<'_, WasmHostState>,
        at: Operands,
    ) -> wasmtime::Result<Option<GcEphemeron>> {
        self.probe(at, None, |entry| {
            Ok(if entry.matches_raw(store, at.owner, at.key)? {
                Candidate::Match(entry)
            } else {
                Candidate::Mismatch
            })
        })
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
    fn rebuild(&mut self, ephemerons: &RawGcEphemerons<'_>) -> wasmtime::Result<()> {
        let mut live = Vec::new();
        live.try_reserve_exact(self.used)
            .map_err(wasmtime::Error::new)?;
        for cell in &self.cells {
            let Some(entry) = cell.entry else {
                continue;
            };
            if ephemerons.is_live(entry)? {
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
        GcEphemeron::with_raw_access(caller, |state, ephemerons| {
            let index = &mut state.weak.index;
            let mut vacancy = Vacancy::Unallocated;
            let updated = index.probe(at, Some(&mut vacancy), |entry| {
                // SAFETY: the caller's contract; the raw access cannot collect.
                let result =
                    unsafe { ephemerons.set_value_if_matches_raw(entry, at.owner, at.key, value)? };
                Ok(match result {
                    GcEphemeronUpdate::Updated => Candidate::Match(()),
                    GcEphemeronUpdate::Mismatch => Candidate::Mismatch,
                    GcEphemeronUpdate::Retired => Candidate::Retired,
                })
            })?;
            if updated.is_some() {
                return Ok(());
            }

            // Prepare a cell before registering the edge, so a failed allocation
            // cannot leave a value-retaining slot outside the index.
            let cell = match vacancy {
                Vacancy::Retired(cell) => cell,
                Vacancy::Empty(cell) if index.has_room() => cell,
                Vacancy::Empty(_) | Vacancy::Unallocated => {
                    index.rebuild(ephemerons)?;
                    Self::first_empty(&index.cells, at.hash)
                }
            };
            // SAFETY: the caller's contract; the raw access cannot collect.
            let entry = unsafe { ephemerons.insert_raw(at.owner, at.key, value)? };
            index.used += usize::from(index.cells[cell].entry.is_none());
            index.cells[cell] = Cell {
                hash: at.hash,
                entry: Some(entry),
            };
            Ok(())
        })
    }
}
