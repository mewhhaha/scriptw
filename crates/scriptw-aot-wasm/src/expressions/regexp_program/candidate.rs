//! Finite compiler-owned RegExp candidate lookup and descriptor publication.

use super::*;
use crate::gc_types::I64Local;

impl FunctionBuilder<'_> {
    /// The program half: the literal `(source, flags)` pairs the compiler
    /// already turned into validated programs. Status `SyntaxError` is a
    /// literal pair the compiler rejected; a miss is a null program with
    /// `Compiled`, which sends the runtime to its own compiler.
    pub(crate) fn compile_regexp_program_candidate_hook(&mut self) -> Result<Function, EmitError> {
        use crate::data::{
            runtime_regexp_record_offset, RuntimeRegExpEntryKind,
            RUNTIME_REGEXP_CANDIDATE_KEY_OFFSET_BASIS, RUNTIME_REGEXP_CANDIDATE_KEY_PRIME,
            RUNTIME_REGEXP_CANDIDATE_KEY_SEPARATOR, RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN,
            RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD, RUNTIME_REGEXP_RECORD_FLAGS_WORD,
            RUNTIME_REGEXP_RECORD_KEY_WORD, RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD,
            RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD, RUNTIME_REGEXP_RECORD_SIZE,
            RUNTIME_REGEXP_RECORD_SOURCE_WORD,
        };
        use crate::runtime_helpers::{
            HelperParameters, RegExpCompilerStatus, RegExpProgramCandidateParameters,
        };
        let mut function = self.begin_helper_body(RuntimeHelperId::RegExpProgramCandidate);
        let parameters = self.helper_parameters::<RegExpProgramCandidateParameters>(&mut function);
        self.push_scope();
        let schema = self.runtime_schema();
        let program = schema
            .reserve_gc_local::<crate::gc_types::RegExpProgram, Nullable>(&mut function)
            .initialize_null(schema, &mut function);
        let status = schema.reserve_i32_local(&mut function);
        status.set_constant(
            RegExpCompilerStatus::Compiled.abi_word() as i32,
            &mut function,
        );
        if let Some(candidates) = self.strings.runtime_regexp_table() {
            let metadata = self.current_program().metadata_span(
                schema,
                candidates.metadata_span(self.strings)?,
                &mut function,
            );
            let pointer = schema.reserve_i32_local(&mut function);
            let row = schema.reserve_i32_local(&mut function);
            let upper = schema.reserve_i32_local(&mut function);
            let middle = schema.reserve_i32_local(&mut function);
            let key = schema.reserve_i64_local(&mut function);
            let literal = schema.reserve_i64_local(&mut function);
            let matched = schema.reserve_i32_local(&mut function);
            let recognized = schema.reserve_i32_local(&mut function);
            let kind_word = schema.reserve_i64_local(&mut function);
            let payload = schema.reserve_i64_local(&mut function);
            key.set_constant(
                RUNTIME_REGEXP_CANDIDATE_KEY_OFFSET_BASIS as i64,
                &mut function,
            );
            self.emit_regexp_candidate_key_units(&parameters.source, key, &mut function);
            key.load(&mut function);
            function.instruction(&Instruction::I64Const(
                RUNTIME_REGEXP_CANDIDATE_KEY_SEPARATOR as i64,
            ));
            function.instruction(&Instruction::I64Xor);
            function.instruction(&Instruction::I64Const(
                RUNTIME_REGEXP_CANDIDATE_KEY_PRIME as i64,
            ));
            function.instruction(&Instruction::I64Mul);
            key.store(&mut function);
            self.emit_regexp_candidate_key_units(&parameters.flags, key, &mut function);
            // Lower bound of `key` in the unsigned-sorted key column.
            row.set_constant(0, &mut function);
            upper.set_constant(candidates.count() as i32, &mut function);
            let searched = self.open_frame(ControlFrameKind::Block, &mut function);
            let halve = self.open_frame(ControlFrameKind::Loop, &mut function);
            row.load(&mut function);
            upper.load(&mut function);
            function.instruction(&Instruction::I32GeU);
            self.emit_branch_if_to_target(searched, &mut function);
            // `row + upper` fits: both are at most the u32 row count, which a
            // checked table extent bounds far below 2^31.
            row.load(&mut function);
            upper.load(&mut function);
            function.instruction(&Instruction::I32Add);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32ShrU);
            middle.store(&mut function);
            middle.load(&mut function);
            function.instruction(&Instruction::I32Const(RUNTIME_REGEXP_RECORD_SIZE as i32));
            function.instruction(&Instruction::I32Mul);
            pointer.store(&mut function);
            metadata.read_u64(
                pointer,
                runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_KEY_WORD) as u32,
                schema,
                &mut function,
            );
            key.load(&mut function);
            function.instruction(&Instruction::I64LtU);
            self.open_frame(ControlFrameKind::If, &mut function);
            middle.load(&mut function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            row.store(&mut function);
            function.instruction(&Instruction::Else);
            middle.load(&mut function);
            upper.store(&mut function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.emit_branch_to_target(halve, &mut function);
            self.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            row.load(&mut function);
            function.instruction(&Instruction::I32Const(RUNTIME_REGEXP_RECORD_SIZE as i32));
            function.instruction(&Instruction::I32Mul);
            pointer.store(&mut function);
            // Rows with an equal key, in table order; the first String-equal
            // row decides, exactly as the former whole-table scan did.
            let done = self.open_frame(ControlFrameKind::Block, &mut function);
            let next = self.open_frame(ControlFrameKind::Loop, &mut function);
            row.load(&mut function);
            function.instruction(&Instruction::I32Const(candidates.count() as i32));
            function.instruction(&Instruction::I32GeU);
            self.emit_branch_if_to_target(done, &mut function);
            metadata.read_u64(
                pointer,
                runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_KEY_WORD) as u32,
                schema,
                &mut function,
            );
            key.load(&mut function);
            function.instruction(&Instruction::I64Ne);
            self.emit_branch_if_to_target(done, &mut function);
            metadata.read_u64(
                pointer,
                runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_SOURCE_WORD) as u32,
                schema,
                &mut function,
            );
            literal.store(&mut function);
            let source = schema
                .reserve_gc_local::<StringValue, NonNullable>(&mut function)
                .initialize(
                    self.current_program().literal_operand(
                        schema,
                        literal,
                        self.runtime_helper_base()?,
                        &mut function,
                    ),
                    &mut function,
                );
            self.emit_string_payload_equality_i32(&parameters.source, &source, &mut function);
            matched.store(&mut function);
            source.clear(&mut function);
            matched.load(&mut function);
            self.open_frame(ControlFrameKind::If, &mut function);
            metadata.read_u64(
                pointer,
                runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_FLAGS_WORD) as u32,
                schema,
                &mut function,
            );
            literal.store(&mut function);
            let flags = schema
                .reserve_gc_local::<StringValue, NonNullable>(&mut function)
                .initialize(
                    self.current_program().literal_operand(
                        schema,
                        literal,
                        self.runtime_helper_base()?,
                        &mut function,
                    ),
                    &mut function,
                );
            self.emit_string_payload_equality_i32(&parameters.flags, &flags, &mut function);
            matched.store(&mut function);
            flags.clear(&mut function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            matched.load(&mut function);
            self.open_frame(ControlFrameKind::If, &mut function);
            metadata.read_u64(
                pointer,
                runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_ENTRY_KIND_WORD) as u32,
                schema,
                &mut function,
            );
            kind_word.store(&mut function);
            recognized.set_constant(0, &mut function);
            for kind in RuntimeRegExpEntryKind::ALL {
                kind_word.load(&mut function);
                function.instruction(&Instruction::I64Const(kind.word() as i64));
                function.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, &mut function);
                recognized.set_constant(1, &mut function);
                if kind.throws_syntax_error() {
                    status.set_constant(
                        RegExpCompilerStatus::SyntaxError.abi_word() as i32,
                        &mut function,
                    );
                }
                match kind {
                    RuntimeRegExpEntryKind::Program => {
                        metadata.read_u64(
                            pointer,
                            runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_PROGRAM_DOMAIN_WORD)
                                as u32,
                            schema,
                            &mut function,
                        );
                        function.instruction(&Instruction::I64Const(
                            RUNTIME_REGEXP_DESCRIPTOR_PROGRAM_DOMAIN as i64,
                        ));
                        function.instruction(&Instruction::I64Ne);
                        self.open_frame(ControlFrameKind::If, &mut function);
                        status.set_constant(
                            RegExpCompilerStatus::CorruptProgram.abi_word() as i32,
                            &mut function,
                        );
                        self.emit_branch_to_target(done, &mut function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                        metadata.read_u64(
                            pointer,
                            runtime_regexp_record_offset(RUNTIME_REGEXP_RECORD_PROGRAM_PAYLOAD_WORD)
                                as u32,
                            schema,
                            &mut function,
                        );
                        payload.store(&mut function);
                        program.replace(
                            self.emit_static_regexp_program_payload(payload, &mut function)
                                .nullable(),
                            &mut function,
                        );
                        self.emit_branch_to_target(done, &mut function);
                    }
                    RuntimeRegExpEntryKind::Rejected => {
                        self.emit_branch_to_target(done, &mut function);
                    }
                    RuntimeRegExpEntryKind::Unsupported => {}
                }
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            recognized.load(&mut function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, &mut function);
            status.set_constant(
                RegExpCompilerStatus::CorruptProgram.abi_word() as i32,
                &mut function,
            );
            self.emit_branch_to_target(done, &mut function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            row.load(&mut function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            row.store(&mut function);
            pointer.load(&mut function);
            function.instruction(&Instruction::I32Const(RUNTIME_REGEXP_RECORD_SIZE as i32));
            function.instruction(&Instruction::I32Add);
            pointer.store(&mut function);
            self.emit_branch_to_target(next, &mut function);
            self.pop_control(ControlFrameKind::Loop);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            metadata.clear(schema, &mut function);
            for local in [pointer, row, upper, middle, matched, recognized] {
                schema.release_i32_local(local, &mut function);
            }
            for local in [kind_word, payload, literal, key] {
                schema.release_i64_local(local, &mut function);
            }
        }
        self.pop_scope();
        parameters.release(&mut function);
        // Result order: program, status, cursor, detail.
        program.load(schema, &mut function);
        status.load(&mut function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64Const(0));
        program.clear(&mut function);
        schema.release_i32_local(status, &mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Mixes `text`'s code units into `key` exactly as the table writer's
    /// [`crate::data::runtime_regexp_candidate_key`] does.
    fn emit_regexp_candidate_key_units(
        &mut self,
        text: &GcLocal<StringValue>,
        key: I64Local,
        function: &mut Function,
    ) {
        use crate::data::RUNTIME_REGEXP_CANDIDATE_KEY_PRIME;
        use crate::gc_types::{CodeUnitArray, StringValueSchema};
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(text, schema, function)
                .reference(),
            function,
        );
        let length = schema.reserve_i32_local(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        length.store(function);
        let index = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        index.set_constant(0, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, index, schema, function)
            .store(unit, function);
        key.load(function);
        unit.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Xor);
        function.instruction(&Instruction::I64Const(
            RUNTIME_REGEXP_CANDIDATE_KEY_PRIME as i64,
        ));
        function.instruction(&Instruction::I64Mul);
        key.store(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for local in [length, index, unit] {
            schema.release_i32_local(local, function);
        }
        units.clear(function);
    }

    /// Literal/constructor collection supplies a checked span in this P image.
    pub(super) fn emit_static_regexp_program_span(
        &mut self,
        span: crate::data::ProgramMetadataSpan,
        function: &mut Function,
    ) -> crate::gc_types::GcStackReference<crate::gc_types::RegExpProgram> {
        let metadata = self
            .current_program()
            .metadata_span(self.runtime_schema(), span, function);
        self.emit_regexp_program_metadata(metadata, function)
    }

    /// Only the checked Program-domain candidate row supplies a runtime payload.
    fn emit_static_regexp_program_payload(
        &mut self,
        payload: I64Local,
        function: &mut Function,
    ) -> crate::gc_types::GcStackReference<crate::gc_types::RegExpProgram> {
        let schema = self.runtime_schema();
        let metadata = self
            .current_program()
            .metadata_payload(schema, payload, function);
        self.emit_regexp_program_metadata(metadata, function)
    }

    /// Both paths carry the actual P owner and checked bounds into one bounded
    /// copy, publishing immutable bytes only after the construction is complete.
    fn emit_regexp_program_metadata(
        &mut self,
        metadata: crate::gc_types::ProgramMetadataView,
        function: &mut Function,
    ) -> crate::gc_types::GcStackReference<crate::gc_types::RegExpProgram> {
        let schema = self.runtime_schema();
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let byte = schema.reserve_i32_local(function);
        metadata.length(function);
        length.store(function);
        index.set_constant(0, function);
        let construction =
            crate::gc_types::RegExpProgramConstruction::allocate(schema, length, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        metadata.read_byte(index, schema, function);
        byte.store(function);
        construction.write(index, byte, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let program = construction.publish(schema, function);
        metadata.clear(schema, function);
        for local in [length, index, byte] {
            schema.release_i32_local(local, function);
        }
        program
    }
}
