use super::*;
mod facts;
mod signatures;
use crate::native_program_contexts::{FunctionContext, ModuleContext, Schema};
pub(super) use facts::Facts;
use std::collections::BTreeMap;
use wasmparser::{CompositeInnerType, Operator, StorageType, ValType};

/// Every rewritten source operand can obtain the metadata of its actual owner.
pub(super) struct OwnedContexts {
    pub schema: Schema,
    pub bootstrap_body: u32,
    pub payload_site: usize,
    pub bodies: BTreeMap<u32, FunctionContext>,
}

/// Accumulate checked body facts without decoding the same instructions again.
/// Only finish can mint the complete owner witness consumed by reencoding.
pub(super) struct Check<'a> {
    schema: Schema,
    bootstrap_body: u32,
    payload_operator: u32,
    rows: BTreeMap<u32, FunctionContext>,
    signatures: &'a signatures::Signatures,
    initializer: u32,
    payload_site: Option<usize>,
    complete: bool,
}
impl<'a> Check<'a> {
    pub(super) fn new(
        rows: Vec<ModuleContext>,
        facts: &'a Facts,
        body_count: usize,
    ) -> Result<Self, NativeProgramTemplateError> {
        let initializers = rows
            .iter()
            .filter_map(|row| match row.context {
                FunctionContext::Bootstrap {
                    schema,
                    payload_operator: Some(operator),
                } => Some((row.body, schema, operator)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(bootstrap_body, schema, payload_operator)] = initializers.as_slice() else {
            return Err(invalid(
                "native contexts require exactly one completed bootstrap",
            ));
        };
        let (bootstrap_body, schema, payload_operator) =
            (*bootstrap_body, *schema, *payload_operator);
        if schema.payload_field != crate::gc_types::ProgramInstanceLocals::native_payload_field()
            || rows
                .iter()
                .any(|row| row.context.schema() != schema || row.body as usize >= body_count)
        {
            return Err(invalid(
                "native program contexts have inconsistent schemas or bodies",
            ));
        }
        if !facts.matches(schema) {
            return Err(invalid(
                "native metadata context does not match the actual immutable P field",
            ));
        }
        let initializer = facts.signatures.initializer(bootstrap_body, schema)?;
        Ok(Self {
            schema,
            bootstrap_body,
            payload_operator,
            rows: rows
                .into_iter()
                .map(|row| (row.body, row.context))
                .collect(),
            signatures: &facts.signatures,
            initializer,
            payload_site: None,
            complete: true,
        })
    }

    pub(super) fn body(
        &mut self,
        index: u32,
        body: &wasmparser::FunctionBody<'_>,
        operators: &[(usize, Operator<'_>)],
        sites: &BTreeMap<usize, super::plan::Site>,
        literal_sites: &BTreeMap<usize, u32>,
    ) -> Result<(), NativeProgramTemplateError> {
        if operators.iter().any(|(_, op)| matches!(op,
            Operator::Call { function_index } | Operator::ReturnCall { function_index } | Operator::RefFunc { function_index }
                if *function_index == self.initializer)) {
            return Err(invalid("native bootstrap cannot be referenced with its original parameterless type"));
        }
        if let Some(FunctionContext::Program { local, ready, .. }) = self.rows.get(&index) {
            self.signatures.local(index, body, *local, self.schema)?;
            if !operators.iter().take(*ready as usize).any(|(_, op)| matches!(op,
                Operator::LocalSet { local_index } | Operator::LocalTee { local_index } if local_index == local)) {
                return Err(invalid("native program owner is not initialized before its ready witness"));
            }
        }
        if index == self.bootstrap_body {
            match operators.get(self.payload_operator as usize) {
                Some((
                    offset,
                    Operator::RefNull {
                        hty: wasmparser::HeapType::Concrete(index),
                    },
                )) if index.as_module_index() == Some(self.schema.payload_type) => {
                    self.payload_site = Some(*offset)
                }
                _ => {
                    return Err(invalid(
                        "native bootstrap source payload slot differs from its witness",
                    ))
                }
            }
        }
        for (ordinal, (offset, _)) in operators.iter().enumerate() {
            if !sites.contains_key(offset) && !literal_sites.contains_key(offset) {
                continue;
            }
            self.complete &= match self.rows.get(&index) {
                Some(FunctionContext::Bootstrap { .. }) => true,
                Some(FunctionContext::Program { ready, .. }) => ordinal >= *ready as usize,
                None => false,
            };
        }
        if let Some(FunctionContext::Program { ready, .. }) = self.rows.get(&index) {
            if *ready as usize > operators.len() {
                return Err(invalid(
                    "native program context becomes ready beyond its body",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Option<OwnedContexts>, NativeProgramTemplateError> {
        let payload_site = self
            .payload_site
            .ok_or_else(|| invalid("native bootstrap metadata site is absent"))?;
        Ok(self.complete.then_some(OwnedContexts {
            schema: self.schema,
            bootstrap_body: self.bootstrap_body,
            payload_site,
            bodies: self.rows,
        }))
    }
}
