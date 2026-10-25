use super::*;

/// Checked rows in R's existing private static segment. Only compiler-pool
/// sealing can create this handle; P literals cannot extend these rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeStringDescriptors {
    address: u32,
    count: u32,
}
impl RuntimeStringDescriptors {
    pub(crate) const ROW_BYTES: u32 = 12;
    pub(crate) fn address(self) -> u32 {
        self.address
    }
    pub(crate) fn count(self) -> u32 {
        self.count
    }
}
impl StringPool {
    pub(super) fn append_runtime_string_descriptors(&mut self) -> Result<(), EmitError> {
        let error = || {
            EmitError::unsupported(
                "runtime string descriptors exceed their checked static/UTF16 domain",
            )
        };
        let count = self.pooled_string_count();
        let length = count
            .checked_mul(RuntimeStringDescriptors::ROW_BYTES)
            .ok_or_else(error)?;
        let address = u32::try_from(self.bytes.len())
            .ok()
            .and_then(|offset| STATIC_DATA_OFFSET.checked_add(offset))
            .ok_or_else(error)?;
        address.checked_add(length).ok_or_else(error)?;
        let mut rows = Vec::with_capacity(length as usize);
        let mut seen = vec![false; count as usize];
        for (ordinal, units) in self.pooled_string_initializers() {
            let slot = seen.get_mut(ordinal.ordinal() as usize).ok_or_else(error)?;
            if std::mem::replace(slot, true)
                || units.byte_offset % 2 != 0
                || units
                    .length
                    .checked_mul(2)
                    .and_then(|len| units.byte_offset.checked_add(len))
                    .is_none_or(|end| end as usize > self.pooled_code_unit_bytes.len())
            {
                return Err(error());
            }
            for word in [ordinal.ordinal(), units.byte_offset, units.length] {
                rows.extend_from_slice(&word.to_le_bytes());
            }
        }
        if seen.iter().any(|present| !present) || rows.len() != length as usize {
            return Err(error());
        }
        self.bytes.extend_from_slice(&rows);
        self.runtime_string_descriptors = Some(RuntimeStringDescriptors { address, count });
        Ok(())
    }
    pub(crate) fn runtime_string_descriptors(&self) -> Result<RuntimeStringDescriptors, EmitError> {
        self.runtime_string_descriptors
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.runtime_string_descriptors)
            })
            .ok_or_else(|| EmitError::unsupported("missing sealed runtime string descriptors"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_rows_preserve_ordinals_utf16_spans_and_initialization_order() {
        let mut pool = StringPool::default();
        for text in ["z", "", "\0", "a", "\u{1f600}", "\u{10ffff}"] {
            pool.intern_string(text);
        }
        let expected = pool
            .pooled_string_initializers()
            .map(|(index, units)| [index.ordinal(), units.byte_offset, units.length])
            .collect::<Vec<_>>();
        assert!(
            expected.windows(2).any(|rows| rows[0][0] > rows[1][0]),
            "lexical initialization order differs from insertion ordinals"
        );
        let utf16 = pool.pooled_code_unit_bytes[..].to_vec();
        let prefix = pool.bytes[..].to_vec();
        pool.append_runtime_string_descriptors().unwrap();
        let descriptors = pool.runtime_string_descriptors().unwrap();
        assert_eq!(descriptors.count() as usize, expected.len());
        assert_eq!(&pool.bytes[..prefix.len()], &prefix);
        assert_eq!(&pool.pooled_code_unit_bytes[..], &utf16);
        let offset = (descriptors.address() - STATIC_DATA_OFFSET) as usize;
        let actual = pool.bytes[offset..]
            .chunks_exact(12)
            .map(|row| {
                std::array::from_fn::<_, 3, _>(|word| {
                    u32::from_le_bytes(row[word * 4..word * 4 + 4].try_into().unwrap())
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_table_has_no_rows_and_bad_utf16_span_cannot_mint_a_handle() {
        let mut empty = StringPool::default();
        empty.append_runtime_string_descriptors().unwrap();
        assert_eq!(empty.runtime_string_descriptors().unwrap().count(), 0);
        assert!(empty.bytes.is_empty());
        let mut bad = StringPool::default();
        bad.intern_string("invalid span");
        bad.pooled_code_unit_bytes = Vec::new().into();
        assert!(bad.append_runtime_string_descriptors().is_err());
        assert!(bad.runtime_string_descriptors().is_err());
    }
}
