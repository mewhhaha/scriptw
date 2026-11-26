//! Optional legacy reflection is owned by actual source-body activations.
//!
//! No JavaScript binding, function name, property containing the callable or
//! depth estimate selects an activation. The ordinary entry owns its actual
//! FunctionObject, and Wasm locals save each nested activation independently.

use super::*;
use crate::gc_types::{
    ArgumentsObject, ExecutableCode, ExecutableCodeSchema, FunctionObject, FunctionObjectSchema,
    FunctionProtocolCode, GcI32Constant, GcLocal, LegacyReflectionPropertyState, Nullable,
    ScalarValue, SourceActivationDepth, SourceDepthOverflow,
};

mod property;
use property::ReflectionProperty;

struct LegacyReflection {
    caller: ReflectionProperty,
    arguments: ReflectionProperty,
}

/// One source-body entry and its matching exit. Strict and non-ordinary source
/// bodies still install their identity: a strict caller is an opaque barrier,
/// rather than a frame which a sloppy callee may skip to reveal an older one.
/// Native entries have a separate invocation-bound barrier; forwarding
/// intrinsics and prepared Scripts are transparent.
#[must_use = "source activations must be restored on every body exit"]
pub(crate) struct SourceActivation {
    depth: SourceActivationDepth,
    previous_function: GcLocal<FunctionObject, Nullable>,
    reflection: Option<LegacyReflection>,
}

impl SourceActivation {
    pub(crate) fn clear(self, function: &mut Function) {
        if let Some(reflection) = self.reflection {
            reflection.arguments.clear(function);
            reflection.caller.clear(function);
        }
        self.previous_function.clear(function);
        self.depth.clear(function);
    }
}

impl FunctionBuilder<'_> {
    /// The one Throw of every rejected source entry, outlined from each
    /// source body's depth guard: a RangeError of the rejected body's
    /// execution Realm, returned as that body's Throw completion. The body
    /// tail-calls it before any parameter, activation or reflection effect.
    pub(crate) fn compile_source_depth_overflow_helper(&mut self) -> Result<Function, EmitError> {
        let mut function =
            self.begin_helper_body(crate::runtime_helpers::RuntimeHelperId::SourceDepthOverflow);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::SourceDepthOverflowParameters>(
                &mut function,
            );
        let overflow = self.runtime_schema().reserve_completion(&mut function);
        self.emit_throw_runtime_error(
            NativeErrorKind::RangeError,
            RuntimeErrorMessage::MAXIMUM_SOURCE_CALL_STACK_SIZE_EXCEEDED,
            &overflow,
            &mut function,
        )?;
        overflow.emit(&mut function);
        overflow.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        crate::runtime_helpers::HelperParameters::release(parameters, &mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    pub(crate) fn emit_enter_source_activation(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self
            .planned_entry()
            .is_some_and(|entry| entry.has_source_activation())
        {
            return Ok(());
        }
        assert!(
            self.source_activation.is_none(),
            "one owner per source body"
        );
        let schema = self.runtime_schema();
        // Admission happens before parameter initialization or reflection
        // publication. The rejected body never owns or retires its caller's
        // depth or legacy properties, and its Throw remains catchable in JS.
        // R-owned bodies (the partition's runtime list) cannot call a
        // program-owned helper.
        let runtime_owned = match self
            .planned_entry()
            .and_then(|entry| entry.source_body_owner())
            .expect("only a source body owns a source activation")
        {
            crate::function_layout::SourceBodyOwner::Runtime => true,
            crate::function_layout::SourceBodyOwner::Program => false,
        };
        let depth = schema.enter_source_activation_depth(function, |function| {
            if runtime_owned {
                let overflow = schema.reserve_completion(function);
                self.emit_throw_runtime_error(
                    NativeErrorKind::RangeError,
                    RuntimeErrorMessage::MAXIMUM_SOURCE_CALL_STACK_SIZE_EXCEEDED,
                    &overflow,
                    function,
                )?;
                return Ok::<_, EmitError>(SourceDepthOverflow::Inline(overflow));
            }
            // A rejected entry is the rare arm.
            function.rarely_reached();
            let base = self.runtime_helper_base()?;
            let realm = self.emit_execution_realm(function);
            let tail = schema.return_call_source_depth_overflow(&realm, base, function);
            realm.clear(function);
            Ok(SourceDepthOverflow::ProgramHelper(tail))
        })?;
        let callable = schema.reserve_gc_local(function).initialize(
            self.body_entry_locals()
                .expect("source activation owns body-entry roles")
                .function_object()
                .expect("source activation owns an actual callable")
                .load(schema, function),
            function,
        );
        let previous_function = schema
            .reserve_gc_local::<FunctionObject, Nullable>(function)
            .initialize(schema.load_active_source_function(function), function);
        let reflection = if self.current_function_meta().is_some_and(|meta| {
            !meta.strict && meta.protocol() == FunctionProtocolIr::OrdinaryCallAndConstruct
        }) {
            let caller_state = schema
                .reserve_gc_local::<LegacyReflectionPropertyState, Nullable>(function)
                .initialize(
                    schema
                        .field(FunctionObjectSchema::LEGACY_CALLER)
                        .read(&callable, schema, function)
                        .reference(),
                    function,
                );
            let caller = ReflectionProperty::capture(self, caller_state, function);
            let arguments_state = schema
                .reserve_gc_local::<LegacyReflectionPropertyState, Nullable>(function)
                .initialize(
                    schema
                        .field(FunctionObjectSchema::LEGACY_ARGUMENTS)
                        .read(&callable, schema, function)
                        .reference(),
                    function,
                );
            let arguments = ReflectionProperty::capture(self, arguments_state, function);
            let value = schema.reserve_value_local(function);
            value.set_scalar(ScalarValue::Null, function);
            previous_function.load(schema, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            let previous = schema.reserve_gc_local(function).initialize(
                previous_function
                    .load(schema, function)
                    .require_non_null(function),
                function,
            );
            let strict = schema.reserve_i32_local(function);
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::STRICT)
                .read(&previous, schema, function)
                .store(strict, function);
            let code = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionObject>()
                    .field(FunctionObjectSchema::CODE)
                    .read(&previous, schema, function)
                    .reference(),
                function,
            );
            let protocol = schema.reserve_i32_local(function);
            let kind = schema.reserve_i32_local(function);
            schema
                .struct_type::<ExecutableCode>()
                .field(ExecutableCodeSchema::PROTOCOL)
                .read(&code, schema, function)
                .store(protocol, function);
            schema
                .struct_type::<ExecutableCode>()
                .field(ExecutableCodeSchema::KIND)
                .read(&code, schema, function)
                .store(kind, function);
            strict.load(function);
            function.instruction(&Instruction::I32Eqz);
            protocol.load(function);
            function.instruction(&Instruction::I32Const(
                FunctionProtocolCode::OrdinaryCallAndConstruct.encoding(),
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            kind.load(function);
            function.instruction(&Instruction::I32Const(
                crate::gc_types::ExecutableCodeKind::JavaScript.encode(),
            ));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32And);
            schema.release_i32_local(kind, function);
            schema.release_i32_local(protocol, function);
            code.clear(function);
            function.instruction(&Instruction::If(BlockType::Empty));
            value.set_reference(&previous, schema, function);
            function.instruction(&Instruction::End);
            schema.release_i32_local(strict, function);
            previous.clear(function);
            function.instruction(&Instruction::End);
            caller.publish(&value, schema, function);
            value.clear(function);
            Some(LegacyReflection { caller, arguments })
        } else {
            None
        };
        schema.replace_active_source_function(&callable, function);
        callable.clear(function);
        self.source_activation = Some(SourceActivation {
            depth,
            previous_function,
            reflection,
        });
        Ok(())
    }

    pub(crate) fn emit_publish_source_activation_arguments(
        &self,
        arguments: &GcLocal<ArgumentsObject>,
        function: &mut Function,
    ) {
        let Some(reflection) = self
            .source_activation
            .as_ref()
            .and_then(|owner| owner.reflection.as_ref())
        else {
            return;
        };
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(arguments, schema, function);
        reflection.arguments.publish(&value, schema, function);
        value.clear(function);
    }

    pub(crate) fn emit_publish_deferred_source_activation_arguments(
        &self,
        plan: crate::arguments_protocol::DeferredLegacyArgumentsPlan,
        function: &mut Function,
    ) {
        let Some(reflection) = self
            .source_activation
            .as_ref()
            .and_then(|owner| owner.reflection.as_ref())
        else {
            return;
        };
        let schema = self.runtime_schema();
        let entry = self
            .body_entry_locals()
            .expect("deferred legacy arguments require body-entry roles");
        let callable = schema.reserve_gc_local(function).initialize(
            entry
                .function_object()
                .expect("deferred legacy arguments require an actual callable")
                .load(schema, function),
            function,
        );
        let arguments = schema
            .reserve_gc_local(function)
            .initialize(entry.arguments().load(schema, function), function);
        reflection.arguments.publish_deferred_arguments(
            &callable,
            &arguments,
            self.current_environment(),
            plan,
            schema,
            function,
        );
        arguments.clear(function);
        callable.clear(function);
    }

    /// This emits restoration, without releasing the compile-time owner: a
    /// return in one branch must not retire locals needed by another branch.
    pub(crate) fn emit_leave_source_activation(&self, function: &mut Function) {
        let Some(owner) = &self.source_activation else {
            return;
        };
        let schema = self.runtime_schema();
        if let Some(reflection) = &owner.reflection {
            reflection.arguments.restore(schema, function);
            reflection.caller.restore(schema, function);
        }
        schema.replace_active_source_function(&owner.previous_function, function);
        owner.depth.restore(function);
    }
}
