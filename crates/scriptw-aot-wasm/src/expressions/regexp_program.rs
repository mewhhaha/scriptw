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
