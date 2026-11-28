//! The exact inventory of linked immutable data and how each part is aligned.

/// Alignment of a linked image's symbol, and so of its file offset modulo
/// the link's page size.
#[derive(Clone, Copy)]
pub(super) enum Alignment {
    Byte,
    /// A native image Wasmtime maps straight from the executable. 64 KiB is
    /// the largest page size of the supported targets, so the image starts on
    /// a page boundary of the file wherever it is linked.
    Page,
}

pub(super) const PAGE_ALIGNMENT: usize = 64 * 1024;

/// `(symbol, file_name, alignment, is_portable_only)`
pub(super) const PARTS: [(&str, &str, Alignment, bool); 13] = [
    (
        "SCRIPTW_RUNTIME_EXECUTION",
        "scriptw-runtime-execution.bin",
        Alignment::Byte,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_EXECUTION",
        "scriptw-runtime-portable-execution.bin",
        Alignment::Byte,
        true,
    ),
    (
        "SCRIPTW_RUNTIME_METADATA",
        "scriptw-runtime-metadata.bin",
        Alignment::Byte,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_METADATA",
        "scriptw-runtime-portable-metadata.bin",
        Alignment::Byte,
        true,
    ),
    (
        "SCRIPTW_RUNTIME_INTL_METADATA",
        "scriptw-runtime-intl-metadata.bin",
        Alignment::Byte,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_PACKAGE",
        "scriptw-runtime-package.bin",
        Alignment::Byte,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_NATIVE",
        "scriptw-runtime-native.bin",
        Alignment::Page,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_NATIVE_DIGEST",
        "scriptw-runtime-native.sha256",
        Alignment::Byte,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_MANIFEST",
        "scriptw-runtime-manifest.bin",
        Alignment::Byte,
        false,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_PACKAGE",
        "scriptw-runtime-portable-package.bin",
        Alignment::Byte,
        true,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_NATIVE",
        "scriptw-runtime-portable-native.bin",
        Alignment::Page,
        true,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_NATIVE_DIGEST",
        "scriptw-runtime-portable-native.sha256",
        Alignment::Byte,
        true,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_MANIFEST",
        "scriptw-runtime-portable-manifest.bin",
        Alignment::Byte,
        true,
    ),
];
