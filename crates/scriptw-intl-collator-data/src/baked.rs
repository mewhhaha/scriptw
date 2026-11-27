//! ICU4X 2.3 baked collation and NFD data provider.
use icu_collator::provider::*;
use icu_collator_data::*;
use icu_normalizer::provider::{NormalizerNfdDataV1, NormalizerNfdTablesV1};
use icu_normalizer_data::*;
use icu_provider::prelude::*;

pub(super) mod icu {
    pub use icu_collator as collator;
    pub use icu_collections as collections;
    pub use icu_normalizer as normalizer;
}

/// Immutable provider for exported collation and NFD data.
pub struct PinnedCollatorProvider;
icu_collator_data::make_provider!(PinnedCollatorProvider);
impl_collation_root_v1!(PinnedCollatorProvider);
impl_collation_tailoring_v1!(PinnedCollatorProvider, ITER);
impl_collation_metadata_v1!(PinnedCollatorProvider, ITER);
impl_collation_diacritics_v1!(PinnedCollatorProvider, ITER);
impl_collation_jamo_v1!(PinnedCollatorProvider);
impl_collation_reordering_v1!(PinnedCollatorProvider, ITER);
impl_collation_special_primaries_v1!(PinnedCollatorProvider);
impl_normalizer_nfd_data_v1!(PinnedCollatorProvider);
impl_normalizer_nfd_tables_v1!(PinnedCollatorProvider);

macro_rules! singleton_ids {
    ($marker:ty) => {
        impl IterableDataProvider<$marker> for PinnedCollatorProvider {
            fn iter_ids(
                &self,
            ) -> Result<std::collections::BTreeSet<DataIdentifierCow<'static>>, DataError> {
                Ok([DataIdentifierCow::default()].into_iter().collect())
            }
        }
    };
}
singleton_ids!(CollationRootV1);
singleton_ids!(CollationJamoV1);
singleton_ids!(CollationSpecialPrimariesV1);
singleton_ids!(NormalizerNfdDataV1);
singleton_ids!(NormalizerNfdTablesV1);
