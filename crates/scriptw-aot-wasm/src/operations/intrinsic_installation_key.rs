//! A bootstrap key's closed plan crosses the call before its String is resolved.
use super::*;
use crate::data::{PooledLiteralDomain, PooledStringIndex, StringPool};
use crate::runtime_helpers::{RuntimeHelperFunctionBase, RuntimeHelperIntrinsicKeyParameter};

#[derive(Clone, Copy)]
enum InstallationKey {
    RuntimeName(PooledStringIndex),
    WellKnown(scriptw_ir::WellKnownSymbol),
}

/// Only runtime-owned pooled names and actual well-known Symbols are admitted.
/// Keeping construction private prevents a Program ordinal or numeric Symbol
/// spelling from becoming a seemingly validated helper argument.
pub(crate) struct IntrinsicInstallationKey(InstallationKey);

impl IntrinsicInstallationKey {
    pub(crate) fn new(
        key: crate::intrinsics::IntrinsicKey<'_>,
        strings: &StringPool,
    ) -> Result<Self, EmitError> {
        Ok(Self(match key {
            crate::intrinsics::IntrinsicKey::Name(name) => {
                let index = strings.pooled_string_index(name)?;
                let PooledLiteralDomain::Runtime(index) = strings.pooled_string_domain(index)?
                else {
                    return Err(EmitError::unsupported(
                        "intrinsic installation key must belong to the runtime string table",
                    ));
                };
                InstallationKey::RuntimeName(index)
            }
            crate::intrinsics::IntrinsicKey::Symbol(symbol) => InstallationKey::WellKnown(symbol),
        }))
    }

    /// Serialization retains the same closed key proof for immutable R rows.
    pub(crate) fn words(&self) -> [u32; 2] {
        let (is_symbol, ordinal) = match self.0 {
            InstallationKey::RuntimeName(index) => (false, index.ordinal()),
            InstallationKey::WellKnown(symbol) => (
                true,
                scriptw_ir::WellKnownSymbol::ALL
                    .iter()
                    .position(|candidate| *candidate == symbol)
                    .expect("closed well-known Symbol row") as u32,
            ),
        };
        [u32::from(is_symbol), ordinal]
    }

    pub(crate) fn emit(&self, f: &mut Function) {
        let [is_symbol, ordinal] = self.words();
        f.instruction(&Instruction::I32Const(is_symbol as i32));
        f.instruction(&Instruction::I32Const(ordinal as i32));
    }
}

impl PropertyKeyLocals {
    /// This marker can be acquired only by the typed installation helper ABI.
    /// Resolve it after recipe allocation, preserving allocation/key order.
    pub(crate) fn from_intrinsic_installation_parameter(
        key: &RuntimeHelperIntrinsicKeyParameter,
        schema: &RuntimeSchema,
        base: RuntimeHelperFunctionBase,
        f: &mut Function,
    ) -> Self {
        let value = schema.reserve_value_local(f);
        key.is_symbol().load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let symbol = schema
            .reserve_gc_local(f)
            .initialize(schema.installation_symbol_reference(key, f), f);
        value.set_reference(&symbol, schema, f);
        symbol.clear(f);
        f.instruction(&Instruction::Else);
        let string = schema.reserve_gc_local(f).initialize(
            schema.resolve_pooled_string_reference(key.ordinal(), base, f),
            f,
        );
        value.set_reference(&string, schema, f);
        string.clear(f);
        f.instruction(&Instruction::End);
        Self { value }
    }
}
