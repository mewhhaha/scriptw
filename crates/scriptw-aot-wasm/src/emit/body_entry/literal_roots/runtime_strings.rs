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
        let string = schema
            .reserve_gc_local::<StringValue, NonNullable>(&mut function)
            .initialize(
                row.publish_string(
                    schema,
                    schema.reserve_gc_local(&mut function),
                    &mut function,
                ),
                &mut function,
            );
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

    /// The first access publishes a well-known Symbol in its Store-wide slot.
    pub(crate) fn compile_well_known_symbol_resolve_helper(&mut self) -> Result<Function, EmitError> {
        self.strings.require_runtime_only()?;
        let base = self.runtime_helper_base()?;
        let mut f = self.begin_helper_body(RuntimeHelperId::WellKnownSymbolResolve);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::WellKnownSymbolResolveParameters>(&mut f);
        let s = self.schema;
        let table = s.reserve_gc_local(&mut f).initialize(
            s.well_known_symbol_table_reference(&mut f),
            &mut f,
        );
        let output = s.reference_type::<SymbolValue>(GcNullability::NonNullable);
        f.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(output))));
        let array = s.array_type::<WellKnownSymbolTable>();
        array.read(&table, parameters.ordinal, s, &mut f);
        f.likely(&Instruction::BrOnNonNull(0));

        // Branch to the R string ordinal that belongs to this symbol ordinal.
        // Keep this mapping independent of pooled-string deduplication.
        let symbols = scriptw_ir::WellKnownSymbol::ALL;
        let count = symbols.len() as u32;
        f.instruction(&Instruction::Block(BlockType::Result(ValType::I32)));
        for _ in 0..=count {
            f.instruction(&Instruction::Block(BlockType::Empty));
        }
        parameters.ordinal.load(&mut f);
        f.instruction(&Instruction::BrTable((0..count).collect::<Vec<_>>().into(), count));
        for (ordinal, symbol) in symbols.into_iter().enumerate() {
            f.instruction(&Instruction::End);
            let description = self.strings.pooled_string_index(symbol.description())?;
            f.instruction(&Instruction::I32Const(description.ordinal() as i32));
            f.instruction(&Instruction::Br(count - ordinal as u32));
        }
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        let description_ordinal = s.reserve_i32_local(&mut f);
        description_ordinal.store(&mut f);
        let description = s.reserve_gc_local(&mut f).initialize(
            s.resolve_pooled_string_reference(description_ordinal, base, &mut f),
            &mut f,
        );
        let symbol = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<SymbolValue>().construct(
                (
                    GcOperand::nullable_reference(&description, s),
                    GcOperand::null(s),
                    GcOperand::i64(0),
                ),
                &mut f,
            ),
            &mut f,
        );
        array.write(
            &table,
            parameters.ordinal,
            GcOperand::nullable_reference(&symbol, s),
            s,
            &mut f,
        );
        symbol.load(s, &mut f);
        symbol.clear(&mut f);
        description.clear(&mut f);
        s.release_i32_local(description_ordinal, &mut f);
        f.instruction(&Instruction::End);
        table.clear(&mut f);
        crate::runtime_helpers::HelperParameters::release(parameters, &mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    /// R owns the canonical string and well-known Symbol caches. The
    /// registered Symbol table stays null until the first insertion.
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
        schema.publish_pooled_strings(&table, function_body);
        table.clear(function_body);
        length.set_constant(scriptw_ir::WellKnownSymbol::ALL.len() as i32, function_body);
        let symbols_slot =
            schema.reserve_gc_local::<WellKnownSymbolTable, NonNullable>(function_body);
        let symbol_table = symbols_slot.initialize(
            schema.array_type::<WellKnownSymbolTable>().filled(
                GcOperand::null(schema),
                length,
                function_body,
            ),
            function_body,
        );
        schema.publish_well_known_symbols(&symbol_table, function_body);
        symbol_table.clear(function_body);
        schema.release_i32_local(length, function_body);
        crate::runtime_helpers::HelperParameters::release(parameters, function_body);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
