//! RegExp program admission, runtime compilation and literal construction.

use super::*;
use scriptw_ir::RegExpProgram;

mod candidate;

impl<'a> FunctionBuilder<'a> {
    fn emit_collected_regexp_program(
        &mut self,
        program: &RegExpProgram,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<crate::gc_types::RegExpProgram>, EmitError> {
        let span = self
            .strings
            .regexp_program(program)
            .metadata_span(self.strings)?;
        Ok(self.emit_static_regexp_program_span(span, function))
    }

    pub(crate) fn emit_regexp_program_slots(
        &mut self,
        object: &GcLocal<crate::gc_types::RegExpObject>,
        program: &RegExpProgram,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let program = schema
            .reserve_gc_local::<crate::gc_types::RegExpProgram, NonNullable>(function)
            .initialize(
                self.emit_collected_regexp_program(program, function)?,
                function,
            );
        schema
            .struct_type::<crate::gc_types::RegExpObject>()
            .field(crate::gc_types::RegExpObjectSchema::PROGRAM)
            .write(
                object,
                GcOperand::nullable_reference(&program, schema),
                schema,
                function,
            );
        program.clear(function);
        Ok(())
    }

    /// Resolve the program's finite native cache by GC String value through its
    /// hook, then invoke the registered runtime compiler on a miss. Only a
    /// validated, non-null program is published; failure leaves the receiver's
    /// previous matcher untouched.
    pub(crate) fn emit_runtime_regexp_program_slots(
        &mut self,
        object: &GcLocal<crate::gc_types::RegExpObject>,
        source: &GcLocal<StringValue>,
        flags: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::runtime_helpers::{
            RegExpCompilerArguments, RegExpCompilerStatus, RegExpProgramCandidateArguments,
        };
        let schema = self.runtime_schema();
        let pending = schema
            .reserve_gc_local::<crate::gc_types::RegExpProgram, Nullable>(function)
            .initialize_null(schema, function);
        let status = schema.reserve_i32_local(function);
        status.set_constant(RegExpCompilerStatus::Compiled.abi_word() as i32, function);
        let cursor = schema.reserve_i64_local(function);
        let detail = schema.reserve_i64_local(function);
        self.emit_program_hook_dispatch::<crate::gc_types::RegExpProgramCandidateHook>(
            |_, hook, function| {
                let candidate = schema
                    .call_hook(
                        hook,
                        RegExpProgramCandidateArguments::new(source, flags),
                        function,
                    )
                    .bind(
                        schema,
                        schema
                            .reserve_gc_local::<crate::gc_types::RegExpProgram, Nullable>(function),
                        status,
                        cursor,
                        detail,
                        function,
                    );
                pending.replace(candidate.load(schema, function), function);
                candidate.clear(function);
                Ok(())
            },
            |_, _| Ok(()),
            function,
        )?;
        self.emit_regexp_compiler_failures(status, function)?;
        pending.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        let compiled = schema
            .call_helper(
                RegExpCompilerArguments::new(source, flags),
                self.runtime_helper_base()?,
                function,
            )
            .bind(
                schema,
                schema.reserve_gc_local::<crate::gc_types::RegExpProgram, Nullable>(function),
                status,
                cursor,
                detail,
                function,
            );
        self.emit_regexp_compiler_failures(status, function)?;
        status.load(function);
        function.instruction(&Instruction::I32Const(
            RegExpCompilerStatus::Compiled.abi_word() as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        compiled.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_expression_native_error(
            NativeErrorKind::Error,
            RuntimeErrorMessage::REGEXP_RUNTIME_COMPILER_PRODUCED_AN_INVALID_PROGRAM,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.replace(compiled.load(schema, function), function);
        compiled.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<crate::gc_types::RegExpObject>()
            .field(crate::gc_types::RegExpObjectSchema::PROGRAM)
            .write(
                object,
                GcOperand::reference(&pending, schema),
                schema,
                function,
            );
        schema.release_i64_local(detail, function);
        schema.release_i64_local(cursor, function);
        schema.release_i32_local(status, function);
        pending.clear(function);
        Ok(())
    }

    /// Throws the JavaScript error for every non-`Compiled` compiler status.
    fn emit_regexp_compiler_failures(
        &mut self,
        status: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::runtime_helpers::RegExpCompilerStatus;
        for outcome in RegExpCompilerStatus::ALL {
            let error = match outcome {
                RegExpCompilerStatus::Compiled => None,
                RegExpCompilerStatus::SyntaxError => Some((
                    NativeErrorKind::SyntaxError,
                    RuntimeErrorMessage::INVALID_REGULAR_EXPRESSION_PATTERN,
                )),
                RegExpCompilerStatus::ResourceExhausted => Some((
                    NativeErrorKind::RangeError,
                    RuntimeErrorMessage::REGEXP_RUNTIME_COMPILER_EXCEEDED_ITS_ADDRESSABLE_RESOURCE_LIMIT,
                )),
                RegExpCompilerStatus::CorruptProgram => Some((
                    NativeErrorKind::Error,
                    RuntimeErrorMessage::REGEXP_RUNTIME_COMPILER_PRODUCED_AN_INVALID_PROGRAM,
                )),
            };
            if let Some((kind, message)) = error {
                status.load(function);
                function.instruction(&Instruction::I32Const(outcome.abi_word() as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_expression_native_error(kind, message, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        Ok(())
    }

    fn emit_gc_string_contains_ascii_byte_i32(
        &mut self,
        string: &GcLocal<StringValue>,
        byte: u8,
        output: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(crate::gc_types::StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        let array = schema.array_type::<crate::gc_types::CodeUnitArray>();
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        array.length(&units, schema, function);
        length.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        function.instruction(&Instruction::I32Const(0));
        output.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(done, function);
        array
            .read(&units, index, schema, function)
            .store(unit, function);
        unit.load(function);
        function.instruction(&Instruction::I32Const(i32::from(byte)));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        output.store(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        units.clear(function);
    }

    pub(crate) fn emit_regexp_program_with_compatible_flags(
        &mut self,
        object: &GcLocal<crate::gc_types::RegExpObject>,
        source: &GcLocal<StringValue>,
        flags: &GcLocal<StringValue>,
        original_flags: &GcLocal<StringValue>,
        original_program: &GcLocal<crate::gc_types::RegExpProgram, Nullable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let compatible = schema.reserve_i32_local(function);
        let original_has_flag = schema.reserve_i32_local(function);
        let replacement_has_flag = schema.reserve_i32_local(function);
        original_program.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        compatible.store(function);
        // g/d/y change cursor/result bookkeeping, not the compiled matcher.
        for flag in b"imsuv" {
            self.emit_gc_string_contains_ascii_byte_i32(
                original_flags,
                *flag,
                original_has_flag,
                function,
            );
            self.emit_gc_string_contains_ascii_byte_i32(
                flags,
                *flag,
                replacement_has_flag,
                function,
            );
            original_has_flag.load(function);
            replacement_has_flag.load(function);
            function.instruction(&Instruction::I32Eq);
            compatible.load(function);
            function.instruction(&Instruction::I32And);
            compatible.store(function);
        }
        compatible.load(function);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<crate::gc_types::RegExpObject>()
            .field(crate::gc_types::RegExpObjectSchema::PROGRAM)
            .write(
                object,
                GcOperand::reference(original_program, schema),
                schema,
                function,
            );
        function.instruction(&Instruction::Else);
        self.emit_runtime_regexp_program_slots(object, source, flags, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(replacement_has_flag, function);
        schema.release_i32_local(original_has_flag, function);
        schema.release_i32_local(compatible, function);
        Ok(())
    }

    pub(super) fn compile_regexp_literal_to_value(
        &mut self,
        source: &str,
        flags: &str,
        program: &RegExpProgram,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_source_literal_prototype_to_value(
            crate::environments::global_environment::SourceLiteralPrototype::RegExp,
            &prototype,
            function,
        );
        let source = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(source, function)?,
            function,
        );
        let flags = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(flags, function)?,
            function,
        );
        let program = schema.reserve_gc_local(function).initialize(
            self.emit_collected_regexp_program(program, function)?,
            function,
        );
        let realm = self.emit_execution_realm(function);
        let object = schema.reserve_gc_local(function).initialize(
            schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::RegExpLiteralAllocateArguments::new(
                    &prototype, &source, &flags, &program, &realm,
                ),
                self.runtime_helper_base()?,
                function,
            )),
            function,
        );
        output.set_reference(&object, schema, function);
        object.clear(function);
        realm.clear(function);
        program.clear(function);
        flags.clear(function);
        source.clear(function);
        prototype.clear(function);
        Ok(())
    }
}
