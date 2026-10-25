//! Compile immutable data only, without serializing its initializer into MIR
//! metadata or incremental caches. The ordinary Cargo target compiler defines
//! the exact array ABI; cc's target archiver packages the resulting data object.

#[path = "../../scriptw-engine/src/embedded_runtime/output.rs"]
mod output;

use std::fmt::Write;
use std::path::Path;
use std::process::Command;

const PARTS: [(&str, &str); 7] = [
    (
        "SCRIPTW_RUNTIME_INTL_METADATA",
        "scriptw-runtime-intl-metadata.bin",
    ),
    ("SCRIPTW_RUNTIME_PACKAGE", "scriptw-runtime-package.bin"),
    ("SCRIPTW_RUNTIME_NATIVE", "scriptw-runtime-native.packed"),
    ("SCRIPTW_RUNTIME_MANIFEST", "scriptw-runtime-manifest.bin"),
    (
        "SCRIPTW_RUNTIME_PORTABLE_PACKAGE",
        "scriptw-runtime-portable-package.bin",
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_NATIVE",
        "scriptw-runtime-portable-native.packed",
    ),
    (
        "SCRIPTW_RUNTIME_PORTABLE_MANIFEST",
        "scriptw-runtime-portable-manifest.bin",
    ),
];

pub(super) fn embed(out: &Path, target: &str) {
    println!("cargo:rustc-check-cfg=cfg(scriptw_runtime_image)");
    let lengths = PARTS.map(|(_, name)| {
        std::fs::metadata(out.join(name))
            .expect("checked runtime producer published every image")
            .len()
    });
    if lengths.iter().all(|&length| length == 0) {
        // Unsupported targets retain the ordinary runtime path. They need
        // neither a native data object nor a target archiver for empty images.
        output::write(&out.join("image_bindings.rs"), "")
            .expect("empty image bindings write completely");
        return;
    }
    assert!(lengths.iter().all(|&length| length > 0));
    let mut definitions = "#![no_std]\n".to_owned();
    let mut declarations = "unsafe extern \"C\" {\n".to_owned();
    for ((symbol, name), length) in PARTS.into_iter().zip(lengths) {
        let path = out.join(name);
        writeln!(
            definitions,
            "#[unsafe(no_mangle)]\npub static {symbol}: [u8; {length}] = *include_bytes!({path:?});"
        )
        .unwrap();
        writeln!(declarations, "    static {symbol}: [u8; {length}];").unwrap();
    }
    declarations.push_str("}\n");
    let source = out.join("image_data.rs");
    let object = out.join("image_data.o");
    output::write(&source, definitions).expect("image data definitions write completely");
    let result = Command::new(std::env::var_os("RUSTC").expect("Cargo target Rust compiler"))
        .args([
            "--edition=2021",
            "--crate-name=scriptw_runtime_image_data",
            "--crate-type=lib",
            "--emit=obj",
            "-Ccodegen-units=1",
            "-Cembed-bitcode=no",
            "-Cdebuginfo=0",
            "-Copt-level=0",
            "-Cpanic=abort",
            "--target",
            target,
        ])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("Cargo target Rust compiler starts for immutable image data");
    assert!(
        result.status.success(),
        "immutable image data compilation failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    cc::Build::new()
        .object(&object)
        // Cargo propagates the native archive to the final link. Keep another
        // copy out of this leaf's Rust archive and every downstream rebuild.
        .link_lib_modifier("-bundle")
        .compile("scriptw_runtime_images");
    // cc does not register custom objects as watched source inputs. Only the
    // complete archive is consumed downstream; retain no duplicate data object.
    std::fs::remove_file(object).expect("archived image data object is retired");
    output::write(&out.join("image_bindings.rs"), declarations)
        .expect("exact image array declarations write completely");
    println!("cargo:rustc-cfg=scriptw_runtime_image");
}
