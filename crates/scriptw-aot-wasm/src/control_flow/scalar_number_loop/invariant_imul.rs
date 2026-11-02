//! Invariant native methods use own-data reads and actual code/Realm identity.
use super::*;
use crate::data::{PooledLiteralDomain, StringPool};
use crate::gc_types::{
    ExecutableCodeKind, ExecutableCodeSchema, FunctionContextSchema, FunctionObjectSchema,
    GcI32Constant, GcNullability, PooledStringTable,
};
use crate::operations::PropertyKeyLocals;

/// Construction proves this ordinal belongs to R, never a Program literal.
/// Cache-only consumers may decline; native-method guards can resolve the key.
#[derive(PartialEq, Eq)]
pub(super) struct CachedRuntimeName(u32);

impl CachedRuntimeName {
    pub(super) fn new(name: &str, strings: &StringPool) -> Option<Self> {
        let index = strings.pooled_string_index(name).ok()?;
        let PooledLiteralDomain::Runtime(index) = strings.pooled_string_domain(index).ok()? else {
            return None;
        };
        Some(Self(index.ordinal()))
    }

    pub(super) fn cached(
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

    /// Resolve only the validated primitive name, without a JavaScript read.
    pub(super) fn resolve(
        &self,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let schema = builder.runtime_schema();
        let index = schema.reserve_i32_local(function);
        index.set_constant(self.0 as i32, function);
        let name = schema.reserve_gc_local(function).initialize(
            schema.resolve_pooled_string_reference(index, builder.runtime_helper_base()?, function),
            function,
        );
        schema.release_i32_local(index, function);
        Ok(name)
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
            ExprIr::Identifier(name) => Receiver::Local(builder.private_local_binding(name)?),
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
                own_data(builder, &global, &property, &receiver, accepted, function)?;
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
        let key = self.key.resolve(builder, function)?;
        let property = PropertyKeyLocals::from_string(schema, &key, function);
        emit_own_native_method_guard(
            builder,
            &receiver,
            &property,
            PureNativeMethod::Imul,
            accepted,
            function,
        )?;
        property.clear(function);
        key.clear(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        receiver.clear(function);
        Ok(())
    }
}

/// The two guarded source operations have dedicated closed region owners.
#[derive(Clone, Copy)]
pub(super) enum PureNativeMethod {
    Imul,
    ArrayJoin,
}

impl PureNativeMethod {
    fn builtin(self) -> StandardBuiltinId {
        match self {
            Self::Imul => StandardBuiltinId::MathImul,
            Self::ArrayJoin => StandardBuiltinId::ArrayPrototypeJoin,
        }
    }
}

/// The caller supplies an ordinary header and an existing primitive key, with
/// accepted initially false. The shared data-read kernel may materialize a
/// deferred native value, but never invokes an accessor or a proxy trap.
pub(super) fn emit_own_native_method_guard(
    builder: &mut FunctionBuilder<'_>,
    target: &ValueLocals,
    key: &PropertyKeyLocals,
    method: PureNativeMethod,
    accepted: I32Local,
    function: &mut Function,
) -> Result<(), EmitError> {
    let schema = builder.runtime_schema();
    let callee = schema.reserve_value_local(function);
    callee.set_undefined(function);
    own_data(builder, target, key, &callee, accepted, function)?;
    accepted.load(function);
    builder.open_frame(ControlFrameKind::If, function);
    emit_identity(builder, &callee, method, accepted, function);
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
    callee.clear(function);
    Ok(())
}

/// Probe only a concrete ordinary header. Absence and accessors decline without
/// consulting prototypes; all supported storage states share the semantic read.
fn own_data(
    builder: &mut FunctionBuilder<'_>,
    target: &ValueLocals,
    key: &PropertyKeyLocals,
    output: &ValueLocals,
    found: I32Local,
    function: &mut Function,
) -> Result<(), EmitError> {
    let schema = builder.runtime_schema();
    found.set_constant(0, function);
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
    builder.emit_ordinary_own_data_value(&object, key, output, found, function)?;
    object.clear(function);
    builder.pop_control(ControlFrameKind::If);
    function.instruction(&Instruction::End);
    Ok(())
}

fn emit_identity(
    builder: &mut FunctionBuilder<'_>,
    callee: &ValueLocals,
    method: PureNativeMethod,
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
    function.instruction(&Instruction::I32Const(method.builtin().encode()));
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
