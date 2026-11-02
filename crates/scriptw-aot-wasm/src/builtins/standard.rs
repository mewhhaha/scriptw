use super::super::*;
use super::binary_data::{
    ArrayBufferAccessor, BufferConstructorKind, BufferSliceKind, BufferTransferKind,
    DataViewAccessor, SharedArrayBufferAccessor, TypedArrayAccessorKind, TypedArrayCreateAccess,
};
use super::data_view_access::{DataViewAccess, DataViewElement};
use super::date::{DateComponentGetter, DateComponentSetter, DateLocaleFormat, DateTimeBasis};
use super::intl_datetimeformat::IntlDateTimeFormatPurpose;
use super::intl_numberformat::NfFormatMode;
use super::iterators::IteratorPrototypeWeirdSetter;
use super::string::{StringCaseOperation, StringHtmlOperation};
use super::temporal::{TemporalZonedDateTimePlainTarget, ZonedDateTimeField};
use super::temporal_duration::TemporalDurationField;
use super::temporal_duration_methods::TemporalDurationStringMode;
use super::temporal_instant::{InstantArithmetic, InstantDifference};
use super::temporal_options::{TemporalTimeUnit, TemporalUnit};
use super::temporal_plain_date::TemporalPlainDateField;
use super::temporal_plain_date_methods::TemporalPlainDateStringMode;
use super::temporal_plain_date_time::TemporalPlainDateTimeField;
use super::temporal_plain_date_time_methods::{
    TemporalPlainArithmeticOperation, TemporalPlainDateTimeComponent,
    TemporalPlainDateTimeStringMode, TemporalPlainDifferenceOperation,
};
use super::temporal_plain_month_day::TemporalPlainMonthDayField;
use super::temporal_plain_month_day::TemporalPlainMonthDayStringMode;
use super::temporal_plain_time_methods::TemporalPlainTimeStringMode;
use super::temporal_plain_year_month::TemporalPlainYearMonthField;
use super::temporal_plain_year_month_methods::TemporalPlainYearMonthStringMode;
use super::weak::WeakBuiltin;
use crate::gc_types::*;

mod array_iterator_creation;
use array_iterator_creation::ArrayIteratorReceiverPolicy;
mod array_mutation;
use array_mutation::ArrayMutationKind;
mod async_generator;
mod eval;
mod generator;
mod iterator_constructor;
mod iterator_helpers;
use iterator_helpers::{
    BufferedIteratorKind, IteratorHelperOperation, IteratorTerminalKind, IteratorZipInput,
    LazyIteratorKind,
};
mod typed_array_create_same_type;
mod typed_array_methods;
use typed_array_methods::TypedArrayNativeMethod;

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn compile_standard_builtin(
        &mut self,
        builtin: StandardBuiltinId,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match builtin {
            StandardBuiltinId::ShadowRealmConstructor => {
                self.compile_shadow_realm_constructor_builtin(function)?;
            }
            StandardBuiltinId::ShadowRealmPrototypeEvaluate => {
                self.compile_shadow_realm_evaluate_builtin(function)?;
            }
            StandardBuiltinId::ShadowRealmPrototypeImportValue => {
                self.compile_shadow_realm_import_value_builtin(function)?;
            }
            StandardBuiltinId::ShadowRealmWrappedFunctionCall => {
                self.compile_shadow_realm_wrapped_function_call_builtin(function)?;
            }
            StandardBuiltinId::ShadowRealmImportFulfilled => {
                self.compile_shadow_realm_import_fulfilled_builtin(function)?;
            }
            StandardBuiltinId::ShadowRealmImportRejected => {
                self.compile_shadow_realm_import_rejected_builtin(function)?;
            }
            StandardBuiltinId::MapConstructor => {
                self.emit_map_constructor(function)?;
            }
            StandardBuiltinId::MapGroupBy => {
                self.emit_map_group_by(function)?;
            }
            StandardBuiltinId::ObjectGroupBy => self.emit_object_group_by(function)?,
            StandardBuiltinId::ObjectFromEntries => self.emit_object_from_entries(function)?,
            StandardBuiltinId::MapPrototypeClear => {
                self.emit_map_prototype_clear(function)?;
            }
            StandardBuiltinId::MapPrototypeDelete => {
                self.emit_map_prototype_delete(function)?;
            }
            StandardBuiltinId::MapPrototypeForEach => {
                self.emit_map_prototype_for_each(function)?;
            }
            StandardBuiltinId::MapPrototypeKeys => {
                self.emit_map_prototype_keys(function)?;
            }
            StandardBuiltinId::MapPrototypeValues => {
                self.emit_map_prototype_values(function)?;
            }
            StandardBuiltinId::MapPrototypeEntries => {
                self.emit_map_prototype_entries(function)?;
            }
            StandardBuiltinId::MapIteratorNext => {
                self.emit_map_iterator_next(function)?;
            }
            StandardBuiltinId::MapPrototypeGet => {
                self.emit_map_prototype_get(function)?;
            }
            StandardBuiltinId::MapPrototypeGetOrInsert => {
                self.emit_map_prototype_get_or_insert(function)?;
            }
            StandardBuiltinId::MapPrototypeGetOrInsertComputed => {
                self.emit_map_prototype_get_or_insert_computed(function)?;
            }
            StandardBuiltinId::MapPrototypeHas => {
                self.emit_map_prototype_has(function)?;
            }
            StandardBuiltinId::MapPrototypeSet => {
                self.emit_map_prototype_set(function)?;
            }
            StandardBuiltinId::MapPrototypeSizeGetter => {
                self.emit_map_prototype_size_getter(function)?;
            }
            StandardBuiltinId::WeakMapConstructor => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapConstructor, function)?;
            }
            StandardBuiltinId::WeakSetConstructor => {
                self.emit_weak_builtin(WeakBuiltin::WeakSetConstructor, function)?;
            }
            StandardBuiltinId::WeakSetPrototypeAdd => {
                self.emit_weak_builtin(WeakBuiltin::WeakSetPrototypeAdd, function)?;
            }
            StandardBuiltinId::WeakSetPrototypeDelete => {
                self.emit_weak_builtin(WeakBuiltin::WeakSetPrototypeDelete, function)?;
            }
            StandardBuiltinId::WeakSetPrototypeHas => {
                self.emit_weak_builtin(WeakBuiltin::WeakSetPrototypeHas, function)?;
            }
            StandardBuiltinId::WeakRefConstructor => {
                self.emit_weak_builtin(WeakBuiltin::WeakRefConstructor, function)?;
            }
            StandardBuiltinId::WeakRefPrototypeDeref => {
                self.emit_weak_builtin(WeakBuiltin::WeakRefPrototypeDeref, function)?;
            }
            StandardBuiltinId::FinalizationRegistryConstructor => {
                self.emit_weak_builtin(WeakBuiltin::FinalizationRegistryConstructor, function)?;
            }
            StandardBuiltinId::FinalizationRegistryPrototypeRegister => {
                self.emit_weak_builtin(
                    WeakBuiltin::FinalizationRegistryPrototypeRegister,
                    function,
                )?;
            }
            StandardBuiltinId::FinalizationRegistryPrototypeUnregister => {
                self.emit_weak_builtin(
                    WeakBuiltin::FinalizationRegistryPrototypeUnregister,
                    function,
                )?;
            }
            StandardBuiltinId::AsyncDisposableStackConstructor => {
                self.emit_async_disposable_stack_constructor(function)?;
            }
            StandardBuiltinId::DisposableStackConstructor => {
                self.emit_disposable_stack_constructor(function)?;
            }
            StandardBuiltinId::DisposableStackPrototypeUse => {
                self.emit_disposable_stack_use(function)?;
            }
            StandardBuiltinId::DisposableStackPrototypeAdopt => {
                self.emit_disposable_stack_adopt(function)?;
            }
            StandardBuiltinId::DisposableStackPrototypeDefer => {
                self.emit_disposable_stack_defer(function)?;
            }
            StandardBuiltinId::DisposableStackPrototypeMove => {
                self.emit_disposable_stack_move(function)?;
            }
            StandardBuiltinId::DisposableStackPrototypeDispose => {
                self.emit_disposable_stack_dispose(function)?;
            }
            StandardBuiltinId::DisposableStackPrototypeDisposedGetter => {
                self.emit_disposable_stack_disposed_getter(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeUse => {
                self.emit_async_disposable_stack_use(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeAdopt => {
                self.emit_async_disposable_stack_adopt(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeDefer => {
                self.emit_async_disposable_stack_defer(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeMove => {
                self.emit_async_disposable_stack_move(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync => {
                self.emit_async_disposable_stack_dispose_async(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackPrototypeDisposedGetter => {
                self.emit_async_disposable_stack_disposed_getter(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled => {
                self.emit_async_disposable_stack_dispose_async_fulfilled(function)?;
            }
            StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected => {
                self.emit_async_disposable_stack_dispose_async_rejected(function)?;
            }
            StandardBuiltinId::WeakMapPrototypeDelete => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapPrototypeDelete, function)?;
            }
            StandardBuiltinId::WeakMapPrototypeGet => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapPrototypeGet, function)?;
            }
            StandardBuiltinId::WeakMapPrototypeGetOrInsert => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapPrototypeGetOrInsert, function)?;
            }
            StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapPrototypeGetOrInsertComputed, function)?;
            }
            StandardBuiltinId::WeakMapPrototypeHas => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapPrototypeHas, function)?;
            }
            StandardBuiltinId::WeakMapPrototypeSet => {
                self.emit_weak_builtin(WeakBuiltin::WeakMapPrototypeSet, function)?;
            }
            StandardBuiltinId::SetConstructor => {
                self.emit_set_constructor(function)?;
            }
            StandardBuiltinId::SetPrototypeAdd => {
                self.emit_set_prototype_add(function)?;
            }
            StandardBuiltinId::SetPrototypeClear => {
                self.emit_set_prototype_clear(function)?;
            }
            StandardBuiltinId::SetPrototypeDelete => {
                self.emit_set_prototype_delete(function)?;
            }
            StandardBuiltinId::SetPrototypeDifference => {
                self.emit_set_prototype_difference(function)?;
            }
            StandardBuiltinId::SetPrototypeForEach => {
                self.emit_set_prototype_for_each(function)?;
            }
            StandardBuiltinId::SetPrototypeIntersection => {
                self.emit_set_prototype_intersection(function)?;
            }
            StandardBuiltinId::SetPrototypeIsDisjointFrom => {
                self.emit_set_prototype_is_disjoint_from(function)?;
            }
            StandardBuiltinId::SetPrototypeIsSubsetOf => {
                self.emit_set_prototype_is_subset_of(function)?;
            }
            StandardBuiltinId::SetPrototypeIsSupersetOf => {
                self.emit_set_prototype_is_superset_of(function)?;
            }
            StandardBuiltinId::SetPrototypeSymmetricDifference => {
                self.emit_set_prototype_symmetric_difference(function)?;
            }
            StandardBuiltinId::SetPrototypeUnion => {
                self.emit_set_prototype_union(function)?;
            }
            StandardBuiltinId::SetPrototypeValues => {
                self.emit_set_prototype_values(function)?;
            }
            StandardBuiltinId::SetPrototypeEntries => {
                self.emit_set_prototype_entries(function)?;
            }
            StandardBuiltinId::SetIteratorNext => {
                self.emit_set_iterator_next(function)?;
            }
            StandardBuiltinId::SetPrototypeHas => {
                self.emit_set_prototype_has(function)?;
            }
            StandardBuiltinId::SetPrototypeSizeGetter => {
                self.emit_set_prototype_size_getter(function)?;
            }
            StandardBuiltinId::ArrayFromAsync => {
                self.emit_array_from_async(function)?;
            }
            StandardBuiltinId::ArrayFromAsyncFulfilled => {
                self.emit_array_from_async_fulfilled(function)?;
            }
            StandardBuiltinId::ArrayFromAsyncRejected => {
                self.emit_array_from_async_rejected(function)?;
            }
            StandardBuiltinId::PromiseConstructor => {
                self.emit_promise_constructor(function)?;
            }
            StandardBuiltinId::PromisePrototypeThen => {
                self.emit_promise_prototype_then(function)?;
            }
            StandardBuiltinId::PromisePrototypeCatch => {
                self.emit_promise_prototype_catch(function)?;
            }
            StandardBuiltinId::PromisePrototypeFinally => {
                self.emit_promise_prototype_finally(function)?;
            }
            StandardBuiltinId::PromiseThenFinally => {
                self.emit_promise_then_finally(function)?;
            }
            StandardBuiltinId::PromiseCatchFinally => {
                self.emit_promise_catch_finally(function)?;
            }
            StandardBuiltinId::PromiseValueThunk => {
                self.emit_promise_value_thunk(function)?;
            }
            StandardBuiltinId::PromiseThrower => {
                self.emit_promise_thrower(function)?;
            }
            StandardBuiltinId::PromiseResolve => {
                self.emit_promise_static_settle(PromiseSettlement::Fulfill, function)?;
            }
            StandardBuiltinId::PromiseWithResolvers => {
                self.emit_promise_with_resolvers(function)?;
            }
            StandardBuiltinId::PromiseTry => {
                self.emit_promise_try(function)?;
            }
            StandardBuiltinId::PromiseReject => {
                self.emit_promise_static_settle(PromiseSettlement::Reject, function)?;
            }
            StandardBuiltinId::PromiseAll => {
                self.emit_promise_all(function)?;
            }
            StandardBuiltinId::PromiseAllSettled => {
                self.emit_promise_all_settled(function)?;
            }
            StandardBuiltinId::PromiseAllKeyed => {
                self.emit_promise_all_keyed(function)?;
            }
            StandardBuiltinId::PromiseAllSettledKeyed => {
                self.emit_promise_all_settled_keyed(function)?;
            }
            StandardBuiltinId::PromiseAny => {
                self.emit_promise_any(function)?;
            }
            StandardBuiltinId::PromiseRace => {
                self.emit_promise_race(function)?;
            }
            StandardBuiltinId::PromiseAllResolveElement => {
                self.emit_promise_all_resolve_element(function)?;
            }
            StandardBuiltinId::PromiseAllSettledResolveElement => {
                self.emit_promise_all_settled_element(PromiseSettlement::Fulfill, function)?;
            }
            StandardBuiltinId::PromiseAllSettledRejectElement => {
                self.emit_promise_all_settled_element(PromiseSettlement::Reject, function)?;
            }
            StandardBuiltinId::PromiseAnyRejectElement => {
                self.emit_promise_any_reject_element(function)?;
            }
            StandardBuiltinId::PromiseAllKeyedResolveElement => {
                self.emit_promise_all_keyed_resolve_element(function)?;
            }
            StandardBuiltinId::PromiseAllSettledKeyedResolveElement => {
                self.emit_promise_all_settled_keyed_element(PromiseSettlement::Fulfill, function)?;
            }
            StandardBuiltinId::PromiseAllSettledKeyedRejectElement => {
                self.emit_promise_all_settled_keyed_element(PromiseSettlement::Reject, function)?;
            }
            StandardBuiltinId::PromiseCapabilityExecutor => {
                self.emit_promise_capability_executor(function)?;
            }
            StandardBuiltinId::PromiseResolveFunction => {
                self.emit_promise_resolving_function(PromiseSettlement::Fulfill, function)?;
            }
            StandardBuiltinId::PromiseRejectFunction => {
                self.emit_promise_resolving_function(PromiseSettlement::Reject, function)?;
            }
            StandardBuiltinId::EvalFunction => {
                self.emit_eval_function_builtin(function)?;
            }
            StandardBuiltinId::ThrowTypeError
            | StandardBuiltinId::AbstractModuleSourceConstructor => {
                let schema = self.runtime_schema();
                let output = schema.reserve_completion(function);
                self.emit_throw_current_function_realm_type_error_without_message(
                    &output, function,
                )?;
                self.completion().copy_from(&output, function);
                output.clear(function);
            }
            StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter => {
                self.emit_module_source_to_string_tag_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayConstructor => {
                self.emit_typed_array_abstract_constructor_builtin(function)?;
            }
            StandardBuiltinId::IteratorConstructor => {
                self.emit_iterator_constructor_builtin(function)?;
            }
            StandardBuiltinId::FunctionConstructor => {
                self.emit_function_constructor_builtin(function)?
            }
            StandardBuiltinId::FunctionPrototype => {
                self.emit_function_prototype_builtin(function)?
            }
            StandardBuiltinId::FunctionPrototypeSymbolHasInstance => {
                self.emit_function_prototype_symbol_has_instance_builtin(function)?
            }
            StandardBuiltinId::FunctionPrototypeCall => {
                self.emit_function_prototype_call_builtin(function)?
            }
            StandardBuiltinId::FunctionPrototypeApply => {
                self.emit_function_prototype_apply_builtin(function)?
            }
            StandardBuiltinId::FunctionPrototypeBind => {
                self.emit_function_prototype_bind_builtin(function)?
            }
            StandardBuiltinId::ObjectConstructor => {
                self.compile_object_constructor_builtin(function)?
            }
            StandardBuiltinId::ObjectCreate => self.compile_object_create_builtin(function)?,
            StandardBuiltinId::ObjectGetPrototypeOf => {
                self.compile_object_get_prototype_of_builtin(function)?
            }
            StandardBuiltinId::ObjectSetPrototypeOf => {
                self.compile_object_set_prototype_of_builtin(function)?
            }
            StandardBuiltinId::ObjectDefineProperty => {
                self.compile_object_define_property_builtin(function)?
            }
            StandardBuiltinId::ObjectDefineProperties => {
                self.compile_object_define_properties_builtin(function)?
            }
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor => {
                self.compile_object_get_own_property_descriptor_builtin(function)?
            }
            StandardBuiltinId::ObjectGetOwnPropertyNames => {
                self.compile_object_get_own_property_names_builtin(function)?
            }
            StandardBuiltinId::ObjectGetOwnPropertySymbols => {
                self.compile_object_get_own_property_symbols_builtin(function)?
            }
            StandardBuiltinId::ObjectKeys => self.compile_object_keys_builtin(function)?,
            StandardBuiltinId::ObjectAssign => self.compile_object_assign_builtin(function)?,
            StandardBuiltinId::ObjectGetOwnPropertyDescriptors => {
                self.compile_object_get_own_property_descriptors_builtin(function)?
            }
            StandardBuiltinId::ObjectEntries => self.compile_object_entries_builtin(function)?,
            StandardBuiltinId::ObjectValues => self.compile_object_values_builtin(function)?,
            StandardBuiltinId::ObjectHasOwn => self.compile_object_has_own_builtin(function)?,
            StandardBuiltinId::ObjectIs => self.compile_object_is_builtin(function)?,
            StandardBuiltinId::ObjectIsSealed => self.compile_object_is_sealed_builtin(function)?,
            StandardBuiltinId::ObjectIsFrozen => self.compile_object_is_frozen_builtin(function)?,
            StandardBuiltinId::ObjectIsExtensible => {
                self.compile_object_is_extensible_builtin(function)?
            }
            StandardBuiltinId::ObjectSeal => self.compile_object_seal_builtin(function)?,
            StandardBuiltinId::ObjectFreeze => self.compile_object_freeze_builtin(function)?,
            StandardBuiltinId::ObjectPreventExtensions => {
                self.compile_object_prevent_extensions_builtin(function)?
            }
            StandardBuiltinId::ProxyConstructor => {
                self.compile_proxy_constructor_builtin(function)?
            }
            StandardBuiltinId::ProxyRevocable => self.compile_proxy_revocable_builtin(function)?,
            StandardBuiltinId::ProxyRevoke => self.compile_proxy_revoke_builtin(function)?,
            StandardBuiltinId::ObjectPrototypeProtoGetter => {
                self.compile_object_prototype_proto_getter_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeProtoSetter => {
                self.compile_object_prototype_proto_setter_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeHasOwnProperty => {
                self.compile_object_prototype_has_own_property_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeDefineGetter => {
                self.compile_object_prototype_define_getter_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeDefineSetter => {
                self.compile_object_prototype_define_setter_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeLookupGetter => {
                self.compile_object_prototype_lookup_getter_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeLookupSetter => {
                self.compile_object_prototype_lookup_setter_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypePropertyIsEnumerable => {
                self.compile_object_prototype_property_is_enumerable_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeIsPrototypeOf => {
                self.compile_object_prototype_is_prototype_of_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeToString => {
                self.compile_object_prototype_to_string_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeToLocaleString => {
                self.compile_object_prototype_to_locale_string_builtin(function)?
            }
            StandardBuiltinId::ObjectPrototypeValueOf => {
                self.compile_object_prototype_value_of_builtin(function)?
            }
            StandardBuiltinId::ReflectConstruct => {
                self.compile_reflect_construct_builtin(function)?
            }
            StandardBuiltinId::ReflectApply => self.compile_reflect_apply_builtin(function)?,
            StandardBuiltinId::ReflectGet => self.compile_reflect_get_builtin(function)?,
            StandardBuiltinId::ReflectGetPrototypeOf => {
                self.compile_reflect_get_prototype_of_builtin(function)?
            }
            StandardBuiltinId::ReflectGetOwnPropertyDescriptor => {
                self.compile_reflect_get_own_property_descriptor_builtin(function)?
            }
            StandardBuiltinId::ReflectSet => self.compile_reflect_set_builtin(function)?,
            StandardBuiltinId::ReflectHas => self.compile_reflect_has_builtin(function)?,
            StandardBuiltinId::ReflectDefineProperty => {
                self.compile_reflect_define_property_builtin(function)?
            }
            StandardBuiltinId::ReflectDeleteProperty => {
                self.compile_reflect_delete_property_builtin(function)?
            }
            StandardBuiltinId::ReflectIsExtensible => {
                self.compile_reflect_is_extensible_builtin(function)?
            }
            StandardBuiltinId::ReflectPreventExtensions => {
                self.compile_reflect_prevent_extensions_builtin(function)?
            }
            StandardBuiltinId::ReflectSetPrototypeOf => {
                self.compile_reflect_set_prototype_of_builtin(function)?
            }
            StandardBuiltinId::ReflectOwnKeys => self.compile_reflect_own_keys_builtin(function)?,
            StandardBuiltinId::ArrayConstructor => {
                self.emit_array_constructor_builtin(function)?;
            }
            StandardBuiltinId::ArrayOf => {
                self.emit_array_of_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayOf => {
                self.emit_typed_array_of_builtin(function)?;
            }
            StandardBuiltinId::ArrayFrom => {
                self.emit_array_from_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayFrom => {
                self.emit_typed_array_from_builtin(function)?;
            }
            StandardBuiltinId::ArrayIsArray => {
                self.emit_array_is_array_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeConcat => {
                self.compile_array_prototype_concat_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeJoin => {
                self.compile_array_prototype_join_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeSlice => {
                self.compile_array_prototype_slice_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeSplice => {
                self.compile_array_prototype_splice_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeFill => {
                self.compile_typed_array_prototype_fill_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFill => {
                self.compile_array_prototype_fill_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeSort => {
                self.compile_array_prototype_sort_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeToSorted => {
                self.compile_array_prototype_to_sorted_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeToLocaleString => {
                self.compile_array_prototype_to_locale_string_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeToLocaleString => {
                self.compile_typed_array_prototype_to_locale_string_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeToString => {
                self.compile_typed_array_prototype_to_string_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeJoin => {
                self.compile_typed_array_prototype_join_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeSlice => {
                self.compile_typed_array_prototype_slice_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeSet => {
                self.emit_typed_array_set_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeReverse => {
                self.emit_typed_array_native_method(TypedArrayNativeMethod::Reverse, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeCopyWithin => {
                self.emit_typed_array_native_method(TypedArrayNativeMethod::CopyWithin, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeSort => {
                self.emit_typed_array_native_method(TypedArrayNativeMethod::Sort, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeToReversed => {
                self.emit_typed_array_native_method(TypedArrayNativeMethod::ToReversed, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeToSorted => {
                self.emit_typed_array_native_method(TypedArrayNativeMethod::ToSorted, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeWith => {
                self.emit_typed_array_native_method(TypedArrayNativeMethod::With, function)?;
            }
            StandardBuiltinId::ArrayPrototypeFlat => {
                self.compile_array_prototype_flat_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFlatMap => {
                self.compile_array_prototype_flat_map_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeAt => {
                self.compile_array_prototype_at_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeAt => {
                self.compile_typed_array_prototype_at_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeToReversed => {
                self.compile_array_prototype_to_reversed_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeToSpliced => {
                self.compile_array_prototype_to_spliced_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeWith => {
                self.compile_array_prototype_with_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeReverse => {
                self.compile_array_prototype_reverse_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeCopyWithin => {
                self.compile_array_prototype_copy_within_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeIncludes => {
                self.compile_array_prototype_includes_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeIndexOf => {
                self.compile_array_prototype_index_of_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeLastIndexOf => {
                self.compile_array_prototype_last_index_of_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeIncludes => {
                self.compile_typed_array_prototype_includes_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeIndexOf => {
                self.compile_typed_array_prototype_index_of_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeLastIndexOf => {
                self.compile_typed_array_prototype_last_index_of_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFind => {
                self.compile_array_prototype_find_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFindIndex => {
                self.compile_array_prototype_find_index_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeFind => {
                self.compile_typed_array_prototype_find_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeFindIndex => {
                self.compile_typed_array_prototype_find_index_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeFindLast => {
                self.compile_typed_array_prototype_find_last_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeFindLastIndex => {
                self.compile_typed_array_prototype_find_last_index_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFindLast => {
                self.compile_array_prototype_find_last_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFindLastIndex => {
                self.compile_array_prototype_find_last_index_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeMap => {
                self.compile_array_prototype_map_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeReduce => {
                self.compile_array_reduce_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeReduceRight => {
                self.compile_array_reduce_right_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeReduce => {
                self.compile_typed_array_reduce_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeReduceRight => {
                self.compile_typed_array_reduce_right_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeEvery => {
                self.compile_array_prototype_every_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeEvery => {
                self.compile_typed_array_prototype_every_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeSome => {
                self.compile_array_prototype_some_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeSome => {
                self.compile_typed_array_prototype_some_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeMap => {
                self.compile_typed_array_prototype_map_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeFilter => {
                self.compile_typed_array_prototype_filter_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeForEach => {
                self.compile_array_prototype_for_each_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeForEach => {
                self.compile_typed_array_prototype_for_each_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypeFilter => {
                self.compile_array_prototype_filter_builtin(function)?;
            }
            StandardBuiltinId::ArrayPrototypePop => {
                self.emit_native_array_mutation(ArrayMutationKind::Pop, function)?;
            }
            StandardBuiltinId::ArrayPrototypePush => {
                self.emit_native_array_mutation(ArrayMutationKind::Push, function)?;
            }
            StandardBuiltinId::ArrayPrototypeShift => {
                self.emit_native_array_mutation(ArrayMutationKind::Shift, function)?;
            }
            StandardBuiltinId::ArrayPrototypeUnshift => {
                self.emit_native_array_mutation(ArrayMutationKind::Unshift, function)?;
            }
            StandardBuiltinId::ArrayPrototypeKeys => {
                self.compile_array_iterator_method_builtin(
                    ArrayIterationKind::Key,
                    ArrayIteratorReceiverPolicy::GenericArrayLike,
                    function,
                )?;
            }
            StandardBuiltinId::ArrayPrototypeEntries => {
                self.compile_array_iterator_method_builtin(
                    ArrayIterationKind::KeyAndValue,
                    ArrayIteratorReceiverPolicy::GenericArrayLike,
                    function,
                )?;
            }
            StandardBuiltinId::ArrayPrototypeValues => {
                self.compile_array_iterator_method_builtin(
                    ArrayIterationKind::Value,
                    ArrayIteratorReceiverPolicy::GenericArrayLike,
                    function,
                )?;
            }
            StandardBuiltinId::TypedArrayPrototypeKeys => {
                self.compile_array_iterator_method_builtin(
                    ArrayIterationKind::Key,
                    ArrayIteratorReceiverPolicy::TypedArray,
                    function,
                )?;
            }
            StandardBuiltinId::TypedArrayPrototypeEntries => {
                self.compile_array_iterator_method_builtin(
                    ArrayIterationKind::KeyAndValue,
                    ArrayIteratorReceiverPolicy::TypedArray,
                    function,
                )?;
            }
            StandardBuiltinId::TypedArrayPrototypeValues => {
                self.compile_array_iterator_method_builtin(
                    ArrayIterationKind::Value,
                    ArrayIteratorReceiverPolicy::TypedArray,
                    function,
                )?;
            }
            StandardBuiltinId::ArrayIteratorIdentity | StandardBuiltinId::AsyncIteratorIdentity => {
                let receiver = self.runtime_schema().reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::IteratorFrom => {
                self.emit_iterator_from_builtin(function)?;
            }
            StandardBuiltinId::IteratorConcat => {
                self.emit_native_iterator_concat_create(function)?;
            }
            StandardBuiltinId::IteratorZipKeyed => {
                self.emit_native_iterator_zip_create(IteratorZipInput::Keyed, function)?;
            }
            StandardBuiltinId::IteratorZip => {
                self.emit_native_iterator_zip_create(IteratorZipInput::Iterable, function)?;
            }
            StandardBuiltinId::IteratorConcatNext => {
                self.emit_native_iterator_concat_resume(IteratorHelperOperation::Next, function)?;
            }
            StandardBuiltinId::IteratorZipNext => {
                self.emit_native_iterator_zip_resume(IteratorHelperOperation::Next, function)?;
            }
            StandardBuiltinId::IteratorConcatReturn => {
                self.emit_native_iterator_concat_resume(IteratorHelperOperation::Return, function)?;
            }
            StandardBuiltinId::IteratorZipReturn => {
                self.emit_native_iterator_zip_resume(IteratorHelperOperation::Return, function)?;
            }
            StandardBuiltinId::IteratorHelperNext => {
                self.emit_native_iterator_helper_dispatch(IteratorHelperOperation::Next, function)?;
            }
            StandardBuiltinId::IteratorHelperReturn => {
                self.emit_native_iterator_helper_dispatch(
                    IteratorHelperOperation::Return,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeToArray => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::ToArray, function)?;
            }
            StandardBuiltinId::IteratorPrototypeIncludes => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::Includes, function)?;
            }
            StandardBuiltinId::IteratorPrototypeJoin => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::Join, function)?;
            }
            StandardBuiltinId::IteratorPrototypeSymbolDispose => {
                self.emit_iterator_prototype_symbol_dispose(function)?;
            }
            StandardBuiltinId::IteratorPrototypeToStringTagGetter => {
                let schema = self.runtime_schema();
                let text = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference("Iterator", function)?,
                    function,
                );
                let value = schema.reserve_value_local(function);
                value.set_reference(&text, schema, function);
                self.completion().set_normal(&value, function);
                value.clear(function);
                text.clear(function);
            }
            StandardBuiltinId::IteratorPrototypeToStringTagSetter => {
                self.emit_iterator_prototype_weird_setter(
                    IteratorPrototypeWeirdSetter::ToStringTag,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeForEach => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::ForEach, function)?;
            }
            StandardBuiltinId::IteratorPrototypeEvery => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::Every, function)?;
            }
            StandardBuiltinId::IteratorPrototypeSome => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::Some, function)?;
            }
            StandardBuiltinId::IteratorPrototypeFind => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::Find, function)?;
            }
            StandardBuiltinId::IteratorPrototypeReduce => {
                self.emit_native_iterator_terminal(IteratorTerminalKind::Reduce, function)?;
            }
            StandardBuiltinId::IteratorPrototypeMap => {
                self.emit_lazy_iterator_create(LazyIteratorKind::Map, function)?;
            }
            StandardBuiltinId::IteratorMapNext => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Map,
                    IteratorHelperOperation::Next,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorMapReturn => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Map,
                    IteratorHelperOperation::Return,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeFilter => {
                self.emit_lazy_iterator_create(LazyIteratorKind::Filter, function)?;
            }
            StandardBuiltinId::IteratorFilterNext => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Filter,
                    IteratorHelperOperation::Next,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorFilterReturn => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Filter,
                    IteratorHelperOperation::Return,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeFlatMap => {
                self.emit_lazy_iterator_create(LazyIteratorKind::FlatMap, function)?;
            }
            StandardBuiltinId::IteratorFlatMapNext => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::FlatMap,
                    IteratorHelperOperation::Next,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorFlatMapReturn => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::FlatMap,
                    IteratorHelperOperation::Return,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeTake => {
                self.emit_lazy_iterator_create(LazyIteratorKind::Take, function)?;
            }
            StandardBuiltinId::IteratorTakeNext => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Take,
                    IteratorHelperOperation::Next,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorTakeReturn => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Take,
                    IteratorHelperOperation::Return,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeChunks => {
                self.emit_buffered_iterator_create(BufferedIteratorKind::Chunks, function)?
            }
            StandardBuiltinId::IteratorPrototypeWindows => {
                self.emit_buffered_iterator_create(BufferedIteratorKind::Windows, function)?
            }
            StandardBuiltinId::IteratorBufferedNext => {
                self.emit_buffered_iterator_resume(IteratorHelperOperation::Next, function)?
            }
            StandardBuiltinId::IteratorBufferedReturn => {
                self.emit_buffered_iterator_resume(IteratorHelperOperation::Return, function)?
            }
            StandardBuiltinId::IteratorPrototypeDrop => {
                self.emit_lazy_iterator_create(LazyIteratorKind::Drop, function)?;
            }
            StandardBuiltinId::IteratorDropNext => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Drop,
                    IteratorHelperOperation::Next,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorDropReturn => {
                self.emit_lazy_iterator_resume(
                    LazyIteratorKind::Drop,
                    IteratorHelperOperation::Return,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorPrototypeConstructorGetter => {
                self.emit_iterator_constructor_getter_builtin(function)?;
            }
            StandardBuiltinId::IteratorPrototypeConstructorSetter => {
                self.emit_iterator_prototype_weird_setter(
                    IteratorPrototypeWeirdSetter::Constructor,
                    function,
                )?;
            }
            StandardBuiltinId::IteratorFromWrapperReturn => {
                self.emit_iterator_from_wrapper_return_builtin(function)?;
            }
            StandardBuiltinId::IteratorFromWrapperNext => {
                self.emit_iterator_from_wrapper_next_builtin(function)?;
            }
            StandardBuiltinId::StringIteratorNext => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                let output = schema.reserve_completion(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.emit_string_iterator_next_from_locals(&receiver, &output, function)?;
                self.completion().copy_from(&output, function);
                output.clear(function);
                receiver.clear(function);
            }
            StandardBuiltinId::RegExpStringIteratorNext => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                let output = schema.reserve_completion(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.emit_regexp_string_iterator_next_from_locals(&receiver, &output, function)?;
                self.completion().copy_from(&output, function);
                output.clear(function);
                receiver.clear(function);
            }
            StandardBuiltinId::GeneratorPrototypeNext => {
                self.emit_generator_method_builtin(GeneratorResumeKind::Normal, function)?;
            }
            StandardBuiltinId::GeneratorPrototypeReturn => {
                self.emit_generator_method_builtin(GeneratorResumeKind::Return, function)?;
            }
            StandardBuiltinId::GeneratorPrototypeThrow => {
                self.emit_generator_method_builtin(GeneratorResumeKind::Throw, function)?;
            }
            StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose => {
                self.emit_async_iterator_prototype_async_dispose(function)?;
            }
            StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled => {
                self.emit_async_iterator_prototype_async_dispose_fulfilled(function)?;
            }
            StandardBuiltinId::AsyncGeneratorPrototypeNext => {
                self.emit_async_generator_method_builtin(
                    AsyncGeneratorRequestCompletionKind::Normal,
                    function,
                )?;
            }
            StandardBuiltinId::AsyncGeneratorPrototypeReturn => {
                self.emit_async_generator_method_builtin(
                    AsyncGeneratorRequestCompletionKind::Return,
                    function,
                )?;
            }
            StandardBuiltinId::AsyncGeneratorPrototypeThrow => {
                self.emit_async_generator_method_builtin(
                    AsyncGeneratorRequestCompletionKind::Throw,
                    function,
                )?;
            }
            StandardBuiltinId::ArrayIteratorNext => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                let output = schema.reserve_completion(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.emit_array_iterator_next_from_locals(&receiver, &output, function)?;
                self.completion().copy_from(&output, function);
                output.clear(function);
                receiver.clear(function);
            }
            StandardBuiltinId::ArrayBufferIsView => {
                self.emit_array_buffer_is_view_builtin(function)?;
            }
            StandardBuiltinId::ArrayBufferConstructor => {
                self.emit_array_buffer_constructor_builtin(
                    BufferConstructorKind::ArrayBuffer,
                    function,
                )?;
            }
            StandardBuiltinId::SharedArrayBufferConstructor => {
                self.emit_array_buffer_constructor_builtin(
                    BufferConstructorKind::SharedArrayBuffer,
                    function,
                )?;
            }
            StandardBuiltinId::ArraySpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::TypedArraySpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::ArrayBufferSpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::RegExpSpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::PromiseSpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::MapSpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::SetSpeciesGetter => {
                let schema = self.runtime_schema();
                let receiver = schema.reserve_value_local(function);
                self.compile_this_to_locals(&receiver, function)?;
                self.completion().set_normal(&receiver, function);
                receiver.clear(function);
            }
            StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter => {
                self.emit_array_buffer_accessor_builtin(ArrayBufferAccessor::ByteLength, function)?;
            }
            StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter => {
                self.emit_shared_array_buffer_accessor_builtin(
                    SharedArrayBufferAccessor::ByteLength,
                    function,
                )?;
            }
            StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter => {
                self.emit_shared_array_buffer_accessor_builtin(
                    SharedArrayBufferAccessor::MaxByteLength,
                    function,
                )?;
            }
            StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter => {
                self.emit_shared_array_buffer_accessor_builtin(
                    SharedArrayBufferAccessor::Growable,
                    function,
                )?;
            }
            StandardBuiltinId::SharedArrayBufferPrototypeGrow => {
                self.emit_shared_array_buffer_grow_builtin(function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeDetachedGetter => {
                self.emit_array_buffer_accessor_builtin(ArrayBufferAccessor::Detached, function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter => {
                self.emit_array_buffer_accessor_builtin(
                    ArrayBufferAccessor::MaxByteLength,
                    function,
                )?;
            }
            StandardBuiltinId::ArrayBufferPrototypeImmutableGetter => {
                self.emit_array_buffer_accessor_builtin(ArrayBufferAccessor::Immutable, function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeResizableGetter => {
                self.emit_array_buffer_accessor_builtin(ArrayBufferAccessor::Resizable, function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeResize => {
                self.emit_array_buffer_resize_builtin(function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeSlice => {
                self.emit_array_buffer_slice_builtin(BufferSliceKind::ArrayBuffer, function)?;
            }
            StandardBuiltinId::SharedArrayBufferPrototypeSlice => {
                self.emit_array_buffer_slice_builtin(BufferSliceKind::SharedArrayBuffer, function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable => {
                self.emit_array_buffer_slice_builtin(BufferSliceKind::Immutable, function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeTransfer => {
                self.emit_array_buffer_transfer_builtin(
                    BufferTransferKind::PreserveResizable,
                    function,
                )?;
            }
            StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength => {
                self.emit_array_buffer_transfer_builtin(BufferTransferKind::FixedLength, function)?;
            }
            StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable => {
                self.emit_array_buffer_transfer_builtin(BufferTransferKind::Immutable, function)?;
            }
            StandardBuiltinId::DataViewPrototypeBufferGetter => {
                self.emit_data_view_accessor_builtin(DataViewAccessor::Buffer, function)?;
            }
            StandardBuiltinId::DataViewPrototypeByteLengthGetter => {
                self.emit_data_view_accessor_builtin(DataViewAccessor::ByteLength, function)?;
            }
            StandardBuiltinId::DataViewPrototypeByteOffsetGetter => {
                self.emit_data_view_accessor_builtin(DataViewAccessor::ByteOffset, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeToStringTagGetter => {
                self.emit_typed_array_tag_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeBufferGetter => {
                self.emit_typed_array_buffer_builtin(function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeByteLengthGetter => {
                self.emit_typed_array_accessor_builtin(
                    TypedArrayAccessorKind::ByteLength,
                    function,
                )?;
            }
            StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter => {
                self.emit_typed_array_accessor_builtin(
                    TypedArrayAccessorKind::ByteOffset,
                    function,
                )?;
            }
            StandardBuiltinId::TypedArrayPrototypeLengthGetter => {
                self.emit_typed_array_accessor_builtin(TypedArrayAccessorKind::Length, function)?;
            }
            StandardBuiltinId::TypedArrayPrototypeSubarray => {
                self.emit_typed_array_subarray_builtin(function)?;
            }
            StandardBuiltinId::DateNow => {
                self.emit_date_now(function)?;
            }
            StandardBuiltinId::DateParse => self.emit_date_parse_builtin(function)?,
            StandardBuiltinId::DateUtc => {
                self.emit_date_utc(function)?;
            }
            StandardBuiltinId::TemporalNowTimeZoneId => {
                self.emit_temporal_now_time_zone_id(function)?;
            }
            StandardBuiltinId::TemporalNowInstant => {
                self.emit_temporal_now_instant(function)?;
            }
            StandardBuiltinId::TemporalNowZonedDateTimeIso => {
                self.emit_temporal_now_zoned_date_time_iso(function)?;
            }
            StandardBuiltinId::TemporalNowPlainDateTimeIso => {
                self.emit_temporal_now_plain_date_time_iso(function)?;
            }
            StandardBuiltinId::TemporalNowPlainDateIso => {
                self.emit_temporal_now_plain_date_iso(function)?;
            }
            StandardBuiltinId::TemporalNowPlainTimeIso => {
                self.emit_temporal_now_plain_time_iso(function)?;
            }
            StandardBuiltinId::TemporalInstantConstructor => {
                self.emit_temporal_instant_constructor(function)?;
            }
            StandardBuiltinId::TemporalInstantFrom => {
                self.emit_temporal_instant_from(function)?;
            }
            StandardBuiltinId::TemporalInstantCompare => {
                self.emit_temporal_instant_compare(function)?;
            }
            StandardBuiltinId::TemporalInstantFromEpochMilliseconds => {
                self.emit_temporal_instant_from_epoch_milliseconds(function)?;
            }
            StandardBuiltinId::TemporalInstantFromEpochNanoseconds => {
                self.emit_temporal_instant_from_epoch_nanoseconds(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter => {
                self.emit_temporal_instant_epoch_milliseconds(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter => {
                self.emit_temporal_instant_epoch_nanoseconds(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeAdd => {
                self.emit_temporal_instant_add_or_subtract(InstantArithmetic::Add, function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeSubtract => {
                self.emit_temporal_instant_add_or_subtract(InstantArithmetic::Subtract, function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeRound => {
                self.emit_temporal_instant_round(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeUntil => {
                self.emit_temporal_instant_until_or_since(InstantDifference::Until, function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeSince => {
                self.emit_temporal_instant_until_or_since(InstantDifference::Since, function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeEquals => {
                self.emit_temporal_instant_equals(function)?;
            }
            // `toJSON` is `TemporalInstantToString(instant, AUTO)` — the same
            // core with `undefined` options, but a distinct function object:
            // `toJSON/prop-desc.js` and `toJSON/name.js` observe that it is
            // not `toString`. The `toJSON` entry never reads its argument, so
            // `toJSON/basic.js`'s throwing Proxy options bag ("should not get
            // properties off argument") passes without any extra guard.
            StandardBuiltinId::TemporalInstantPrototypeToString => {
                self.emit_temporal_instant_to_string(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeToJson => {
                self.emit_temporal_instant_to_json(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeToLocaleString => {
                self.emit_temporal_instant_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeValueOf => {
                self.emit_temporal_instant_value_of(function)?;
            }
            StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso => {
                self.emit_temporal_instant_to_zoned_date_time_iso(function)?;
            }
            StandardBuiltinId::TemporalPlainDateConstructor => {
                self.emit_temporal_plain_date_constructor(function)?;
            }
            StandardBuiltinId::TemporalPlainDateFrom => {
                self.emit_temporal_plain_date_from(function)?;
            }
            StandardBuiltinId::TemporalPlainDateCompare => {
                self.emit_temporal_plain_date_compare(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::CalendarId, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeEraGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::Era, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::EraYear, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeYearGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::Year, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::Month, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::MonthCode, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeDayGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::Day, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::DayOfWeek, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::DayOfYear, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::WeekOfYear, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::YearOfWeek, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::DaysInWeek, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::DaysInMonth, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::DaysInYear, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter => {
                self.emit_temporal_plain_date_field(
                    TemporalPlainDateField::MonthsInYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter => {
                self.emit_temporal_plain_date_field(TemporalPlainDateField::InLeapYear, function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeWith => {
                self.emit_temporal_plain_date_with(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeAdd => {
                self.emit_temporal_plain_date_add_or_subtract(
                    TemporalPlainArithmeticOperation::Add,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeSubtract => {
                self.emit_temporal_plain_date_add_or_subtract(
                    TemporalPlainArithmeticOperation::Subtract,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeUntil => {
                self.emit_temporal_plain_date_until_or_since(
                    TemporalPlainDifferenceOperation::Until,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeSince => {
                self.emit_temporal_plain_date_until_or_since(
                    TemporalPlainDifferenceOperation::Since,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime => {
                self.emit_temporal_plain_date_to_plain_date_time(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth => {
                self.emit_temporal_plain_date_to_plain_year_month(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay => {
                self.emit_temporal_plain_date_to_plain_month_day(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar => {
                self.emit_temporal_plain_date_with_calendar(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeEquals => {
                self.emit_temporal_plain_date_equals(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToString => {
                self.emit_temporal_plain_date_to_string(
                    TemporalPlainDateStringMode::ToString,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToJson => {
                self.emit_temporal_plain_date_to_string(
                    TemporalPlainDateStringMode::ToJson,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString => {
                self.emit_temporal_plain_date_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeValueOf => {
                self.emit_temporal_plain_date_value_of(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthConstructor => {
                self.emit_temporal_plain_year_month_constructor(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthFrom => {
                self.emit_temporal_plain_year_month_from(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthCompare => {
                self.emit_temporal_plain_year_month_compare(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::CalendarId,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::Era,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::EraYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::Year,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::Month,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::MonthCode,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::DaysInYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::DaysInMonth,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::MonthsInYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter => {
                self.emit_temporal_plain_year_month_field(
                    TemporalPlainYearMonthField::InLeapYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeWith => {
                self.emit_temporal_plain_year_month_with(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd => {
                self.emit_temporal_plain_year_month_add_or_subtract(
                    TemporalPlainArithmeticOperation::Add,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract => {
                self.emit_temporal_plain_year_month_add_or_subtract(
                    TemporalPlainArithmeticOperation::Subtract,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil => {
                self.emit_temporal_plain_year_month_until_or_since(
                    TemporalPlainDifferenceOperation::Until,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeSince => {
                self.emit_temporal_plain_year_month_until_or_since(
                    TemporalPlainDifferenceOperation::Since,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals => {
                self.emit_temporal_plain_year_month_equals(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeToString => {
                self.emit_temporal_plain_year_month_to_string(
                    TemporalPlainYearMonthStringMode::ToString,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson => {
                self.emit_temporal_plain_year_month_to_string(
                    TemporalPlainYearMonthStringMode::ToJson,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString => {
                self.emit_temporal_plain_year_month_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate => {
                self.emit_temporal_plain_year_month_to_plain_date(function)?;
            }
            StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf => {
                self.emit_temporal_partial_date_value_of(RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_DOES_NOT_SUPPORT_IMPLICIT_CONVERSION_USE_COMPARE_OR_EQUALS, function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayConstructor => {
                self.emit_temporal_plain_month_day_constructor(function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayFrom => {
                self.emit_temporal_plain_month_day_from(function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter => {
                self.emit_temporal_plain_month_day_field(
                    TemporalPlainMonthDayField::CalendarId,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter => {
                self.emit_temporal_plain_month_day_field(
                    TemporalPlainMonthDayField::MonthCode,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter => {
                self.emit_temporal_plain_month_day_field(
                    TemporalPlainMonthDayField::Day,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeWith => {
                self.emit_temporal_plain_month_day_with(function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals => {
                self.emit_temporal_plain_month_day_equals(function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeToString => {
                self.emit_temporal_plain_month_day_to_string(
                    TemporalPlainMonthDayStringMode::ToString,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson => {
                self.emit_temporal_plain_month_day_to_string(
                    TemporalPlainMonthDayStringMode::ToJson,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString => {
                self.emit_temporal_plain_month_day_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate => {
                self.emit_temporal_plain_month_day_to_plain_date(function)?;
            }
            StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf => {
                self.emit_temporal_partial_date_value_of(RuntimeErrorMessage::TEMPORAL_PLAINMONTHDAY_DOES_NOT_SUPPORT_IMPLICIT_CONVERSION_USE_COMPARE_OR_EQUALS, function)?;
            }
            StandardBuiltinId::TemporalDurationConstructor => {
                self.emit_temporal_duration_constructor(function)?;
            }
            StandardBuiltinId::TemporalDurationFrom => {
                self.emit_temporal_duration_from(function)?;
            }
            StandardBuiltinId::TemporalDurationCompare => {
                self.emit_temporal_duration_compare(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeYearsGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Year),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeMonthsGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Month),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeWeeksGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Week),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeDaysGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Day),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeHoursGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Hour),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeMinutesGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Minute),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeSecondsGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Second),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Millisecond),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Microsecond),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter => {
                self.emit_temporal_duration_field(
                    TemporalDurationField::Unit(TemporalUnit::Nanosecond),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeSignGetter => {
                self.emit_temporal_duration_field(TemporalDurationField::Sign, function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeBlankGetter => {
                self.emit_temporal_duration_field(TemporalDurationField::Blank, function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeWith => {
                self.emit_temporal_duration_with(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeNegated => {
                self.emit_temporal_duration_negated(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeAbs => {
                self.emit_temporal_duration_abs(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeAdd => {
                self.emit_temporal_duration_add(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeSubtract => {
                self.emit_temporal_duration_subtract(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeRound => {
                self.emit_temporal_duration_round(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeTotal => {
                self.emit_temporal_duration_total(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeToString => {
                self.emit_temporal_duration_to_string(
                    TemporalDurationStringMode::ToString,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeToJson => {
                self.emit_temporal_duration_to_string(
                    TemporalDurationStringMode::ToJson,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalDurationPrototypeToLocaleString => {
                self.emit_temporal_duration_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalDurationPrototypeValueOf => {
                self.emit_temporal_duration_value_of(function)?;
            }
            StandardBuiltinId::TemporalPlainTimeConstructor => {
                self.emit_temporal_plain_time_constructor(function)?;
            }
            StandardBuiltinId::TemporalPlainTimeFrom => {
                self.emit_temporal_plain_time_from(function)?;
            }
            StandardBuiltinId::TemporalPlainTimeCompare => {
                self.emit_temporal_plain_time_compare(function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeHourGetter => {
                self.emit_temporal_plain_time_field(TemporalTimeUnit::Hour, function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter => {
                self.emit_temporal_plain_time_field(TemporalTimeUnit::Minute, function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter => {
                self.emit_temporal_plain_time_field(TemporalTimeUnit::Second, function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter => {
                self.emit_temporal_plain_time_field(TemporalTimeUnit::Millisecond, function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter => {
                self.emit_temporal_plain_time_field(TemporalTimeUnit::Microsecond, function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter => {
                self.emit_temporal_plain_time_field(TemporalTimeUnit::Nanosecond, function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeWith => {
                self.emit_temporal_plain_time_with(function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeAdd => {
                self.emit_temporal_plain_time_add_or_subtract(
                    TemporalPlainArithmeticOperation::Add,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeSubtract => {
                self.emit_temporal_plain_time_add_or_subtract(
                    TemporalPlainArithmeticOperation::Subtract,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeUntil => {
                self.emit_temporal_plain_time_until_or_since(
                    TemporalPlainDifferenceOperation::Until,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeSince => {
                self.emit_temporal_plain_time_until_or_since(
                    TemporalPlainDifferenceOperation::Since,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeRound => {
                self.emit_temporal_plain_time_round(function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeEquals => {
                self.emit_temporal_plain_time_equals(function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeToString => {
                self.emit_temporal_plain_time_to_string(
                    TemporalPlainTimeStringMode::ToString,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeToJson => {
                self.emit_temporal_plain_time_to_string(
                    TemporalPlainTimeStringMode::ToJson,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString => {
                self.emit_temporal_plain_time_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalPlainTimePrototypeValueOf => {
                self.emit_temporal_plain_time_value_of(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimeConstructor => {
                self.emit_temporal_plain_date_time_constructor(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimeFrom => {
                self.emit_temporal_plain_date_time_from(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimeCompare => {
                self.emit_temporal_plain_date_time_compare(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::CalendarId),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::Era),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::EraYear),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::Year),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::Month),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::MonthCode),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::Day),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Time(TemporalTimeUnit::Hour),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Time(TemporalTimeUnit::Minute),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Time(TemporalTimeUnit::Second),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Time(TemporalTimeUnit::Millisecond),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Time(TemporalTimeUnit::Microsecond),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Time(TemporalTimeUnit::Nanosecond),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::DayOfWeek),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::DayOfYear),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::WeekOfYear),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::YearOfWeek),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::DaysInWeek),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::DaysInMonth),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::DaysInYear),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::MonthsInYear),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter => {
                self.emit_temporal_plain_date_time_field(
                    TemporalPlainDateTimeField::Date(TemporalPlainDateField::InLeapYear),
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeWith => {
                self.emit_temporal_plain_date_time_with(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime => {
                self.emit_temporal_plain_date_time_with_plain_time(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar => {
                self.emit_temporal_plain_date_time_with_calendar(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeAdd => {
                self.emit_temporal_plain_date_time_add_or_subtract(
                    TemporalPlainArithmeticOperation::Add,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract => {
                self.emit_temporal_plain_date_time_add_or_subtract(
                    TemporalPlainArithmeticOperation::Subtract,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeUntil => {
                self.emit_temporal_plain_date_time_until_or_since(
                    TemporalPlainDifferenceOperation::Until,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeSince => {
                self.emit_temporal_plain_date_time_until_or_since(
                    TemporalPlainDifferenceOperation::Since,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeRound => {
                self.emit_temporal_plain_date_time_round(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeEquals => {
                self.emit_temporal_plain_date_time_equals(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeToString => {
                self.emit_temporal_plain_date_time_to_string(
                    TemporalPlainDateTimeStringMode::ToString,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeToJson => {
                self.emit_temporal_plain_date_time_to_string(
                    TemporalPlainDateTimeStringMode::ToJson,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString => {
                self.emit_temporal_plain_date_time_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf => {
                self.emit_temporal_plain_date_time_value_of(function)?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate => {
                self.emit_temporal_plain_date_time_to_component(
                    TemporalPlainDateTimeComponent::PlainDate,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime => {
                self.emit_temporal_plain_date_time_to_component(
                    TemporalPlainDateTimeComponent::PlainTime,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime => {
                self.emit_temporal_plain_date_time_to_zoned_date_time(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimeConstructor => {
                self.emit_temporal_zoned_date_time_constructor(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimeFrom => {
                self.emit_temporal_zoned_date_time_from(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimeCompare => {
                self.emit_temporal_zoned_date_time_compare(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeWith => {
                self.emit_temporal_zoned_date_time_with(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime => {
                self.emit_temporal_zoned_date_time_with_plain_time(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeRound => {
                self.emit_temporal_zoned_date_time_round(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition => {
                self.emit_temporal_zoned_date_time_get_time_zone_transition(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter => {
                self.emit_temporal_zoned_date_time_hours_in_day(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay => {
                self.emit_temporal_zoned_date_time_start_of_day(function)?;
            }
            StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime => {
                self.emit_temporal_plain_date_to_zoned_date_time(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToString => {
                self.emit_temporal_zoned_date_time_to_string(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToJson => {
                self.emit_temporal_zoned_date_time_to_json(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf => {
                self.emit_temporal_zoned_date_time_value_of(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString => {
                self.emit_temporal_zoned_date_time_to_locale_string(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime => {
                self.emit_temporal_zoned_date_time_to_plain(
                    TemporalZonedDateTimePlainTarget::Time,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::DayOfWeek,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::DayOfYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::WeekOfYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::YearOfWeek,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::DaysInWeek,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::DaysInMonth,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::DaysInYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::MonthsInYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::InLeapYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter => {
                self.emit_temporal_zoned_date_time_epoch_milliseconds(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter => {
                self.emit_temporal_zoned_date_time_epoch_nanoseconds(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter => {
                self.emit_temporal_zoned_date_time_offset(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter => {
                self.emit_temporal_zoned_date_time_offset_nanoseconds(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter => {
                self.emit_temporal_zoned_date_time_time_zone_id(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter => {
                self.emit_temporal_zoned_date_time_calendar_id(function)?;
            }
            // One arm per accessor rather than an or-pattern that forwards
            // `builtin`: this is the only `StandardBuiltinId ->
            // ZonedDateTimeField` conversion, and spelling it here means the
            // emitter never has to match a several-hundred-variant enum with a
            // catch-all. Adding a ZonedDateTime accessor is a compile error in
            // this match until it names its field.
            StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Era, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::EraYear,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Year, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Month, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::MonthCode,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Day, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Hour, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Minute, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter => {
                self.emit_temporal_zoned_date_time_iso_field(ZonedDateTimeField::Second, function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::Millisecond,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::Microsecond,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter => {
                self.emit_temporal_zoned_date_time_iso_field(
                    ZonedDateTimeField::Nanosecond,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeEquals => {
                self.emit_temporal_zoned_date_time_equals(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant => {
                self.emit_temporal_zoned_date_time_to_instant(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate => {
                self.emit_temporal_zoned_date_time_to_plain(
                    TemporalZonedDateTimePlainTarget::Date,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime => {
                self.emit_temporal_zoned_date_time_to_plain(
                    TemporalZonedDateTimePlainTarget::DateTime,
                    function,
                )?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone => {
                self.emit_temporal_zoned_date_time_with_time_zone(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar => {
                self.emit_temporal_zoned_date_time_with_calendar(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeAdd => {
                self.emit_temporal_zoned_date_time_add_builtin(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract => {
                self.emit_temporal_zoned_date_time_subtract_builtin(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeUntil => {
                self.emit_temporal_zoned_date_time_until_builtin(function)?;
            }
            StandardBuiltinId::TemporalZonedDateTimePrototypeSince => {
                self.emit_temporal_zoned_date_time_since_builtin(function)?;
            }
            StandardBuiltinId::IntlSupportedValuesOf => {
                self.emit_intl_supported_values_of(function)?;
            }
            StandardBuiltinId::IntlGetCanonicalLocales => {
                self.emit_intl_get_canonical_locales(function)?;
            }
            StandardBuiltinId::IntlLocaleConstructor => {
                self.emit_intl_locale_constructor(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeLanguageGetter => {
                self.emit_intl_locale_language_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeScriptGetter => {
                self.emit_intl_locale_script_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeRegionGetter => {
                self.emit_intl_locale_region_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeBaseNameGetter => {
                self.emit_intl_locale_base_name_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeCalendarGetter => {
                self.emit_intl_locale_calendar_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeCollationGetter => {
                self.emit_intl_locale_collation_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter => {
                self.emit_intl_locale_first_day_of_week_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeHourCycleGetter => {
                self.emit_intl_locale_hour_cycle_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter => {
                self.emit_intl_locale_case_first_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeNumericGetter => {
                self.emit_intl_locale_numeric_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter => {
                self.emit_intl_locale_numbering_system_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeVariantsGetter => {
                self.emit_intl_locale_variants_getter_builtin(function)?;
            }
            StandardBuiltinId::IntlPluralRulesConstructor => {
                self.emit_intl_plural_rules_constructor(function)?;
            }
            StandardBuiltinId::IntlPluralRulesSupportedLocalesOf => {
                self.emit_intl_plural_rules_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions => {
                self.emit_intl_plural_rules_resolved_options(function)?;
            }
            StandardBuiltinId::IntlPluralRulesPrototypeSelect => {
                self.emit_intl_plural_rules_select(function)?;
            }
            StandardBuiltinId::IntlPluralRulesPrototypeSelectRange => {
                self.emit_intl_plural_rules_select_range(function)?;
            }
            StandardBuiltinId::IntlDisplayNamesConstructor => {
                self.emit_intl_display_names_constructor(function)?;
            }
            StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf => {
                self.emit_intl_display_names_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions => {
                self.emit_intl_display_names_resolved_options(function)?;
            }
            StandardBuiltinId::IntlDisplayNamesPrototypeOf => {
                self.emit_intl_display_names_of(function)?;
            }
            StandardBuiltinId::IntlRelativeTimeFormatConstructor => {
                self.emit_intl_relative_time_constructor(function)?;
            }
            StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf => {
                self.emit_intl_relative_time_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions => {
                self.emit_intl_relative_time_resolved_options(function)?;
            }
            StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat => {
                self.emit_intl_relative_time_format(
                    function,
                    super::intl_relativetime::RelativeTimeFormatOutput::String,
                )?;
            }
            StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts => {
                self.emit_intl_relative_time_format(
                    function,
                    super::intl_relativetime::RelativeTimeFormatOutput::Parts,
                )?;
            }
            StandardBuiltinId::IntlDurationFormatConstructor => {
                self.emit_intl_durationformat_constructor(function)?;
            }
            StandardBuiltinId::IntlDurationFormatSupportedLocalesOf => {
                self.emit_intl_durationformat_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions => {
                self.emit_intl_durationformat_resolved_options(function)?;
            }
            StandardBuiltinId::IntlDurationFormatPrototypeFormat => {
                self.emit_intl_durationformat_format(function)?;
            }
            StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts => {
                self.emit_intl_durationformat_format_to_parts(function)?;
            }
            StandardBuiltinId::IntlSegmenterConstructor => {
                self.emit_intl_segmenter_constructor(function)?;
            }
            StandardBuiltinId::IntlSegmenterSupportedLocalesOf => {
                self.emit_intl_segmenter_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlSegmenterPrototypeSegment => {
                self.emit_intl_segmenter_segment(function)?;
            }
            StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions => {
                self.emit_intl_segmenter_resolved_options(function)?;
            }
            StandardBuiltinId::IntlSegmentsPrototypeContaining => {
                self.emit_intl_segments_containing(function)?;
            }
            StandardBuiltinId::IntlSegmentsPrototypeIterator => {
                self.emit_intl_segments_iterator(function)?;
            }
            StandardBuiltinId::IntlSegmentIteratorPrototypeNext => {
                self.emit_intl_segment_iterator_next(function)?;
            }
            StandardBuiltinId::IntlCollatorConstructor => {
                self.emit_intl_collator_constructor(function)?;
            }
            StandardBuiltinId::IntlCollatorSupportedLocalesOf => {
                self.emit_intl_collator_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlCollatorPrototypeResolvedOptions => {
                self.emit_intl_collator_resolved_options(function)?;
            }
            StandardBuiltinId::IntlCollatorPrototypeCompareGetter => {
                self.emit_intl_collator_compare_getter(function)?;
            }
            StandardBuiltinId::IntlCollatorBoundCompare => {
                self.emit_intl_collator_bound_compare(function)?;
            }
            StandardBuiltinId::IntlListFormatConstructor => {
                self.emit_intl_list_format_constructor(function)?;
            }
            StandardBuiltinId::IntlListFormatSupportedLocalesOf => {
                self.emit_intl_list_format_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlListFormatPrototypeResolvedOptions => {
                self.emit_intl_list_format_resolved_options(function)?;
            }
            StandardBuiltinId::IntlListFormatPrototypeFormat => {
                self.emit_intl_list_format(
                    function,
                    crate::builtins::intl_listformat::ListFormatOutput::String,
                )?;
            }
            StandardBuiltinId::IntlListFormatPrototypeFormatToParts => {
                self.emit_intl_list_format(
                    function,
                    crate::builtins::intl_listformat::ListFormatOutput::Parts,
                )?;
            }
            StandardBuiltinId::IntlNumberFormatConstructor => {
                self.emit_intl_number_format_constructor(function)?;
            }
            StandardBuiltinId::IntlNumberFormatSupportedLocalesOf => {
                self.emit_intl_number_format_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions => {
                self.emit_intl_number_format_resolved_options(function)?;
            }
            StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter => {
                self.emit_intl_number_format_getter(function)?;
            }
            StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts => {
                self.emit_intl_number_format_to_parts(function)?;
            }
            StandardBuiltinId::IntlNumberFormatPrototypeFormatRange => {
                self.emit_intl_number_format_range(NfFormatMode::String, function)?;
            }
            StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts => {
                self.emit_intl_number_format_range(NfFormatMode::Parts, function)?;
            }
            StandardBuiltinId::IntlNumberFormatBoundFormat => {
                self.emit_intl_number_format_bound_format(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatConstructor => {
                self.emit_intl_create_date_time_format(
                    IntlDateTimeFormatPurpose::Constructor,
                    function,
                )?;
            }
            StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf => {
                self.emit_intl_date_time_format_supported_locales_of(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions => {
                self.emit_intl_date_time_format_resolved_options(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter => {
                self.emit_intl_date_time_format_format_getter(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts => {
                self.emit_intl_date_time_format_format_to_parts(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange => {
                self.emit_intl_date_time_format_format_range(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts => {
                self.emit_intl_date_time_format_format_range_to_parts(function)?;
            }
            StandardBuiltinId::IntlDateTimeFormatBoundFormat => {
                self.emit_intl_date_time_format_bound_format(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeMaximize => {
                self.emit_intl_locale_likely_subtags_builtin::<scriptw_intl::MaximizeLocale>(
                    function,
                )?;
            }
            StandardBuiltinId::IntlLocalePrototypeMinimize => {
                self.emit_intl_locale_likely_subtags_builtin::<scriptw_intl::MinimizeLocale>(
                    function,
                )?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetCalendars => {
                self.emit_intl_locale_get_calendars(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetCollations => {
                self.emit_intl_locale_get_collations(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetTimeZones => {
                self.emit_intl_locale_get_time_zones(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems => {
                self.emit_intl_locale_get_numbering_systems(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetHourCycles => {
                self.emit_intl_locale_get_hour_cycles(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetTextInfo => {
                self.emit_intl_locale_get_text_info(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeGetWeekInfo => {
                self.emit_intl_locale_get_week_info(function)?;
            }
            StandardBuiltinId::IntlLocalePrototypeToString => {
                self.emit_intl_locale_to_string_builtin(function)?;
            }
            StandardBuiltinId::DateConstructor => {
                self.emit_date_constructor(function)?;
            }
            StandardBuiltinId::DatePrototypeGetTime | StandardBuiltinId::DatePrototypeValueOf => {
                self.emit_date_value_of_builtin(function)?
            }
            StandardBuiltinId::DatePrototypeSetTime => {
                self.emit_date_set_time(function)?;
            }
            StandardBuiltinId::DatePrototypeGetFullYear => {
                self.emit_date_component_getter(
                    DateComponentGetter::FullYear(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcFullYear => {
                self.emit_date_component_getter(
                    DateComponentGetter::FullYear(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetMonth => {
                self.emit_date_component_getter(
                    DateComponentGetter::Month(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcMonth => {
                self.emit_date_component_getter(
                    DateComponentGetter::Month(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetDate => {
                self.emit_date_component_getter(
                    DateComponentGetter::Date(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcDate => {
                self.emit_date_component_getter(
                    DateComponentGetter::Date(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetDay => {
                self.emit_date_component_getter(
                    DateComponentGetter::Day(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcDay => {
                self.emit_date_component_getter(
                    DateComponentGetter::Day(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetHours => {
                self.emit_date_component_getter(
                    DateComponentGetter::Hours(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcHours => {
                self.emit_date_component_getter(
                    DateComponentGetter::Hours(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetMinutes => {
                self.emit_date_component_getter(
                    DateComponentGetter::Minutes(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcMinutes => {
                self.emit_date_component_getter(
                    DateComponentGetter::Minutes(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetSeconds => {
                self.emit_date_component_getter(
                    DateComponentGetter::Seconds(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcSeconds => {
                self.emit_date_component_getter(
                    DateComponentGetter::Seconds(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetMilliseconds => {
                self.emit_date_component_getter(
                    DateComponentGetter::Milliseconds(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetUtcMilliseconds => {
                self.emit_date_component_getter(
                    DateComponentGetter::Milliseconds(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeGetYear => {
                self.emit_date_component_getter(DateComponentGetter::Year, function)?;
            }
            StandardBuiltinId::DatePrototypeGetTimezoneOffset => {
                self.emit_date_component_getter(DateComponentGetter::TimezoneOffset, function)?;
            }
            StandardBuiltinId::DatePrototypeSetFullYear => {
                self.emit_date_component_setter(
                    DateComponentSetter::FullYear(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcFullYear => {
                self.emit_date_component_setter(
                    DateComponentSetter::FullYear(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetMonth => {
                self.emit_date_component_setter(
                    DateComponentSetter::Month(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcMonth => {
                self.emit_date_component_setter(
                    DateComponentSetter::Month(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetDate => {
                self.emit_date_component_setter(
                    DateComponentSetter::Date(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcDate => {
                self.emit_date_component_setter(
                    DateComponentSetter::Date(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetHours => {
                self.emit_date_component_setter(
                    DateComponentSetter::Hours(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcHours => {
                self.emit_date_component_setter(
                    DateComponentSetter::Hours(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetMinutes => {
                self.emit_date_component_setter(
                    DateComponentSetter::Minutes(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcMinutes => {
                self.emit_date_component_setter(
                    DateComponentSetter::Minutes(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetSeconds => {
                self.emit_date_component_setter(
                    DateComponentSetter::Seconds(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcSeconds => {
                self.emit_date_component_setter(
                    DateComponentSetter::Seconds(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetMilliseconds => {
                self.emit_date_component_setter(
                    DateComponentSetter::Milliseconds(DateTimeBasis::Local),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetUtcMilliseconds => {
                self.emit_date_component_setter(
                    DateComponentSetter::Milliseconds(DateTimeBasis::Utc),
                    function,
                )?;
            }
            StandardBuiltinId::DatePrototypeSetYear => {
                self.emit_date_component_setter(DateComponentSetter::Year, function)?;
            }
            StandardBuiltinId::DatePrototypeToIsoString => {
                self.emit_date_to_iso_string(function)?;
            }
            StandardBuiltinId::DatePrototypeToJson => {
                self.emit_date_to_json(function)?;
            }
            StandardBuiltinId::DatePrototypeToPrimitive => {
                self.emit_date_to_primitive(function)?;
            }
            StandardBuiltinId::DatePrototypeToDateString => {
                self.emit_date_to_date_string(function)?;
            }
            StandardBuiltinId::DatePrototypeToLocaleDateString => {
                self.emit_date_to_locale_string(DateLocaleFormat::Date, function)?;
            }
            StandardBuiltinId::DatePrototypeToLocaleString => {
                self.emit_date_to_locale_string(DateLocaleFormat::DateAndTime, function)?;
            }
            StandardBuiltinId::DatePrototypeToLocaleTimeString => {
                self.emit_date_to_locale_string(DateLocaleFormat::Time, function)?;
            }
            StandardBuiltinId::DatePrototypeToTemporalInstant => {
                self.emit_date_to_temporal_instant(function)?;
            }
            StandardBuiltinId::DatePrototypeToTimeString => {
                self.emit_date_to_time_string(function)?;
            }
            StandardBuiltinId::DatePrototypeToString => {
                self.emit_date_to_string(function)?;
            }
            StandardBuiltinId::DatePrototypeToUtcString => {
                self.emit_date_to_utc_string(function)?;
            }
            StandardBuiltinId::DataViewConstructor => {
                self.emit_data_view_constructor_builtin(function)?;
            }
            StandardBuiltinId::Uint8ArrayFromBase64 => {
                self.emit_uint8_array_from_base64(function)?;
            }
            StandardBuiltinId::Uint8ArrayFromHex => {
                self.emit_uint8_array_from_hex(function)?;
            }
            StandardBuiltinId::Uint8ArrayPrototypeSetFromBase64 => {
                self.emit_uint8_array_set_from_base64(function)?;
            }
            StandardBuiltinId::Uint8ArrayPrototypeSetFromHex => {
                self.emit_uint8_array_set_from_hex(function)?;
            }
            StandardBuiltinId::Uint8ArrayPrototypeToBase64 => {
                self.emit_uint8_array_to_base64(function)?;
            }
            StandardBuiltinId::Uint8ArrayPrototypeToHex => {
                self.emit_uint8_array_to_hex(function)?;
            }
            StandardBuiltinId::Float64ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Float64,
                    function,
                )?;
            }
            StandardBuiltinId::Float32ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Float32,
                    function,
                )?;
            }
            StandardBuiltinId::Float16ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Float16,
                    function,
                )?;
            }
            StandardBuiltinId::Int32ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Int32,
                    function,
                )?;
            }
            StandardBuiltinId::Int16ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Int16,
                    function,
                )?;
            }
            StandardBuiltinId::Int8ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Int8,
                    function,
                )?;
            }
            StandardBuiltinId::Uint32ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Uint32,
                    function,
                )?;
            }
            StandardBuiltinId::Uint16ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Uint16,
                    function,
                )?;
            }
            StandardBuiltinId::Uint8ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Uint8,
                    function,
                )?;
            }
            StandardBuiltinId::Uint8ClampedArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::Uint8Clamped,
                    function,
                )?;
            }
            StandardBuiltinId::BigInt64ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::BigInt64,
                    function,
                )?;
            }
            StandardBuiltinId::BigUint64ArrayConstructor => {
                self.emit_typed_array_constructor_builtin(
                    crate::module::TypedArrayElementKind::BigUint64,
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetUint8 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Uint8),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetInt8 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Int8),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetUint16 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Uint16),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetInt16 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Int16),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetUint32 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Uint32),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetInt32 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Int32),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetBigInt64 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::BigInt64),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetBigUint64 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::BigUint64),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetFloat16 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Float16),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetFloat32 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Float32),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeGetFloat64 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Get(DataViewElement::Float64),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetUint8 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Uint8),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetInt8 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Int8),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetUint16 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Uint16),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetInt16 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Int16),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetUint32 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Uint32),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetInt32 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Int32),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetFloat16 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Float16),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetFloat32 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Float32),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetFloat64 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::Float64),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetBigInt64 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::BigInt64),
                    function,
                )?;
            }
            StandardBuiltinId::DataViewPrototypeSetBigUint64 => {
                self.emit_data_view_access_builtin(
                    DataViewAccess::Set(DataViewElement::BigUint64),
                    function,
                )?;
            }
            StandardBuiltinId::BigIntConstructor => {
                self.emit_bigint_constructor_builtin(function)?
            }
            StandardBuiltinId::BigIntAsIntN => self.emit_bigint_as_int_n_builtin(function)?,
            StandardBuiltinId::BigIntAsUintN => self.emit_bigint_as_uint_n_builtin(function)?,
            StandardBuiltinId::NumberIsInteger => self.emit_number_is_integer_builtin(function)?,
            StandardBuiltinId::NumberIsSafeInteger => {
                self.emit_number_is_safe_integer_builtin(function)?
            }
            StandardBuiltinId::NumberIsNaN => self.emit_number_is_nan_builtin(function)?,
            StandardBuiltinId::NumberIsFinite => self.emit_number_is_finite_builtin(function)?,
            StandardBuiltinId::NumberPrototypeToExponential => {
                self.emit_number_prototype_to_exponential_builtin(function)?
            }
            StandardBuiltinId::NumberPrototypeToFixed => {
                self.emit_number_prototype_to_fixed_builtin(function)?
            }
            StandardBuiltinId::NumberPrototypeToPrecision => {
                self.emit_number_prototype_to_precision_builtin(function)?
            }
            StandardBuiltinId::NumberPrototypeToString => {
                self.emit_number_prototype_to_string_builtin(function)?
            }
            StandardBuiltinId::NumberPrototypeToLocaleString => {
                self.emit_number_prototype_to_locale_string_builtin(function)?
            }
            StandardBuiltinId::NumberPrototypeValueOf => {
                self.emit_number_prototype_value_of_builtin(function)?
            }
            StandardBuiltinId::BigIntPrototypeToString => {
                self.emit_bigint_prototype_to_string_builtin(function)?
            }
            StandardBuiltinId::BigIntPrototypeToLocaleString => {
                self.emit_bigint_prototype_to_locale_string_builtin(function)?
            }
            StandardBuiltinId::BigIntPrototypeValueOf => {
                self.emit_bigint_prototype_value_of_builtin(function)?
            }
            StandardBuiltinId::StringRaw => self.emit_string_raw_builtin(function)?,
            StandardBuiltinId::StringFromCharCode => {
                self.emit_string_from_char_code_builtin(function)?
            }
            StandardBuiltinId::StringFromCodePoint => {
                self.emit_string_from_code_point_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeToString
            | StandardBuiltinId::StringPrototypeValueOf => {
                self.emit_string_prototype_value_of_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeConcat => {
                self.compile_string_concat_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeCharAt => {
                self.emit_string_prototype_char_at_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeAt => {
                self.emit_string_prototype_at_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeCharCodeAt => {
                self.emit_string_prototype_char_code_at_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeCodePointAt => {
                self.emit_string_prototype_code_point_at_builtin(function)?
            }
            StandardBuiltinId::BooleanPrototypeToString => {
                self.emit_boolean_prototype_to_string_builtin(function)?
            }
            StandardBuiltinId::BooleanPrototypeValueOf => {
                self.emit_boolean_prototype_value_of_builtin(function)?
            }
            StandardBuiltinId::GlobalIsFinite => self.emit_global_is_finite_builtin(function)?,
            StandardBuiltinId::GlobalIsNaN => self.emit_global_is_nan_builtin(function)?,
            StandardBuiltinId::MathAbs => self.emit_math_abs_builtin(function)?,
            StandardBuiltinId::MathAcos => self.emit_math_acos_builtin(function)?,
            StandardBuiltinId::MathAcosh => self.emit_math_acosh_builtin(function)?,
            StandardBuiltinId::MathAsin => self.emit_math_asin_builtin(function)?,
            StandardBuiltinId::MathAsinh => self.emit_math_asinh_builtin(function)?,
            StandardBuiltinId::MathAtan => self.emit_math_atan_builtin(function)?,
            StandardBuiltinId::MathAtan2 => self.emit_math_atan2_builtin(function)?,
            StandardBuiltinId::MathAtanh => self.emit_math_atanh_builtin(function)?,
            StandardBuiltinId::MathCbrt => self.emit_math_cbrt_builtin(function)?,
            StandardBuiltinId::MathCeil => self.emit_math_ceil_builtin(function)?,
            StandardBuiltinId::MathClz32 => self.emit_math_clz32_builtin(function)?,
            StandardBuiltinId::MathCos => self.emit_math_cos_builtin(function)?,
            StandardBuiltinId::MathCosh => self.emit_math_cosh_builtin(function)?,
            StandardBuiltinId::MathExp => self.emit_math_exp_builtin(function)?,
            StandardBuiltinId::MathExpm1 => self.emit_math_expm1_builtin(function)?,
            StandardBuiltinId::MathF16Round => self.emit_math_f16round_builtin(function)?,
            StandardBuiltinId::MathFloor => self.emit_math_floor_builtin(function)?,
            StandardBuiltinId::MathFround => self.emit_math_fround_builtin(function)?,
            StandardBuiltinId::MathHypot => self.emit_math_hypot_builtin(function)?,
            StandardBuiltinId::MathImul => self.emit_math_imul_builtin(function)?,
            StandardBuiltinId::MathLog => self.emit_math_log_builtin(function)?,
            StandardBuiltinId::MathLog10 => self.emit_math_log10_builtin(function)?,
            StandardBuiltinId::MathLog1p => self.emit_math_log1p_builtin(function)?,
            StandardBuiltinId::MathLog2 => self.emit_math_log2_builtin(function)?,
            StandardBuiltinId::MathPow => self.emit_math_pow_builtin(function)?,
            StandardBuiltinId::MathRandom => self.emit_math_random_builtin(function)?,
            StandardBuiltinId::MathRound => self.emit_math_round_builtin(function)?,
            StandardBuiltinId::MathSign => self.emit_math_sign_builtin(function)?,
            StandardBuiltinId::MathSin => self.emit_math_sin_builtin(function)?,
            StandardBuiltinId::MathSinh => self.emit_math_sinh_builtin(function)?,
            StandardBuiltinId::MathSqrt => self.emit_math_sqrt_builtin(function)?,
            StandardBuiltinId::MathSumPrecise => self.emit_math_sum_precise_builtin(function)?,
            StandardBuiltinId::MathTan => self.emit_math_tan_builtin(function)?,
            StandardBuiltinId::MathTanh => self.emit_math_tanh_builtin(function)?,
            StandardBuiltinId::MathTrunc => self.emit_math_trunc_builtin(function)?,
            StandardBuiltinId::MathMin => self.emit_math_min_builtin(function)?,
            StandardBuiltinId::MathMax => self.emit_math_max_builtin(function)?,
            StandardBuiltinId::SymbolConstructor => {
                self.emit_symbol_constructor_builtin(function)?
            }
            StandardBuiltinId::SymbolFor => self.emit_symbol_for_builtin(function)?,
            StandardBuiltinId::SymbolKeyFor => self.emit_symbol_key_for_builtin(function)?,
            StandardBuiltinId::SymbolPrototypeDescriptionGetter => {
                self.emit_symbol_prototype_description_getter_builtin(function)?
            }
            StandardBuiltinId::SymbolPrototypeToString => {
                self.emit_symbol_prototype_to_string_builtin(function)?
            }
            StandardBuiltinId::SymbolPrototypeValueOf => {
                self.emit_symbol_prototype_value_of_builtin(function)?
            }
            StandardBuiltinId::SymbolPrototypeToPrimitive => {
                self.emit_symbol_prototype_to_primitive_builtin(function)?
            }
            StandardBuiltinId::BooleanConstructor => {
                self.emit_boolean_constructor_builtin(function)?
            }
            StandardBuiltinId::NumberConstructor => {
                self.emit_number_constructor_builtin(function)?
            }
            StandardBuiltinId::StringConstructor => {
                self.emit_string_constructor_builtin(function)?
            }
            StandardBuiltinId::ErrorConstructor => self.emit_error_constructor_builtin(function)?,
            StandardBuiltinId::ErrorIsError => self.emit_error_is_error_builtin(function)?,
            StandardBuiltinId::ErrorPrototypeStackGetter => {
                self.emit_error_stack_getter_builtin(function)?
            }
            StandardBuiltinId::ErrorPrototypeStackSetter => {
                self.emit_error_stack_setter_builtin(function)?
            }
            StandardBuiltinId::EvalErrorConstructor => {
                self.emit_eval_error_constructor_builtin(function)?
            }
            StandardBuiltinId::AggregateErrorConstructor => {
                self.emit_aggregate_error_constructor_builtin(function)?
            }
            StandardBuiltinId::SuppressedErrorConstructor => {
                self.emit_suppressed_error_constructor_builtin(function)?
            }
            StandardBuiltinId::RangeErrorConstructor => {
                self.emit_range_error_constructor_builtin(function)?
            }
            StandardBuiltinId::SyntaxErrorConstructor => {
                self.emit_syntax_error_constructor_builtin(function)?
            }
            StandardBuiltinId::TypeErrorConstructor => {
                self.emit_type_error_constructor_builtin(function)?
            }
            StandardBuiltinId::URIErrorConstructor => {
                self.emit_uri_error_constructor_builtin(function)?
            }
            StandardBuiltinId::ReferenceErrorConstructor => {
                self.emit_reference_error_constructor_builtin(function)?
            }
            StandardBuiltinId::FunctionPrototypeToString => {
                self.emit_function_prototype_to_string_builtin(function)?
            }
            StandardBuiltinId::Escape => self.emit_escape_builtin(function)?,
            StandardBuiltinId::Unescape => self.emit_unescape_builtin(function)?,
            StandardBuiltinId::EncodeUri => self.emit_encode_uri_builtin(function)?,
            StandardBuiltinId::EncodeUriComponent => {
                self.emit_encode_uri_component_builtin(function)?
            }
            StandardBuiltinId::DecodeUri => self.emit_decode_uri_builtin(function)?,
            StandardBuiltinId::DecodeUriComponent => {
                self.emit_decode_uri_component_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeAnchor => {
                self.emit_string_html_builtin(StringHtmlOperation::Anchor, function)?;
            }
            StandardBuiltinId::StringPrototypeBig => {
                self.emit_string_html_builtin(StringHtmlOperation::Big, function)?;
            }
            StandardBuiltinId::StringPrototypeBlink => {
                self.emit_string_html_builtin(StringHtmlOperation::Blink, function)?;
            }
            StandardBuiltinId::StringPrototypeBold => {
                self.emit_string_html_builtin(StringHtmlOperation::Bold, function)?;
            }
            StandardBuiltinId::StringPrototypeFixed => {
                self.emit_string_html_builtin(StringHtmlOperation::Fixed, function)?;
            }
            StandardBuiltinId::StringPrototypeFontcolor => {
                self.emit_string_html_builtin(StringHtmlOperation::Fontcolor, function)?;
            }
            StandardBuiltinId::StringPrototypeFontsize => {
                self.emit_string_html_builtin(StringHtmlOperation::Fontsize, function)?;
            }
            StandardBuiltinId::StringPrototypeItalics => {
                self.emit_string_html_builtin(StringHtmlOperation::Italics, function)?;
            }
            StandardBuiltinId::StringPrototypeLink => {
                self.emit_string_html_builtin(StringHtmlOperation::Link, function)?;
            }
            StandardBuiltinId::StringPrototypeSmall => {
                self.emit_string_html_builtin(StringHtmlOperation::Small, function)?;
            }
            StandardBuiltinId::StringPrototypeStrike => {
                self.emit_string_html_builtin(StringHtmlOperation::Strike, function)?;
            }
            StandardBuiltinId::StringPrototypeSub => {
                self.emit_string_html_builtin(StringHtmlOperation::Sub, function)?;
            }
            StandardBuiltinId::StringPrototypeSup => {
                self.emit_string_html_builtin(StringHtmlOperation::Sup, function)?;
            }
            StandardBuiltinId::StringPrototypeSubstr => {
                self.emit_string_prototype_substr_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeSubstring => {
                self.compile_string_substring_range_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeSlice => {
                self.compile_string_slice_range_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeIndexOf => {
                self.emit_string_prototype_index_of_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeLastIndexOf => {
                self.emit_string_prototype_last_index_of_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeEndsWith => {
                self.emit_string_prototype_ends_with_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeIncludes => {
                self.emit_string_prototype_includes_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeStartsWith => {
                self.emit_string_prototype_starts_with_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeMatch => {
                self.emit_string_match_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypeMatchAll => {
                self.emit_string_match_all_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypeReplace => {
                self.emit_string_replace_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypeReplaceAll => {
                self.emit_string_replace_all_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypeSearch => {
                self.emit_string_search_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypeSplit => {
                self.emit_string_split_builtin(function)?;
            }
            StandardBuiltinId::RegExpConstructor => {
                self.emit_regexp_constructor_builtin(function)?;
            }
            StandardBuiltinId::JsonParse => self.emit_json_parse_builtin(function)?,
            StandardBuiltinId::JsonStringify => self.emit_json_stringify_builtin(function)?,
            StandardBuiltinId::JsonRawJson => self.emit_json_raw_json_builtin(function)?,
            StandardBuiltinId::JsonIsRawJson => self.emit_json_is_raw_json_builtin(function)?,
            StandardBuiltinId::AtomicsAdd => self.emit_atomics_add_builtin(function)?,
            StandardBuiltinId::AtomicsAnd => self.emit_atomics_and_builtin(function)?,
            StandardBuiltinId::AtomicsCompareExchange => {
                self.emit_atomics_compare_exchange_builtin(function)?
            }
            StandardBuiltinId::AtomicsExchange => self.emit_atomics_exchange_builtin(function)?,
            StandardBuiltinId::AtomicsLoad => self.emit_atomics_load_builtin(function)?,
            StandardBuiltinId::AtomicsNotify => self.emit_atomics_notify_builtin(function)?,
            StandardBuiltinId::AtomicsOr => self.emit_atomics_or_builtin(function)?,
            StandardBuiltinId::AtomicsPause => self.emit_atomics_pause_builtin(function)?,
            StandardBuiltinId::AtomicsStore => self.emit_atomics_store_builtin(function)?,
            StandardBuiltinId::AtomicsSub => self.emit_atomics_sub_builtin(function)?,
            StandardBuiltinId::AtomicsWait => self.emit_atomics_wait_builtin(function)?,
            StandardBuiltinId::AtomicsWaitAsync => {
                self.emit_atomics_wait_async_builtin(function)?
            }
            StandardBuiltinId::AtomicsXor => self.emit_atomics_xor_builtin(function)?,
            StandardBuiltinId::AtomicsIsLockFree => {
                self.emit_atomics_is_lock_free_builtin(function)?
            }
            StandardBuiltinId::RegExpLegacyStaticGetter => {
                self.emit_regexp_legacy_static_accessor(
                    crate::builtins::string::RegExpLegacyAccessorKind::Getter,
                    function,
                )?;
            }
            StandardBuiltinId::RegExpLegacyStaticSetter => {
                self.emit_regexp_legacy_static_accessor(
                    crate::builtins::string::RegExpLegacyAccessorKind::InputSetter,
                    function,
                )?;
            }
            StandardBuiltinId::RegExpEscape => {
                self.emit_regexp_escape_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeFlagsGetter => {
                self.emit_regexp_prototype_flags_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeSourceGetter => {
                self.emit_regexp_prototype_source_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeHasIndicesGetter => {
                self.emit_regexp_prototype_has_indices_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeGlobalGetter => {
                self.emit_regexp_prototype_global_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter => {
                self.emit_regexp_prototype_ignore_case_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeMultilineGetter => {
                self.emit_regexp_prototype_multiline_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeDotAllGetter => {
                self.emit_regexp_prototype_dot_all_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeUnicodeGetter => {
                self.emit_regexp_prototype_unicode_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter => {
                self.emit_regexp_prototype_unicode_sets_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeStickyGetter => {
                self.emit_regexp_prototype_sticky_getter_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeSymbolMatch => {
                self.emit_regexp_prototype_symbol_match_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeCompile => {
                self.emit_regexp_prototype_compile_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeExec => {
                self.emit_regexp_prototype_exec_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeTest => {
                self.emit_regexp_prototype_test_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeToString => {
                self.emit_regexp_prototype_to_string_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeSymbolMatchAll => {
                self.emit_regexp_prototype_symbol_match_all_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeSymbolReplace => {
                self.emit_regexp_prototype_symbol_replace_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeSymbolSearch => {
                self.emit_regexp_prototype_symbol_search_builtin(function)?;
            }
            StandardBuiltinId::RegExpPrototypeSymbolSplit => {
                self.emit_regexp_prototype_symbol_split_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypePadStart => {
                self.emit_string_prototype_pad_start_builtin(function)?
            }
            StandardBuiltinId::StringPrototypePadEnd => {
                self.emit_string_prototype_pad_end_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeRepeat => {
                self.emit_string_prototype_repeat_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeNormalize => {
                self.emit_string_normalize_builtin(function)?;
            }
            StandardBuiltinId::StringPrototypeLocaleCompare => {
                self.emit_string_prototype_locale_compare_builtin(function)?
            }

            StandardBuiltinId::StringPrototypeIterator => {
                self.emit_string_prototype_iterator_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeToLocaleLowerCase => {
                self.emit_string_case_builtin(StringCaseOperation::LocaleLower, function)?;
            }
            StandardBuiltinId::StringPrototypeToLocaleUpperCase => {
                self.emit_string_case_builtin(StringCaseOperation::LocaleUpper, function)?;
            }
            StandardBuiltinId::StringPrototypeToLowerCase => {
                self.emit_string_case_builtin(StringCaseOperation::Lower, function)?;
            }
            StandardBuiltinId::StringPrototypeToUpperCase => {
                self.emit_string_case_builtin(StringCaseOperation::Upper, function)?;
            }
            StandardBuiltinId::StringPrototypeIsWellFormed => {
                self.emit_string_is_well_formed_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeToWellFormed => {
                self.emit_string_to_well_formed_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeTrim => {
                self.emit_string_prototype_trim_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeTrimStart => {
                self.emit_string_prototype_trim_start_builtin(function)?
            }
            StandardBuiltinId::StringPrototypeTrimEnd => {
                self.emit_string_prototype_trim_end_builtin(function)?
            }
            StandardBuiltinId::ErrorPrototypeToString => {
                self.emit_error_prototype_to_string_builtin(function)?
            }
        }
        Ok(())
    }
}
