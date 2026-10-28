//! One validated pure body owns its scalar declaration, identity and code ref.
use super::*;
use crate::function_layout::BinaryNumericFunctionIndex;
use crate::functions::BinaryNumericLeafPlan;
use crate::gc_types::RuntimeSchema;

#[derive(Debug, Clone)]
pub(crate) struct PlannedBinaryNumericEntry {
    id: FunctionId,
    name: String,
    index: BinaryNumericFunctionIndex,
    proof: BinaryNumericLeafPlan,
}

impl PlannedBinaryNumericEntry {
    pub(crate) fn new(
        source: &FunctionIr,
        proof: BinaryNumericLeafPlan,
        index: BinaryNumericFunctionIndex,
    ) -> Self {
        assert_eq!(
            proof.source(),
            &source.id,
            "binary callback proof belongs to its source"
        );
        Self {
            id: source.id.clone(),
            name: source.name.clone(),
            index,
            proof,
        }
    }

    pub(crate) fn identity(&self) -> FunctionIdentity {
        FunctionIdentity::BinaryNumericSource {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }

    pub(super) fn require_source(&self, source: &FunctionIr) {
        assert_eq!(
            self.id, source.id,
            "scalar entry belongs to its actual callable"
        );
    }

    pub(crate) fn assert_body_index(&self, actual: u32) {
        self.index.assert_body_index(actual);
    }

    pub(crate) fn emit_ref_func(&self, function: &mut Function) {
        self.index.emit_ref_func(function);
    }

    pub(crate) fn compile_body(&self, schema: &RuntimeSchema) -> Function {
        self.proof.compile_body(schema)
    }
}
