//! Internal data-property values whose native function has not been observed.
//!
//! A recipe is shared by an installation's aliases. It captures its defining
//! Realm, never a public constructor/prototype lookup or the caller's Realm.
//! Canonical-slot intrinsics and constructors remain eager: their existing
//! table is the sole identity authority for that bootstrap graph.

use super::*;
use crate::gc_types::*;
use crate::runtime_helpers::{HelperParameters, RuntimeHelperId};

#[derive(Clone, Copy)]
pub(super) struct DeferredIntrinsicMethod(StandardBuiltinId);

impl DeferredIntrinsicMethod {
    pub(super) fn new(builtin: StandardBuiltinId) -> Option<Self> {
        (crate::module::standard_builtin_function_realm_slot(builtin).is_none()
            && !builtin.constructable()
            && builtin.global_name().is_none())
        .then_some(Self(builtin))
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_install_deferred_intrinsic_method_aliases(
        &mut self,
        target: &ValueLocals,
        keys: &[IntrinsicKey<'_>],
        method: DeferredIntrinsicMethod,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        writable: bool,
        configurable: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        // Planning is checked at recipe creation, rather than failing only
        // when a previously unread property is finally observed.
        if self.functions.get(&method.0.function_id()).is_none() {
            return Err(EmitError::unsupported(
                "deferred intrinsic lacks planned native entry",
            ));
        }
        let s = self.runtime_schema();
        let recipe = s.reserve_gc_local(f).initialize(
            s.struct_type::<DeferredIntrinsicValue>().construct(
                (
                    GcOperand::reference(realm.realm(), s),
                    GcOperand::constant(method.0),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        let absent = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let header = s
            .reserve_gc_local(f)
            .initialize(self.emit_object_header_projection(target, f), f);
        for &key in keys {
            let key = self.intrinsic_key(key, f)?;
            // Aliases share the immutable recipe, but independently mutable
            // descriptor records, just like eager native data properties.
            let copy = s.reserve_gc_local(f).initialize(
                s.struct_type::<PropertyDescriptor>().construct(
                    (
                        GcOperand::descriptor_word(
                            StoredPropertyAttributes::Data {
                                writable,
                                enumerable: false,
                                configurable,
                            }
                            .descriptor_word(),
                        ),
                        GcOperand::subtype_reference(&recipe, s),
                        GcOperand::reference(&absent, s),
                        GcOperand::reference(&absent, s),
                    ),
                    f,
                ),
                f,
            );
            self.emit_ordinary_append_property_entry(&header, &key, &copy, f)?;
            copy.clear(f);
            key.clear(f);
        }
        header.clear(f);
        absent.clear(f);
        recipe.clear(f);
        Ok(())
    }

    /// Ordinary stored values keep their direct read. The cold recipe branch
    /// is the only path that enters the function materialization helper.
    pub(crate) fn emit_property_value_into(
        &self,
        value: &GcLocal<PropertyValue>,
        out: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        value.is_subtype::<StoredValue>(s, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let stored = s
            .reserve_gc_local(f)
            .initialize(value.load(s, f).cast_subtype::<StoredValue>(s, f), f);
        s.struct_type::<StoredValue>().read_into(&stored, out, s, f);
        stored.clear(f);
        f.instruction(&Instruction::Else);
        let stored = s.reserve_gc_local(f).initialize(
            s.helper_reference_on_stack(
                s.call_helper(
                    crate::runtime_helpers::PropertyValueResolveArguments::new(value),
                    self.runtime_helper_base()
                        .expect("property reads require the registered runtime helpers"),
                    f,
                ),
            ),
            f,
        );
        s.struct_type::<StoredValue>().read_into(&stored, out, s, f);
        stored.clear(f);
        f.instruction(&Instruction::End);
    }

    pub(crate) fn compile_property_value_resolve_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::PropertyValueResolve);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::PropertyValueResolveParameters>(&mut f);
        let s = self.runtime_schema();
        // Snapshot readers may call this entry with either variant.
        parameters.value.is_subtype::<StoredValue>(s, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        parameters
            .value
            .load(s, &mut f)
            .cast_subtype::<StoredValue>(s, &mut f);
        f.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let recipe = s.reserve_gc_local(&mut f).initialize(
            parameters
                .value
                .load(s, &mut f)
                .cast_subtype::<DeferredIntrinsicValue>(s, &mut f),
            &mut f,
        );
        let cached = s.reserve_gc_local(&mut f).initialize(
            s.field(DeferredIntrinsicValueSchema::CACHED)
                .read(&recipe, s, &mut f)
                .reference(),
            &mut f,
        );
        cached.load(s, &mut f).is_null(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let realm = s.reserve_gc_local(&mut f).initialize(
            s.field(DeferredIntrinsicValueSchema::REALM)
                .read(&recipe, s, &mut f)
                .reference(),
            &mut f,
        );
        let context = self.emit_realm_function_materialization_context_from_realm(&realm, &mut f);
        let builtin = s.reserve_i32_local(&mut f);
        s.field(DeferredIntrinsicValueSchema::BUILTIN)
            .read(&recipe, s, &mut f)
            .store(builtin, &mut f);
        let value = s.reserve_value_local(&mut f);
        let mut methods: Vec<_> = self
            .functions
            .values()
            .filter_map(|meta| {
                meta.standard_builtin
                    .and_then(DeferredIntrinsicMethod::new)
                    .map(|method| (method, meta.clone()))
            })
            .collect();
        methods.sort_by_key(|(method, _)| method.0);
        self.emit_deferred_intrinsic_dispatch(&methods, builtin, &context, &value, &mut f)?;
        let stored = s.reserve_gc_local(&mut f).initialize(
            s.struct_type::<StoredValue>().from_value(&value, &mut f),
            &mut f,
        );
        // Allocation has no JavaScript calls. Publish only the completed
        // function; all aliases now resolve the same immutable snapshot.
        s.field(DeferredIntrinsicValueSchema::CACHED).write(
            &recipe,
            GcOperand::nullable_reference(&stored, s),
            s,
            &mut f,
        );
        cached.replace(stored.load(s, &mut f).nullable(), &mut f);
        stored.clear(&mut f);
        value.clear(&mut f);
        s.release_i32_local(builtin, &mut f);
        self.release_realm_function_materialization_context(context, &mut f);
        realm.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cached.load(s, &mut f).require_non_null(&mut f);
        cached.clear(&mut f);
        recipe.clear(&mut f);
        parameters.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    // A balanced native-ID decision tree keeps cold first reads logarithmic.
    // The leaf checks exact identity too; invalid/unplanned IDs always trap.
    fn emit_deferred_intrinsic_dispatch(
        &mut self,
        methods: &[(DeferredIntrinsicMethod, WasmFunctionMeta)],
        builtin: I32Local,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match methods {
            [] => {
                f.instruction(&Instruction::Unreachable);
            }
            [(method, meta)] => {
                builtin.load(f);
                f.instruction(&Instruction::I32Const(method.0 as i32));
                f.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let s = self.runtime_schema();
                let callable = s.reserve_gc_local(f).initialize(
                    self.emit_function_value_payload_in_realm(meta, realm, f)?,
                    f,
                );
                out.set_reference(&callable, s, f);
                callable.clear(f);
            }
            _ => {
                let middle = methods.len() / 2;
                builtin.load(f);
                f.instruction(&Instruction::I32Const((methods[middle].0).0 as i32));
                f.instruction(&Instruction::I32LtU);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_deferred_intrinsic_dispatch(&methods[..middle], builtin, realm, out, f)?;
                f.instruction(&Instruction::Else);
                self.emit_deferred_intrinsic_dispatch(&methods[middle..], builtin, realm, out, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        Ok(())
    }
}
