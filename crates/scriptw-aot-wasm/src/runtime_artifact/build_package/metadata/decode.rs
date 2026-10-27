//! Bounded parsing of a producer receipt; its caller owns build authority.
use super::*;

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        cache::take(&mut self.0)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take()?))
    }
    fn word(&mut self) -> Option<usize> {
        usize::try_from(u64::from_le_bytes(self.take()?)).ok()
    }
    fn field(&mut self) -> Option<&'a [u8]> {
        let n = usize::try_from(self.u32()?).ok()?;
        let (value, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(value)
    }
    fn span(&mut self, wasm: &Range<usize>) -> Option<SpanDigest> {
        let start = self.word()?;
        let end = start.checked_add(self.word()?)?;
        if start < wasm.start || end > wasm.end {
            return None;
        }
        Some(SpanDigest {
            span: start..end,
            digest: self.take()?,
        })
    }
}

impl BuildMetadata {
    // This function is private to the unsafe build-candidate boundary. A valid
    // checksum protects framing mistakes; it cannot authorize caller metadata.
    pub(in crate::runtime_artifact::build_package) fn decode(
        proof: super::super::inputs::ImmutableMetadata,
        profile: RuntimeSourceCompilationProfile,
    ) -> Option<Self> {
        let (package, source, intl_metadata, metadata) = proof.parts();
        Self::parse(metadata, package, source, profile, intl_metadata)
    }

    fn parse(
        metadata: &[u8],
        package: &'static [u8],
        source: &[u8; 32],
        profile: RuntimeSourceCompilationProfile,
        intl_metadata: &[u8],
    ) -> Option<Self> {
        if metadata.len() > MAX_METADATA_BYTES {
            return None;
        }
        let (body, checksum) = metadata.split_at_checked(metadata.len().checked_sub(32)?)?;
        if Sha256::digest(body).as_slice() != checksum {
            return None;
        }
        let mut r = Reader(body);
        if &r.take::<8>()? != METADATA_MAGIC
            || r.u32()? != LAYOUT_VERSION
            || &r.take::<32>()? != source
            || r.take::<1>()? != [profile.wire()]
            || r.word()? != package.len()
        {
            return None;
        }
        let package_digest = r.take()?;
        let envelope_digest = r.take::<32>()?;
        if package.get(package.len().checked_sub(32)?..)? != envelope_digest {
            return None;
        }
        let start = r.word()?;
        let end = start.checked_add(r.word()?)?;
        if end != package.len().checked_sub(32)? {
            return None;
        }
        let wasm = start..end;
        package.get(wasm.clone())?;
        let wasm_digest = r.take()?;
        let key = r.take()?;
        let selection_digest = r.take()?;
        let intl_identity = std::str::from_utf8(r.field()?).ok()?.into();
        let intl_metadata_digest = r.take::<32>()?;
        if Sha256::digest(intl_metadata).as_slice() != intl_metadata_digest {
            return None;
        }
        // Check the small raw envelope too, without reading the Wasm payload.
        let mut header = Reader(package.get(..start)?);
        if &header.take::<8>()? != MAGIC
            || header.u32()? != LAYOUT_VERSION
            || &header.take::<32>()? != source
            || header.take::<32>()? != selection_digest
            || header.take::<32>()? != key
        {
            return None;
        }
        let static_len = r.word()?;
        let utf16_len = r.word()?;
        let strings = r.u32()?;
        let pool = PoolBoundary::from_runtime_cache(static_len, utf16_len, strings)?;
        if header.word()? != static_len
            || header.word()? != utf16_len
            || header.u32()? != strings
            || header.word()? != wasm.len()
            || !header.0.is_empty()
        {
            return None;
        }
        let imported_functions = r.u32()?;
        let count = usize::try_from(r.u32()?).ok()?;
        if count == 0 || count > r.0.len() / 4 {
            return None;
        }
        let function_types = (0..count).map(|_| r.u32()).collect::<Option<Vec<_>>>()?;
        let count = usize::try_from(r.u32()?).ok()?;
        if count == 0 || count > r.0.len() / 2 {
            return None;
        }
        let mut globals = Vec::with_capacity(count);
        for _ in 0..count {
            let mut reader = wasmparser::BinaryReader::new(r.0, 0);
            let global: wasmparser::GlobalType = reader.read().ok()?;
            globals.push(wasm_encoder::GlobalType::try_from(global).ok()?);
            r.0 = r.0.get(reader.original_position()..)?;
        }
        let largest_body = r.word()?;
        if largest_body == 0 || largest_body > wasm.len() {
            return None;
        }
        let data = [
            r.span(&wasm)?,
            r.span(&wasm)?,
            r.span(&wasm)?,
            r.span(&wasm)?,
            r.span(&wasm)?,
            r.span(&wasm)?,
        ];
        if data[0].span.len() != utf16_len
            || data[1..]
                .iter()
                .try_fold(0usize, |sum, row| sum.checked_add(row.span.len()))?
                != static_len
            || data
                .windows(2)
                .any(|rows| rows[0].span.end > rows[1].span.start)
        {
            return None;
        }
        let mut frames = Vec::with_capacity(IntlDataComponent::ALL.len());
        for component in IntlDataComponent::ALL {
            if r.take::<2>()? != component.code().to_le_bytes() {
                return None;
            }
            frames.push((component, r.span(&wasm)?));
        }
        // Duplicate/overlapping spans cannot describe the producer's sections.
        let mut spans: Vec<_> = frames
            .iter()
            .map(|(_, frame)| frame.span.clone())
            .chain(data.iter().map(|data| data.span.clone()))
            .collect();
        spans.sort_by_key(|span| span.start);
        if !r.0.is_empty() || spans.windows(2).any(|pair| pair[0].end > pair[1].start) {
            return None;
        }
        Some(Self {
            package,
            package_digest,
            wasm,
            wasm_digest,
            key,
            selection_digest,
            intl_identity,
            profile,
            layout: RuntimeLayout::new(imported_functions, function_types, globals, pool),
            largest_body,
            data,
            frames,
        })
    }

    /// Corruption controls inspect parser rejection without exposing authority.
    #[cfg(test)]
    pub(super) fn parser_rejects(
        metadata: &[u8],
        package: &'static [u8],
        source: &[u8; 32],
        profile: RuntimeSourceCompilationProfile,
        intl_metadata: &[u8],
    ) -> bool {
        Self::parse(metadata, package, source, profile, intl_metadata).is_none()
    }
}
