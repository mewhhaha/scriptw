//! A frozen compiler image followed by one program's append-only storage.
use super::StringRef;
use std::collections::BTreeMap;
use std::ops::{Bound, Index, RangeBounds};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PoolBytes {
    prefix: Arc<[u8]>,
    appended: Vec<u8>,
}

impl PoolBytes {
    pub(super) fn freeze(&mut self) {
        assert!(self.prefix.is_empty(), "compiler bytes are frozen once");
        self.prefix = Arc::from(std::mem::take(&mut self.appended));
    }
    pub(super) fn program_overlay(&self) -> Self {
        assert!(
            self.appended.is_empty(),
            "shared prefix has no program data"
        );
        Self {
            prefix: Arc::clone(&self.prefix),
            appended: Vec::new(),
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.prefix.len() + self.appended.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(crate) fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.appended.extend_from_slice(bytes);
    }
    pub(crate) fn try_reserve_exact(
        &mut self,
        additional: usize,
    ) -> Result<(), std::collections::TryReserveError> {
        self.appended.try_reserve_exact(additional)
    }
    pub(crate) fn resize(&mut self, length: usize, value: u8) {
        assert!(
            length >= self.prefix.len(),
            "immutable prefix cannot shrink"
        );
        self.appended.resize(length - self.prefix.len(), value);
    }
    pub(super) fn replace_suffix(&mut self, start: usize, bytes: Vec<u8>) {
        assert!(start >= self.prefix.len() && start <= self.len());
        self.appended.truncate(start - self.prefix.len());
        self.appended.extend(bytes);
    }
    /// Standalone and R emission have one contiguous image. A linked P reads
    /// its explicit suffix, so it never requests a combined allocation.
    pub(crate) fn as_slice(&self) -> &[u8] {
        if self.prefix.is_empty() {
            &self.appended
        } else {
            assert!(self.appended.is_empty(), "use the P-local image accessor");
            &self.prefix
        }
    }
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, u8> {
        self.as_slice().iter()
    }
}

impl From<Vec<u8>> for PoolBytes {
    fn from(appended: Vec<u8>) -> Self {
        Self {
            appended,
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl PartialEq<Vec<u8>> for PoolBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.len() == other.len()
            && self
                .prefix
                .iter()
                .chain(self.appended.iter())
                .eq(other.iter())
    }
}

impl<R: RangeBounds<usize>> Index<R> for PoolBytes {
    type Output = [u8];
    fn index(&self, range: R) -> &Self::Output {
        let start = match range.start_bound() {
            Bound::Included(start) => *start,
            Bound::Excluded(start) => start.checked_add(1).expect("image slice start"),
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(end) => end.checked_add(1).expect("image slice end"),
            Bound::Excluded(end) => *end,
            Bound::Unbounded => self.len(),
        };
        assert!(start <= end && end <= self.len(), "image slice bounds");
        if end <= self.prefix.len() {
            &self.prefix[start..end]
        } else {
            assert!(start >= self.prefix.len(), "slice cannot cross R/P images");
            &self.appended[start - self.prefix.len()..end - self.prefix.len()]
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct PoolStringRefs {
    prefix: Arc<BTreeMap<String, StringRef>>,
    appended: BTreeMap<String, StringRef>,
}

impl PoolStringRefs {
    pub(super) fn freeze(&mut self) {
        assert!(self.prefix.is_empty(), "compiler refs are frozen once");
        self.prefix = Arc::new(std::mem::take(&mut self.appended));
    }
    pub(super) fn program_overlay(&self) -> Self {
        assert!(
            self.appended.is_empty(),
            "shared refs have no program entries"
        );
        Self {
            prefix: Arc::clone(&self.prefix),
            appended: BTreeMap::new(),
        }
    }
    pub(super) fn len(&self) -> usize {
        self.prefix.len() + self.appended.len()
    }
    pub(super) fn get(&self, value: &str) -> Option<&StringRef> {
        self.prefix.get(value).or_else(|| self.appended.get(value))
    }
    pub(super) fn contains_key(&self, value: &str) -> bool {
        self.get(value).is_some()
    }
    pub(super) fn insert(&mut self, value: String, reference: StringRef) {
        assert!(
            !self.prefix.contains_key(&value),
            "runtime ordinal is immutable"
        );
        assert!(self.appended.insert(value, reference).is_none());
    }
    pub(super) fn values(&self) -> impl Iterator<Item = &StringRef> {
        self.prefix.values().chain(self.appended.values())
    }
    pub(super) fn program_values(&self) -> impl Iterator<Item = &StringRef> {
        self.appended.values()
    }
    pub(super) fn program_values_mut(&mut self) -> impl Iterator<Item = &mut StringRef> {
        self.appended.values_mut()
    }
}
