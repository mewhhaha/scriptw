//! Export all seven exact Segmenter markers with their consumed locale descriptor.

use icu_provider::dynutil::UpcastDataPayload;
use icu_provider::export::{DataExporter, ExportMarker};
use icu_provider::prelude::*;
use icu_provider_blob::export::BlobExporter;
use icu_segmenter::provider::*;
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"SCRWSEG1";

struct PinnedSegmenterProvider;
const _: () = {
    use icu_segmenter_data::*;
    mod icu {
        pub use icu_collections as collections;
        pub use icu_locale as locale;
        pub use icu_segmenter as segmenter;
    }
    make_provider!(PinnedSegmenterProvider);
    // The three upstream singleton ITER arms spell BtreeSet incorrectly.
    // Preserve the locked generated inputs and own their exact singleton IDs.
    impl_segmenter_break_grapheme_cluster_v1!(PinnedSegmenterProvider);
    impl_segmenter_break_word_v1!(PinnedSegmenterProvider);
    impl_segmenter_break_sentence_v1!(PinnedSegmenterProvider);
    impl_segmenter_break_word_override_v1!(PinnedSegmenterProvider, ITER);
    impl_segmenter_break_sentence_override_v1!(PinnedSegmenterProvider, ITER);
    impl_segmenter_dictionary_auto_v1!(PinnedSegmenterProvider, ITER);
    impl_segmenter_lstm_auto_v1!(PinnedSegmenterProvider, ITER);
};
macro_rules! singleton_ids {
    ($marker:ty) => {
        impl IterableDataProvider<$marker> for PinnedSegmenterProvider {
            fn iter_ids(&self) -> Result<BTreeSet<DataIdentifierCow<'static>>, DataError> {
                Ok(BTreeSet::from([DataIdentifierCow::default()]))
            }
        }
    };
}
singleton_ids!(SegmenterBreakGraphemeClusterV1);
singleton_ids!(SegmenterBreakWordV1);
singleton_ids!(SegmenterBreakSentenceV1);

pub(super) fn export() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let provider = PinnedSegmenterProvider;
    let mut blob = Vec::new();
    {
        let mut exporter = BlobExporter::new_with_sink(Box::new(&mut blob));
        macro_rules! export_marker {
            ($marker:ty) => {{
                let ids = <PinnedSegmenterProvider as IterableDataProvider<$marker>>::iter_ids(
                    &provider,
                )?;
                if <$marker>::INFO != SegmenterBreakWordOverrideV1::INFO {
                    crate::export_contract::require_identifiers(<$marker>::INFO, &ids)?;
                }
                for id in ids {
                    let response = <PinnedSegmenterProvider as DataProvider<$marker>>::load(
                        &provider,
                        DataRequest {
                            id: id.as_borrowed(),
                            metadata: Default::default(),
                        },
                    )?;
                    if response.metadata.locale.is_some() {
                        return Err(
                            "exact Segmenter inventory row unexpectedly used fallback".into()
                        );
                    }
                    let payload: DataPayload<ExportMarker> =
                        UpcastDataPayload::upcast(response.payload);
                    exporter.put_payload(<$marker>::INFO, id.as_borrowed(), &payload)?;
                }
                exporter.flush(<$marker>::INFO, Default::default())?;
            }};
        }
        export_marker!(SegmenterBreakGraphemeClusterV1);
        export_marker!(SegmenterBreakWordV1);
        export_marker!(SegmenterBreakSentenceV1);
        export_marker!(SegmenterBreakWordOverrideV1);
        export_marker!(SegmenterBreakSentenceOverrideV1);
        export_marker!(SegmenterDictionaryAutoV1);
        export_marker!(SegmenterLstmAutoV1);
        exporter.close()?;
    }
    if blob.is_empty() {
        return Err("Segmenter export produced no ICU payload".into());
    }
    let descriptor = include_bytes!("../segmenter/profile.json");
    let mut payload = MAGIC.to_vec();
    payload.extend_from_slice(&u32::try_from(descriptor.len())?.to_le_bytes());
    payload.extend_from_slice(descriptor);
    payload.extend_from_slice(&blob);
    Ok(payload)
}
