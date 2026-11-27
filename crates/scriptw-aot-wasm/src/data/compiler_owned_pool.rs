//! Canonical immutable compiler data, admitted once by the exact R owner.
use super::*;
use scriptw_intl::IntlDataSelection;
use std::sync::Arc;

const BUILD_PREFIX_MAGIC: &[u8; 8] = b"SCRWSP01";

struct PrefixReader<'a>(&'a [u8]);

impl<'a> PrefixReader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (value, rest) = self.0.split_at_checked(count)?;
        self.0 = rest;
        Some(value)
    }

    fn word(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn flag(&mut self) -> Option<bool> {
        match self.take(1)? {
            [0] => Some(false),
            [1] => Some(true),
            _ => None,
        }
    }

    fn table(&mut self, row_bytes: u32, bound: u32) -> Option<(u32, u32)> {
        let pointer = self.word()?;
        let count = self.word()?;
        if pointer == 0 && count == 0 {
            return Some((pointer, count));
        }
        let end = pointer.checked_add(count.checked_mul(row_bytes)?)?;
        (pointer >= STATIC_DATA_OFFSET && end <= bound).then_some((pointer, count))
    }
}

impl CompilerOwnedPool {
    /// Describe the exact compiler names and addresses from the producer's
    /// sealed R pool. The raw R payload already owns the UTF-16 and static
    /// images, so the build receipt carries only the lookup data.
    pub(crate) fn encode_build_prefix(pool: &StringPool) -> Result<Vec<u8>, EmitError> {
        pool.require_runtime_only()?;
        let table_start = pool
            .runtime_table_start
            .and_then(|start| u32::try_from(start).ok())
            .ok_or_else(|| EmitError::unsupported("missing sealed compiler table boundary"))?;
        let mut out = BUILD_PREFIX_MAGIC.to_vec();
        out.extend_from_slice(&table_start.to_le_bytes());
        out.push(u8::from(pool.uses_heap));
        out.push(u8::from(pool.intl_default_locale.is_some()));
        for table in [
            (
                pool.lowercase_mapping_table_ptr,
                pool.lowercase_mapping_count,
            ),
            (
                pool.uppercase_mapping_table_ptr,
                pool.uppercase_mapping_count,
            ),
            (pool.cased_range_table_ptr, pool.cased_range_count),
            (
                pool.case_ignorable_range_table_ptr,
                pool.case_ignorable_range_count,
            ),
            (
                pool.canonical_decomposition_table_ptr,
                pool.canonical_decomposition_count,
            ),
            (
                pool.compatibility_decomposition_table_ptr,
                pool.compatibility_decomposition_count,
            ),
            (pool.combining_class_table_ptr, pool.combining_class_count),
            (pool.composition_table_ptr, pool.composition_count),
        ] {
            out.extend_from_slice(&table.0.to_le_bytes());
            out.extend_from_slice(&table.1.to_le_bytes());
        }
        for folding in [RegExpCaseFolding::Legacy, RegExpCaseFolding::Unicode] {
            let table = pool
                .regexp_case_folding_table(folding)
                .ok_or_else(|| EmitError::unsupported("missing compiler RegExp case table"))?;
            out.extend_from_slice(&table.ptr.to_le_bytes());
            out.extend_from_slice(&table.count.to_le_bytes());
        }
        // Pool refs iterate in String-key order. Recover the reversible
        // compiler spelling and verify it against the exact original ref.
        for reference in pool.refs.values() {
            let units = &reference.code_units;
            let start = units.byte_offset as usize;
            let end = start + units.length as usize * 2;
            let utf16 = pool.pooled_code_unit_bytes[start..end]
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                .collect::<Vec<_>>();
            let key = scriptw_ir::encode_js_string_utf16(&utf16);
            if !pool
                .refs
                .get(&key)
                .is_some_and(|actual| std::ptr::eq(actual, reference))
            {
                return Err(EmitError::unsupported(
                    "compiler prefix string has a noncanonical spelling",
                ));
            }
            let len = u32::try_from(key.len())
                .map_err(|_| EmitError::unsupported("compiler prefix string exceeds Wasm32"))?;
            out.extend_from_slice(&reference.index.ordinal().to_le_bytes());
            out.extend_from_slice(&units.byte_offset.to_le_bytes());
            out.extend_from_slice(&units.length.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(key.as_bytes());
        }
        Ok(out)
    }

    /// Restore a producer-verified R prefix for P emission. This constructor
    /// is reached only through the immutable package admission boundary.
    pub(crate) fn from_build_prefix(
        prefix: &[u8],
        utf16: &[u8],
        static_parts: [&[u8]; 7],
        boundary: PoolBoundary,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
        selection: &IntlDataSelection,
    ) -> Result<Self, EmitError> {
        let error = || EmitError::unsupported("immutable compiler prefix differs from checked R");
        let (mut pool, expected_tail, has_default_locale) =
            Self::decode_build_prefix(prefix, utf16, static_parts, boundary).ok_or_else(error)?;
        if has_default_locale {
            pool.intl_default_locale = Some(
                selection
                    .compilation_data()
                    .map_err(|error| EmitError::unsupported(error.to_string()))?
                    .identity()
                    .default_locale()
                    .clone(),
            );
        }
        let start = pool.bytes.len();
        // Mint typed handles through the existing checked constructors. All
        // catalogue strings are already interned and the large images have
        // their final addresses. Compare the small trailing tables to R.
        pool.seal_runtime_tables(functions)?;
        if pool.boundary() != boundary || &pool.bytes[start..] != expected_tail {
            return Err(error());
        }
        Ok(Self(pool).freeze())
    }

    fn decode_build_prefix<'a>(
        prefix: &[u8],
        utf16: &[u8],
        static_parts: [&'a [u8]; 7],
        boundary: PoolBoundary,
    ) -> Option<(StringPool, &'a [u8], bool)> {
        let mut reader = PrefixReader(prefix);
        if reader.take(BUILD_PREFIX_MAGIC.len())? != BUILD_PREFIX_MAGIC
            || utf16.len() != boundary.code_unit_len()
        {
            return None;
        }
        let start = reader.word()?;
        let bound = STATIC_DATA_OFFSET.checked_add(start)?;
        let uses_heap = reader.flag()?;
        let has_default_locale = reader.flag()?;
        let (lowercase_mapping_table_ptr, lowercase_mapping_count) = reader.table(16, bound)?;
        let (uppercase_mapping_table_ptr, uppercase_mapping_count) = reader.table(16, bound)?;
        let (cased_range_table_ptr, cased_range_count) = reader.table(8, bound)?;
        let (case_ignorable_range_table_ptr, case_ignorable_range_count) =
            reader.table(8, bound)?;
        let (canonical_decomposition_table_ptr, canonical_decomposition_count) =
            reader.table(16, bound)?;
        let (compatibility_decomposition_table_ptr, compatibility_decomposition_count) =
            reader.table(16, bound)?;
        let (combining_class_table_ptr, combining_class_count) = reader.table(8, bound)?;
        let (composition_table_ptr, composition_count) = reader.table(16, bound)?;
        let mut regexp_case_folding_tables = BTreeMap::new();
        for folding in [RegExpCaseFolding::Legacy, RegExpCaseFolding::Unicode] {
            let (ptr, count) = reader.table(8, bound)?;
            regexp_case_folding_tables.insert(folding, RegExpCaseFoldingTable { ptr, count });
        }
        let (&last, earlier) = static_parts.split_last()?;
        let before_last = earlier
            .iter()
            .try_fold(0usize, |sum, part| sum.checked_add(part.len()))?;
        if before_last.checked_add(last.len())? != boundary.static_len() {
            return None;
        }
        let (last_prefix, expected_tail) =
            last.split_at_checked((start as usize).checked_sub(before_last)?)?;
        // The virtual order places normalization after the first active
        // fragment, Unicode properties, and the second active fragment.
        let normalization_start = STATIC_DATA_OFFSET.checked_add(
            u32::try_from(
                static_parts[..3]
                    .iter()
                    .try_fold(0usize, |sum, part| sum.checked_add(part.len()))?,
            )
            .ok()?,
        )?;
        let normalization_end =
            normalization_start.checked_add(u32::try_from(static_parts[3].len()).ok()?)?;
        let normalization_region =
            (normalization_start != normalization_end).then_some(NormalizationRegion {
                start: normalization_start,
                end: normalization_end,
            });
        let mut refs = PoolStringRefs::default();
        let mut previous = None;
        for _ in 0..boundary.strings() {
            let ordinal = reader.word()?;
            let byte_offset = reader.word()?;
            let length = reader.word()?;
            let name_len = reader.word()?;
            let name = std::str::from_utf8(reader.take(name_len as usize)?).ok()?;
            let end = byte_offset.checked_add(length.checked_mul(2)?)?;
            if ordinal >= boundary.strings()
                || byte_offset % 2 != 0
                || end as usize > utf16.len()
                || previous.is_some_and(|previous| previous >= name)
            {
                return None;
            }
            previous = Some(name);
            refs.insert(
                name.to_owned(),
                StringRef {
                    index: PooledStringIndex(ordinal),
                    code_units: PooledCodeUnits {
                        byte_offset,
                        length,
                    },
                },
            );
        }
        if !reader.0.is_empty() {
            return None;
        }
        let mut bytes = Vec::with_capacity(boundary.static_len());
        for part in earlier {
            bytes.extend_from_slice(part);
        }
        bytes.extend_from_slice(last_prefix);
        // Only R's builtin/helper bodies consume the Unicode property,
        // retained calendar, and Intl catalogue handles. P uses R's exact
        // bytes and calls those bodies without opening a provider/catalogue.
        let pool = StringPool {
            bytes: bytes.into(),
            pooled_code_unit_bytes: utf16.to_vec().into(),
            refs,
            uses_heap,
            normalization_region,
            lowercase_mapping_table_ptr,
            lowercase_mapping_count,
            uppercase_mapping_table_ptr,
            uppercase_mapping_count,
            cased_range_table_ptr,
            cased_range_count,
            case_ignorable_range_table_ptr,
            case_ignorable_range_count,
            canonical_decomposition_table_ptr,
            canonical_decomposition_count,
            compatibility_decomposition_table_ptr,
            compatibility_decomposition_count,
            combining_class_table_ptr,
            combining_class_count,
            composition_table_ptr,
            composition_count,
            regexp_case_folding_tables,
            ..StringPool::default()
        };
        Some((pool, expected_tail, has_default_locale))
    }

    pub(crate) fn for_runtime(
        runtime: &crate::RuntimeArtifact,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
        builtins: &[StandardBuiltinId],
        temporal: bool,
        selection: &scriptw_intl::IntlDataSelection,
    ) -> Result<Self, EmitError> {
        let prefix = Self::collect(functions, builtins, temporal, selection)?;
        runtime.layout().require_pool(&prefix.0)?;
        if let Some(matches) = runtime.admitted_compiler_pool_matches(
            &prefix.0.pooled_code_unit_bytes[..],
            &prefix.0.bytes[..],
        ) {
            if !matches {
                return Err(EmitError::unsupported(
                    "canonical compiler pool differs from its immutable producer receipt",
                ));
            }
        } else {
            prefix.require_runtime_data(runtime.bytes())?;
        }
        Ok(prefix.freeze())
    }

    fn freeze(mut self) -> Self {
        self.0.bytes.freeze();
        self.0.pooled_code_unit_bytes.freeze();
        self.0.refs.freeze();
        self
    }

    // Matching lengths alone cannot admit another compiler's same-sized data.
    // Compare the canonical passive UTF-16 and active static images once,
    // before this opaque prefix enters the R-owned OnceLock.
    fn require_runtime_data(&self, bytes: &[u8]) -> Result<(), EmitError> {
        use wasmparser::{DataKind, Operator, Parser, Payload};
        let error = || {
            EmitError::unsupported(
                "canonical compiler pool does not match its exact runtime data images",
            )
        };
        let mut found = false;
        for payload in Parser::new(0).parse_all(bytes) {
            if let Payload::DataSection(section) = payload.map_err(|_| error())? {
                if found || section.count() as usize != RUNTIME_DATA_SEGMENT_COUNT {
                    return Err(error());
                }
                found = true;
                let region = self.0.normalization_image_region();
                let span = region.offsets();
                let properties = self.0.regexp_unicode_property_region();
                let property = properties.offsets();
                let years = self.0.temporal_east_asian_year_region();
                let year = years.offsets();
                let expected = [
                    (&self.0.pooled_code_unit_bytes[..], None),
                    (&self.0.bytes[span.clone()], None),
                    (&self.0.bytes[property.clone()], None),
                    (&self.0.bytes[year.clone()], None),
                    (&self.0.bytes[..property.start], Some(STATIC_DATA_OFFSET)),
                    (
                        &self.0.bytes[property.end..span.start],
                        Some(properties.end()),
                    ),
                    (&self.0.bytes[span.end..year.start], Some(region.end())),
                    (&self.0.bytes[year.end..], Some(years.end())),
                ];
                for (row, (bytes, address)) in section.into_iter().zip(expected) {
                    let row = row.map_err(|_| error())?;
                    if row.data != bytes {
                        return Err(error());
                    }
                    match (row.kind, address) {
                        (DataKind::Passive, None) => {}
                        (
                            DataKind::Active {
                                memory_index:
                                    crate::runtime_private_memory::RUNTIME_PRIVATE_MEMORY_INDEX,
                                offset_expr,
                            },
                            Some(address),
                        ) => {
                            let mut ops = offset_expr.get_operators_reader();
                            if !matches!(ops.read().map_err(|_| error())?, Operator::I32Const { value } if value == address as i32)
                                || !matches!(ops.read().map_err(|_| error())?, Operator::End)
                                || !ops.eof()
                            {
                                return Err(error());
                            }
                        }
                        _ => return Err(error()),
                    }
                }
            }
        }
        if found {
            Ok(())
        } else {
            Err(error())
        }
    }

    pub(crate) fn collect_program(
        prefix: &Arc<Self>,
        script: &ScriptIr,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<StringPool, EmitError> {
        Self(StringPool::from_shared_prefix(prefix)).collect_script(script, functions)
    }
}

impl StringPool {
    pub(super) fn seal_runtime_tables(
        &mut self,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<(), EmitError> {
        self.runtime_table_start = Some(self.bytes.len());
        self.compiler_owned = self.boundary();
        self.append_runtime_intrinsic_installations(functions)?;
        self.append_runtime_intrinsic_property_layouts(functions)?;
        self.append_runtime_string_descriptors()?;
        self.compiler_owned = self.boundary();
        Ok(())
    }

    fn from_shared_prefix(prefix: &Arc<CompilerOwnedPool>) -> Self {
        let base = &prefix.0;
        Self {
            compiler_owned: base.compiler_owned,
            shared_prefix: Some(Arc::clone(prefix)),
            bytes: base.bytes.program_overlay(),
            pooled_code_unit_bytes: base.pooled_code_unit_bytes.program_overlay(),
            refs: base.refs.program_overlay(),
            uses_heap: base.uses_heap,
            lowercase_mapping_table_ptr: base.lowercase_mapping_table_ptr,
            lowercase_mapping_count: base.lowercase_mapping_count,
            uppercase_mapping_table_ptr: base.uppercase_mapping_table_ptr,
            uppercase_mapping_count: base.uppercase_mapping_count,
            cased_range_table_ptr: base.cased_range_table_ptr,
            cased_range_count: base.cased_range_count,
            case_ignorable_range_table_ptr: base.case_ignorable_range_table_ptr,
            case_ignorable_range_count: base.case_ignorable_range_count,
            normalization_region: base.normalization_region,
            canonical_decomposition_table_ptr: base.canonical_decomposition_table_ptr,
            canonical_decomposition_count: base.canonical_decomposition_count,
            compatibility_decomposition_table_ptr: base.compatibility_decomposition_table_ptr,
            compatibility_decomposition_count: base.compatibility_decomposition_count,
            combining_class_table_ptr: base.combining_class_table_ptr,
            combining_class_count: base.combining_class_count,
            composition_table_ptr: base.composition_table_ptr,
            composition_count: base.composition_count,
            ..Self::default()
        }
    }

    pub(super) fn runtime_prefix(&self) -> Option<&Self> {
        self.shared_prefix.as_ref().map(|prefix| &prefix.0)
    }

    /// Borrowed native-template data contains only this P's appended bytes.
    pub(crate) fn program_utf16_bytes(&self) -> &[u8] {
        &self.pooled_code_unit_bytes[self.compiler_owned.code_unit_bytes..]
    }
    pub(crate) fn program_metadata_bytes(&self) -> &[u8] {
        &self.bytes[self.compiler_owned.static_bytes..]
    }
    pub(crate) fn program_recipe_bytes(&self) -> &[u8] {
        self.compact_array_recipes.bytes()
    }
}

#[cfg(test)]
mod tests;
