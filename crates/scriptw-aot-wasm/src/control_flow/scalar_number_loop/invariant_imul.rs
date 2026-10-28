//! Invariant native methods are selected by nonallocating metadata guards.
use super::*;
use crate::data::{PooledLiteralDomain, StringPool};
use crate::gc_types::{
    DeferredIntrinsicValue, DeferredIntrinsicValueSchema, ExecutableCodeKind, ExecutableCodeSchema,
    FunctionContextSchema, FunctionObjectSchema, GcI32Constant, GcNullability,
    OrdinaryPropertyStorage, PooledStringTable, PropertyValue,
};
use crate::operations::PropertyKeyLocals;

/// Construction proves this ordinal belongs to R, never a Program literal.
/// Emission reads only an existing cache entry; an absent name declines.
#[derive(PartialEq, Eq)]
struct CachedRuntimeName(u32);

impl CachedRuntimeName {
    fn new(name: &str, strings: &StringPool) -> Option<Self> {
        let index = strings.pooled_string_index(name).ok()?;
        let PooledLiteralDomain::Runtime(index) = strings.pooled_string_domain(index).ok()? else {
            return None;
        };
        Some(Self(index.ordinal()))
    }

    fn cached(
        &self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> GcLocal<StringValue, Nullable> {
        let schema = builder.runtime_schema();
        let table = schema
            .reserve_gc_local(function)
            .initialize(schema.pooled_string_table_reference(function), function);
        let index = schema.reserve_i32_local(function);
        index.set_constant(self.0 as i32, function);
        let cached = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PooledStringTable>()
                .read(&table, index, schema, function)
                .reference(),
            function,
        );
        schema.release_i32_local(index, function);
        table.clear(function);
        cached
    }
}

#[derive(PartialEq, Eq)]
enum Receiver {
    Local(LocalBindingId),
    Global(CachedRuntimeName),
}

#[derive(PartialEq, Eq)]
pub(super) struct InvariantImul {
    receiver: Receiver,
    key: CachedRuntimeName,
}

impl InvariantImul {
    pub(super) fn of_call<'a>(
        expression: &'a TypedExpr,
        builder: &FunctionBuilder<'_>,
    ) -> Option<(Self, &'a TypedExpr, &'a TypedExpr)> {
        let ExprIr::MaterializeBinding { name, value, body } = &expression.expr else {
            return None;
        };
        let ExprIr::CallIndirect {
            direct_eval: None,
            callee,
            this_arg: Some(this_arg),
            args,
            static_regexp_compilation: None,
        } = &body.expr
        else {
            return None;
        };
        let ExprIr::Identifier(this_name) = &this_arg.expr else {
            return None;
        };
        let ExprIr::PropertyRead {
            target,
            key: PropertyKeyIr::StaticString(key),
        } = &callee.expr
        else {
            return None;
        };
        let ExprIr::Identifier(target_name) = &target.expr else {
            return None;
        };
        let [left, right] = args.as_slice() else {
            return None;
        };
        // Candidate knowledge bounds generated guards. Only the actual live
        // descriptor/code/Realm checks below can select native arithmetic.
        if name != this_name
            || name != target_name
            || !callee.function_targets.known_targets().iter().any(|id| {
                StandardBuiltinId::from_function_id(id) == Some(StandardBuiltinId::MathImul)
            })
        {
            return None;
        }
        let receiver = match &value.expr {
            // These are the lowerer's located global references. Eval-visible
            // activations instead emit EnvironmentIdentifier; with selection
            // wraps the reference. Neither form is admitted by this match.
            ExprIr::GlobalPropertyRead { name } | ExprIr::GlobalIdentifierRead { name } => {
                Receiver::Global(CachedRuntimeName::new(name, builder.strings)?)
            }
            ExprIr::Identifier(name) if !builder.is_script_global_binding(name) => {
                let BindingStorage::Local(id) = builder.lookup_binding(name)? else {
                    return None;
                };
                Receiver::Local(id)
            }
            _ => return None,
        };
        Some((
            Self {
                receiver,
                key: CachedRuntimeName::new(key, builder.strings)?,
            },
            left,
            right,
        ))
    }

    pub(super) fn local_receiver(&self) -> Option<LocalBindingId> {
        match &self.receiver {
            Receiver::Local(id) => Some(*id),
            Receiver::Global(_) => None,
        }
    }

    pub(super) fn emit_guard(
        &self,
        builder: &mut FunctionBuilder<'_>,
        accepted: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        accepted.set_constant(0, function);
        let receiver = schema.reserve_value_local(function);
        receiver.set_undefined(function);
        match &self.receiver {
            Receiver::Local(id) => {
                let source = builder.local_binding(*id);
                if let Some(initialized) = source.initialization_check() {
                    initialized.load(function);
                } else {
                    function.instruction(&Instruction::I32Const(1));
                }
                builder.open_frame(ControlFrameKind::If, function);
                receiver.copy_from(builder.local_binding(*id).value(), function);
                accepted.set_constant(1, function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            Receiver::Global(name) => {
                let cached = name.cached(builder, function);
                cached.load(schema, function).is_null(function);
                function.instruction(&Instruction::I32Eqz);
                builder.open_frame(ControlFrameKind::If, function);
                let key = schema.reserve_gc_local(function).initialize(
                    cached.load(schema, function).require_non_null(function),
                    function,
                );
                let environment = builder.emit_source_global_environment_to_local(function);
                let entry = schema
                    .reserve_gc_local::<NamedBinding, Nullable>(function)
                    .initialize_null(schema, function);
                builder.emit_find_own_named_binding(&environment, &key, &entry, function);
                entry.load(schema, function).is_null(function);
                builder.open_frame(ControlFrameKind::If, function);
                let global = schema.reserve_value_local(function);
                builder.emit_execution_global_object_to_locals(&global, function);
                let property = PropertyKeyLocals::from_string(schema, &key, function);
                let stored = own_data(builder, &global, &property, function)?;
                stored.is_subtype::<StoredValue>(schema, function);
                builder.open_frame(ControlFrameKind::If, function);
                let value = schema.reserve_gc_local(function).initialize(
                    stored
                        .load(schema, function)
                        .cast_subtype::<StoredValue>(schema, function),
                    function,
                );
                schema
                    .struct_type::<StoredValue>()
                    .read_into(&value, &receiver, schema, function);
                accepted.set_constant(1, function);
                value.clear(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                stored.clear(function);
                property.clear(function);
                global.clear(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                entry.clear(function);
                environment.clear(function);
                key.clear(function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                cached.clear(function);
            }
        }
        accepted.load(function);
        builder.open_frame(ControlFrameKind::If, function);
        accepted.set_constant(0, function);
        let cached = self.key.cached(builder, function);
        cached.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        builder.open_frame(ControlFrameKind::If, function);
        let key = schema.reserve_gc_local(function).initialize(
            cached.load(schema, function).require_non_null(function),
            function,
        );
        let property = PropertyKeyLocals::from_string(schema, &key, function);
        let stored = own_data(builder, &receiver, &property, function)?;
        stored.is_subtype::<DeferredIntrinsicValue>(schema, function);
        builder.open_frame(ControlFrameKind::If, function);
        let recipe = schema.reserve_gc_local(function).initialize(
            stored
                .load(schema, function)
                .cast_subtype::<DeferredIntrinsicValue>(schema, function),
            function,
        );
        let builtin = schema.reserve_i32_local(function);
        schema
            .field(DeferredIntrinsicValueSchema::BUILTIN)
            .read(&recipe, schema, function)
            .store(builtin, function);
        builtin.load(function);
        function.instruction(&Instruction::I32Const(StandardBuiltinId::MathImul.encode()));
        function.instruction(&Instruction::I32Eq);
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(DeferredIntrinsicValueSchema::REALM)
                .read(&recipe, schema, function)
                .reference(),
            function,
        );
        let current = builder.load_current_realm(function);
        realm.load(schema, function);
        current.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32And);
        accepted.store(function);
        current.clear(function);
        realm.clear(function);
        schema.release_i32_local(builtin, function);
        recipe.clear(function);
        function.instruction(&Instruction::Else);
        stored.is_subtype::<StoredValue>(schema, function);
        builder.open_frame(ControlFrameKind::If, function);
        let value = schema.reserve_gc_local(function).initialize(
            stored
                .load(schema, function)
                .cast_subtype::<StoredValue>(schema, function),
            function,
        );
        let callee = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&value, &callee, schema, function);
        emit_identity(builder, &callee, accepted, function);
        callee.clear(function);
        value.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        stored.clear(function);
        property.clear(function);
        key.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        cached.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        receiver.clear(function);
        Ok(())
    }
}

/// Only an existing dictionary descriptor can be returned. Compact storage
/// declines before the shared helper's descriptor-snapshot allocation arm.
fn own_data(
    builder: &mut FunctionBuilder<'_>,
    target: &ValueLocals,
    key: &PropertyKeyLocals,
    function: &mut Function,
) -> Result<GcLocal<PropertyValue, Nullable>, EmitError> {
    let schema = builder.runtime_schema();
    let output = schema
        .reserve_gc_local::<PropertyValue, Nullable>(function)
        .initialize_null(schema, function);
    target.reference().load(function);
    function.instruction(&Instruction::RefTestNonNull(
        schema
            .reference_type::<OrdinaryObject>(GcNullability::NonNullable)
            .heap_type,
    ));
    builder.open_frame(ControlFrameKind::If, function);
    let object = schema.reserve_gc_local(function).initialize(
        target.cast_reference::<OrdinaryObject>(schema, function),
        function,
    );
    let storage = schema.reserve_gc_local(function).initialize(
        schema
            .field(OrdinaryObjectSchema::PROPERTIES)
            .read(&object, schema, function)
            .reference(),
        function,
    );
    storage.is_subtype::<OrdinaryPropertyStorage>(schema, function);
    builder.open_frame(ControlFrameKind::If, function);
    let descriptor = builder.emit_ordinary_own_descriptor_reference(&object, key, function)?;
    descriptor.load(schema, function).is_null(function);
    function.instruction(&Instruction::I32Eqz);
    builder.open_frame(ControlFrameKind::If, function);
    let flags = schema.reserve_i32_local(function);
    schema
        .field(PropertyDescriptorSchema::FLAGS)
        .read(&descriptor, schema, function)
        .store(flags, function);
    flags.load(function);
    function.instruction(&Instruction::I32Const(
        crate::heap::DescriptorMask::ACCESSOR.as_i32(),
    ));
    function.instruction(&Instruction::I32And);
    function.instruction(&Instruction::I32Eqz);
    builder.open_frame(ControlFrameKind::If, function);
    output.replace(
        schema
            .field(PropertyDescriptorSchema::VALUE)
            .read(&descriptor, schema, function)
            .reference()
            .nullable(),
        function,
    );
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
    schema.release_i32_local(flags, function);
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
    descriptor.clear(function);
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
    storage.clear(function);
    object.clear(function);
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
    Ok(output)
}

fn emit_identity(
    builder: &mut FunctionBuilder<'_>,
    callee: &ValueLocals,
    accepted: I32Local,
    function: &mut Function,
) {
    let schema = builder.runtime_schema();
    accepted.set_constant(0, function);
    callee.tag().load(function);
    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
    function.instruction(&Instruction::I32Eq);
    callee.reference().load(function);
    function.instruction(&Instruction::RefTestNonNull(
        schema
            .reference_type::<FunctionObject>(GcNullability::NonNullable)
            .heap_type,
    ));
    function.instruction(&Instruction::I32And);
    builder.open_frame(ControlFrameKind::If, function);
    let callable = schema.reserve_gc_local(function).initialize(
        callee.cast_reference::<FunctionObject>(schema, function),
        function,
    );
    let code = schema.reserve_gc_local(function).initialize(
        schema
            .field(FunctionObjectSchema::CODE)
            .read(&callable, schema, function)
            .reference(),
        function,
    );
    let context = schema.reserve_gc_local(function).initialize(
        schema
            .field(FunctionObjectSchema::CONTEXT)
            .read(&callable, schema, function)
            .reference(),
        function,
    );
    let realm = schema.reserve_gc_local(function).initialize(
        schema
            .field(FunctionContextSchema::REALM)
            .read(&context, schema, function)
            .reference(),
        function,
    );
    let kind = schema.reserve_i32_local(function);
    let builtin = schema.reserve_i32_local(function);
    schema
        .field(ExecutableCodeSchema::KIND)
        .read(&code, schema, function)
        .store(kind, function);
    schema
        .field(ExecutableCodeSchema::STANDARD_BUILTIN)
        .read(&code, schema, function)
        .store(builtin, function);
    kind.load(function);
    function.instruction(&Instruction::I32Const(
        ExecutableCodeKind::StandardBuiltin.encode(),
    ));
    function.instruction(&Instruction::I32Eq);
    builtin.load(function);
    function.instruction(&Instruction::I32Const(StandardBuiltinId::MathImul.encode()));
    function.instruction(&Instruction::I32Eq);
    function.instruction(&Instruction::I32And);
    let current = builder.load_current_realm(function);
    realm.load(schema, function);
    current.load(schema, function);
    function.instruction(&Instruction::RefEq);
    function.instruction(&Instruction::I32And);
    accepted.store(function);
    current.clear(function);
    schema.release_i32_local(builtin, function);
    schema.release_i32_local(kind, function);
    realm.clear(function);
    context.clear(function);
    code.clear(function);
    callable.clear(function);
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
}
