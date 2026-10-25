//! The narrow trust boundary between immutable Cargo output and safe Intl
//! metadata admission. This crate has no dependencies or parsing/runtime code.

/// A producer-admitted complete Minimal component/catalogue snapshot.
/// Private fields prevent caller data acquiring build authority in safe code.
pub struct ImmutableIntlBuildData<Component> {
    metadata: &'static [u8],
    source: &'static [u8; 32],
    frames: Vec<(Component, &'static [u8])>,
}

impl<Component> ImmutableIntlBuildData<Component> {
    /// # Safety
    /// All inputs must be immutable output from the current compiler's Cargo
    /// producer. `metadata` must be its unchanged `minimal_build_metadata`
    /// output; `frames` must be the exact complete frames of that producer's
    /// fully admitted Minimal R package. It must have admitted all operation
    /// consumers and derived all six catalogues from those same consumers.
    /// Caller bytes, caches and re-encoded metadata cannot grant this authority.
    /// The consumer may reuse the producer's complete frame hashes rather than
    /// faulting every immutable frame into resident memory again.
    ///
    /// ```compile_fail
    /// use scriptw_intl_build_data::ImmutableIntlBuildData;
    /// let _ = ImmutableIntlBuildData::<u16>::new(b"caller bytes", &[0; 32], vec![]);
    /// ```
    pub unsafe fn new(
        metadata: &'static [u8],
        source: &'static [u8; 32],
        frames: Vec<(Component, &'static [u8])>,
    ) -> Self {
        Self {
            metadata,
            source,
            frames,
        }
    }

    /// Consuming the proof exposes inert views for the safe admission reader.
    pub fn into_parts(
        self,
    ) -> (
        &'static [u8],
        &'static [u8; 32],
        Vec<(Component, &'static [u8])>,
    ) {
        (self.metadata, self.source, self.frames)
    }
}
