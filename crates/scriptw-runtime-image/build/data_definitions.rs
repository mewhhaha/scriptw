//! Pure source generation shared by image embedding and the small ELF preflight.

use std::fmt::Write;
use std::path::Path;

pub(super) enum Placement {
    Metadata,
    Payload(&'static str),
}

use Placement::{Metadata, Payload};

pub(super) const PARTS: [(&str, &str, Placement); 11] = [
    (
        "SCRIPTW_RUNTIME_EXECUTION",
        "scriptw-runtime-execution.bin",
        Metadata,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_EXECUTION",
        "scriptw-runtime-portable-execution.bin",
        Metadata,
    ),
    (
        "SCRIPTW_RUNTIME_METADATA",
        "scriptw-runtime-metadata.bin",
        Metadata,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_METADATA",
        "scriptw-runtime-portable-metadata.bin",
        Metadata,
    ),
    (
        "SCRIPTW_RUNTIME_INTL_METADATA",
        "scriptw-runtime-intl-metadata.bin",
        Metadata,
    ),
    (
        "SCRIPTW_RUNTIME_PACKAGE",
        "scriptw-runtime-package.bin",
        Payload(".scriptw_runtime_payload_package"),
    ),
    (
        "SCRIPTW_RUNTIME_NATIVE",
        "scriptw-runtime-native.packed",
        Payload(".scriptw_runtime_payload_native"),
    ),
    (
        "SCRIPTW_RUNTIME_MANIFEST",
        "scriptw-runtime-manifest.bin",
        Metadata,
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_PACKAGE",
        "scriptw-runtime-portable-package.bin",
        Payload(".scriptw_runtime_payload_portable_package"),
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_NATIVE",
        "scriptw-runtime-portable-native.packed",
        Payload(".scriptw_runtime_payload_portable_native"),
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_MANIFEST",
        "scriptw-runtime-portable-manifest.bin",
        Metadata,
    ),
];

pub(super) fn sources(out: &Path, lengths: [u64; PARTS.len()]) -> (String, String) {
    let mut definitions = "#![no_std]\n".to_owned();
    // Keep cold payloads outside the receipts' ordinary Linux ELF rodata, with
    // independent section garbage collection and unchanged exported arrays.
    // Natural alignment and final adjacency remain linker outcomes to inspect.
    let mut declarations = "unsafe extern \"C\" {\n".to_owned();
    for ((symbol, name, placement), length) in PARTS.into_iter().zip(lengths) {
        let section = match placement {
            Metadata => None,
            Payload(section) => Some(section),
        };
        if let Some(section) = section {
            writeln!(
                definitions,
                "#[cfg_attr(target_os = \"linux\", unsafe(link_section = {section:?}))]"
            )
            .unwrap();
        }
        let path = out.join(name);
        writeln!(
            definitions,
            "#[unsafe(no_mangle)]\npub static {symbol}: [u8; {length}] = *include_bytes!({path:?});"
        )
        .unwrap();
        writeln!(declarations, "    static {symbol}: [u8; {length}];").unwrap();
    }
    declarations.push_str("}\n");
    (definitions, declarations)
}
