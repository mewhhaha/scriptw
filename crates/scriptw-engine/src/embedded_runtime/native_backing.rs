//! Private immutable disk backing for admitted Cargo native bytes. It owns no
//! Module or Engine. Wasmtime owns each independent private executable mapping.
use scriptw_aot_wasm::RuntimeSourceCompilationProfile;
use std::fs::File;
#[cfg(all(test, target_os = "linux"))]
use std::io::Write;

pub(super) struct NativeBacking(File);

impl NativeBacking {
    pub(super) fn from_cargo(
        profile: RuntimeSourceCompilationProfile,
        expected: &[u8; 32],
    ) -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            let mut file = Self::private_file()?;
            // The only writer is the linked Cargo image decoder. Neither paths,
            // caller bytes nor a digest-selected external cache enter this seam.
            let written = match profile {
                RuntimeSourceCompilationProfile::NativeHost => {
                    scriptw_runtime_image::write_native(&mut file)
                }
                RuntimeSourceCompilationProfile::Portable => {
                    scriptw_runtime_image::write_portable_native(&mut file)
                }
            }?;
            if written.digest() != expected
                || file.metadata().ok()?.len() != written.decoded_bytes() as u64
            {
                return None;
            }
            return Self::publish(file);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (profile, expected);
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn publish(file: File) -> Option<Self> {
        file.sync_data().ok()?;
        Some(Self(file))
    }

    #[cfg(target_os = "linux")]
    fn private_file() -> Option<File> {
        use rustix::fs::{Mode, OFlags};
        let directory = crate::cache::module_cache_directory().join("private-native-backing-v1");
        std::fs::create_dir_all(&directory).ok()?;
        // O_TMPFILE|EXCL has no pathname and cannot later be linked. Never
        // accept a previously existing native file or expose a write handle.
        let fd = rustix::fs::open(
            &directory,
            OFlags::RDWR | OFlags::TMPFILE | OFlags::EXCL | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .ok()?;
        // Linux uapi magic.h: tmpfs/ramfs are unreclaimable without swap.
        // Moving the image there would merely hide its bytes from RSS.
        let stat = rustix::fs::fstatfs(&fd).ok()?;
        if matches!(stat.f_type as u64, 0x0102_1994 | 0x8584_58f6) {
            return None;
        }
        Some(File::from(fd))
    }

    #[cfg(test)]
    fn create(bytes: &[u8]) -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            let mut file = Self::private_file()?;
            file.write_all(bytes).ok()?;
            Self::publish(file)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = bytes;
            None
        }
    }

    /// # Safety
    /// This file must contain the exact already-admitted Cargo producer native
    /// image. File construction proves immutability, not native-code authority.
    pub(super) unsafe fn deserialize(
        &self,
        engine: &wasmtime::Engine,
    ) -> wasmtime::Result<wasmtime::Module> {
        // SAFETY: caller supplies native authority; the unlinked inode has no
        // exposed mutation path. This type publishes no writable File/descriptor,
        // and Wasmtime retains its own Arc<File> through Module/Store lifetimes.
        unsafe { wasmtime::Module::deserialize_open_file(engine, self.0.try_clone()?) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_inode_has_no_name_and_module_owns_its_lifetime() {
        let engine = crate::shared_wasm_engine().unwrap();
        // Validated module: (func (export "answer") (result i32) i32.const 7).
        let wasm = [
            0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 7, 10, 1, 6, 97, 110,
            115, 119, 101, 114, 0, 0, 10, 6, 1, 4, 0, 65, 7, 11,
        ];
        let original = wasmtime::Module::new(&engine, wasm).unwrap();
        let native = original.serialize().unwrap();
        let loaded = match NativeBacking::create(&native) {
            Some(backing) => {
                #[cfg(target_os = "linux")]
                {
                    use std::os::unix::fs::MetadataExt;
                    assert_eq!(backing.0.metadata().unwrap().nlink(), 0);
                    assert_eq!(backing.0.metadata().unwrap().len(), native.len() as u64);
                }
                // SAFETY: exact Module::serialize output of this pinned engine.
                // Dropping the backing before invocation proves Wasmtime retains
                // the inode independently through all active module users.
                unsafe { backing.deserialize(&engine).unwrap() }
            }
            None => {
                // SAFETY: same exact trusted serialization, on a filesystem
                // without private disk-backed mapping support.
                unsafe { wasmtime::Module::deserialize(&engine, &native).unwrap() }
            }
        };
        drop(original);
        drop(native);
        let mut store = wasmtime::Store::new(&engine, ());
        store.set_epoch_deadline(1);
        let instance = wasmtime::Instance::new(&mut store, &loaded, &[]).unwrap();
        let answer = instance
            .get_typed_func::<(), i32>(&mut store, "answer")
            .unwrap();
        drop(loaded);
        assert_eq!(answer.call(&mut store, ()).unwrap(), 7);
    }
}
