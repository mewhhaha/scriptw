//! Immutable R metadata for ordered, single-key deferred method installation.
use super::*;
use crate::intrinsics::{DeferredIntrinsicMethod, IntrinsicMethodPlan, IntrinsicMethodSpec};

/// Only the validated pool constructor can mint a bootstrap installation span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RInstallationSpan {
    address: u32,
    count: u32,
}
impl RInstallationSpan {
    pub(crate) fn emit(self, f: &mut Function) {
        f.instruction(&Instruction::I32Const(self.address as i32));
        f.instruction(&Instruction::I32Const(self.count as i32));
    }
}

/// Bounds of the sealed, runtime-owned table, never Program metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RInstallationRows {
    address: u32,
    count: u32,
}
impl RInstallationRows {
    pub(crate) const ROW_BYTES: u32 = 16;
    pub(crate) const WRITABLE: u32 = 1;
    pub(crate) const CONFIGURABLE: u32 = 2;
    pub(crate) fn address(self) -> u32 {
        self.address
    }
    pub(crate) fn byte_length(self) -> u32 {
        self.count * Self::ROW_BYTES
    }
}

struct InstallationRow([u32; 4]);
impl InstallationRow {
    fn new(
        method: IntrinsicMethodSpec,
        strings: &StringPool,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<Option<Self>, EmitError> {
        // Narrow compiler pools can omit a family entirely. Its missing rows
        // cannot produce a span; a consumer that emits it will fail admission.
        let Some(meta) = functions.get(&method.builtin.function_id()) else {
            return Ok(None);
        };
        if DeferredIntrinsicMethod::validated(method.builtin, meta)?.is_none() {
            return Ok(None);
        }
        let key = crate::operations::IntrinsicInstallationKey::new(method.key, strings)?;
        let [kind, ordinal] = key.words();
        let attributes = u32::from(method.writable) * RInstallationRows::WRITABLE
            | u32::from(method.configurable) * RInstallationRows::CONFIGURABLE;
        Ok(Some(Self([
            method.builtin as u32,
            kind,
            ordinal,
            attributes,
        ])))
    }
}

#[derive(Debug)]
pub(super) struct RuntimeIntrinsicInstallations {
    rows: RInstallationRows,
    // Eager/canonical entries retain their position and split emitted runs.
    plans: BTreeMap<IntrinsicMethodPlan, Vec<Option<u32>>>,
}
impl RuntimeIntrinsicInstallations {
    fn span(
        &self,
        plan: IntrinsicMethodPlan,
        entries: std::ops::Range<usize>,
    ) -> Result<RInstallationSpan, EmitError> {
        let error = || EmitError::unsupported("intrinsic method run has no sealed R row span");
        let ordinals = self
            .plans
            .get(&plan)
            .and_then(|rows| rows.get(entries))
            .filter(|rows| !rows.is_empty())
            .ok_or_else(error)?;
        let first = ordinals[0].ok_or_else(error)?;
        let count = u32::try_from(ordinals.len()).map_err(|_| error())?;
        if first
            .checked_add(count)
            .is_none_or(|end| end > self.rows.count)
            || ordinals
                .iter()
                .enumerate()
                .any(|(i, row)| *row != Some(first + i as u32))
        {
            return Err(error());
        }
        Ok(RInstallationSpan {
            address: self.rows.address + first * RInstallationRows::ROW_BYTES,
            count,
        })
    }
}

impl StringPool {
    pub(super) fn append_runtime_intrinsic_installations(
        &mut self,
        functions: &BTreeMap<FunctionId, WasmFunctionMeta>,
    ) -> Result<(), EmitError> {
        let error = || EmitError::unsupported("intrinsic installation rows exceed R static bounds");
        let mut plans = BTreeMap::new();
        let mut rows = Vec::new();
        for plan in IntrinsicMethodPlan::all() {
            let mut ordinals = Vec::new();
            for &method in plan.methods()?.iter() {
                let row = InstallationRow::new(method, self, functions)?;
                ordinals.push(if let Some(row) = row {
                    let ordinal = u32::try_from(rows.len()).map_err(|_| error())?;
                    rows.push(row);
                    Some(ordinal)
                } else {
                    None
                });
            }
            plans.insert(plan, ordinals);
        }
        let count = u32::try_from(rows.len()).map_err(|_| error())?;
        let length = count
            .checked_mul(RInstallationRows::ROW_BYTES)
            .ok_or_else(error)?;
        let address = u32::try_from(self.bytes.len())
            .ok()
            .and_then(|offset| STATIC_DATA_OFFSET.checked_add(offset))
            .ok_or_else(error)?;
        address.checked_add(length).ok_or_else(error)?;
        let mut bytes = Vec::with_capacity(length as usize);
        for InstallationRow(words) in rows {
            for word in words {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        self.bytes.extend_from_slice(&bytes);
        self.runtime_intrinsic_installations = Some(RuntimeIntrinsicInstallations {
            rows: RInstallationRows { address, count },
            plans,
        });
        Ok(())
    }

    fn runtime_intrinsic_installations(&self) -> Result<&RuntimeIntrinsicInstallations, EmitError> {
        self.runtime_intrinsic_installations
            .as_ref()
            .or_else(|| {
                self.runtime_prefix()
                    .and_then(|prefix| prefix.runtime_intrinsic_installations.as_ref())
            })
            .ok_or_else(|| EmitError::unsupported("missing sealed R intrinsic installation rows"))
    }

    pub(crate) fn runtime_intrinsic_installation_rows(
        &self,
    ) -> Result<RInstallationRows, EmitError> {
        Ok(self.runtime_intrinsic_installations()?.rows)
    }

    pub(crate) fn intrinsic_installation_span(
        &self,
        plan: IntrinsicMethodPlan,
        entries: std::ops::Range<usize>,
    ) -> Result<RInstallationSpan, EmitError> {
        self.runtime_intrinsic_installations()?.span(plan, entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_spans_cannot_cross_eager_entries_or_invent_uncollected_rows() {
        let table = RuntimeIntrinsicInstallations {
            rows: RInstallationRows {
                address: 4096,
                count: 4,
            },
            plans: BTreeMap::from([
                (
                    IntrinsicMethodPlan::Math,
                    vec![Some(0), Some(1), None, Some(2)],
                ),
                (IntrinsicMethodPlan::DatePrototype, vec![Some(3)]),
                (IntrinsicMethodPlan::ArrayConstructor, vec![None, None]),
            ]),
        };
        assert_eq!(
            table.span(IntrinsicMethodPlan::Math, 0..2).unwrap(),
            RInstallationSpan {
                address: 4096,
                count: 2
            }
        );
        assert_eq!(
            table.span(IntrinsicMethodPlan::Math, 3..4).unwrap(),
            RInstallationSpan {
                address: 4096 + 2 * 16,
                count: 1
            }
        );
        assert!(table.span(IntrinsicMethodPlan::Math, 0..3).is_err());
        assert!(table.span(IntrinsicMethodPlan::Math, 2..4).is_err());
        assert!(table.span(IntrinsicMethodPlan::Math, 0..5).is_err());
        assert!(table.span(IntrinsicMethodPlan::Math, 0..0).is_err());
        assert!(table
            .span(IntrinsicMethodPlan::ArrayConstructor, 0..1)
            .is_err());
        assert!(table.span(IntrinsicMethodPlan::Reflect, 0..1).is_err());
    }

    #[test]
    fn sealed_spans_reject_noncontiguous_and_out_of_table_ordinals() {
        for ordinals in [
            vec![Some(0), Some(2)],
            vec![Some(3), Some(4)],
            vec![Some(u32::MAX)],
        ] {
            let count = ordinals.len();
            let table = RuntimeIntrinsicInstallations {
                rows: RInstallationRows {
                    address: 4096,
                    count: 4,
                },
                plans: BTreeMap::from([(IntrinsicMethodPlan::Math, ordinals)]),
            };
            assert!(table.span(IntrinsicMethodPlan::Math, 0..count).is_err());
        }
    }
}
