//! Direct publication into the fresh global dictionary, in binding order.
use super::*;
use crate::heap::StoredPropertyAttributes;

struct GlobalBindingRow<'a> {
    name: &'a str,
    hash: i64,
    bucket: u32,
}

/// A checked binding order with unique keys and final linear-probe buckets.
/// Computing the initial values runs no JavaScript and reads no global property.
#[must_use = "every planned binding must reach the global dictionary"]
pub(super) struct GlobalBindingPublication<'a> {
    rows: std::vec::IntoIter<GlobalBindingRow<'a>>,
    capacity: u32,
    position: u32,
    storage: GcLocal<OrdinaryPropertyStorage>,
    pooled_names: GcLocal<PooledStringTable>,
    absent: GcLocal<StoredValue>,
}

impl<'a> GlobalBindingPublication<'a> {
    pub(super) fn new(
        object: &GcLocal<OrdinaryObject>,
        builder: &FunctionBuilder<'_>,
        names: impl ExactSizeIterator<Item = &'a str>,
        function: &mut Function,
    ) -> Result<Self, EmitError> {
        let count = names.len();
        if count > (1 << 30) {
            return Err(EmitError::unsupported("too many global bindings"));
        }
        let capacity = count.max(4).next_power_of_two();
        let mask = 2 * capacity - 1;
        let mut occupied = vec![false; 2 * capacity];
        let mut distinct = std::collections::BTreeSet::new();
        let mut rows = Vec::with_capacity(count);
        for name in names {
            if !distinct.insert(name) {
                return Err(EmitError::unsupported(format!(
                    "global binding {name} is published twice"
                )));
            }
            let hash = crate::operations::string_property_key_hash(name);
            let mut bucket = hash as u32 as usize & mask;
            while occupied[bucket] {
                bucket = (bucket + 1) & mask;
            }
            occupied[bucket] = true;
            rows.push(GlobalBindingRow {
                name,
                hash,
                bucket: bucket as u32,
            });
        }
        let schema = builder.runtime_schema();
        let storage = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, schema, function)
                .reference()
                .cast_subtype::<OrdinaryPropertyStorage>(schema, function),
            function,
        );
        let absent = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let pooled_names = schema
            .reserve_gc_local(function)
            .initialize(schema.pooled_string_table_reference(function), function);
        Ok(Self {
            rows: rows.into_iter(),
            capacity: capacity as u32,
            position: 0,
            storage,
            pooled_names,
            absent,
        })
    }

    pub(super) fn push(
        &mut self,
        builder: &FunctionBuilder<'_>,
        name: &str,
        value: &GcLocal<PropertyValue>,
        attributes: StoredPropertyAttributes,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let row = self
            .rows
            .next()
            .filter(|row| row.name == name)
            .ok_or_else(|| {
                EmitError::unsupported("global bindings must follow publication order")
            })?;
        let schema = builder.runtime_schema();
        let key = match schema.try_bootstrap_pooled_name(
            &self.pooled_names,
            builder.strings,
            name,
            self.position,
            function,
        )? {
            Some(key) => key,
            None => builder.emit_interned_string_reference(name, function)?,
        };
        let key = schema.reserve_gc_local(function).initialize(key, function);
        let descriptor = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::descriptor_word(attributes.descriptor_word()),
                    GcOperand::reference(value, schema),
                    GcOperand::reference(&self.absent, schema),
                    GcOperand::reference(&self.absent, schema),
                ),
                function,
            ),
            function,
        );
        let hash = schema.reserve_i64_local(function);
        hash.set_constant(row.hash, function);
        let capacity = if self.position == 0 { self.capacity } else { 0 };
        let final_length = if self.rows.len() == 0 {
            self.position + 1
        } else {
            0
        };
        schema.call_helper(
            crate::runtime_helpers::FreshDataPropertiesPublishArguments::new(
                &self.storage,
                &key,
                &descriptor,
                GcIndex::Constant(self.position),
                GcIndex::Constant(capacity),
                GcIndex::Constant(final_length),
                hash,
                GcIndex::Constant(row.bucket),
            ),
            builder.runtime_helper_base()?,
            function,
        );
        schema.release_i64_local(hash, function);
        descriptor.clear(function);
        key.clear(function);
        self.position += 1;
        Ok(())
    }

    pub(super) fn finish(self, function: &mut Function) -> Result<(), EmitError> {
        if self.rows.len() != 0 {
            return Err(EmitError::unsupported("a global binding was not published"));
        }
        self.absent.clear(function);
        self.pooled_names.clear(function);
        self.storage.clear(function);
        Ok(())
    }
}
