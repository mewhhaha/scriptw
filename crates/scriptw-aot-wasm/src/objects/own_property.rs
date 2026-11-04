//! Sole [[GetOwnProperty]] runtime owner. The private result is a completed
//! snapshot, never the JavaScript object exposed by public descriptor methods.
use super::*;
use crate::runtime_helpers::{
    ObjectGetOwnPropertyArguments, ObjectGetOwnPropertyParameters, RuntimeHelperId,
};

mod proxy;

impl FunctionBuilder<'_> {
    /// Root both results before the caller chooses its abrupt-completion route.
    /// Iterator acquisition and native callers may need cleanup before throwing.
    pub(crate) fn emit_object_get_own_property(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        completion: &CompletionLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let descriptor = schema
            .call_helper(
                ObjectGetOwnPropertyArguments::new(
                    target,
                    key,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .bind(schema, completion, function);
        realm.clear(function);
        Ok(descriptor)
    }

    pub(crate) fn compile_object_get_own_property_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ObjectGetOwnProperty);
        let parameters = self.helper_parameters::<ObjectGetOwnPropertyParameters>(&mut function);
        let schema = self.runtime_schema();
        self.completion().initialize(&mut function);
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(&mut function)
            .initialize_null(schema, &mut function);
        // Namespace TDZ, descriptor conversion and recursive Proxy failures all
        // leave through this extended ABI, preserving the whole Throw. Publish
        // the snapshot only after the entire operation has completed normally.
        let exit = self.open_frame(ControlFrameKind::Block, &mut function);
        self.throw_handler_stack.push(exit);
        let snapshot =
            self.emit_get_own_property_kernel(&parameters.target, &parameters.key, &mut function)?;
        descriptor.replace(snapshot.load(schema, &mut function), &mut function);
        snapshot.clear(&mut function);
        self.completion().initialize(&mut function);
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().emit(&mut function);
        descriptor.load(schema, &mut function);
        descriptor.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_get_own_property_kernel(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            &pending,
            function,
            |builder, slots, function| {
                builder.emit_proxy_get_own_property_descriptor(
                    slots,
                    key,
                    &descriptor,
                    &pending,
                    function,
                )
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        let source = self.emit_non_proxy_own_descriptor(target, key, function)?;
        self.emit_completed_descriptor_snapshot(source, &descriptor, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(descriptor)
    }
}
