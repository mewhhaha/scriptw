//! R defines its private memory with a copy-on-write image of its active data.
//! A pooled slot is reset by writing the image's bytes back into the pages a
//! Store dirtied, and P's active data segments write into that memory at every
//! P instantiation. Each new Store, pooled or on demand, must still start from
//! R's pristine bytes, and code that reads R's tables must still work there.
use super::*;
use crate::host_memory_imports::{PrivateMemory, HOST_MODULE_MEMORY_EXPORT};
use crate::pooled_stores::{configure_with_pagemap, PooledStoreReset, PooledStores};
use wasmparser::{DataKind, ExternalKind, Operator, Parser, Payload, TypeRef};

/// Case mapping reads R's Unicode case tables, which are part of R's active
/// static data.
const CASE_TEXT: &str = "ÀÉÎ Ǆǅ ΣΑΣ İ ẞ Straße ﬀ ŉ ǰ";

fn case_source() -> String {
    format!("print('{CASE_TEXT}'.toLowerCase() + '|' + '{CASE_TEXT}'.toUpperCase());")
}

fn case_output() -> String {
    format!("{}|{}", CASE_TEXT.to_lowercase(), CASE_TEXT.to_uppercase())
}

/// `(offset, bytes)` of every active data segment `module` writes into its
/// memory `memory`, which must have constant offsets.
fn active_segments(module: &[u8], memory: u32) -> Vec<(usize, Vec<u8>)> {
    let mut segments = Vec::new();
    for payload in Parser::new(0).parse_all(module) {
        let Payload::DataSection(section) = payload.unwrap() else {
            continue;
        };
        for data in section {
            let data = data.unwrap();
            let DataKind::Active {
                memory_index,
                offset_expr,
            } = data.kind
            else {
                continue;
            };
            if memory_index != memory {
                continue;
            }
            let mut operators = offset_expr.get_operators_reader();
            let Operator::I32Const { value } = operators.read().unwrap() else {
                panic!("a data segment of memory {memory} has a non-constant offset");
            };
            assert!(matches!(operators.read().unwrap(), Operator::End));
            segments.push((u32::try_from(value).unwrap() as usize, data.data.to_vec()));
        }
    }
    segments
}

/// The index P gives the private memory it imports from the host namespace.
fn imported_private_memory_index(program: &[u8]) -> Option<u32> {
    let mut memories = 0;
    for payload in Parser::new(0).parse_all(program) {
        let Payload::ImportSection(section) = payload.unwrap() else {
            continue;
        };
        for import in section.into_imports() {
            let import = import.unwrap();
            if let TypeRef::Memory(_) = import.ty {
                if import.module == WASM_HOST_IMPORT_NAMESPACE
                    && import.name == WASM_HOST_IMPORT_PRIVATE_MEMORY
                {
                    return Some(memories);
                }
                memories += 1;
            }
        }
    }
    None
}

/// The index of the memory R defines and exports as its private memory.
fn exported_private_memory_index(runtime: &[u8]) -> u32 {
    for payload in Parser::new(0).parse_all(runtime) {
        let Payload::ExportSection(section) = payload.unwrap() else {
            continue;
        };
        for export in section {
            let export = export.unwrap();
            if export.kind == ExternalKind::Memory && export.name == HOST_MODULE_MEMORY_EXPORT {
                return export.index;
            }
        }
    }
    panic!("R exports no private memory");
}

/// What R's private memory holds after R and P are instantiated: zeros, R's
/// active data image, and P's active data segments on top.
struct ExpectedMemory {
    /// R's own segments, which P must not overlap.
    runtime: Vec<(usize, Vec<u8>)>,
    program: Vec<(usize, Vec<u8>)>,
}

impl ExpectedMemory {
    fn new(artifact: &Artifact) -> Self {
        let runtime = artifact.runtime.as_ref().expect("the program links R");
        let runtime = active_segments(
            runtime.bytes(),
            exported_private_memory_index(runtime.bytes()),
        );
        let image_bytes: usize = runtime.iter().map(|(_, bytes)| bytes.len()).sum();
        // R's static data (about 180 KB) spans several Wasm pages.
        assert!(
            image_bytes > 64 * 1024,
            "R's data image holds {image_bytes} bytes, too few to be its static data"
        );
        let program = imported_private_memory_index(&artifact.bytes)
            .map(|memory| active_segments(&artifact.bytes, memory))
            .unwrap_or_default();
        for (start, bytes) in &program {
            let end = start + bytes.len();
            for (image_start, image) in &runtime {
                assert!(
                    end <= *image_start || *start >= image_start + image.len(),
                    "P's segment {start:#x}..{end:#x} overlaps R's image"
                );
            }
        }
        Self { runtime, program }
    }

    /// The image-covered range: from the first byte R's data sets to the last.
    fn image_range(&self) -> std::ops::Range<usize> {
        let start = self.runtime.iter().map(|(start, _)| *start).min().unwrap();
        let end = self
            .runtime
            .iter()
            .map(|(start, bytes)| start + bytes.len())
            .max()
            .unwrap();
        start..end
    }

    fn bytes(&self, len: usize) -> Vec<u8> {
        let mut memory = vec![0; len];
        for (start, bytes) in self.runtime.iter().chain(&self.program) {
            memory[*start..start + bytes.len()].copy_from_slice(bytes);
        }
        memory
    }
}

/// The first byte of `actual` that differs from `expected`, described.
fn first_difference(
    actual: &[u8],
    expected: &[u8],
    image: &std::ops::Range<usize>,
) -> Option<String> {
    let offset = actual
        .iter()
        .zip(expected)
        .position(|(actual, expected)| actual != expected)?;
    let differing = actual
        .iter()
        .zip(expected)
        .filter(|(actual, expected)| actual != expected)
        .count();
    let region = if image.contains(&offset) {
        "inside R's data image"
    } else {
        "outside R's data image"
    };
    Some(format!(
        "byte {offset:#x} ({region} {:#x}..{:#x}) is {:#04x}, not {:#04x}; \
         {differing} of {:#x} bytes differ",
        image.start,
        image.end,
        actual[offset],
        expected[offset],
        actual.len()
    ))
}

/// Present host pages of `bytes` at `base`, from `/proc/self/pagemap`.
#[cfg(target_os = "linux")]
fn present_pages(base: *const u8, bytes: usize) -> (usize, usize) {
    use std::io::{Read, Seek, SeekFrom};
    let page = rustix::param::page_size();
    let first = base as usize / page;
    let last = (base as usize + bytes).div_ceil(page);
    let mut pagemap = std::fs::File::open("/proc/self/pagemap").unwrap();
    pagemap.seek(SeekFrom::Start(first as u64 * 8)).unwrap();
    let mut entries = vec![0u8; (last - first) * 8];
    pagemap.read_exact(&mut entries).unwrap();
    let present = entries
        .chunks_exact(8)
        .filter(|entry| u64::from_le_bytes((*entry).try_into().unwrap()) >> 63 == 1)
        .count();
    (present, last - first)
}

/// How the Engine allocates each Store's memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Allocation {
    Pooled(PooledStoreReset),
    OnDemand,
}

fn engines() -> Vec<(WasmtimeEngine, Allocation)> {
    let mut config = WasmtimeConfig::new();
    configure_wasm_runtime(
        &mut config,
        NativeCompilationPolicy::Fast,
        crate::CompilationJobs::current(),
    );
    let mut engines = Vec::new();
    let pagemap_scan = [true, false];
    for available in pagemap_scan {
        if available && !wasmtime::PoolingAllocationConfig::is_pagemap_scan_available() {
            continue;
        }
        let mut pooled = config.clone();
        let reset = configure_with_pagemap(&mut pooled, &PooledStores::sequential(), available);
        engines.push((
            WasmtimeEngine::new(&pooled).unwrap(),
            Allocation::Pooled(reset),
        ));
    }
    engines.push((WasmtimeEngine::new(&config).unwrap(), Allocation::OnDemand));
    engines
}

struct Instantiated {
    store: WasmtimeStore<WasmStoreState>,
    instance: EmbeddedInstance,
    memory: wasmtime::Memory,
}

fn instantiate(runtime: &EmbeddedRuntime) -> Instantiated {
    let mut store = store(runtime, 0.0);
    let mut linker = linker(runtime);
    runtime.register(&mut store, &mut linker).unwrap();
    let instance = runtime.instantiate(&mut store, &mut linker).unwrap();
    let memory = instance
        .roots
        .get_memory(&mut store, HOST_MODULE_MEMORY_EXPORT)
        .expect("R exports its private memory");
    Instantiated {
        store,
        instance,
        memory,
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn every_store_starts_from_rs_pristine_private_memory_after_a_store_wrote_over_its_image() {
    let artifact = artifact(&case_source(), registry(false));
    let expected = ExpectedMemory::new(&artifact);
    let image = expected.image_range();
    let output = case_output();
    for (engine, allocation) in engines() {
        let runtime = EmbeddedRuntime::compile(&engine, &artifact).unwrap();
        assert!(
            matches!(runtime.modules.memories().private(), PrivateMemory::Defined),
            "R defines its private memory"
        );
        let mut slot = None;
        for round in 0..3 {
            let context = format!("{allocation:?}, Store {round}");
            let Instantiated {
                mut store,
                instance,
                memory,
            } = instantiate(&runtime);
            let base = memory.data_ptr(&store) as usize;
            let len = memory.data_size(&store);
            assert!(image.end <= len, "{context}: R's image fits its memory");
            if let Allocation::Pooled(reset) = allocation {
                // A one-Store pool hands R's memory the slot it had before,
                // so every later Store exercises the slot's reset.
                assert_eq!(
                    *slot.get_or_insert(base),
                    base,
                    "{context}: R's memory reuses its warm slot"
                );
                // Before anything reads the memory: the image pages the last
                // Store dirtied were reset in place and stayed resident.
                #[cfg(target_os = "linux")]
                if round > 0 {
                    if let PooledStoreReset::KeepDirtyPagesResident { .. } = reset {
                        let (present, pages) = present_pages(
                            memory.data_ptr(&store).wrapping_add(image.start),
                            image.len(),
                        );
                        assert_eq!(
                            present, pages,
                            "{context}: the reset kept the dirtied image pages resident"
                        );
                    }
                }
                #[cfg(not(target_os = "linux"))]
                let _ = reset;
            }
            let pristine = expected.bytes(len);
            let image_mismatch = first_difference(memory.data(&store), &pristine, &image);
            // R's case tables decide this program's output.
            let outcome = instance.evaluate(&mut store);
            assert_eq!(
                image_mismatch, None,
                "{context}: R's private memory at instantiation is not R's data image \
                 with P's segments (the program then gave {outcome:?})"
            );
            assert_output(outcome.unwrap(), &output);
            // Write over every byte of the image and past it, as a Store whose
            // code corrupted R's data would: the next Store must not see it.
            let dirtied = (image.end + 64 * 1024).min(len);
            for (byte, pristine) in memory.data_mut(&mut store)[..dirtied]
                .iter_mut()
                .zip(&pristine)
            {
                *byte = !*pristine;
            }
            drop(store);
        }
    }
}
