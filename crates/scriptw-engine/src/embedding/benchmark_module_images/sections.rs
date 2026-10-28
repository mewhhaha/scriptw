//! Pinned Wasmtime 47 serializes the exact EngineCode image exposed by
//! Module::image_range. File-backed ELF section offsets therefore identify
//! bytes in that same original image; no ELF virtual-address rebasing is used.
use super::EngineError;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ops::Range;

pub(super) const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_SECTIONS: usize = 4096;
const MAX_NAME_BYTES: usize = 2048;
const MAX_NAMES_BYTES: usize = 128 * 1024;

/// Metadata only; all ranges and offsets are relative to the original image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkModuleSections {
    serialized_bytes: usize,
    serialized_sha256: String,
    elf_header: Range<usize>,
    section_table: Range<usize>,
    file_sections: Vec<BenchmarkModuleSection>,
}

impl BenchmarkModuleSections {
    pub fn serialized_bytes(&self) -> usize {
        self.serialized_bytes
    }
    pub fn serialized_sha256(&self) -> &str {
        &self.serialized_sha256
    }
    pub fn elf_header_range(&self) -> Range<usize> {
        self.elf_header.clone()
    }
    pub fn section_table_range(&self) -> Range<usize> {
        self.section_table.clone()
    }
    pub fn file_sections(&self) -> &[BenchmarkModuleSection] {
        &self.file_sections
    }
}

/// One nonempty file-backed section. Null, empty and NOBITS rows are omitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkModuleSection {
    name: String,
    offset: usize,
    bytes: usize,
    flags: u64,
    section_type: u32,
}

impl BenchmarkModuleSection {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn flags(&self) -> u64 {
        self.flags
    }
    pub fn section_type(&self) -> u32 {
        self.section_type
    }
}

pub(super) fn invalid(message: impl std::fmt::Display) -> EngineError {
    EngineError::new(format!("benchmark Module sections: {message}"))
}

fn field<const N: usize>(raw: &[u8], offset: usize) -> Result<[u8; N], EngineError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| invalid("ELF field overflow"))?;
    let bytes = raw
        .get(offset..end)
        .ok_or_else(|| invalid("truncated ELF field"))?;
    Ok(bytes.try_into().expect("checked fixed field width"))
}
fn u16_at(raw: &[u8], offset: usize) -> Result<u16, EngineError> {
    Ok(u16::from_le_bytes(field(raw, offset)?))
}
fn u32_at(raw: &[u8], offset: usize) -> Result<u32, EngineError> {
    Ok(u32::from_le_bytes(field(raw, offset)?))
}
fn u64_at(raw: &[u8], offset: usize) -> Result<u64, EngineError> {
    Ok(u64::from_le_bytes(field(raw, offset)?))
}
fn size(value: u64) -> Result<usize, EngineError> {
    usize::try_from(value).map_err(|_| invalid("ELF extent does not fit host address space"))
}
fn extent(offset: usize, bytes: usize, length: usize) -> Result<Range<usize>, EngineError> {
    let end = offset
        .checked_add(bytes)
        .ok_or_else(|| invalid("ELF extent overflow"))?;
    if end > length {
        return Err(invalid("ELF file extent exceeds original image"));
    }
    Ok(offset..end)
}
fn overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

struct Section {
    name: usize,
    kind: u32,
    flags: u64,
    offset: usize,
    bytes: usize,
}

pub(super) fn parse(
    raw: &[u8],
    actual_text: Range<usize>,
) -> Result<BenchmarkModuleSections, EngineError> {
    if !(64..=MAX_IMAGE_BYTES).contains(&raw.len())
        || raw.get(..7) != Some(b"\x7fELF\x02\x01\x01".as_slice())
    {
        return Err(invalid("requires bounded ELF64 little-endian image"));
    }
    if !matches!(u16_at(raw, 16)?, 1..=3) || u32_at(raw, 20)? != 1 || u16_at(raw, 52)? != 64 {
        return Err(invalid("unsupported ELF header"));
    }
    let section_count = usize::from(u16_at(raw, 60)?);
    let names_index = usize::from(u16_at(raw, 62)?);
    if !(1..=MAX_SECTIONS).contains(&section_count)
        || u16_at(raw, 58)? != 64
        || names_index == 0
        || names_index >= section_count
    {
        return Err(invalid("unsupported ELF section counts/width/name index"));
    }
    let header = 0..64;
    let table = extent(size(u64_at(raw, 40)?)?, section_count * 64, raw.len())?;
    if table.start % 8 != 0 || overlap(&header, &table) {
        return Err(invalid(
            "section table is misaligned or overlaps ELF header",
        ));
    }
    let program_count = usize::from(u16_at(raw, 56)?);
    let program_offset = size(u64_at(raw, 32)?)?;
    let program_width = usize::from(u16_at(raw, 54)?);
    if program_count == 0 {
        if program_offset != 0 || program_width != 0 {
            return Err(invalid("empty ELF program table has nonzero extent"));
        }
    } else {
        if program_count > 256 || program_width != 56 {
            return Err(invalid("unsupported ELF program table"));
        }
        let programs = extent(program_offset, program_count * program_width, raw.len())?;
        if overlap(&programs, &header) || overlap(&programs, &table) {
            return Err(invalid("ELF metadata tables overlap"));
        }
    }
    let mut sections = Vec::with_capacity(section_count);
    for index in 0..section_count {
        let at = table.start + index * 64;
        let kind = u32_at(raw, at + 4)?;
        if index == 0 && kind != 0 {
            return Err(invalid("ELF section zero must be null"));
        }
        let bytes = size(u64_at(raw, at + 32)?)?;
        let address = u64_at(raw, at + 16)?;
        address
            .checked_add(bytes as u64)
            .ok_or_else(|| invalid("ELF section virtual extent overflow"))?;
        let offset = size(u64_at(raw, at + 24)?)?;
        let alignment = u64_at(raw, at + 48)?;
        if bytes > MAX_IMAGE_BYTES || (alignment > 1 && !alignment.is_power_of_two()) {
            return Err(invalid("oversized/misaligned ELF section declaration"));
        }
        if kind == 0 {
            if index != 0 || field::<64>(raw, at)?.iter().any(|byte| *byte != 0) {
                return Err(invalid("malformed null ELF section"));
            }
        } else if kind == 8 {
            // NOBITS has no file payload. Validate its logical arithmetic but
            // never translate its size into resident image bytes.
            if offset > raw.len() || offset.checked_add(bytes).is_none() {
                return Err(invalid("malformed NOBITS extent"));
            }
        } else {
            extent(offset, bytes, raw.len())?;
            if bytes != 0 && alignment > 1 && offset as u64 % alignment != 0 {
                return Err(invalid("file-backed ELF section is not aligned"));
            }
        }
        sections.push(Section {
            name: u32_at(raw, at)? as usize,
            kind,
            flags: u64_at(raw, at + 8)?,
            offset,
            bytes,
        });
    }
    let names = &sections[names_index];
    if names.kind != 3 || names.bytes == 0 || names.bytes > MAX_NAMES_BYTES {
        return Err(invalid("invalid ELF section-name string table"));
    }
    let names = &raw[extent(names.offset, names.bytes, raw.len())?];
    if names.first() != Some(&0) {
        return Err(invalid("ELF section names lack initial NUL"));
    }
    let mut name_bytes = 0usize;
    let mut used_names = BTreeSet::new();
    let mut file_sections = Vec::new();
    let mut text = None;
    for section in sections {
        let tail = names
            .get(section.name..)
            .ok_or_else(|| invalid("ELF name outside string table"))?;
        let end = tail
            .iter()
            .take(MAX_NAME_BYTES + 1)
            .position(|byte| *byte == 0)
            .ok_or_else(|| invalid("unterminated/oversized ELF section name"))?;
        name_bytes = name_bytes
            .checked_add(end)
            .ok_or_else(|| invalid("ELF name budget overflow"))?;
        if name_bytes > MAX_NAMES_BYTES {
            return Err(invalid("ELF section name budget exceeded"));
        }
        let name = std::str::from_utf8(&tail[..end])
            .map_err(|_| invalid("ELF section name is not UTF-8"))?;
        if !name.is_empty() && !used_names.insert(name) {
            return Err(invalid("duplicate ELF section name"));
        }
        if name == ".text" {
            if section.kind != 1 || section.flags & 4 == 0 || section.bytes == 0 || text.is_some() {
                return Err(invalid(
                    "invalid/duplicate file-backed executable .text section",
                ));
            }
            text = Some(extent(section.offset, section.bytes, raw.len())?);
        }
        if section.kind == 0 || section.kind == 8 || section.bytes == 0 {
            continue;
        }
        if name.is_empty() {
            return Err(invalid("nonempty file section has an empty name"));
        }
        let span = extent(section.offset, section.bytes, raw.len())?;
        if overlap(&span, &header) || overlap(&span, &table) {
            return Err(invalid(
                "file-backed ELF section overlaps metadata/another section",
            ));
        }
        file_sections.push(BenchmarkModuleSection {
            name: name.to_owned(),
            offset: section.offset,
            bytes: section.bytes,
            flags: section.flags,
            section_type: section.kind,
        });
    }
    file_sections.sort_unstable_by_key(|section| section.offset);
    if file_sections
        .windows(2)
        .any(|pair| pair[0].offset + pair[0].bytes > pair[1].offset)
    {
        return Err(invalid("file-backed ELF sections overlap"));
    }
    if text.as_ref() != Some(&actual_text) || actual_text.start >= actual_text.end {
        return Err(invalid(
            "ELF .text offset/length differs from actual Module text",
        ));
    }
    Ok(BenchmarkModuleSections {
        serialized_bytes: raw.len(),
        serialized_sha256: format!("{:x}", Sha256::digest(raw)),
        elf_header: header,
        section_table: table,
        file_sections,
    })
}

#[cfg(test)]
mod tests;
