use super::*;
use crate::number_format::numeric::ObservedNumericInput;
use crate::number_format::options::*;
use crate::number_format::NumberLocaleRequest;
use crate::{
    CanonicalLocaleId, EmbeddedIntlProvider, FormatNumberParts, IntlCompilationProfile,
    IntlDataSelection, LocaleId, NumberFormatRequest, ResolveNumberLocale,
};

fn data() -> Arc<IntlCompilationData> {
    Arc::clone(
        IntlDataSelection::new(IntlCompilationProfile::Minimal)
            .compilation_data()
            .unwrap(),
    )
}

#[test]
fn typed_locale_and_number_keep_full_identity_without_retaining_compilation_metadata() {
    let data = data();
    assert!(data.minimal_consumers.locale.get().is_none());
    assert!(data.minimal_consumers.number.get().is_none());
    let locale = data.minimal_locale_kernel().unwrap();
    assert_eq!(locale.identity(), data.identity());
    assert!(data.minimal_consumers.number.get().is_none());
    let canonical = locale
        .operation::<CanonicalizeLocale>()
        .unwrap()
        .execute(LocaleTransformRequest::new(
            LocaleId::parse("EN-us").unwrap(),
        ))
        .unwrap();
    assert_eq!(canonical.locale().as_str(), "en-US");
    let number = data.minimal_number_kernel().unwrap();
    assert_eq!(number.identity(), data.identity());
    assert!(Arc::ptr_eq(&locale, number.provider().locale_kernel()));
    let resolved = number
        .operation::<ResolveNumberLocale>()
        .unwrap()
        .execute(NumberLocaleRequest {
            requested: vec![CanonicalLocaleId::from_data("en-US").unwrap()].into_boxed_slice(),
            matcher: LocaleMatcher::Lookup,
            numbering_system: None,
        })
        .unwrap();
    let parts = number
        .operation::<FormatNumberParts>()
        .unwrap()
        .execute(NumberFormatRequest {
            configuration: crate::number_format::NumberFormatConfiguration {
                locale: resolved,
                options: NumberFormatOptions {
                    style: NumberStyle::Decimal,
                    notation: Notation::Standard,
                    minimum_integer_digits: IntegerDigitCount::new(1).unwrap(),
                    precision: Precision::Fraction(FractionPrecision::Range(
                        FractionDigitRange::new(
                            FractionDigitCount::new(1).unwrap(),
                            FractionDigitCount::new(1).unwrap(),
                        )
                        .unwrap(),
                    )),
                    rounding_mode: RoundingMode::HalfExpand,
                    trailing_zero_display: TrailingZeroDisplay::Auto,
                    grouping: Grouping::Never,
                    sign_display: SignDisplay::Auto,
                },
            },
            input: ObservedNumericInput::NumberShortestDecimal("12345.6".into()),
        })
        .unwrap();
    assert_eq!(parts.to_text(), "12345.6");
    let complete = EmbeddedIntlProvider::new().unwrap();
    assert_eq!(complete.identity(), data.identity());
    assert!(
        Arc::ptr_eq(
            complete.number_profiles().unwrap(),
            number.provider().number_profiles().unwrap(),
        ),
        "the complete provider must reuse the already selected Number owner"
    );
    assert!(locale
        .provider()
        .image
        .same_owner(&crate::embedded_locale_data_image().unwrap()));
    let weak = Arc::downgrade(&data);
    drop(data);
    assert!(
        weak.upgrade().is_none(),
        "cached kernels must not own their metadata cache"
    );
    assert_eq!(number.identity(), locale.identity());
}

#[test]
fn concurrent_number_initialization_publishes_one_locale_and_number_owner() {
    let data = data();
    std::thread::scope(|scope| {
        let workers = (0..4)
            .map(|_| scope.spawn(|| data.minimal_number_kernel().unwrap()))
            .collect::<Vec<_>>();
        let kernels = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert!(kernels
            .iter()
            .all(|kernel| Arc::ptr_eq(kernel, &kernels[0])));
        assert!(Arc::ptr_eq(
            &data.minimal_locale_kernel().unwrap(),
            kernels[0].provider().locale_kernel()
        ));
    });
}

#[test]
fn custom_profiles_cannot_acquire_canonical_minimal_consumers() {
    let selection = IntlDataSelection::new(IntlCompilationProfile::Custom(
        crate::CustomProfileId::parse("minimal-consumer-negative").unwrap(),
    ));
    let data = selection.compilation_data().unwrap();
    assert!(data.minimal_locale_kernel().is_err());
    assert!(data.minimal_number_kernel().is_err());
    assert!(matches!(
        data.identity().profile().profile(),
        IntlDataProfile::Custom(_)
    ));
}
