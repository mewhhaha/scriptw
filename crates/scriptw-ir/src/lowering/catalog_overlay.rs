//! Immutable compiler recipes with isolated, copy-on-write lowering facts.

use std::collections::{btree_map, BTreeMap, BTreeSet};
use std::iter::Peekable;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(super) struct CatalogOverlay<V: 'static> {
    baseline: Option<&'static BTreeMap<String, V>>,
    changes: BTreeMap<String, Option<Arc<V>>>,
    /// The last whole-key-set idempotent edit and the entries changed since.
    /// Every mutable access records its key here, so an entry can never be
    /// treated as settled after it changed.
    settled: Option<SettledEdit>,
}

/// An edit of each entry of a key set which is idempotent and reads or writes
/// no other entry: applying it again to an entry that has not been mutated
/// since leaves the entry unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdempotentEdit {
    /// Merge an unknown receiver and unknown arguments into a signature, as
    /// an unknown property hook may call any planned source function.
    UnknownPropertyHook,
}

#[derive(Debug, Clone)]
struct SettledEdit {
    edit: IdempotentEdit,
    keys: Arc<BTreeSet<String>>,
    /// Keys of `keys` accessed mutably since `edit` was applied to all keys.
    touched: BTreeSet<String>,
}

impl<V: 'static> Default for CatalogOverlay<V> {
    fn default() -> Self {
        Self {
            baseline: None,
            changes: BTreeMap::new(),
            settled: None,
        }
    }
}

impl<V: 'static> CatalogOverlay<V> {
    pub(super) fn new(baseline: &'static BTreeMap<String, V>) -> Self {
        Self {
            baseline: Some(baseline),
            changes: BTreeMap::new(),
            settled: None,
        }
    }

    fn touch(&mut self, key: &str) {
        if let Some(settled) = &mut self.settled {
            if settled.keys.contains(key) && !settled.touched.contains(key) {
                settled.touched.insert(key.to_string());
            }
        }
    }

    /// The keys of `keys` on which `edit` may still change an entry, in key
    /// order: all of them unless `edit` was settled for this same key set,
    /// otherwise only the entries mutated since.
    pub(super) fn unsettled_keys(
        &self,
        edit: IdempotentEdit,
        keys: &Arc<BTreeSet<String>>,
    ) -> Vec<String> {
        match &self.settled {
            Some(settled) if settled.edit == edit && Arc::ptr_eq(&settled.keys, keys) => {
                settled.touched.iter().cloned().collect()
            }
            _ => keys.iter().cloned().collect(),
        }
    }

    /// Record that `edit` was just applied to every key that
    /// [`Self::unsettled_keys`] returned for `keys`, with no other mutation
    /// in between.
    pub(super) fn settle(&mut self, edit: IdempotentEdit, keys: Arc<BTreeSet<String>>) {
        self.settled = Some(SettledEdit {
            edit,
            keys,
            touched: BTreeSet::new(),
        });
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
            settled: None,
        }
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
}

impl<V: PartialEq + 'static> CatalogOverlay<V> {
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
