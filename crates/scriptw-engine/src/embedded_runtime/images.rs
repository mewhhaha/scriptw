//! Select a single profile's immutable Cargo authority before touching payloads.
use scriptw_aot_wasm::RuntimeSourceCompilationProfile;
use scriptw_runtime_image as image;

#[derive(Clone, Copy)]
pub(super) struct Images {
    pub(super) profile: RuntimeSourceCompilationProfile,
    pub(super) package: &'static [u8],
    pub(super) manifest: &'static [u8],
}

impl Images {
    pub(super) fn metadata_for_profile(profile: RuntimeSourceCompilationProfile) -> &'static [u8] {
        match profile {
            RuntimeSourceCompilationProfile::NativeHost => image::metadata(),
            RuntimeSourceCompilationProfile::Portable => image::portable_metadata(),
        }
    }

    pub(super) fn package_for_profile(profile: RuntimeSourceCompilationProfile) -> &'static [u8] {
        match profile {
            RuntimeSourceCompilationProfile::NativeHost => image::package(),
            RuntimeSourceCompilationProfile::Portable => image::portable_package(),
        }
    }

    pub(super) fn for_profile(profile: RuntimeSourceCompilationProfile) -> Self {
        let package = Self::package_for_profile(profile);
        let manifest = match profile {
            RuntimeSourceCompilationProfile::NativeHost => image::manifest_bytes(),
            RuntimeSourceCompilationProfile::Portable => image::portable_manifest_bytes(),
        };
        Self {
            profile,
            package,
            manifest,
        }
    }
}
