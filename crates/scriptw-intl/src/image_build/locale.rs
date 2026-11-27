//! Deterministic singleton export from the exact locked ICU Locale provider.

use icu_locale::provider::{
    Baked as LocaleBaked, LocaleAliasesV1, LocaleLikelySubtagsExtendedV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_locale_fallback::provider::{
    Baked as FallbackBaked, LocaleLikelySubtagsLanguageV1, LocaleParentsV1,
};
use icu_provider::dynutil::UpcastDataPayload;
use icu_provider::export::{DataExporter, ExportMarker, FlushMetadata};
use icu_provider::prelude::*;
use icu_provider_blob::export::BlobExporter;

pub fn export() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    {
        let mut exporter = BlobExporter::new_with_sink(Box::new(&mut bytes));
        macro_rules! singleton {
            ($provider:expr, $marker:ty) => {
                let response = DataProvider::<$marker>::load(&$provider, Default::default())?;
                let payload: DataPayload<ExportMarker> =
                    UpcastDataPayload::upcast(response.payload);
                exporter.put_payload(<$marker>::INFO, Default::default(), &payload)?;
                exporter.flush(<$marker>::INFO, FlushMetadata::default())?;
            };
        }
        singleton!(LocaleBaked, LocaleAliasesV1);
        singleton!(LocaleBaked, LocaleLikelySubtagsExtendedV1);
        singleton!(FallbackBaked, LocaleLikelySubtagsLanguageV1);
        singleton!(LocaleBaked, LocaleLikelySubtagsScriptRegionV1);
        singleton!(FallbackBaked, LocaleParentsV1);
        exporter.close()?;
    }
    if bytes.is_empty() {
        return Err("ICU Locale export produced no payload".into());
    }
    Ok(bytes)
}
