//! Owned-image metadata for excluded diagnostics after their final capture.
use super::{EmbeddedRuntime, EngineError};
use std::ops::Range;

mod sections;
pub use sections::{BenchmarkModuleSection, BenchmarkModuleSections};

/// Original Module image/text addresses and checked, image-relative sections.
/// Neither ranges nor section membership establish residency or execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkModuleImage {
    image: Range<usize>,
    text: Range<usize>,
    sections: BenchmarkModuleSections,
}

impl BenchmarkModuleImage {
    /// Capture this exact retained Module only after all diagnostic captures.
    /// Serialization reads the complete image and can fault previously cold
    /// pages in. Never use residency observed after this call as a capture.
    pub fn capture(module: &wasmtime::Module) -> Result<Self, EngineError> {
        let image = module.image_range();
        let text = module.text().as_ptr_range();
        let image = image.start.addr()..image.end.addr();
        let text = text.start.addr()..text.end.addr();
        let bytes = image
            .end
            .checked_sub(image.start)
            .ok_or_else(|| sections::invalid("reversed Module image range"))?;
        if bytes == 0 || bytes > sections::MAX_IMAGE_BYTES {
            return Err(sections::invalid(
                "Module image exceeds the 128-MiB diagnostic capability",
            ));
        }
        if !(image.start <= text.start && text.start < text.end && text.end <= image.end) {
            return Err(sections::invalid(
                "Module text is empty or outside its original image",
            ));
        }
        let serialized = module.serialize().map_err(|error| {
            sections::invalid(format!(
                "actual Module serialization unavailable: {error:#}"
            ))
        })?;
        if serialized.len() != bytes {
            return Err(sections::invalid(
                "serialized length differs from actual Module image",
            ));
        }
        let sections = sections::parse(
            &serialized,
            text.start - image.start..text.end - image.start,
        )?;
        // Only metadata escapes. No serialized code/data buffer is retained.
        Ok(Self {
            image,
            text,
            sections,
        })
    }

    pub fn image_range(&self) -> Range<usize> {
        self.image.clone()
    }

    pub fn text_range(&self) -> Range<usize> {
        self.text.clone()
    }

    pub fn sections(&self) -> &BenchmarkModuleSections {
        &self.sections
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
    /// Capture the actual immutable P/R owners after the final acknowledged
    /// diagnostic capture. Serialization reads each complete original image;
    /// returned metadata retains no image bytes or Module ownership. Addresses
    /// remain valid only while this owner lives, including after Store drop.
    pub fn benchmark_module_images(&self) -> Result<BenchmarkModuleImages, EngineError> {
        Ok(BenchmarkModuleImages {
            program: BenchmarkModuleImage::capture(&self.modules.program)?,
            runtime: self
                .modules
                .runtime
                .as_ref()
                .map(BenchmarkModuleImage::capture)
                .transpose()?,
        })
    }
}
