//! Share immutable program source slices without changing String identities.
use super::*;

impl StringPool {
    pub(super) fn compact_program_utf16(&mut self) {
        let boundary = self.compiler_owned;
        let original = &self.pooled_code_unit_bytes[boundary.code_unit_bytes..];
        let count = self.pooled_string_count() - boundary.strings;
        if count < 2 || original.is_empty() {
            return;
        }
        let mut strings: Vec<_> = self
            .refs
            .program_values()
            .filter(|entry| entry.index.ordinal() >= boundary.strings)
            .collect();
        strings.sort_unstable_by(|a, b| {
            b.code_units
                .length
                .cmp(&a.code_units.length)
                .then_with(|| a.index.ordinal().cmp(&b.index.ordinal()))
        });
        // Bound search work independently of the number of literals. Exhausting
        // the budget only appends exact bytes; it never changes string contents.
        let mut budget = original.len().saturating_mul(64).min(64 * 1024 * 1024);
        let mut packed = Vec::with_capacity(original.len());
        let mut offsets = vec![0; count as usize];
        for entry in strings {
            let start = entry.code_units.byte_offset as usize - boundary.code_unit_bytes;
            let end = start + entry.code_units.length as usize * 2;
            let bytes = &original[start..end];
            let offset = match aligned_substring(&packed, bytes, &mut budget) {
                Some(offset) => offset,
                None => {
                    let offset = packed.len();
                    packed.extend_from_slice(bytes);
                    offset
                }
            };
            offsets[(entry.index.ordinal() - boundary.strings) as usize] =
                u32::try_from(boundary.code_unit_bytes + offset)
                    .expect("packed UTF-16 retains the original Wasm byte domain");
        }
        if packed.len() >= original.len() {
            return;
        }
        // No code has been emitted yet. Ordinals, WTF-8 metadata addresses and
        // the shared runtime prefix stay fixed; only final UTF-16 spans change.
        self.pooled_code_unit_bytes
            .replace_suffix(boundary.code_unit_bytes, packed);
        for entry in self.refs.program_values_mut() {
            if entry.index.ordinal() >= boundary.strings {
                entry.code_units.byte_offset =
                    offsets[(entry.index.ordinal() - boundary.strings) as usize];
            }
        }
    }
}

fn aligned_substring(haystack: &[u8], needle: &[u8], budget: &mut usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut start = 0;
    loop {
        let remaining = haystack.len() - start;
        if needle.len() > remaining || remaining > *budget {
            return None;
        }
        *budget -= remaining;
        let found = start + memchr::memmem::find(&haystack[start..], needle)?;
        if found % 2 == 0 {
            return Some(found);
        }
        start = found + 1;
    }
}
