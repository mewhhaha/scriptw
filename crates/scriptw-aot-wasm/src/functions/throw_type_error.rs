use super::*;
use crate::gc_types::{
    FunctionContext, FunctionContextSchema, FunctionObject, FunctionObjectSchema, GcLocal,
    ValueLocals,
};

impl FunctionBuilder<'_> {
    /// The callable's retained FunctionContext is the only Realm authority.
    pub(crate) fn emit_load_required_function_realm_throw_type_error(
        &mut self,
        function_object: &GcLocal<FunctionObject>,
        result: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CONTEXT)
                .read(function_object, schema, function)
                .reference(),
            function,
        );
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::REALM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ThrowTypeError,
            result,
            function,
        );
        realm.clear(function);
        context.clear(function);
    }
}
