//! Ordinary reads keep ResolveBinding and GetValue in the shared runtime.
//! References that cross source evaluation retain their separate capture route.

use super::*;
use crate::runtime_helpers::{
    EnvironmentIdentifierReadSloppyArguments, EnvironmentIdentifierReadSloppyParameters,
    EnvironmentIdentifierReadStrictArguments, EnvironmentIdentifierReadStrictParameters,
    EnvironmentIdentifierReadTypeofSloppyArguments,
    EnvironmentIdentifierReadTypeofSloppyParameters,
    EnvironmentIdentifierReadTypeofStrictArguments,
    EnvironmentIdentifierReadTypeofStrictParameters, HelperParameters,
};

macro_rules! environment_identifier_read_helper {
    ($compile:ident, $id:ident, $parameters:ident, $strictness:ident, $read:ident) => {
        pub(crate) fn $compile(&mut self) -> Result<Function, EmitError> {
            let mut function = self.begin_helper_body(RuntimeHelperId::$id);
            let parameters = self.helper_parameters::<$parameters>(&mut function);
            self.push_scope();
            self.completion().initialize(&mut function);
            let schema = self.runtime_schema();
            let record = schema
                .reserve_gc_local(&mut function)
                .initialize(parameters.record.load(schema, &mut function), &mut function);
            let value = schema.reserve_value_local(&mut function);
            // An own data property of the global object needs no Reference.
            let direct = self.emit_global_own_data_identifier_read_i32(
                &parameters.name,
                &record,
                &value,
                &mut function,
            )?;
            direct.load(&mut function);
            function.instruction(&Instruction::I32Eqz);
            schema.release_i32_local(direct, &mut function);
            self.open_frame(ControlFrameKind::If, &mut function);
            // An abrupt search skips GetValue. Both paths reach the same
            // Reference retirement before returning the complete Completion.
            let exit = self.open_frame(ControlFrameKind::Block, &mut function);
            self.throw_handler_stack.push(exit);
            let reference = self.emit_resolve_environment_identifier_from_record(
                &parameters.name,
                Strictness::$strictness,
                record,
                &mut function,
            )?;
            self.emit_environment_identifier_get(
                &reference,
                EnvironmentIdentifierRead::$read,
                &value,
                &mut function,
            )?;
            self.throw_handler_stack.pop();
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            self.release_environment_identifier_reference(reference, &mut function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.completion().commit_if_normal(&value, &mut function);
            value.clear(&mut function);
            self.pop_scope();
            self.clear_helper_execution_realm(&mut function);
            parameters.release(&mut function);
            self.completion().emit(&mut function);
            function.instruction(&Instruction::End);
            Ok(self.finish_function(function))
        }
    };
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_environment_identifier_read_from_record(
        &mut self,
        name: &GcLocal<StringValue>,
        strictness: Strictness,
        read: EnvironmentIdentifierRead,
        record: GcLocal<Environment, Nullable>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let base = self.runtime_helper_base()?;
        macro_rules! call {
            ($arguments:ident) => {
                schema
                    .call_helper(
                        $arguments::new(
                            name,
                            &record,
                            &realm,
                            self.current_environment(),
                            self.current_program(),
                        ),
                        base,
                        function,
                    )
                    .store(self.completion(), function)
            };
        }
        match (strictness, read) {
            (Strictness::Sloppy, EnvironmentIdentifierRead::Value) => {
                call!(EnvironmentIdentifierReadSloppyArguments)
            }
            (Strictness::Strict, EnvironmentIdentifierRead::Value) => {
                call!(EnvironmentIdentifierReadStrictArguments)
            }
            (Strictness::Sloppy, EnvironmentIdentifierRead::Typeof) => {
                call!(EnvironmentIdentifierReadTypeofSloppyArguments)
            }
            (Strictness::Strict, EnvironmentIdentifierRead::Typeof) => {
                call!(EnvironmentIdentifierReadTypeofStrictArguments)
            }
        }
        record.clear(function);
        realm.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        value.copy_from(self.completion().value(), function);
        Ok(())
    }

    environment_identifier_read_helper!(
        compile_environment_identifier_read_sloppy_helper,
        EnvironmentIdentifierReadSloppy,
        EnvironmentIdentifierReadSloppyParameters,
        Sloppy,
        Value
    );
    environment_identifier_read_helper!(
        compile_environment_identifier_read_strict_helper,
        EnvironmentIdentifierReadStrict,
        EnvironmentIdentifierReadStrictParameters,
        Strict,
        Value
    );
    environment_identifier_read_helper!(
        compile_environment_identifier_read_typeof_sloppy_helper,
        EnvironmentIdentifierReadTypeofSloppy,
        EnvironmentIdentifierReadTypeofSloppyParameters,
        Sloppy,
        Typeof
    );
    environment_identifier_read_helper!(
        compile_environment_identifier_read_typeof_strict_helper,
        EnvironmentIdentifierReadTypeofStrict,
        EnvironmentIdentifierReadTypeofStrictParameters,
        Strict,
        Typeof
    );
}
