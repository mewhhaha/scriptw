//! Address observation only, for excluded benchmark diagnostics after capture.
use super::EmbeddedRuntime;
use std::ops::Range;

/// Independent image and native text addresses; neither range implies that its
/// pages are resident, nor that the image and text ranges contain one another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkModuleImage {
    image: Range<usize>,
    text: Range<usize>,
}

impl BenchmarkModuleImage {
    fn observe(module: &wasmtime::Module) -> Self {
        let image = module.image_range();
        let text = module.text().as_ptr_range();
        Self {
            image: image.start.addr()..image.end.addr(),
            text: text.start.addr()..text.end.addr(),
        }
    }

    pub fn image_range(&self) -> Range<usize> {
        self.image.clone()
    }

    pub fn text_range(&self) -> Range<usize> {
        self.text.clone()
    }
}

/// The immutable actual modules retained by one admitted embedding owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkModuleImages {
    program: BenchmarkModuleImage,
    runtime: Option<BenchmarkModuleImage>,
}

impl BenchmarkModuleImages {
    pub fn program(&self) -> &BenchmarkModuleImage {
        &self.program
    }

    pub fn runtime(&self) -> Option<&BenchmarkModuleImage> {
        self.runtime.as_ref()
    }
}

impl EmbeddedRuntime {
    /// Observe the actual immutable P/R owners without traversing code bytes,
    /// names, function metadata, or compiler state. These addresses remain valid
    /// while this embedding owner lives, including after its Stores are dropped.
    /// Call only after the final acknowledged diagnostic capture: even metadata
    /// access must not perturb the preceding phase observations.
    pub fn benchmark_module_images(&self) -> BenchmarkModuleImages {
        BenchmarkModuleImages {
            program: BenchmarkModuleImage::observe(&self.modules.program),
            runtime: self
                .modules
                .runtime
                .as_ref()
                .map(BenchmarkModuleImage::observe),
        }
    }
}
