use super::*;

/// Checked rows in R's existing private static segment. Only compiler-pool
/// sealing can create this handle; P literals cannot extend these rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeStringDescriptors {
    address: u32,
    count: u32,
}
impl RuntimeStringDescriptors {
    pub(crate) const ROW_BYTES: u32 = 16;
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
        let mut rows = vec![0; length as usize];
        let mut seen = vec![false; count as usize];
        for (ordinal, units) in self.pooled_string_initializers() {
            let slot = seen.get_mut(ordinal.ordinal() as usize).ok_or_else(error)?;
            let end = units
                .length
                .checked_mul(2)
                .and_then(|len| units.byte_offset.checked_add(len))
                .ok_or_else(error)?;
            if std::mem::replace(slot, true)
                || units.byte_offset % 2 != 0
                || end as usize > self.pooled_code_unit_bytes.len()
            {
                return Err(error());
            }
            // Match emit_string_key_hash's raw UTF-16 FNV-1a exactly, before
            // caller-specific tag mixing. A real zero retains its sentinel
            // meaning and may be safely recomputed by the runtime owner.
            let mut raw_hash = 14695981039346656037u64;
            for unit in self.pooled_code_unit_bytes[units.byte_offset as usize..end as usize]
                .chunks_exact(2)
            {
                raw_hash ^= u64::from(u16::from_le_bytes(unit.try_into().unwrap()));
                raw_hash = raw_hash.wrapping_mul(1099511628211);
            }
            let start = ordinal.ordinal() as usize * RuntimeStringDescriptors::ROW_BYTES as usize;
            let row = &mut rows[start..start + RuntimeStringDescriptors::ROW_BYTES as usize];
            row[..8].copy_from_slice(&raw_hash.to_le_bytes());
            row[8..12].copy_from_slice(&units.byte_offset.to_le_bytes());
            row[12..16].copy_from_slice(&units.length.to_le_bytes());
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
    fn descriptor_rows_bind_exact_utf16_hashes_and_spans_in_ordinal_order() {
        let mut pool = StringPool::default();
        // Known raw hashes include lone surrogates, a surrogate pair, NUL and
        // a non-ASCII unit whose hash differs from hashing its encoded bytes.
        let cases: &[(&[u16], u64)] = &[
            (&[0x7a], 0xaf63f74c86021a6d),
            (&[], 0xcbf29ce484222325),
            (&[0], 0xaf63bd4c8601b7df),
            (&[0x61], 0xaf63dc4c8601ec8c),
            (&[0x100], 0xaf62bd4c860004df),
            (&[0x61, 0, 0x62], 0xe5d29919042666b2),
            (&[0xd800], 0xb03bbd4c8770bfdf),
            (&[0xdfff], 0xb03d724c8773a66e),
            (&[0xd83d, 0xde00], 0xe5e45a0a241b88d8),
            (&[0xdbff, 0xdfff], 0xdda2c90a1c8d7563),
        ];
        for (units, _) in cases {
            pool.intern_string(&scriptw_ir::encode_js_string_utf16(units));
        }
        let expected = pool
            .pooled_string_initializers()
            .map(|(index, units)| {
                (
                    index.ordinal(),
                    (
                        cases[index.ordinal() as usize].1,
                        units.byte_offset,
                        units.length,
                    ),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            expected.windows(2).any(|rows| rows[0].0 > rows[1].0),
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
            .chunks_exact(RuntimeStringDescriptors::ROW_BYTES as usize)
            .map(|row| {
                (
                    u64::from_le_bytes(row[..8].try_into().unwrap()),
                    u32::from_le_bytes(row[8..12].try_into().unwrap()),
                    u32::from_le_bytes(row[12..16].try_into().unwrap()),
                )
            })
            .collect::<Vec<_>>();
        let mut ordered = expected;
        ordered.sort_by_key(|row| row.0);
        assert_eq!(
            actual,
            ordered.into_iter().map(|row| row.1).collect::<Vec<_>>()
        );
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
