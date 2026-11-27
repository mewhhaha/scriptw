//! Exact deterministic export of every payload consumed by the closed calendars.

use icu_calendar::provider::{Baked, CalendarJapaneseModernV1};
use icu_provider::dynutil::UpcastDataPayload;
use icu_provider::export::{DataExporter, ExportMarker, FlushMetadata};
use icu_provider::prelude::*;
use icu_provider_blob::export::BlobExporter;

pub fn export() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    {
        let mut exporter = BlobExporter::new_with_sink(Box::new(&mut bytes));
        macro_rules! singleton {
            ($marker:ty) => {
                let response = DataProvider::<$marker>::load(&Baked, Default::default())?;
                let payload: DataPayload<ExportMarker> =
                    UpcastDataPayload::upcast(response.payload);
                exporter.put_payload(<$marker>::INFO, Default::default(), &payload)?;
                exporter.flush(<$marker>::INFO, FlushMetadata::default())?;
            };
        }
        singleton!(CalendarJapaneseModernV1);
        exporter.close()?;
    }
    if bytes.is_empty() {
        return Err("calendar export produced no payload".into());
    }
    Ok(bytes)
}
