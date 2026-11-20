//! The host-created memories a program's host module imports, admitted once.
//!
//! Scriptw's host module (R when linked, the standalone module otherwise) may
//! import a private memory and a shared memory, which the host creates per
//! Store (private) or per agent group (shared) from the import's type. Neither
//! is bounded by anything else: a shared memory ignores `StoreLimits`, and a
//! private memory is bounded by a Store limiter only when the embedder
//! installs one. So both are bounded here, by their types, at admission.

use crate::wasmtime_config::WASM_STORE_MEMORY_CAP_BYTES;
use crate::{EngineError, WASM_HOST_IMPORT_NAMESPACE};
use crate::{WASM_HOST_IMPORT_PRIVATE_MEMORY, WASM_HOST_IMPORT_SHARED_MEMORY};
use wasmtime::{ExternType, MemoryType, Module};

const WASM_PAGE_BYTES: u64 = 64 * 1024;
/// The per-Store memory cap in Wasm pages.
const CAP_PAGES: u64 = WASM_STORE_MEMORY_CAP_BYTES as u64 / WASM_PAGE_BYTES;
const _: () = assert!(CAP_PAGES * WASM_PAGE_BYTES == WASM_STORE_MEMORY_CAP_BYTES as u64);

/// The memory types to create for the host module's memory imports, each
/// bounded by the per-Store cap.
#[derive(Debug, Clone)]
pub(crate) struct HostMemoryImports {
    private: Option<MemoryType>,
    shared: Option<MemoryType>,
}

impl HostMemoryImports {
    /// Refuses a shared-memory import whose maximum exceeds the cap (the
    /// emitter declares exactly the cap), and gives the private memory the
    /// cap as its maximum unless the import declares a smaller one.
    pub(crate) fn admit(host: &Module) -> Result<Self, EngineError> {
        let mut admitted = Self {
            private: None,
            shared: None,
        };
        for import in host.imports() {
            if import.module() != WASM_HOST_IMPORT_NAMESPACE {
                continue;
            }
            let (slot, admitted_type) = match import.name() {
                WASM_HOST_IMPORT_PRIVATE_MEMORY => (
                    &mut admitted.private,
                    private_memory_type(&declared_memory(import.ty(), "private")?)?,
                ),
                WASM_HOST_IMPORT_SHARED_MEMORY => (
                    &mut admitted.shared,
                    shared_memory_type(declared_memory(import.ty(), "shared")?)?,
                ),
                _ => continue,
            };
            if slot.replace(admitted_type).is_some() {
                return Err(EngineError::new(format!(
                    "Scriptw memory import {} appears twice",
                    import.name()
                )));
            }
        }
        Ok(admitted)
    }

    /// The private memory to create in each Store, if the module imports one.
    pub(crate) fn private(&self) -> Option<&MemoryType> {
        self.private.as_ref()
    }

    /// The shared memory to create per agent group, if the module imports one.
    pub(crate) fn shared(&self) -> Option<&MemoryType> {
        self.shared.as_ref()
    }
}

fn declared_memory(ty: ExternType, kind: &str) -> Result<MemoryType, EngineError> {
    match ty {
        ExternType::Memory(declared) => Ok(declared),
        _ => Err(EngineError::new(format!(
            "Scriptw {kind} memory import is not a memory"
        ))),
    }
}

fn checked_32_bit(declared: &MemoryType, kind: &str) -> Result<(), EngineError> {
    if declared.is_64() || declared.page_size() != WASM_PAGE_BYTES {
        return Err(EngineError::new(format!(
            "Scriptw {kind} memory import must be a 32-bit memory of 64 KiB pages"
        )));
    }
    if declared.minimum() > CAP_PAGES {
        return Err(EngineError::new(format!(
            "Scriptw {kind} memory import needs {} pages, more than the {CAP_PAGES}-page \
             per-Store cap",
            declared.minimum()
        )));
    }
    Ok(())
}

/// The import's type with an explicit maximum of at most the cap, which a
/// memory of that type satisfies whatever maximum (or none) the import
/// declares: growth past it fails even without a Store limiter.
fn private_memory_type(declared: &MemoryType) -> Result<MemoryType, EngineError> {
    checked_32_bit(declared, "private")?;
    if declared.is_shared() {
        return Err(EngineError::new(
            "Scriptw private memory import must not be shared",
        ));
    }
    let maximum = declared
        .maximum()
        .map_or(CAP_PAGES, |maximum| maximum.min(CAP_PAGES));
    Ok(MemoryType::new(
        u32::try_from(declared.minimum()).expect("checked against the cap"),
        Some(u32::try_from(maximum).expect("at most the cap")),
    ))
}

/// A shared memory must match its import's maximum exactly, so the import
/// itself must stay within the cap.
fn shared_memory_type(declared: MemoryType) -> Result<MemoryType, EngineError> {
    checked_32_bit(&declared, "shared")?;
    if !declared.is_shared() {
        return Err(EngineError::new(
            "Scriptw shared memory import must be shared",
        ));
    }
    match declared.maximum() {
        Some(maximum) if maximum <= CAP_PAGES => Ok(declared),
        maximum => Err(EngineError::new(format!(
            "Scriptw shared memory import declares a maximum of {maximum:?} pages; \
             a shared memory ignores Store limits, so at most the {CAP_PAGES}-page \
             per-Store cap is admitted"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leb(mut value: u64, bytes: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                bytes.push(byte);
                return;
            }
            bytes.push(byte | 0x80);
        }
    }

    /// A module whose only import is the memory `namespace`.`name` with
    /// `minimum` and `maximum` pages.
    fn host(
        namespace: &str,
        name: &str,
        minimum: u64,
        maximum: Option<u64>,
        shared: bool,
    ) -> Module {
        let mut config = wasmtime::Config::new();
        config.wasm_threads(true).shared_memory(true);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let mut import = vec![1];
        for text in [namespace, name] {
            leb(text.len() as u64, &mut import);
            import.extend_from_slice(text.as_bytes());
        }
        import.push(0x02);
        import.push(match (maximum.is_some(), shared) {
            (false, _) => 0x00,
            (true, false) => 0x01,
            (true, true) => 0x03,
        });
        leb(minimum, &mut import);
        if let Some(maximum) = maximum {
            leb(maximum, &mut import);
        }
        let mut bytes = b"\0asm\x01\0\0\0\x02".to_vec();
        leb(import.len() as u64, &mut bytes);
        bytes.extend_from_slice(&import);
        Module::new(&engine, bytes).unwrap()
    }

    fn private(minimum: u64, maximum: Option<u64>) -> Result<HostMemoryImports, EngineError> {
        HostMemoryImports::admit(&host(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_PRIVATE_MEMORY,
            minimum,
            maximum,
            false,
        ))
    }

    fn shared(maximum: u64) -> Result<HostMemoryImports, EngineError> {
        HostMemoryImports::admit(&host(
            WASM_HOST_IMPORT_NAMESPACE,
            WASM_HOST_IMPORT_SHARED_MEMORY,
            1,
            Some(maximum),
            true,
        ))
    }

    #[test]
    fn the_private_memory_gets_at_most_the_cap_as_its_maximum() {
        for (declared, maximum) in [(None, CAP_PAGES), (Some(7), 7), (Some(65536), CAP_PAGES)] {
            let admitted = private(1, declared).unwrap();
            let private = admitted.private().unwrap();
            assert_eq!(private.minimum(), 1, "{declared:?}");
            assert_eq!(private.maximum(), Some(maximum), "{declared:?}");
            assert!(!private.is_shared());
            assert!(admitted.shared().is_none());
        }
        let error = private(CAP_PAGES + 1, None).unwrap_err();
        assert!(error.message().contains("per-Store cap"), "{error:?}");
    }

    #[test]
    fn a_shared_memory_above_the_cap_is_refused_at_admission() {
        let admitted = shared(CAP_PAGES).unwrap();
        assert_eq!(admitted.shared().unwrap().maximum(), Some(CAP_PAGES));
        assert!(admitted.shared().unwrap().is_shared());
        let error = shared(CAP_PAGES + 1).unwrap_err();
        assert!(
            error.message().contains("ignores Store limits"),
            "{error:?}"
        );
        let error = shared(65536).unwrap_err();
        assert!(
            error.message().contains("ignores Store limits"),
            "{error:?}"
        );
    }

    #[test]
    fn other_namespaces_name_no_memory_to_create() {
        let admitted = HostMemoryImports::admit(&host(
            "other",
            WASM_HOST_IMPORT_PRIVATE_MEMORY,
            1,
            None,
            false,
        ))
        .unwrap();
        assert!(admitted.private().is_none() && admitted.shared().is_none());
    }
}
