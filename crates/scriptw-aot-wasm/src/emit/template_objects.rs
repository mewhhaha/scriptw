//! Parsed-source executions and the Realm's GetTemplateObject cache.
use super::*;
use crate::gc_types::{
    ArrayIndexStorage, ArrayObject, FunctionContext, FunctionContextSchema, FunctionObject,
    GcLocal, GcOperand, GcStackReference, Nullable, OrdinaryObject, OrdinaryObjectSchema,
    PropertyDescriptor, RealmRecord, RealmRecordSchema, StoredValue, TemplateSiteTable,
    TemplateSource, TemplateSourceSchema, ValueLocals,
};
use crate::operations::PropertyKeyLocals;
use scriptw_ir::{TemplateObjectIr, TemplateSourceIr};

/// The plan and its actual rooted source execution cannot be separated into
/// an integer or copied ownership token. Function contexts borrow its capture.
#[derive(Debug)]
pub(crate) struct TemplateSourceExecution {
    source: TemplateSourceIr,
    owner: GcLocal<TemplateSource>,
}

impl TemplateSourceExecution {
    pub(crate) fn source(&self) -> TemplateSourceIr {
        self.source
    }
    pub(crate) fn capture(
        &self,
        schema: &crate::gc_types::RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<TemplateSource> {
        self.owner.load(schema, function)
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.owner.clear(function);
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn inherited_template_source(
        &self,
        meta: &WasmFunctionMeta,
    ) -> Result<Option<&TemplateSourceExecution>, EmitError> {
        match meta.template_source {
            None => Ok(None),
            Some(source) => match self.template_source_execution.as_ref() {
                Some(owner) if owner.source == source => Ok(Some(owner)),
                Some(_) | None => Err(EmitError::unsupported(
                    "source function allocation requires its owning source execution",
                )),
            },
        }
    }

    pub(crate) fn allocate_template_source(
        &mut self,
        source: TemplateSourceIr,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> TemplateSourceExecution {
        let schema = self.runtime_schema();
        let count = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(source.site_count() as i32));
        count.store(function);
        let sites = schema.reserve_gc_local(function).initialize(
            schema.array_type::<TemplateSiteTable>().filled(
                GcOperand::null(schema),
                count,
                function,
            ),
            function,
        );
        let owner = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<TemplateSource>().construct(
                (
                    GcOperand::i64(i64::from(source.id().index())),
                    GcOperand::reference(realm, schema),
                    GcOperand::reference(&sites, schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        sites.clear(function);
        schema.release_i32_local(count, function);
        TemplateSourceExecution { source, owner }
    }

    pub(super) fn init_template_source_execution(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let Some(source) = self.template_source_body else {
            return Ok(());
        };
        let schema = self.runtime_schema();
        let owner = match self.module_state {
            FunctionModuleState::Internal(_) => {
                let context = self
                    .body_entry_locals()
                    .and_then(|entry| entry.function_context())
                    .expect("parsed callable owns its completed FunctionContext");
                let owner = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<FunctionContext>()
                        .field(FunctionContextSchema::TEMPLATE_SOURCE)
                        .read(context, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                let id = schema.reserve_i64_local(function);
                schema
                    .struct_type::<TemplateSource>()
                    .field(TemplateSourceSchema::SOURCE_ID)
                    .read(&owner, schema, function)
                    .store_i64(id, function);
                id.load(function);
                function.instruction(&Instruction::I64Const(i64::from(source.id().index())));
                function.instruction(&Instruction::I64Ne);
                function.instruction(&Instruction::If(BlockType::Empty));
                function.instruction(&Instruction::Unreachable);
                function.instruction(&Instruction::End);
                schema.release_i64_local(id, function);
                TemplateSourceExecution { source, owner }
            }
            FunctionModuleState::PreparedScript(_, _) => {
                let realm = self.load_current_realm(function);
                let owner = self.allocate_template_source(source, &realm, function);
                realm.clear(function);
                owner
            }
            FunctionModuleState::Main(_, _) => {
                let realm = self.load_current_realm(function);
                let cursor = schema
                    .reserve_gc_local::<TemplateSource, Nullable>(function)
                    .initialize(
                        schema
                            .struct_type::<RealmRecord>()
                            .field(RealmRecordSchema::TEMPLATE_REGISTRY)
                            .read(&realm, schema, function)
                            .reference(),
                        function,
                    );
                let id = schema.reserve_i64_local(function);
                function.instruction(&Instruction::Block(BlockType::Empty));
                function.instruction(&Instruction::Loop(BlockType::Empty));
                cursor.load(schema, function);
                function.instruction(&Instruction::RefIsNull);
                function.instruction(&Instruction::BrIf(1));
                schema
                    .struct_type::<TemplateSource>()
                    .field(TemplateSourceSchema::SOURCE_ID)
                    .read(&cursor, schema, function)
                    .store_i64(id, function);
                id.load(function);
                function.instruction(&Instruction::I64Const(i64::from(source.id().index())));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::BrIf(1));
                cursor.replace(
                    schema
                        .struct_type::<TemplateSource>()
                        .field(TemplateSourceSchema::NEXT)
                        .read(&cursor, schema, function)
                        .reference(),
                    function,
                );
                function.instruction(&Instruction::Br(0));
                function.instruction(&Instruction::End);
                function.instruction(&Instruction::End);
                cursor.load(schema, function);
                function.instruction(&Instruction::RefIsNull);
                function.instruction(&Instruction::If(BlockType::Empty));
                let fresh = self.allocate_template_source(source, &realm, function);
                let previous = schema
                    .reserve_gc_local::<TemplateSource, Nullable>(function)
                    .initialize(
                        schema
                            .struct_type::<RealmRecord>()
                            .field(RealmRecordSchema::TEMPLATE_REGISTRY)
                            .read(&realm, schema, function)
                            .reference(),
                        function,
                    );
                schema
                    .struct_type::<TemplateSource>()
                    .field(TemplateSourceSchema::NEXT)
                    .write(
                        &fresh.owner,
                        GcOperand::reference(&previous, schema),
                        schema,
                        function,
                    );
                cursor.replace(fresh.owner.load(schema, function).nullable(), function);
                schema
                    .struct_type::<RealmRecord>()
                    .field(RealmRecordSchema::TEMPLATE_REGISTRY)
                    .write(
                        &realm,
                        GcOperand::reference(&cursor, schema),
                        schema,
                        function,
                    );
                previous.clear(function);
                fresh.clear(function);
                function.instruction(&Instruction::End);
                let owner = schema.reserve_gc_local(function).initialize(
                    cursor.load(schema, function).require_non_null(function),
                    function,
                );
                cursor.clear(function);
                schema.release_i64_local(id, function);
                realm.clear(function);
                TemplateSourceExecution { source, owner }
            }
            FunctionModuleState::RuntimeOperation(_) | FunctionModuleState::ProgramInitialize => {
                unreachable!("runtime helpers do not own parsed template sites",)
            }
        };
        assert!(
            self.template_source_execution.is_none(),
            "source owner initialized once"
        );
        self.template_source_execution = Some(owner);
        Ok(())
    }

    pub(crate) fn emit_fresh_template_function_value_payload(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let Some(source) = meta.template_source else {
            return self.emit_source_function_in_realm_with_template_source(
                meta,
                realm,
                crate::functions::FunctionPrototypeMaterialization::Automatic,
                None,
                function,
            );
        };
        let owner = self.allocate_template_source(source, realm, function);
        let value = self.emit_source_function_in_realm_with_template_source(
            meta,
            realm,
            crate::functions::FunctionPrototypeMaterialization::Automatic,
            Some(&owner),
            function,
        )?;
        owner.clear(function);
        Ok(value)
    }

    pub(crate) fn emit_template_function_from_class_context(
        &mut self,
        meta: &WasmFunctionMeta,
        context: &GcLocal<FunctionContext>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let Some(source) = meta.template_source else {
            return self.emit_function_value_payload(meta, function);
        };
        let schema = self.runtime_schema();
        let owner = TemplateSourceExecution {
            source,
            owner: schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionContext>()
                    .field(FunctionContextSchema::TEMPLATE_SOURCE)
                    .read(context, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            ),
        };
        let value = self.emit_function_value_payload_with_template_source(
            meta,
            crate::functions::FunctionPrototypeMaterialization::Automatic,
            Some(&owner),
            function,
        )?;
        owner.clear(function);
        Ok(value)
    }

    pub(crate) fn emit_template_object(
        &mut self,
        template: &TemplateObjectIr,
        function: &mut Function,
    ) -> Result<GcStackReference<ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let execution = self
            .template_source_execution
            .as_ref()
            .filter(|owner| owner.source.id() == template.site_id.source())
            .ok_or_else(|| {
                EmitError::unsupported("GetTemplateObject requires its owning source execution")
            })?;
        let owner = schema
            .reserve_gc_local(function)
            .initialize(execution.capture(schema, function), function);
        let sites = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<TemplateSource>()
                .field(TemplateSourceSchema::SITES)
                .read(&owner, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(template.site_id.cache_slot() as i32));
        index.store(function);
        let cooked = schema
            .reserve_gc_local::<ArrayObject, Nullable>(function)
            .initialize(
                schema
                    .array_type::<TemplateSiteTable>()
                    .read(&sites, index, schema, function)
                    .reference(),
                function,
            );
        cooked.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<TemplateSource>()
                .field(TemplateSourceSchema::REALM)
                .read(&owner, schema, function)
                .reference(),
            function,
        );
        let prototype = schema.reserve_gc_local(function).initialize(
            self.emit_load_realm_array_prototype(&realm, function),
            function,
        );
        let prototype_value = schema.reserve_value_local(function);
        prototype_value.set_reference(&prototype, schema, function);
        let raw_elements = template
            .raw
            .iter()
            .map(|value| Some(value.as_str()))
            .collect::<Vec<_>>();
        let cooked_elements = template
            .cooked
            .iter()
            .map(|value| value.as_deref())
            .collect::<Vec<_>>();
        let raw =
            self.emit_frozen_template_array(&raw_elements, &prototype_value, None, function)?;
        let fresh = self.emit_frozen_template_array(
            &cooked_elements,
            &prototype_value,
            Some(&raw),
            function,
        )?;
        cooked.replace(fresh.load(schema, function).nullable(), function);
        schema.array_type::<TemplateSiteTable>().write(
            &sites,
            index,
            GcOperand::nullable_reference(&fresh, schema),
            schema,
            function,
        );
        fresh.clear(function);
        raw.clear(function);
        prototype_value.clear(function);
        prototype.clear(function);
        realm.clear(function);
        function.instruction(&Instruction::End);
        let result = cooked.load(schema, function).require_non_null(function);
        cooked.clear(function);
        schema.release_i32_local(index, function);
        sites.clear(function);
        owner.clear(function);
        Ok(result)
    }

    /// Only a private, completed template array reaches this producer. Every
    /// descriptor, raw edge and frozen flag exists before its cache publication.
    fn emit_frozen_template_array(
        &mut self,
        elements: &[Option<&str>],
        prototype: &ValueLocals,
        raw: Option<&GcLocal<ArrayObject>>,
        function: &mut Function,
    ) -> Result<GcLocal<ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype), function)?,
            function,
        );
        let element = schema.reserve_value_local(function);
        element.set_undefined(function);
        let absent_accessor = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&element, function),
            function,
        );
        let mut descriptors = Vec::with_capacity(elements.len());
        for text in elements {
            match text {
                Some(text) => {
                    let string = schema.reserve_gc_local(function).initialize(
                        self.emit_interned_string_reference(text, function)?,
                        function,
                    );
                    element.set_reference(&string, schema, function);
                    string.clear(function);
                }
                None => element.set_undefined(function),
            }
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&element, function),
                function,
            );
            descriptors.push(
                schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<PropertyDescriptor>().construct(
                        (
                            GcOperand::descriptor_word(
                                StoredPropertyAttributes::Data {
                                    writable: false,
                                    enumerable: true,
                                    configurable: false,
                                }
                                .descriptor_word(),
                            ),
                            GcOperand::subtype_reference(&stored, schema),
                            GcOperand::reference(&absent_accessor, schema),
                            GcOperand::reference(&absent_accessor, schema),
                        ),
                        function,
                    ),
                    function,
                ),
            );
            stored.clear(function);
        }
        let indexed = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ArrayIndexStorage>()
                .empty(schema, function),
            function,
        );
        if let Some(raw) = raw {
            element.set_reference(raw, schema, function);
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference("raw", function)?,
                function,
            );
            let key = PropertyKeyLocals::from_string(schema, &text, function);
            self.emit_object_append_data_property_with_flags(
                &header, &key, &element, false, false, false, function,
            )?;
            key.clear(function);
            text.clear(function);
        }
        schema
            .struct_type::<OrdinaryObject>()
            .field(OrdinaryObjectSchema::EXTENSIBLE)
            .write(&header, GcOperand::boolean(false), schema, function);
        let array = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ArrayObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&indexed, schema),
                    GcOperand::i64(
                        i64::try_from(elements.len()).expect("template length fits i64"),
                    ),
                    GcOperand::boolean(false),
                ),
                function,
            ),
            function,
        );
        let index = schema.reserve_i64_local(function);
        for (ordinal, descriptor) in descriptors.iter().enumerate() {
            function.instruction(&Instruction::I64Const(
                i64::try_from(ordinal).expect("template index fits i64"),
            ));
            index.store(function);
            self.emit_array_indexed_publish_descriptor(&array, index, descriptor, function)?;
        }
        schema.release_i64_local(index, function);
        for descriptor in descriptors.into_iter().rev() {
            descriptor.clear(function);
        }
        indexed.clear(function);
        absent_accessor.clear(function);
        element.clear(function);
        header.clear(function);
        Ok(array)
    }
}
