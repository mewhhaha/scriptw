//! Immutable compiler recipes with isolated, copy-on-write lowering facts.

use std::collections::{btree_map, BTreeMap, BTreeSet};
use std::iter::Peekable;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub(super) struct CatalogOverlay<V: 'static> {
    baseline: Option<&'static BTreeMap<String, V>>,
    changes: BTreeMap<String, Option<Arc<V>>>,
    /// The last whole-key-set application of each idempotent edit and the
    /// entries changed since. Every mutable access records its key here, so
    /// an entry can never be treated as settled after it changed.
    settled: Vec<SettledEdit>,
    /// Opt-in record of which recipe rows any state of this overlay, or of an
    /// overlay forked, joined or restored from it, ever held differently.
    deviations: Option<DeviationJournal<V>>,
}

/// The recipe keys whose effective row differed from the recipe row in at
/// least one state of a family of overlays.
///
/// A flow analysis forks, joins, restores and discards fact overlays at every
/// branch, return and loop, so no single surviving overlay can say whether a
/// recipe fact held at *every* analysed program point. This journal is shared
/// by every overlay cloned from the one that started it and is fed from the
/// only place a row can change: each mutable access queues its key, and the
/// row is compared with the recipe before the next mutable access, when the
/// overlay is dropped, and when the journal is read. Every state an analysed
/// program point can observe is therefore compared.
#[derive(Debug, Clone)]
struct DeviationJournal<V: 'static> {
    recipe: &'static BTreeMap<String, V>,
    differs: fn(Option<&V>, Option<&V>) -> bool,
    deviated: Arc<Mutex<BTreeSet<String>>>,
    /// Recipe keys accessed mutably since this overlay last compared them.
    unchecked: BTreeSet<String>,
}

impl<V: 'static> DeviationJournal<V> {
    /// The same journal for a new overlay with no queued comparisons.
    fn fork_empty(&self) -> Self {
        Self {
            recipe: self.recipe,
            differs: self.differs,
            deviated: Arc::clone(&self.deviated),
            unchecked: BTreeSet::new(),
        }
    }

    fn deviated(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        // A poisoned set is still append-only and complete up to the panic.
        self.deviated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// An edit of each entry of a key set which is idempotent and reads or writes
/// no other entry: applying it again to an entry that has not been mutated
/// since leaves the entry unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdempotentEdit {
    /// Merge an unknown receiver and unknown arguments into a signature, as
    /// an unknown property hook may call any planned source function.
    UnknownPropertyHook,
    /// Discard the heap shape of every entry that has one.
    DiscardHeapShape,
    /// Stop proving presence of every configurable entry proven present.
    ForgetConfigurablePresence,
}

#[derive(Debug, Clone)]
enum SettledKeys {
    Listed(Arc<BTreeSet<String>>),
    /// Every key, including keys inserted later.
    All,
}

impl SettledKeys {
    fn contains(&self, key: &str) -> bool {
        match self {
            Self::Listed(keys) => keys.contains(key),
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone)]
struct SettledEdit {
    edit: IdempotentEdit,
    keys: SettledKeys,
    /// Keys of `keys` accessed mutably since `edit` was applied to all keys.
    touched: BTreeSet<String>,
}

impl<V: 'static> Default for CatalogOverlay<V> {
    fn default() -> Self {
        Self {
            baseline: None,
            changes: BTreeMap::new(),
            settled: Vec::new(),
            deviations: None,
        }
    }
}

impl<V: 'static> Drop for CatalogOverlay<V> {
    /// A discarded branch or callee state is still a state some analysed
    /// program point observed.
    fn drop(&mut self) {
        self.compare_queued_rows();
    }
}

impl<V: 'static> CatalogOverlay<V> {
    pub(super) fn new(baseline: &'static BTreeMap<String, V>) -> Self {
        Self {
            baseline: Some(baseline),
            changes: BTreeMap::new(),
            settled: Vec::new(),
            deviations: None,
        }
    }

    /// Journal every queued recipe key whose current row is not its recipe
    /// row. Runs before each mutable access, so the compared state is the one
    /// the previous mutation left.
    fn compare_queued_rows(&mut self) {
        let queued = match &mut self.deviations {
            Some(journal) if !journal.unchecked.is_empty() => {
                std::mem::take(&mut journal.unchecked)
            }
            _ => return,
        };
        let journal = self
            .deviations
            .as_ref()
            .expect("queued rows belong to a journal");
        let deviating = queued
            .into_iter()
            .filter(|key| (journal.differs)(self.get(key), journal.recipe.get(key)))
            .collect::<Vec<_>>();
        if !deviating.is_empty() {
            journal.deviated().extend(deviating);
        }
    }

    fn touch(&mut self, key: &str) {
        self.compare_queued_rows();
        if let Some(journal) = &mut self.deviations {
            if journal.recipe.contains_key(key) && !journal.deviated().contains(key) {
                journal.unchecked.insert(key.to_string());
            }
        }
        for settled in &mut self.settled {
            if settled.keys.contains(key) && !settled.touched.contains(key) {
                settled.touched.insert(key.to_string());
            }
        }
    }

    fn settled_edit(&self, edit: IdempotentEdit) -> Option<&SettledEdit> {
        self.settled.iter().find(|settled| settled.edit == edit)
    }

    fn record_settled(&mut self, edit: IdempotentEdit, keys: SettledKeys) {
        self.settled.retain(|settled| settled.edit != edit);
        self.settled.push(SettledEdit {
            edit,
            keys,
            touched: BTreeSet::new(),
        });
    }

    /// The keys `select` picks for a whole-catalog `edit`, in key order. Once
    /// `edit` was applied to every selected entry, only entries mutated since
    /// can be selected again: the edit leaves `select` false on the entry,
    /// and both read only that entry.
    fn selected_keys(
        &self,
        edit: IdempotentEdit,
        mut select: impl FnMut(&String, &V) -> bool,
    ) -> Vec<String> {
        match self.settled_edit(edit) {
            Some(SettledEdit {
                keys: SettledKeys::All,
                touched,
                ..
            }) => touched
                .iter()
                .filter(|key| self.get(key).is_some_and(|value| select(key, value)))
                .cloned()
                .collect(),
            _ => self
                .iter()
                .filter_map(|(key, value)| select(key, value).then(|| key.clone()))
                .collect(),
        }
    }

    /// Replace every entry `select` picks with `replace` of it, for an
    /// [`IdempotentEdit`] that leaves `select` false. Each replacement is
    /// built from the current entry by reference, so a baseline or shared
    /// entry is not first copied whole when the edit discards most of it.
    pub(super) fn replace_selected(
        &mut self,
        edit: IdempotentEdit,
        select: impl FnMut(&String, &V) -> bool,
        mut replace: impl FnMut(&String, &V) -> V,
    ) {
        let replacements = self
            .selected_keys(edit, select)
            .into_iter()
            .map(|key| {
                let value = replace(
                    &key,
                    self.get(&key).expect("selected catalog entry must exist"),
                );
                (key, value)
            })
            .collect::<Vec<_>>();
        for (key, value) in replacements {
            self.insert(key, value);
        }
        self.record_settled(edit, SettledKeys::All);
    }

    /// The keys of `keys` on which `edit` may still change an entry, in key
    /// order: all of them unless `edit` was settled for this same key set,
    /// otherwise only the entries mutated since.
    pub(super) fn unsettled_keys(
        &self,
        edit: IdempotentEdit,
        keys: &Arc<BTreeSet<String>>,
    ) -> Vec<String> {
        match self.settled_edit(edit) {
            Some(SettledEdit {
                keys: SettledKeys::Listed(settled),
                touched,
                ..
            }) if Arc::ptr_eq(settled, keys) => touched.iter().cloned().collect(),
            _ => keys.iter().cloned().collect(),
        }
    }

    /// Whether `edit` was applied to the entry for `key` as part of the
    /// settled key set and the entry was not mutated since, so applying it
    /// again would leave the entry unchanged.
    pub(super) fn is_settled(&self, edit: IdempotentEdit, key: &str) -> bool {
        self.settled_edit(edit)
            .is_some_and(|settled| settled.keys.contains(key) && !settled.touched.contains(key))
    }

    /// Record that `edit` was just applied to every key that
    /// [`Self::unsettled_keys`] returned for `keys`, with no other mutation
    /// in between.
    pub(super) fn settle(&mut self, edit: IdempotentEdit, keys: Arc<BTreeSet<String>>) {
        self.record_settled(edit, SettledKeys::Listed(keys));
    }

    pub(super) fn get(&self, key: &str) -> Option<&V> {
        match self.changes.get(key) {
            Some(value) => value.as_deref(),
            None => self.baseline.and_then(|baseline| baseline.get(key)),
        }
    }

    /// The row this overlay owns for `key`, ignoring a baseline row it has not
    /// changed. A caller skipping a write which would equal the current row
    /// must also know that the write would not first copy a baseline row.
    pub(super) fn owned_change(&self, key: &str) -> Option<&V> {
        self.changes.get(key).and_then(Option::as_deref)
    }

    /// The shared row this overlay owns for `key`. Overlays forked from one
    /// another hold the same row until either changes it.
    pub(super) fn shared_change(&self, key: &str) -> Option<&Arc<V>> {
        self.changes.get(key).and_then(Option::as_ref)
    }

    /// Replace a fact without materializing the discarded previous value.
    /// Forks retain their own immutable row; no Clone bound is needed here.
    pub(super) fn insert(&mut self, key: String, value: V) {
        self.touch(&key);
        self.changes.insert(key, Some(Arc::new(value)));
    }

    pub(super) fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub(super) fn iter(&self) -> CatalogIter<'_, V> {
        CatalogIter {
            baseline: self.baseline.map(|baseline| baseline.iter().peekable()),
            changes: self.changes.iter().peekable(),
        }
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &String> {
        self.iter().map(|(key, _)| key)
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, value)| value)
    }

    pub(super) fn changed_keys(&self) -> impl Iterator<Item = &String> {
        self.changes.keys()
    }

    pub(super) fn same_baseline(&self, other: &Self) -> bool {
        match (self.baseline, other.baseline) {
            (Some(left), Some(right)) => std::ptr::eq(left, right),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }

    pub(super) fn empty_changes(&self) -> Self {
        Self {
            baseline: self.baseline,
            changes: BTreeMap::new(),
            settled: Vec::new(),
            deviations: self.deviations.as_ref().map(DeviationJournal::fork_empty),
        }
    }

    /// Start journaling which recipe rows this overlay, and every overlay
    /// later cloned, joined or restored from it, ever holds differently from
    /// the recipe. `differs` decides whether a row means something other
    /// than its recipe row; it may treat two encodings of one fact as equal.
    /// Read the journal with [`Self::recipe_deviations`].
    pub(super) fn record_recipe_deviations(&mut self, differs: fn(Option<&V>, Option<&V>) -> bool) {
        let recipe = self
            .baseline
            .expect("only an overlay of a recipe catalog can compare rows with it");
        assert!(
            self.changes.is_empty() && self.deviations.is_none(),
            "a deviation journal must start from the unchanged recipe"
        );
        self.deviations = Some(DeviationJournal {
            recipe,
            differs,
            deviated: Arc::default(),
            unchecked: BTreeSet::new(),
        });
    }

    pub(super) fn records_recipe_deviations(&self) -> bool {
        self.deviations.is_some()
    }

    /// An empty overlay with no recipe that still feeds this overlay's
    /// deviation journal, for a join whose inputs do not share a recipe.
    pub(super) fn empty_without_baseline(&self) -> Self {
        Self {
            baseline: None,
            changes: BTreeMap::new(),
            settled: Vec::new(),
            deviations: self.deviations.as_ref().map(DeviationJournal::fork_empty),
        }
    }

    /// Every recipe key whose effective row differed from the recipe row in
    /// some state of this overlay or of any overlay cloned from it since
    /// [`Self::record_recipe_deviations`].
    pub(super) fn recipe_deviations(&mut self) -> BTreeSet<String> {
        self.compare_queued_rows();
        self.deviations
            .as_ref()
            .expect("recipe deviations are read only from an overlay that records them")
            .deviated()
            .clone()
    }
}

impl<V: Clone + 'static> CatalogOverlay<V> {
    pub(super) fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.touch(key);
        if !self.changes.contains_key(key) {
            let (key, value) = self.baseline?.get_key_value(key)?;
            self.changes
                .insert(key.clone(), Some(Arc::new(value.clone())));
        }
        self.changes
            .get_mut(key)
            .and_then(Option::as_mut)
            .map(Arc::make_mut)
    }

    #[cfg(test)]
    pub(super) fn remove(&mut self, key: &str) -> Option<V> {
        let previous = self.get(key).cloned();
        self.touch(key);
        if self
            .baseline
            .is_some_and(|baseline| baseline.contains_key(key))
        {
            self.changes.insert(key.to_string(), None);
        } else {
            self.changes.remove(key);
        }
        previous
    }

    /// Test the effective immutable view first. Only selected entries become
    /// owned, and updates retain the same order as a BTreeMap traversal.
    pub(super) fn edit_matching(
        &mut self,
        mut select: impl FnMut(&String, &V) -> bool,
        mut edit: impl FnMut(&String, &mut V),
    ) {
        let keys = self
            .iter()
            .filter_map(|(key, value)| select(key, value).then(|| key.clone()))
            .collect::<Vec<_>>();
        for key in keys {
            edit(
                &key,
                self.get_mut(&key)
                    .expect("selected catalog entry must exist"),
            );
        }
    }

    /// `edit_matching` for an [`IdempotentEdit`] that leaves `select` false
    /// on each entry it edits. After the first pass only entries mutated
    /// since are tested again.
    pub(super) fn edit_selected(
        &mut self,
        edit_kind: IdempotentEdit,
        select: impl FnMut(&String, &V) -> bool,
        mut edit: impl FnMut(&String, &mut V),
    ) {
        for key in self.selected_keys(edit_kind, select) {
            edit(
                &key,
                self.get_mut(&key)
                    .expect("selected catalog entry must exist"),
            );
        }
        self.record_settled(edit_kind, SettledKeys::All);
    }
}

impl<V: PartialEq + 'static> CatalogOverlay<V> {
    /// [`Self::set_effective`] with a row this or another overlay already
    /// owns, kept shared instead of copied.
    pub(super) fn set_effective_shared(&mut self, key: String, value: Arc<V>) {
        self.touch(&key);
        if self.baseline.and_then(|baseline| baseline.get(&key)) == Some(&*value) {
            self.changes.remove(&key);
        } else {
            self.changes.insert(key, Some(value));
        }
    }

    pub(super) fn set_effective(&mut self, key: String, value: Option<V>) {
        self.touch(&key);
        if self.baseline.and_then(|baseline| baseline.get(&key)) == value.as_ref() {
            self.changes.remove(&key);
        } else {
            self.changes.insert(key, value.map(Arc::new));
        }
    }
}

impl<V: PartialEq + 'static> PartialEq for CatalogOverlay<V> {
    fn eq(&self, other: &Self) -> bool {
        if self.same_baseline(other) {
            self.changes
                .keys()
                .chain(other.changes.keys())
                .all(|key| self.get(key) == other.get(key))
        } else {
            self.iter().eq(other.iter())
        }
    }
}

impl<V: Eq + 'static> Eq for CatalogOverlay<V> {}

pub(super) struct CatalogIter<'a, V: 'a> {
    baseline: Option<Peekable<btree_map::Iter<'a, String, V>>>,
    changes: Peekable<btree_map::Iter<'a, String, Option<Arc<V>>>>,
}

impl<'a, V: 'a> Iterator for CatalogIter<'a, V> {
    type Item = (&'a String, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let baseline = self.baseline.as_mut().and_then(|values| values.peek());
            let change = self.changes.peek();
            let take_baseline = match (baseline, change) {
                (Some((left, _)), Some((right, _))) => left < right,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => return None,
            };
            if take_baseline {
                return self.baseline.as_mut().and_then(|values| values.next());
            }
            if let (Some((left, _)), Some((right, _))) = (baseline, change) {
                if left == right {
                    let _ = self.baseline.as_mut().and_then(|values| values.next());
                }
            }
            let (key, value) = self
                .changes
                .next()
                .expect("selected catalog change must exist");
            if let Some(value) = value {
                return Some((key, value.as_ref()));
            }
        }
    }
}

impl<'a, V: 'static> IntoIterator for &'a CatalogOverlay<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = CatalogIter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests;
