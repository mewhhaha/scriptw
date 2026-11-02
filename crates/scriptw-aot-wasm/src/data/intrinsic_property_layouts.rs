//! Sealed packed R rows for complete intrinsic property layouts.
use super::*;
use crate::intrinsics::property_layouts::{
    IntrinsicPropertyLayoutId, IntrinsicPropertyRecipe, IntrinsicPropertySpec,
};
use crate::intrinsics::{DeferredIntrinsicMethod, IntrinsicKey};

/// Only whole, compiler-validated layouts can become storage constructor operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RPropertyLayoutSpan {
    address: u32,
    count: u32,
    group_count: u32,
    index_address: u32,
    index_capacity: u32,
}
impl RPropertyLayoutSpan {
    pub(crate) fn address(self) -> u32 {
        self.address
    }
    pub(crate) fn count(self) -> u32 {
        self.count
    }
    pub(crate) fn group_count(self) -> u32 {
        self.group_count
    }
    pub(crate) fn index_address(self) -> u32 {
        self.index_address
    }
    pub(crate) fn index_capacity(self) -> u32 {
        self.index_capacity
    }
    pub(crate) fn emit(self, f: &mut Function) {
        for word in [self.address, self.count, self.group_count] {
            f.instruction(&Instruction::I32Const(word as i32));
        }
    }
}

/// Exact spans, rather than a byte interval admitting interior/fabricated layouts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RPropertyLayoutRows {
    spans: [Option<RPropertyLayoutSpan>; IntrinsicPropertyLayoutId::ALL.len()],
}
impl RPropertyLayoutRows {
    pub(crate) const ROW_BYTES: u32 = 40;
    #[cfg(test)]
    pub(crate) fn spans(self) -> impl Iterator<Item = RPropertyLayoutSpan> {
        self.spans.into_iter().flatten()
    }
    fn span(self, id: IntrinsicPropertyLayoutId) -> Result<RPropertyLayoutSpan, EmitError> {
        self.spans[id as usize].ok_or_else(|| {
            EmitError::unsupported("intrinsic property layout has no complete sealed R span")
        })
    }
}

#[derive(Debug)]
pub(super) struct RuntimeIntrinsicPropertyLayouts {
    rows: RPropertyLayoutRows,
}

fn raw_key_hash(key: IntrinsicKey<'_>) -> u64 {
    let IntrinsicKey::Name(name) = key else {
        return 0;
    };
    let mut hash = 14695981039346656037u64;
    for unit in scriptw_ir::decode_js_string_utf16(name) {
        hash ^= u64::from(unit);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn admitted_method(
    builtin: scriptw_ir::StandardBuiltinId,
    functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
) -> Result<bool, EmitError> {
    let Some(meta) = functions.get(&builtin.function_id()) else {
        return Ok(false);
    };
    if DeferredIntrinsicMethod::validated(builtin, meta)?.is_none() {
        return Err(EmitError::unsupported(
            "intrinsic layout method lacks deferred native authority",
        ));
    }
    Ok(true)
}

fn runtime_string_ordinal(strings: &StringPool, name: &str) -> Result<u32, EmitError> {
    let index = strings.pooled_string_index(name)?;
    let PooledLiteralDomain::Runtime(index) = strings.pooled_string_domain(index)? else {
        return Err(EmitError::unsupported(
            "intrinsic property layout string is not R-owned",
        ));
    };
    Ok(index.ordinal())
}

fn encode_row(
    property: &IntrinsicPropertySpec,
    strings: &StringPool,
) -> Result<[u32; 10], EmitError> {
    let key = crate::operations::IntrinsicInstallationKey::new(property.key, strings)?;
    let [key_kind, key_ordinal] = key.words();
    let mut payload = [0; 3];
    match property.recipe {
        IntrinsicPropertyRecipe::Number(bits) => {
            payload[0] = bits as u32;
            payload[1] = (bits >> 32) as u32;
        }
        IntrinsicPropertyRecipe::Boolean(value) => payload[0] = u32::from(value),
        IntrinsicPropertyRecipe::String(text) => {
            payload[0] = runtime_string_ordinal(strings, text)?
        }
        IntrinsicPropertyRecipe::Method(builtin) => payload[0] = builtin as u32,
        IntrinsicPropertyRecipe::RealmIntrinsic(slot) => payload[0] = slot.gc_index(),
        IntrinsicPropertyRecipe::Accessor { getter, setter } => {
            payload[0] = getter as u32;
            payload[1] = setter as u32;
        }
        IntrinsicPropertyRecipe::LayoutObject(layout) => payload[0] = layout as u32,
    }
    let hash = raw_key_hash(property.key);
    Ok([
        key_kind,
        key_ordinal,
        property.flags.bits(),
        property.group.index(),
        property.recipe.kind() as u32,
        payload[0],
        payload[1],
        payload[2],
        hash as u32,
        (hash >> 32) as u32,
    ])
}

/// String lookup does not construct GC keys for preceding rows. The immutable
/// index owns no insertion order: its words are ordered row positions plus one.
fn string_index(properties: &[IntrinsicPropertySpec]) -> Result<Vec<u32>, EmitError> {
    let error = || EmitError::unsupported("intrinsic property index exceeds static bounds");
    let count = u32::try_from(properties.len()).map_err(|_| error())?;
    let capacity = count
        .checked_next_power_of_two()
        .and_then(|count| count.checked_mul(2))
        .ok_or_else(error)?;
    let mut buckets = vec![0; capacity as usize];
    for (index, property) in properties.iter().enumerate() {
        if !matches!(property.key, IntrinsicKey::Name(_)) {
            continue;
        }
        let mut bucket = raw_key_hash(property.key) as u32 & (capacity - 1);
        while buckets[bucket as usize] != 0 {
            bucket = (bucket + 1) & (capacity - 1);
        }
        buckets[bucket as usize] = index as u32 + 1;
    }
    Ok(buckets)
}

impl StringPool {
    pub(super) fn collect_intrinsic_property_layout_strings(&mut self) -> Result<(), EmitError> {
        for id in IntrinsicPropertyLayoutId::ALL {
            for property in id.layout()?.properties() {
                if let IntrinsicKey::Name(name) = property.key {
                    self.intern_string(name);
                }
                if let IntrinsicPropertyRecipe::String(text) = property.recipe {
                    self.intern_string(text);
                }
            }
        }
        Ok(())
    }

    pub(super) fn append_runtime_intrinsic_property_layouts(
        &mut self,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<(), EmitError> {
        let error = || EmitError::unsupported("intrinsic property layouts exceed R static bounds");
        let mut spans = [None; IntrinsicPropertyLayoutId::ALL.len()];
        let mut bytes = Vec::new();
        let address = u32::try_from(self.bytes.len())
            .ok()
            .and_then(|offset| STATIC_DATA_OFFSET.checked_add(offset))
            .ok_or_else(error)?;
        for id in IntrinsicPropertyLayoutId::ALL {
            let plan = id.layout()?;
            let mut present = true;
            for property in plan.properties() {
                let ready = match property.recipe {
                    IntrinsicPropertyRecipe::Method(builtin) => {
                        admitted_method(builtin, functions)?
                    }
                    IntrinsicPropertyRecipe::Accessor { getter, setter } => {
                        admitted_method(getter, functions)? && admitted_method(setter, functions)?
                    }
                    IntrinsicPropertyRecipe::Number(_)
                    | IntrinsicPropertyRecipe::Boolean(_)
                    | IntrinsicPropertyRecipe::String(_)
                    | IntrinsicPropertyRecipe::RealmIntrinsic(_)
                    | IntrinsicPropertyRecipe::LayoutObject(_) => true,
                };
                present &= ready;
            }
            // Narrow compiler pools omit whole layouts, never partial property objects.
            if !present {
                continue;
            }
            let offset = u32::try_from(bytes.len()).map_err(|_| error())?;
            let count = u32::try_from(plan.properties().len()).map_err(|_| error())?;
            let span_address = address.checked_add(offset).ok_or_else(error)?;
            let length = count
                .checked_mul(RPropertyLayoutRows::ROW_BYTES)
                .ok_or_else(error)?;
            let index_address = span_address.checked_add(length).ok_or_else(error)?;
            let buckets = string_index(plan.properties())?;
            let index_capacity = u32::try_from(buckets.len()).map_err(|_| error())?;
            index_capacity
                .checked_mul(4)
                .and_then(|length| index_address.checked_add(length))
                .ok_or_else(error)?;
            for property in plan.properties() {
                for word in encode_row(property, self)? {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
            for bucket in buckets {
                bytes.extend_from_slice(&bucket.to_le_bytes());
            }
            spans[id as usize] = Some(RPropertyLayoutSpan {
                address: span_address,
                count,
                group_count: plan.group_count(),
                index_address,
                index_capacity,
            });
        }
        self.bytes.extend_from_slice(&bytes);
        self.runtime_intrinsic_property_layouts = Some(RuntimeIntrinsicPropertyLayouts {
            rows: RPropertyLayoutRows { spans },
        });
        Ok(())
    }

    fn runtime_intrinsic_property_layouts(
        &self,
    ) -> Result<&RuntimeIntrinsicPropertyLayouts, EmitError> {
        self.runtime_intrinsic_property_layouts
            .as_ref()
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.runtime_intrinsic_property_layouts.as_ref())
            })
            .ok_or_else(|| EmitError::unsupported("missing sealed R intrinsic property layouts"))
    }
    #[cfg(test)]
    pub(crate) fn runtime_intrinsic_property_layout_rows(
        &self,
    ) -> Result<RPropertyLayoutRows, EmitError> {
        Ok(self.runtime_intrinsic_property_layouts()?.rows)
    }
    pub(crate) fn intrinsic_property_layout_span(
        &self,
        id: IntrinsicPropertyLayoutId,
    ) -> Result<RPropertyLayoutSpan, EmitError> {
        self.runtime_intrinsic_property_layouts()?.rows.span(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_rows_preserve_symbol_domain_exact_numbers_and_utf16_hashes() {
        let mut pool = StringPool::default();
        pool.collect_intrinsic_property_layout_strings().unwrap();
        pool.compiler_owned = pool.boundary();
        let math = IntrinsicPropertyLayoutId::Math.layout().unwrap();
        let pi = math
            .properties()
            .iter()
            .find(|p| matches!(p.key, IntrinsicKey::Name("PI")))
            .unwrap();
        let row = encode_row(pi, &pool).unwrap();
        assert_eq!(
            u64::from(row[5]) | (u64::from(row[6]) << 32),
            std::f64::consts::PI.to_bits()
        );
        assert_eq!(row[0], 0);
        let tag = math.properties().last().unwrap();
        assert!(matches!(
            tag.key,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag)
        ));
        let symbol = encode_row(tag, &pool).unwrap();
        assert_eq!(symbol[0], 1);
        assert_eq!(&symbol[8..], &[0, 0]);
        assert_eq!(raw_key_hash(IntrinsicKey::Name("a")), 0xaf63dc4c8601ec8c);
        assert_eq!(
            raw_key_hash(IntrinsicKey::Name("\u{100}")),
            0xaf62bd4c860004df
        );
    }
    #[test]
    fn missing_native_metadata_never_publishes_a_partial_layout() {
        let mut pool = StringPool::default();
        pool.collect_intrinsic_property_layout_strings().unwrap();
        pool.compiler_owned = pool.boundary();
        pool.append_runtime_intrinsic_property_layouts(&BTreeMap::new())
            .unwrap();
        assert!(pool
            .intrinsic_property_layout_span(IntrinsicPropertyLayoutId::Math)
            .is_err());
        let span = pool
            .intrinsic_property_layout_span(IntrinsicPropertyLayoutId::ArrayUnscopables)
            .unwrap();
        assert_eq!(span.count(), 16);
        assert_eq!(span.group_count(), 16);
        assert_eq!(
            span.index_address(),
            span.address() + 16 * RPropertyLayoutRows::ROW_BYTES
        );
        assert_eq!(span.index_capacity(), 32);
        assert_eq!(
            pool.runtime_intrinsic_property_layout_rows()
                .unwrap()
                .spans()
                .count(),
            1
        );
    }

    #[test]
    fn immutable_index_handles_collisions_without_changing_property_order() {
        for id in IntrinsicPropertyLayoutId::ALL {
            let plan = id.layout().unwrap();
            let index = string_index(plan.properties()).unwrap();
            let mask = index.len() as u32 - 1;
            for (ordinal, property) in plan.properties().iter().enumerate() {
                let IntrinsicKey::Name(name) = property.key else {
                    continue;
                };
                let mut bucket = raw_key_hash(property.key) as u32 & mask;
                loop {
                    let position = index[bucket as usize];
                    assert_ne!(position, 0, "missing layout key {name}");
                    if position == ordinal as u32 + 1 {
                        break;
                    }
                    bucket = (bucket + 1) & mask;
                }
            }
            assert_eq!(
                index.iter().filter(|&&word| word != 0).count(),
                plan.properties()
                    .iter()
                    .filter(|p| matches!(p.key, IntrinsicKey::Name(_)))
                    .count()
            );
        }
    }
}
