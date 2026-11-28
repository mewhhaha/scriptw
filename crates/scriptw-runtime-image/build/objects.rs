//! Compile immutable data only, without serializing its initializer into MIR
//! metadata or incremental caches. The ordinary Cargo target compiler defines
//! the exact array ABI; cc's target archiver packages the resulting data object.

#[path = "../../scriptw-engine/src/embedded_runtime/output.rs"]
mod output;

use super::parts::{Alignment, PAGE_ALIGNMENT, PARTS};
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

pub(super) fn embed(out: &Path, target: &str) {
    println!("cargo:rustc-check-cfg=cfg(scriptw_runtime_image)");
    let native_host_only = std::env::var_os("CARGO_FEATURE_NATIVE_HOST_ONLY").is_some();
    let active_parts: Vec<_> = PARTS
        .into_iter()
        .filter(|(_, _, _, is_portable)| !native_host_only || !is_portable)
        .collect();
    let lengths: Vec<u64> = active_parts
        .iter()
        .map(|(_, name, _, _)| {
            std::fs::metadata(out.join(name))
                .expect("checked runtime producer published every image")
                .len()
        })
        .collect();
    if lengths.iter().all(|&length| length == 0) {
        // Unsupported targets retain the ordinary runtime path. They need
        // neither a native data object nor a target archiver for empty images.
        output::write(&out.join("image_bindings.rs"), "")
            .expect("empty image bindings write completely");
        return;
    }
    assert!(lengths.iter().all(|&length| length > 0));
    // Both sides name one layout: the aligned wrapper's first field is the
    // exact image, and its trailing padding is never read as image bytes.
    let wrapper = |visibility: &str| {
        format!(
            "#[repr(C, align({PAGE_ALIGNMENT}))]\n{visibility}struct PageAligned<const N: usize>({visibility}[u8; N]);\n"
        )
    };
    let mut definitions = format!("#![no_std]\n{}", wrapper("pub "));
    let mut declarations = format!("{}unsafe extern \"C\" {{\n", wrapper(""));
    for ((symbol, name, alignment, _), length) in active_parts.into_iter().zip(lengths) {
        let path = out.join(name);
        match alignment {
            Alignment::Byte => {
                writeln!(
                    definitions,
                    "#[unsafe(no_mangle)]\npub static {symbol}: [u8; {length}] = *include_bytes!({path:?});"
                )
                .unwrap();
                writeln!(declarations, "    static {symbol}: [u8; {length}];").unwrap();
            }
            Alignment::Page => {
                writeln!(
                    definitions,
                    "#[unsafe(no_mangle)]\npub static {symbol}: PageAligned<{length}> = PageAligned(*include_bytes!({path:?}));"
                )
                .unwrap();
                writeln!(declarations, "    static {symbol}: PageAligned<{length}>;").unwrap();
            }
        }
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
