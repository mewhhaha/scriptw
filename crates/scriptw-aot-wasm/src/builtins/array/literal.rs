//! Fixed Array literals initialize one fresh, unpublished actual GC Array.
//! Spread and suspension-owned accumulation retain their observable owner.

use super::*;
use crate::gc_types::GcLocal;

mod compact;
mod numeric;

/// There is no constructor from an existing Array and no borrowed-array view.
/// Only consuming finish can publish this fresh object's whole Value.
#[must_use = "a fresh literal stays private until all elements are initialized"]
struct FreshArrayLiteral {
    array: GcLocal<ArrayObject>,
    index: I64Local,
}

impl FreshArrayLiteral {
    fn new(
        length: u32,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<Self, EmitError> {
        let schema = builder.runtime_schema();
        let length_local = schema.reserve_i64_local(f);
        length_local.set_constant(i64::from(length), f);
        let array = builder.emit_array_literal_object(length_local, f)?;
        schema.release_i64_local(length_local, f);
        let index = schema.reserve_i64_local(f);
        Ok(Self { array, index })
    }

    /// The sole caller derives ordinals from a length-checked fixed literal.
    /// Every non-hole ordinal is below 2^32-1 and is published exactly once.
    fn publish_element(
        &mut self,
        ordinal: u32,
        value: &ValueLocals,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.index.set_constant(i64::from(ordinal), f);
        builder.emit_array_indexed_publish_default_data(&self.array, self.index, value, f)?;
        Ok(())
    }

    fn finish(self, output: &ValueLocals, builder: &FunctionBuilder<'_>, f: &mut Function) {
        let schema = builder.runtime_schema();
        output.set_reference(&self.array, schema, f);
        schema.release_i64_local(self.index, f);
        self.array.clear(f);
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_array_literal_payload(
        &mut self,
        elements: &[TypedExpr],
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        // A fixed Array literal can contain at most 2^32-1 elements, and its
        // largest occupied ordinal is consequently at most 2^32-2. Validate
        // once during compilation; never narrow a usize silently to an index.
        let length = u32::try_from(elements.len()).map_err(|_| {
            EmitError::unsupported("Array literal exceeds the maximum ECMAScript Array length")
        })?;
        if let Some(recipe) = self.strings.compact_array_recipe(elements)? {
            return self.compile_compact_array_literal(recipe, output, f);
        }
        if let Some(literal) = numeric::SmallDenseArrayLiteral::new(elements) {
            return self.compile_small_dense_array_literal(literal, output, f);
        }
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(f);
        let mut literal = FreshArrayLiteral::new(length, self, f)?;
        for (ordinal, element) in elements.iter().enumerate() {
            if matches!(element.expr, ExprIr::ArrayHole) {
                continue;
            }
            self.compile_expr_to_value(element, &value, f)?;
            self.emit_propagate_current_throw_if_needed(f);
            let ordinal = u32::try_from(ordinal).expect("literal length validated before emission");
            literal.publish_element(ordinal, &value, self, f)?;
        }
        literal.finish(output, self, f);
        value.clear(f);
        Ok(())
    }
}
