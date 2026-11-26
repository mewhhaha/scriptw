//! A fresh global object's bindings, published together in binding order.
use super::*;
use crate::objects::FreshDataAttributes;

/// The keys, values and attributes of one global object's bindings, filled in
/// binding order and published by one runtime call. Construction proves the
/// names distinct, which the publication helper relies on instead of probing
/// each key for a previous entry. Computing a binding's value runs no
/// JavaScript, so publishing after every value exists is unobservable.
#[must_use = "recorded bindings reach the global object only through publish"]
pub(super) struct GlobalBindingPublication {
    keys: GcLocal<PooledStringTable>,
    values: GcLocal<IntrinsicPropertyValueCache>,
    flags: Vec<i32>,
    hashes: Vec<i64>,
    count: usize,
    row: I32Local,
}

impl GlobalBindingPublication {
    pub(super) fn new<'a>(
        builder: &FunctionBuilder<'_>,
        names: impl IntoIterator<Item = &'a str>,
        function: &mut Function,
    ) -> Result<Self, EmitError> {
        let mut distinct = std::collections::BTreeSet::new();
        for name in names {
            if !distinct.insert(name) {
                return Err(EmitError::unsupported(format!(
                    "global binding {name} is published twice"
                )));
            }
        }
        let count = distinct.len();
        let schema = builder.runtime_schema();
        let row = schema.reserve_i32_local(function);
        row.set_constant(
            i32::try_from(count).map_err(|_| EmitError::unsupported("too many global bindings"))?,
            function,
        );
        let keys = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PooledStringTable>()
                .filled(GcOperand::null(schema), row, function),
            function,
        );
        let values = schema.reserve_gc_local(function).initialize(
            schema.array_type::<IntrinsicPropertyValueCache>().filled(
                GcOperand::null(schema),
                row,
                function,
            ),
            function,
        );
        Ok(Self {
            keys,
            values,
            flags: Vec::with_capacity(count),
            hashes: Vec::with_capacity(count),
            count,
            row,
        })
    }

    /// Records the next binding. Bindings arrive in their publication order.
    pub(super) fn push(
        &mut self,
        builder: &FunctionBuilder<'_>,
        name: &str,
        value: &GcLocal<PropertyValue>,
        attributes: FreshDataAttributes,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.flags.len() == self.count {
            return Err(EmitError::unsupported(
                "global binding outside the published name set",
            ));
        }
        let schema = builder.runtime_schema();
        self.row.set_constant(self.flags.len() as i32, function);
        let key = schema.reserve_gc_local(function).initialize(
            builder.emit_interned_string_reference(name, function)?,
            function,
        );
        schema.array_type::<PooledStringTable>().write(
            &self.keys,
            self.row,
            GcOperand::nullable_reference(&key, schema),
            schema,
            function,
        );
        key.clear(function);
        schema.array_type::<IntrinsicPropertyValueCache>().write(
            &self.values,
            self.row,
            GcOperand::nullable_reference(value, schema),
            schema,
            function,
        );
        self.flags.push(attributes.word());
        self.hashes
            .push(crate::operations::string_property_key_hash(name));
        Ok(())
    }

    /// Publishes every recorded binding into the fresh global object.
    pub(super) fn publish(
        self,
        builder: &FunctionBuilder<'_>,
        object: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.flags.len() != self.count {
            return Err(EmitError::unsupported("a global binding was not recorded"));
        }
        let schema = builder.runtime_schema();
        let flags = schema.reserve_gc_local(function).initialize(
            schema.array_type::<OrdinaryPropertyIndex>().fixed(
                self.flags.iter().map(|word| GcOperand::i32(*word)),
                function,
            ),
            function,
        );
        let hashes = schema.reserve_gc_local(function).initialize(
            schema.array_type::<NumberBitsI64Array>().fixed(
                self.hashes.iter().map(|hash| GcOperand::i64(*hash)),
                function,
            ),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            builder.emit_object_header_projection(object, function),
            function,
        );
        schema.call_helper(
            crate::runtime_helpers::FreshDataPropertiesPublishArguments::new(
                &header,
                &self.keys,
                &self.values,
                &flags,
                &hashes,
            ),
            builder.runtime_helper_base()?,
            function,
        );
        header.clear(function);
        hashes.clear(function);
        flags.clear(function);
        self.values.clear(function);
        self.keys.clear(function);
        schema.release_i32_local(self.row, function);
        Ok(())
    }
}
