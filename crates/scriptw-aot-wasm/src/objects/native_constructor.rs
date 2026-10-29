//! Eager intrinsic constructors start in their final ordinary dictionary state.
//! Bootstrap adds public prototype/family properties immediately afterwards;
//! ordinary native methods retain their separate compact metadata allocator.
use super::property_storage::PreparedOrdinaryPropertyKey;
use super::*;
use crate::runtime_helpers::{NativeConstructorHeaderAllocateParameters, RuntimeHelperId};

impl FunctionBuilder<'_> {
    pub(crate) fn compile_native_constructor_header_allocate_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::NativeConstructorHeaderAllocate);
        let p = self.helper_parameters::<NativeConstructorHeaderAllocateParameters>(&mut f);
        let s = self.runtime_schema();
        // Match the ordinary dictionary's first reservation: four ordered
        // entries and twice as many index buckets. Metadata owns the first two
        // positions, leaving room for bootstrap's next public properties.
        let capacity = s.reserve_i32_local(&mut f);
        capacity.set_constant(4, &mut f);
        let entries = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<PropertyTable>()
                .filled(GcOperand::null(s), capacity, &mut f),
            &mut f,
        );
        capacity.set_constant(8, &mut f);
        let buckets = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<OrdinaryPropertyIndex>()
                .filled(GcOperand::i32(0), capacity, &mut f),
            &mut f,
        );
        let dictionary = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<OrdinaryPropertyStorage>().construct(
                (
                    GcOperand::reference(&entries, s),
                    GcOperand::i32(0),
                    GcOperand::reference(&buckets, s),
                ),
                &mut f,
            ),
            &mut f,
        );
        let zero = s.reserve_i32_local(&mut f);
        zero.set_constant(0, &mut f);
        let length_value = s.reserve_value_local(&mut f);
        length_value.set_number(p.length_bits, &mut f);
        let name_value = s.reserve_value_local(&mut f);
        name_value.set_reference(&p.name, s, &mut f);
        for (name, value) in [("length", &length_value), ("name", &name_value)] {
            let text = s
                .reserve_gc_local(&mut f)
                .initialize(self.emit_interned_string_reference(name, &mut f)?, &mut f);
            let key = PropertyKeyLocals::from_string(s, &text, &mut f);
            let descriptor = self.emit_alloc_data_property_descriptor_with_word(
                GcOperand::data_descriptor_flags(zero, zero, p.configurable),
                value,
                &mut f,
            );
            let hash = s.reserve_i64_local(&mut f);
            self.emit_property_key_hash(&key, hash, &mut f);
            // These two literal keys are distinct, and this unpublished fresh
            // dictionary has no user callbacks. No absence probe is needed.
            self.emit_ordinary_insert_in_dictionary(
                &dictionary,
                &PreparedOrdinaryPropertyKey { key: &key, hash },
                &descriptor,
                &mut f,
            );
            s.release_i64_local(hash, &mut f);
            descriptor.clear(&mut f);
            key.clear(&mut f);
            text.clear(&mut f);
        }
        let storage = s
            .reserve_gc_local(&mut f)
            .initialize(dictionary.load(s, &mut f).upcast(), &mut f);
        let object =
            self.emit_alloc_object_header_with_storage(&p.prototype, zero, &storage, &mut f);
        let _ = object;
        storage.clear(&mut f);
        name_value.clear(&mut f);
        length_value.clear(&mut f);
        s.release_i32_local(zero, &mut f);
        dictionary.clear(&mut f);
        buckets.clear(&mut f);
        entries.clear(&mut f);
        s.release_i32_local(capacity, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
