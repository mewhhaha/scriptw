use super::*;
use crate::runtime_helpers::{
    GlobalIdentifierReadSloppyArguments, GlobalIdentifierReadSloppyParameters,
    GlobalIdentifierReadStrictArguments, GlobalIdentifierReadStrictParameters,
    GlobalIdentifierTypeofSloppyArguments, GlobalIdentifierTypeofSloppyParameters,
    GlobalIdentifierTypeofStrictArguments, GlobalIdentifierTypeofStrictParameters,
    HelperParameters,
};

// Strictness and typeof behavior are closed helper declarations. Every body
// reuses the actual Reference owner; no Reference crosses this call boundary.
macro_rules! global_identifier_read_helper {
    ($compile:ident, $id:ident, $parameters:ident, $strictness:ident, $read:ident) => {
        pub(crate) fn $compile(&mut self) -> Result<Function, EmitError> {
            let mut function = self.begin_helper_body(RuntimeHelperId::$id);
            let parameters = self.helper_parameters::<$parameters>(&mut function);
            self.push_scope();
            self.completion().initialize(&mut function);
            self.emit_global_identifier_read_body(
                &parameters.name,
                &parameters.global_environment,
                Strictness::$strictness,
                EnvironmentIdentifierRead::$read,
                &mut function,
            )?;
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
    pub(crate) fn emit_global_identifier_read(
        &mut self,
        name: &str,
        read: EnvironmentIdentifierRead,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        // Only the source caller can select its lexical Global Environment.
        // Error construction separately retains that caller's execution Realm.
        let global = self.emit_source_global_environment_to_local(function);
        let realm = self.emit_execution_realm(function);
        let base = self.runtime_helper_base()?;
        let strictness = if self.is_current_function_strict() {
            Strictness::Strict
        } else {
            Strictness::Sloppy
        };
        macro_rules! call {
            ($arguments:ident) => {
                schema
                    .call_helper(
                        $arguments::new(
                            &key,
                            &global,
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
                call!(GlobalIdentifierReadSloppyArguments)
            }
            (Strictness::Strict, EnvironmentIdentifierRead::Value) => {
                call!(GlobalIdentifierReadStrictArguments)
            }
            (Strictness::Sloppy, EnvironmentIdentifierRead::Typeof) => {
                call!(GlobalIdentifierTypeofSloppyArguments)
            }
            (Strictness::Strict, EnvironmentIdentifierRead::Typeof) => {
                call!(GlobalIdentifierTypeofStrictArguments)
            }
        }
        realm.clear(function);
        global.clear(function);
        key.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        value.copy_from(self.completion().value(), function);
        Ok(())
    }

    fn emit_global_identifier_read_body(
        &mut self,
        name: &GcLocal<StringValue>,
        global: &GcLocal<Environment>,
        strictness: Strictness,
        read: EnvironmentIdentifierRead,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(global.load(schema, function).nullable(), function);
        let value = schema.reserve_value_local(function);
        // An own data property of the global object, or an unresolvable
        // typeof operand, needs no Reference.
        let direct =
            self.emit_global_identifier_direct_read_i32(name, &record, read, &value, function)?;
        direct.load(function);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(direct, function);
        self.open_frame(ControlFrameKind::If, function);
        let reference = self
            .emit_resolve_environment_identifier_from_record(name, strictness, record, function)?;
        self.emit_environment_identifier_get(&reference, read, &value, function)?;
        self.release_environment_identifier_reference(reference, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().commit_if_normal(&value, function);
        value.clear(function);
        Ok(())
    }

    global_identifier_read_helper!(
        compile_global_identifier_read_sloppy_helper,
        GlobalIdentifierReadSloppy,
        GlobalIdentifierReadSloppyParameters,
        Sloppy,
        Value
    );
    global_identifier_read_helper!(
        compile_global_identifier_read_strict_helper,
        GlobalIdentifierReadStrict,
        GlobalIdentifierReadStrictParameters,
        Strict,
        Value
    );
    global_identifier_read_helper!(
        compile_global_identifier_typeof_sloppy_helper,
        GlobalIdentifierTypeofSloppy,
        GlobalIdentifierTypeofSloppyParameters,
        Sloppy,
        Typeof
    );
    global_identifier_read_helper!(
        compile_global_identifier_typeof_strict_helper,
        GlobalIdentifierTypeofStrict,
        GlobalIdentifierTypeofStrictParameters,
        Strict,
        Typeof
    );
}
