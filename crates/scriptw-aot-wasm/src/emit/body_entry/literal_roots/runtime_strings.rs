use super::*;

impl FunctionBuilder<'_> {
    /// Resolve only a checked ordinal in R's sealed canonical prefix. The
    /// passive UTF-16 segment remains alive for the lifetime of this cache.
    pub(crate) fn compile_pooled_string_resolve_helper(&mut self) -> Result<Function, EmitError> {
        self.strings.require_runtime_only()?;
        let mut function = self.begin_helper_body(RuntimeHelperId::PooledStringResolve);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::PooledStringResolveParameters>(
                &mut function,
            );
        let schema = self.schema;
        let descriptors = self.strings.runtime_string_descriptors()?;
        let table = schema.reserve_gc_local(&mut function).initialize(
            schema.pooled_string_table_reference(&mut function),
            &mut function,
        );
        parameters.ordinal.load(&mut function);
        function.instruction(&Instruction::I32Const(descriptors.count() as i32));
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        let cached = schema
            .reserve_gc_local::<StringValue, Nullable>(&mut function)
            .initialize(
                schema
                    .array_type::<PooledStringTable>()
                    .read(&table, parameters.ordinal, schema, &mut function)
                    .reference(),
                &mut function,
            );
        cached.load(schema, &mut function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        let row = crate::gc_types::RuntimeStringDescriptorRowLocals::read(
            descriptors,
            schema,
            parameters.ordinal,
            &mut function,
        );
        let construction = StringConstruction::from_runtime_pooled_row(
            schema,
            schema.reserve_gc_local(&mut function),
            &row,
            &mut function,
        );
        let string = schema
            .reserve_gc_local::<StringValue, NonNullable>(&mut function)
            .initialize(construction.publish(schema, &mut function), &mut function);
        schema.array_type::<PooledStringTable>().write(
            &table,
            parameters.ordinal,
            GcOperand::nullable_reference(&string, schema),
            schema,
            &mut function,
        );
        cached.replace(string.load(schema, &mut function).nullable(), &mut function);
        string.clear(&mut function);
        row.release(schema, &mut function);
        function.instruction(&Instruction::End);
        cached
            .load(schema, &mut function)
            .require_non_null(&mut function);
        cached.clear(&mut function);
        table.clear(&mut function);
        crate::runtime_helpers::HelperParameters::release(parameters, &mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// R owns exactly its stable prefix, the well-known symbols and registry.
    /// There is no parameter through which a P can resize the shared table.
    pub(crate) fn compile_pooled_strings_initialize_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        self.strings.require_runtime_only()?;
        let mut function = self.begin_helper_body(RuntimeHelperId::PooledStringsInitialize);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::PooledStringsInitializeParameters>(
                &mut function,
            );
        let schema = self.schema;
        let function_body = &mut function;
        let length = schema.reserve_i32_local(function_body);
        let index = schema.reserve_i32_local(function_body);
        length.set_constant(
            self.strings.compiler_owned_boundary().strings() as i32,
            function_body,
        );
        let table_slot = schema.reserve_gc_local::<PooledStringTable, NonNullable>(function_body);
        let table = table_slot.initialize(
            schema.array_type::<PooledStringTable>().filled(
                GcOperand::null(schema),
                length,
                function_body,
            ),
            function_body,
        );
        // Publish the fixed R-only nullable cache before resolving symbol
        // descriptions. Unused canonical strings never allocate GC objects.
        schema.publish_pooled_strings(&table, function_body);
        let mut symbols = Vec::new();
        for symbol in scriptw_ir::WellKnownSymbol::ALL {
            let description_slot =
                schema.reserve_gc_local::<StringValue, NonNullable>(function_body);
            function_body.instruction(&Instruction::I32Const(
                self.strings
                    .pooled_string_index(symbol.description())?
                    .ordinal() as i32,
            ));
            index.store(function_body);
            let description = description_slot.initialize(
                schema.resolve_pooled_string_reference(
                    index,
                    self.runtime_helper_base()?,
                    function_body,
                ),
                function_body,
            );
            let symbol_slot = schema.reserve_gc_local::<SymbolValue, NonNullable>(function_body);
            let value = symbol_slot.initialize(
                schema.struct_type::<SymbolValue>().construct(
                    (
                        GcOperand::nullable_reference(&description, schema),
                        GcOperand::null(schema),
                        GcOperand::i64(0),
                    ),
                    function_body,
                ),
                function_body,
            );
            description.clear(function_body);
            symbols.push(value);
        }
        let symbols_slot =
            schema.reserve_gc_local::<WellKnownSymbolTable, NonNullable>(function_body);
        let symbol_table = symbols_slot.initialize(
            schema.array_type::<WellKnownSymbolTable>().fixed(
                symbols
                    .iter()
                    .map(|symbol| GcOperand::reference(symbol, schema)),
                function_body,
            ),
            function_body,
        );
        schema.publish_well_known_symbols(&symbol_table, function_body);
        let registry = schema
            .reserve_gc_local::<RegisteredSymbolTable, NonNullable>(function_body)
            .initialize(
                schema
                    .array_type::<RegisteredSymbolTable>()
                    .fixed([], function_body),
                function_body,
            );
        schema.replace_symbol_registry(&registry, function_body);
        registry.clear(function_body);
        symbol_table.clear(function_body);
        for symbol in symbols.into_iter().rev() {
            symbol.clear(function_body);
        }
        table.clear(function_body);
        schema.release_i32_local(index, function_body);
        schema.release_i32_local(length, function_body);
        crate::runtime_helpers::HelperParameters::release(parameters, function_body);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
