//! Mapping source for an admitted Cargo native image: the image's own range
//! of the running executable. It owns no Module or Engine. Wasmtime owns each
//! independent private executable mapping of that range, whose clean pages
//! are the executable's page-cache pages, shared with every other process
//! running the same file. Nothing is decoded, hashed, copied or written, and
//! no other file can back native code: no pre-existing, cache-directory or
//! digest-selected file is ever opened here.
use scriptw_runtime_image::{ExecutableImageRange, LinkedNativeImage};

/// Where an admitted embedded native runtime was deserialized from.
#[derive(Clone, Copy)]
pub(super) enum NativeLoadPath {
    /// A private mapping of the image's range of the running executable.
    ExecutableMapping,
    /// A Wasmtime-owned copy of the linked bytes, used where the process
    /// cannot prove the image's place in its executable (no procfs, an
    /// image linked outside the main executable, or not Linux).
    Copied,
}

impl NativeLoadPath {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            NativeLoadPath::ExecutableMapping => "executable-mapping",
            NativeLoadPath::Copied => "copied",
        }
    }
}

pub(super) struct NativeBacking {
    range: ExecutableImageRange,
}

impl NativeBacking {
    /// Resolved once per profile: the proof reads `/proc/self/maps`, which is
    /// short at the first load and grows with every later mapping.
    pub(super) fn from_cargo(image: &LinkedNativeImage) -> Option<Self> {
        image.executable_range().map(|range| Self { range })
    }

    /// Length of the admitted native image, for the load trace.
    pub(super) fn bytes(&self) -> u64 {
        self.range.length() as u64
    }

    /// # Safety
    /// The image this range was proved for must already be admitted against
    /// the Cargo manifest. The range proves origin, not native-code authority.
    pub(super) unsafe fn deserialize(
        &self,
        engine: &wasmtime::Engine,
    ) -> wasmtime::Result<wasmtime::Module> {
        #[cfg(unix)]
        {
            // SAFETY: caller supplies native authority. The range is the
            // linked image in the kernel's handle to the running executable,
            // which the kernel keeps unwritable while it runs; this type
            // publishes no descriptor. Wasmtime retains its own handle through
            // Module/Store lifetimes and rejects an offset that is not a
            // multiple of the host page size.
            unsafe {
                wasmtime::Module::deserialize_open_file_range(
                    engine,
                    self.range.try_clone_file()?,
                    self.range.offset(),
                    self.range.length(),
                )
            }
        }
        #[cfg(not(unix))]
        {
            let _ = engine;
            wasmtime::bail!("executable range mapping is unavailable on this host")
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::Write;

    /// The patched Wasmtime entry maps only the given range: code runs from
    /// it, a data segment's copy-on-write image comes from the same range of
    /// the file, and an unaligned or out-of-file range maps nothing.
    #[test]
    fn a_page_aligned_file_range_is_a_complete_module_owned_by_wasmtime() {
        let engine = crate::shared_wasm_engine().unwrap();
        // Validated module: (memory 1) (data (i32.const 8) "\2a")
        // (func (export "answer") (result i32) (i32.load8_u (i32.const 8))).
        let wasm = [
            0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 10,
            1, 6, 97, 110, 115, 119, 101, 114, 0, 0, 10, 9, 1, 7, 0, 65, 8, 45, 0, 0, 11, 11, 7, 1,
            0, 65, 8, 11, 1, 42,
        ];
        let original = wasmtime::Module::new(&engine, wasm).unwrap();
        let native = original.serialize().unwrap();
        drop(original);
        let offset = 3 * 64 * 1024_u64;
        let directory = std::env::temp_dir().join(format!(
            "scriptw-range-mapping-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("container");
        let mut file = std::fs::File::create_new(&path).unwrap();
        // Neighbours that are not a module: a whole-file or misplaced mapping
        // cannot deserialize.
        file.write_all(&vec![0xa5; offset as usize]).unwrap();
        file.write_all(&native).unwrap();
        file.write_all(&[0x5a; 4097]).unwrap();
        drop(file);
        let open = || std::fs::File::open(&path).unwrap();
        // SAFETY: every range below is either the exact trusted serialization
        // of this pinned engine or is rejected before any byte is interpreted
        // as code (alignment, bounds and the image header checks).
        unsafe {
            for (bad_offset, length) in [
                (offset + 1, native.len() - 1),
                (offset, 0),
                (offset, native.len() + 4098),
                (u64::MAX - 4095, native.len()),
                (0, native.len()),
            ] {
                assert!(
                    wasmtime::Module::deserialize_open_file_range(
                        &engine,
                        open(),
                        bad_offset,
                        length
                    )
                    .is_err(),
                    "range {bad_offset:#x}+{length:#x}"
                );
            }
            assert!(wasmtime::Module::deserialize_open_file(&engine, open()).is_err());
        }
        // SAFETY: the exact trusted serialization at its page-aligned offset.
        let loaded = unsafe {
            wasmtime::Module::deserialize_open_file_range(&engine, open(), offset, native.len())
                .unwrap()
        };
        // The mapping and Wasmtime's handle outlive the name.
        std::fs::remove_dir_all(&directory).unwrap();
        for _ in 0..2 {
            let mut store = wasmtime::Store::new(&engine, ());
            let instance = wasmtime::Instance::new(&mut store, &loaded, &[]).unwrap();
            let answer = instance
                .get_typed_func::<(), i32>(&mut store, "answer")
                .unwrap();
            assert_eq!(answer.call(&mut store, ()).unwrap(), 42);
        }
    }
}
