//! Sealed packed R rows for complete intrinsic property layouts.
use super::*;
use crate::intrinsics::property_layouts::{
    IntrinsicCallableRecipe, IntrinsicIdentityGroup, IntrinsicPropertyLayoutId,
    IntrinsicPropertyRecipe, IntrinsicPropertySpec,
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
    named_data_groups:
        BTreeMap<IntrinsicPropertyLayoutId, BTreeMap<&'static str, IntrinsicIdentityGroup>>,
}

/// A named DATA identity group belonging to this exact complete R layout.
#[derive(Clone, Copy)]
pub(crate) struct RNamedDataProperty {
    span: RPropertyLayoutSpan,
    group: IntrinsicIdentityGroup,
}
impl RNamedDataProperty {
    pub(crate) fn span(self) -> RPropertyLayoutSpan {
        self.span
    }
    pub(crate) fn group(self) -> IntrinsicIdentityGroup {
        self.group
    }
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
        return Err(EmitError::unsupported(format!(
            "intrinsic layout method {} lacks deferred native authority",
            builtin.debug_name(),
        )));
    }
    Ok(true)
}

fn admitted_callable(
    recipe: IntrinsicCallableRecipe,
    functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
) -> Result<bool, EmitError> {
    match recipe {
        IntrinsicCallableRecipe::Undefined => Ok(true),
        IntrinsicCallableRecipe::Method(method) => admitted_method(method.builtin(), functions),
        IntrinsicCallableRecipe::Ready(slot) => Ok(functions.contains_key(&slot.function_id())),
    }
}

fn native_metadata(
    id: IntrinsicPropertyLayoutId,
    functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
) -> Result<Option<&WasmFunctionMeta>, EmitError> {
    let function_id = id.native_function_id().ok_or_else(|| {
        EmitError::unsupported("native metadata recipe requires a callable layout")
    })?;
    let Some(meta) = functions.get(&function_id) else {
        return Ok(None);
    };
    id.validate_native_metadata(meta)?;
    Ok(Some(meta))
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
    functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
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
        IntrinsicPropertyRecipe::ReadyRealmSlot(slot) => payload[0] = slot.gc_index(),
        IntrinsicPropertyRecipe::Accessor { getter, setter } => {
            let (getter_kind, getter_payload) = getter.words();
            let (setter_kind, setter_payload) = setter.words();
            payload[0] = getter_payload;
            payload[1] = setter_payload;
            payload[2] = (getter_kind as u32) | ((setter_kind as u32) << 2);
        }
        IntrinsicPropertyRecipe::LayoutObject(layout) => payload[0] = layout as u32,
        IntrinsicPropertyRecipe::ArrayPrototype => {}
        IntrinsicPropertyRecipe::WellKnownSymbol(symbol) => {
            payload[0] = scriptw_ir::WellKnownSymbol::ALL
                .iter()
                .position(|candidate| *candidate == symbol)
                .expect("closed well-known Symbol row") as u32
        }
        IntrinsicPropertyRecipe::NativeLength(id) => {
            let meta = native_metadata(id, functions)?.ok_or_else(|| {
                EmitError::unsupported("native length recipe has no planned metadata")
            })?;
            let bits = (meta.length as f64).to_bits();
            payload[0] = bits as u32;
            payload[1] = (bits >> 32) as u32;
        }
        IntrinsicPropertyRecipe::NativeName(id) => {
            let meta = native_metadata(id, functions)?.ok_or_else(|| {
                EmitError::unsupported("native name recipe has no planned metadata")
            })?;
            payload[0] = runtime_string_ordinal(strings, meta.runtime_name())?;
        }
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
        let mut named_data_groups = BTreeMap::new();
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
                        admitted_callable(getter, functions)?
                            && admitted_callable(setter, functions)?
                    }
                    IntrinsicPropertyRecipe::Number(_)
                    | IntrinsicPropertyRecipe::Boolean(_)
                    | IntrinsicPropertyRecipe::String(_)
                    | IntrinsicPropertyRecipe::LayoutObject(_)
                    | IntrinsicPropertyRecipe::ArrayPrototype
                    | IntrinsicPropertyRecipe::WellKnownSymbol(_) => true,
                    IntrinsicPropertyRecipe::ReadyRealmSlot(slot) => slot
                        .function_id()
                        .is_none_or(|id| functions.contains_key(&id)),
                    IntrinsicPropertyRecipe::NativeLength(id)
                    | IntrinsicPropertyRecipe::NativeName(id) => {
                        native_metadata(id, functions)?.is_some()
                    }
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
                for word in encode_row(property, self, functions)? {
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
            let groups = plan
                .properties()
                .iter()
                .filter_map(|property| {
                    let IntrinsicKey::Name(name) = property.key else {
                        return None;
                    };
                    if matches!(property.recipe, IntrinsicPropertyRecipe::Accessor { .. }) {
                        return None;
                    }
                    Some((name, property.group))
                })
                .collect();
            named_data_groups.insert(id, groups);
        }
        self.bytes.extend_from_slice(&bytes);
        self.runtime_intrinsic_property_layouts = Some(RuntimeIntrinsicPropertyLayouts {
            rows: RPropertyLayoutRows { spans },
            named_data_groups,
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
    pub(crate) fn has_intrinsic_property_layout(
        &self,
        id: IntrinsicPropertyLayoutId,
    ) -> Result<bool, EmitError> {
        Ok(self.runtime_intrinsic_property_layouts()?.rows.spans[id as usize].is_some())
    }

    pub(crate) fn intrinsic_property_layout_span(
        &self,
        id: IntrinsicPropertyLayoutId,
    ) -> Result<RPropertyLayoutSpan, EmitError> {
        self.runtime_intrinsic_property_layouts()?.rows.span(id)
    }

    pub(crate) fn intrinsic_named_data_property(
        &self,
        id: IntrinsicPropertyLayoutId,
        name: &str,
    ) -> Option<RNamedDataProperty> {
        let layouts = self.runtime_intrinsic_property_layouts().ok()?;
        let group = *layouts.named_data_groups.get(&id)?.get(name)?;
        Some(RNamedDataProperty {
            span: layouts.rows.spans[id as usize]?,
            group,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn array_metadata() -> WasmFunctionMeta {
        let builtin = scriptw_ir::StandardBuiltinId::ArrayConstructor;
        WasmFunctionMeta {
            captured_call: None,
            constant_leaf: None,
            numeric_leaf: None,
            async_generator_resume_environment_plan: None,
            template_source: None,
            name: "Planned Array Name".into(),
            to_string_value: "function Array() { [native code] }".into(),
            standard_builtin: Some(builtin),
            host_builtin: None,
            length: 37,
            length_name_configurable: true,
            entry: crate::function_entry::PlannedFunctionEntry::standard(builtin, 17),
            strict: true,
            is_named_expression: false,
            class_element_execution_kind: scriptw_ir::ClassElementExecutionKind::None,
            class_heritage_kind: scriptw_ir::ClassHeritageKind::None,
            is_static_class_member: false,
            is_derived_constructor: false,
            is_synthetic_default_derived_constructor: false,
            class_instance_element_plan: None,
            uses_super: false,
            this_before_super: false,
            captures_private_environment: false,
            needs_active_function_identity: false,
        }
    }

    #[test]
    fn native_metadata_rows_seal_actual_names_lengths_and_reject_wrong_identity() {
        let id = IntrinsicPropertyLayoutId::ArrayConstructor;
        let plan = id.layout().unwrap();
        let meta = array_metadata();
        let mut pool = StringPool::default();
        pool.collect_intrinsic_property_layout_strings().unwrap();
        pool.intern_string(meta.runtime_name());
        pool.compiler_owned = pool.boundary();
        let mut functions = BTreeMap::from([(id.native_function_id().unwrap(), meta)]);
        let length = encode_row(&plan.properties()[0], &pool, &functions).unwrap();
        assert_eq!(
            length[4],
            crate::intrinsics::IntrinsicPropertyRecipeKind::Number as u32
        );
        assert_eq!(
            u64::from(length[5]) | (u64::from(length[6]) << 32),
            37.0f64.to_bits()
        );
        let name = encode_row(&plan.properties()[1], &pool, &functions).unwrap();
        assert_eq!(
            name[4],
            crate::intrinsics::IntrinsicPropertyRecipeKind::String as u32
        );
        assert_eq!(
            name[5],
            runtime_string_ordinal(&pool, "Planned Array Name").unwrap()
        );
        let meta = functions
            .get_mut(&id.native_function_id().unwrap())
            .unwrap();
        meta.entry = crate::function_entry::PlannedFunctionEntry::standard(
            scriptw_ir::StandardBuiltinId::ObjectConstructor,
            17,
        );
        assert!(encode_row(&plan.properties()[0], &pool, &functions).is_err());
        assert!(native_metadata(id, &BTreeMap::new()).unwrap().is_none());
    }

    #[test]
    fn accessor_wire_preserves_absence_and_canonical_identity_separately_from_methods() {
        use crate::intrinsics::property_layouts::{
            IntrinsicCallableRecipeKind as Side, IntrinsicPropertyRecipeKind as Kind,
        };
        let mut pool = StringPool::default();
        pool.collect_intrinsic_property_layout_strings().unwrap();
        pool.compiler_owned = pool.boundary();
        let map = IntrinsicPropertyLayoutId::MapConstructor.layout().unwrap();
        let getter = encode_row(map.properties().last().unwrap(), &pool, &BTreeMap::new()).unwrap();
        assert_eq!(getter[4], Kind::Accessor as u32);
        assert_eq!(
            getter[5],
            scriptw_ir::StandardBuiltinId::MapSpeciesGetter as u32
        );
        assert_eq!(getter[6], 0);
        assert_eq!(
            getter[7],
            (Side::Method as u32) | ((Side::Undefined as u32) << 2)
        );
        let function = IntrinsicPropertyLayoutId::FunctionPrototype
            .layout()
            .unwrap();
        let thrower = encode_row(
            function.properties().last().unwrap(),
            &pool,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            thrower[5],
            crate::functions::NonArrayRealmIntrinsicSlot::ThrowTypeError.gc_index()
        );
        assert_eq!(thrower[5], thrower[6]);
        assert_eq!(
            thrower[7],
            (Side::Ready as u32) | ((Side::Ready as u32) << 2)
        );
        assert_eq!(RPropertyLayoutRows::ROW_BYTES, 40);
    }

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
        let row = encode_row(pi, &pool, &BTreeMap::new()).unwrap();
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
        let symbol = encode_row(tag, &pool, &BTreeMap::new()).unwrap();
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
