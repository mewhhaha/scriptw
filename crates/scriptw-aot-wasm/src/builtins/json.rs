//! JSON owns UTF-16 grammar, rooted parse/reviver frames and SerializeJSONProperty.
use super::super::*;
use crate::functions::{ArgumentListConstruction, NonArrayRealmIntrinsicSlot};
use crate::gc_types::*;
use crate::heap::DescriptorMask;
use crate::runtime_helpers::{
    HelperParameters, JsonStringifyValueArguments, JsonStringifyValueParameters,
};
use scriptw_ir::NativeErrorKind;
mod grammar;
mod module_value;
mod parse;
mod parse_array;
mod parse_frame_state;
mod quote;
mod reviver;
mod string_output;
mod stringify;
use string_output::StringOutputConstruction;
mod stringify_replacer;
use stringify_replacer::{
    JsonStringifyReplacerFunctionLocals, JsonStringifyReplacerInvocationLocals,
    JsonStringifyReplacerPropertyKeyLocals, JsonStringifyReplacerReceiverLocals,
    JsonStringifyReplacerValueLocals,
};

enum JsonBuiltin {
    Parse,
    Stringify,
    RawJson,
    IsRawJson,
}
macro_rules! json_domain {
    ($name:ident { $($variant:ident = $word:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum $name { $($variant),+ }
        impl $name {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub(crate) const fn wire_code(self) -> i32 { match self { $(Self::$variant => $word),+ } }
        }
        // The emitted exhaustive state dispatch must select exactly one arm.
        const _: () = {
            let mut index=0;
            while index < $name::ALL.len() {
                assert!($name::ALL[index].wire_code() == index as i32);
                index += 1;
            }
        };
    };
}
json_domain!(JsonParseFrameState {
    ArrayFirstOrEnd=0, ArrayValue=1, ArrayCommaOrEnd=2,
    ObjectFirstKeyOrEnd=3, ObjectKey=4, ObjectColon=5, ObjectValue=6, ObjectCommaOrEnd=7,
});
json_domain!(JsonReviverFrameState { Enter=0, ArrayChildren=1, ObjectChildren=2, Apply=3 });
json_domain!(JsonReviverPropertyRole { Nested=0, Root=1 });
fn json_i32(local: I32Local, value: i32, f: &mut Function) {
    f.instruction(&Instruction::I32Const(value));
    local.store(f);
}
fn json_i64(local: I64Local, value: i64, f: &mut Function) {
    f.instruction(&Instruction::I64Const(value));
    local.store(f);
}
fn json_increment(local: I64Local, f: &mut Function) {
    local.load(f);
    f.instruction(&Instruction::I64Const(1));
    f.instruction(&Instruction::I64Add);
    local.store(f);
}

impl FunctionBuilder<'_> {
    fn emit_json_builtin(
        &mut self,
        operation: JsonBuiltin,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match operation {
            JsonBuiltin::Parse => self.emit_json_parse_entry(f),
            JsonBuiltin::Stringify => self.emit_json_stringify_entry(f),
            JsonBuiltin::RawJson => self.emit_json_raw_entry(f),
            JsonBuiltin::IsRawJson => {
                let s = self.runtime_schema();
                let value = s.reserve_value_local(f);
                let yes = s.reserve_i32_local(f);
                self.emit_builtin_arg_to_value(0, &value, f);
                self.emit_json_reference_test::<RawJsonObject>(&value, f);
                yes.store(f);
                self.completion().initialize(f);
                self.completion().value().set_boolean(yes, f);
                s.release_i32_local(yes, f);
                value.clear(f);
                Ok(())
            }
        }
    }
    pub(super) fn emit_json_parse_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_json_builtin(JsonBuiltin::Parse, f)
    }
    pub(super) fn emit_json_stringify_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_json_builtin(JsonBuiltin::Stringify, f)
    }
    pub(super) fn emit_json_raw_json_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        self.emit_json_builtin(JsonBuiltin::RawJson, f)
    }
    pub(super) fn emit_json_is_raw_json_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_json_builtin(JsonBuiltin::IsRawJson, f)
    }
    fn emit_json_reference_test<T: GcHeapType>(&self, value: &ValueLocals, f: &mut Function) {
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            self.runtime_schema()
                .reference_type::<T>(GcNullability::NonNullable)
                .heap_type,
        ));
    }
    fn emit_json_tag_test(&self, value: &ValueLocals, tag: WasmRuntimeValueTag, f: &mut Function) {
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(tag as i32));
        f.instruction(&Instruction::I32Eq);
    }
    fn emit_json_string(
        &mut self,
        text: &str,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        Ok(s.reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(text, f)?, f))
    }
    fn emit_json_error(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let slot = match kind {
            NativeErrorKind::SyntaxError => NonArrayRealmIntrinsicSlot::SyntaxErrorPrototype,
            NativeErrorKind::TypeError => NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            _ => {
                return Err(EmitError::unsupported(
                    "JSON error must be SyntaxError or TypeError",
                ));
            }
        };
        self.emit_load_non_array_realm_intrinsic(realm, slot, &prototype, f);
        self.emit_throw_runtime_error_with_prototype(kind, message, &prototype, &pending, f)?;
        self.completion().copy_from(&pending, f);
        pending.clear(f);
        prototype.clear(f);
        self.emit_propagate_current_throw_if_needed(f);
        Ok(())
    }
    fn emit_json_get(
        &mut self,
        holder: &ValueLocals,
        key: &GcLocal<StringValue>,
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key_value = s.reserve_value_local(f);
        key_value.set_reference(key, s, f);
        let property = self.emit_value_to_property_key_locals(&key_value, f)?;
        let result = s.reserve_completion(f);
        self.emit_object_read(holder, holder, &property, &result, f)?;
        self.completion().copy_from(&result, f);
        self.emit_propagate_current_throw_if_needed(f);
        out.copy_from(result.value(), f);
        result.clear(f);
        property.clear(f);
        key_value.clear(f);
        Ok(())
    }
    fn emit_json_define(
        &mut self,
        holder: &ValueLocals,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        silent: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key_value = s.reserve_value_local(f);
        key_value.set_reference(key, s, f);
        let property = self.emit_value_to_property_key_locals(&key_value, f)?;
        let result = s.reserve_completion(f);
        if silent {
            self.emit_object_create_data_property_silent(holder, &property, value, &result, f)?;
        } else {
            self.emit_create_data_property_or_throw(holder, &property, value, &result, f)?;
        }
        self.completion().copy_from(&result, f);
        self.emit_propagate_current_throw_if_needed(f);
        result.clear(f);
        property.clear(f);
        key_value.clear(f);
        Ok(())
    }
    fn emit_json_index_key(
        &mut self,
        index: I64Local,
        f: &mut Function,
    ) -> Result<GcLocal<StringValue>, EmitError> {
        let s = self.runtime_schema();
        let bits = s.reserve_i64_local(f);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        bits.store(f);
        let key = s
            .reserve_gc_local(f)
            .initialize(self.emit_number_to_string_payload(bits, f)?, f);
        s.release_i64_local(bits, f);
        Ok(key)
    }
    fn emit_json_plain_object(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            f,
        );
        let object = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        prototype.clear(f);
        Ok(object)
    }
    /// Enumerable own String keys are snapshotted before any descendant callbacks.
    fn emit_json_enumerable_keys(
        &mut self,
        holder: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let s = self.runtime_schema();
        let keys = self.emit_object_own_property_keys(holder, f)?;
        let list = ArgumentListConstruction::new(self, f);
        let count = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let flags = s.reserve_i64_local(f);
        keys.length(count, s, f);
        json_i32(index, 0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let key = keys.read_key(index, self, f)?;
        self.emit_json_tag_test(key.value(), WasmRuntimeValueTag::String, f);
        self.open_frame(ControlFrameKind::If, f);
        let descriptor = self.emit_direct_own_descriptor_fact(holder, &key, f)?;
        descriptor.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(descriptor.load(s, f).require_non_null(f), f);
        s.struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&present, s, f)
            .store_i64(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I64Const(DescriptorMask::ENUMERABLE.as_i64()));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        list.append(key.value(), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        present.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.clear(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(flags, f);
        s.release_i32_local(index, f);
        s.release_i32_local(count, f);
        keys.clear(f);
        Ok(list.finish(self, f))
    }
    fn emit_json_call(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        args: &[&ValueLocals],
        out: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let argv = self.emit_pre_evaluated_arg_vector(args, f);
        let pending = s.reserve_completion(f);
        self.emit_function_or_proxy_call_with_argv(callee, receiver, &argv, &pending, f)?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw_if_needed(f);
        out.copy_from(pending.value(), f);
        pending.clear(f);
        argv.clear(f);
        Ok(())
    }
}
