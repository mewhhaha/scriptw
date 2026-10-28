//! Receipt emission runs only after complete raw producer admission.
use super::*;
use wasm_encoder::Encode;
use wasmparser::{Parser, Payload};

fn word(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u64).to_le_bytes());
}
fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}
fn span(out: &mut Vec<u8>, package: &[u8], bytes: &[u8]) {
    let start = bytes
        .as_ptr()
        .addr()
        .checked_sub(package.as_ptr().addr())
        .expect("producer package span");
    assert!(start
        .checked_add(bytes.len())
        .is_some_and(|end| end <= package.len()));
    word(out, start);
    word(out, bytes.len());
    out.extend_from_slice(&Sha256::digest(bytes));
}

impl RuntimeBuildArtifact {
    /// A small receipt bound to this fully validated package, source and Intl
    /// compilation snapshot. Only immutable Cargo output may reuse it.
    pub fn immutable_build_metadata(
        &self,
        selection: &scriptw_intl::IntlDataSelection,
        source: &[u8; 32],
        intl_metadata: &[u8],
    ) -> Result<Vec<u8>, EmitError> {
        let profile = self.runtime.source_compilation_profile();
        let identity = cache::RuntimeIdentity::admit(selection, profile)?;
        let selected = selection
            .compilation_data()
            .map_err(|error| EmitError::unsupported(error.to_string()))?;
        if selected.identity().profile().profile() != &scriptw_intl::IntlDataProfile::Minimal {
            return Err(EmitError::unsupported(
                "immutable build metadata requires the complete Minimal profile",
            ));
        }
        let mut out = METADATA_MAGIC.to_vec();
        out.extend_from_slice(&LAYOUT_VERSION.to_le_bytes());
        out.extend_from_slice(source);
        out.push(profile.wire());
        word(&mut out, self.package.len());
        out.extend_from_slice(&Sha256::digest(&self.package));
        out.extend_from_slice(&self.package[self.package.len() - 32..]);
        let offset = self.package.len() - 32 - self.runtime.bytes().len();
        word(&mut out, offset);
        word(&mut out, self.runtime.bytes().len());
        out.extend_from_slice(&self.runtime.wasm_sha256());
        out.extend_from_slice(self.runtime.key().as_bytes());
        out.extend_from_slice(&identity.digest());
        field(&mut out, selected.identity().artifact_identity().as_bytes());
        out.extend_from_slice(&Sha256::digest(intl_metadata));
        let layout = self.runtime.layout();
        word(&mut out, layout.pool.static_len());
        word(&mut out, layout.pool.code_unit_len());
        out.extend_from_slice(&layout.pool.strings().to_le_bytes());
        out.extend_from_slice(&layout.imported_functions.to_le_bytes());
        out.extend_from_slice(&(layout.function_types.len() as u32).to_le_bytes());
        for index in &layout.function_types {
            out.extend_from_slice(&index.to_le_bytes());
        }
        out.extend_from_slice(&(layout.globals.len() as u32).to_le_bytes());
        for global in &layout.globals {
            global.encode(&mut out);
        }
        let wasm = &self.package[offset..self.package.len() - 32];
        let mut largest_body = 0;
        let mut data = Vec::new();
        let mut frames = std::collections::BTreeMap::new();
        for payload in Parser::new(0).parse_all(wasm) {
            match payload.map_err(|error| EmitError::unsupported(error.to_string()))? {
                Payload::CodeSectionEntry(body) => {
                    largest_body = largest_body.max(body.range().len())
                }
                Payload::DataSection(reader) => {
                    for row in reader {
                        data.push(
                            row.map_err(|error| EmitError::unsupported(error.to_string()))?
                                .data,
                        );
                    }
                }
                Payload::CustomSection(section) => {
                    if let Some(component) = IntlDataComponent::ALL
                        .into_iter()
                        .find(|component| component.section_name() == section.name())
                    {
                        assert!(frames.insert(component, section.data()).is_none());
                    }
                }
                _ => {}
            }
        }
        word(&mut out, largest_body);
        assert_eq!(data.len(), RUNTIME_DATA_SEGMENT_COUNT);
        for bytes in data {
            span(&mut out, &self.package, bytes);
        }
        assert_eq!(frames.len(), IntlDataComponent::ALL.len());
        for component in IntlDataComponent::ALL {
            out.extend_from_slice(&component.code().to_le_bytes());
            span(&mut out, &self.package, frames[&component]);
        }
        if out.len() + 32 > MAX_METADATA_BYTES {
            return Err(EmitError::unsupported(
                "immutable runtime receipt exceeds its bound",
            ));
        }
        out.extend_from_slice(&Sha256::digest(&out));
        Ok(out)
    }
}
