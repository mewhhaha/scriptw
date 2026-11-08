//! The cell owns its mutable value. Immutable snapshots remain StoredValue.
use super::*;

impl GcStructType<BindingCell> {
    pub(crate) fn read_value<N: GcFieldNullability>(
        &self,
        cell: &GcLocal<BindingCell, N>,
        value: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.field(BindingCellSchema::VALUE_TAG)
            .read(cell, schema, function)
            .store(value.tag(), function);
        self.field(BindingCellSchema::VALUE_SCALAR)
            .read(cell, schema, function)
            .store_i64(value.scalar(), function);
        self.field(BindingCellSchema::VALUE_REFERENCE)
            .read(cell, schema, function)
            .store_eq(value.reference(), function);
    }

    pub(crate) fn write_value<N: GcFieldNullability>(
        &self,
        cell: &GcLocal<BindingCell, N>,
        value: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.field(BindingCellSchema::VALUE_SCALAR).write(
            cell,
            GcOperand::i64_local(value.scalar()),
            schema,
            function,
        );
        self.field(BindingCellSchema::VALUE_TAG).write(
            cell,
            GcOperand::i32_local(value.tag()),
            schema,
            function,
        );
        // Also overwrite null: scalar assignments must retire the old GC edge.
        self.field(BindingCellSchema::VALUE_REFERENCE).write(
            cell,
            GcOperand::eq_reference(value.reference()),
            schema,
            function,
        );
    }
}
