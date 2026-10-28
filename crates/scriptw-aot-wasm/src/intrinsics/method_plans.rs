//! Ordered installation plans shared by R data collection and emission.
//! These are metadata for private bootstrap, never JavaScript instructions.
use super::temporal::TemporalIntrinsicFamily;
use super::*;
use std::borrow::Cow;

mod array;
mod binary_data;
mod bootstrap;
mod collections;
mod date;
mod function;
mod iterator;
mod numeric;
mod object;
mod promise;
mod regexp;
mod resource_management;
mod string;

// Each static plan has one enum row, table-collection row and emitter view.
macro_rules! intrinsic_method_plans {
    ($($name:ident => $module:ident::$rows:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub(crate) enum IntrinsicMethodPlan {
            TemporalConstructor(TemporalIntrinsicFamily),
            TemporalPrototype(TemporalIntrinsicFamily),
            $($name,)+
        }

        impl IntrinsicMethodPlan {
            pub(crate) fn all() -> impl Iterator<Item = Self> {
                TemporalIntrinsicFamily::ALL.into_iter().flat_map(|family| [
                    Self::TemporalConstructor(family),
                    Self::TemporalPrototype(family),
                ]).chain([$(Self::$name),+])
            }

            pub(crate) fn methods(self) -> Result<Cow<'static, [IntrinsicMethodSpec]>, EmitError> {
                match self {
                    Self::TemporalConstructor(family) => family.constructor_installation_methods().map(Cow::Owned),
                    Self::TemporalPrototype(family) => family.prototype_installation_methods().map(Cow::Owned),
                    $(Self::$name => Ok(Cow::Borrowed($module::$rows)),)+
                }
            }
        }
    };
}

intrinsic_method_plans! {
   NumberPrototype => numeric::NUMBER_PROTOTYPE,
   NumberConstructor => numeric::NUMBER_CONSTRUCTOR,
   PromisePrototype => promise::PROMISE_PROTOTYPE,
   PromiseConstructor => promise::PROMISE_CONSTRUCTOR,
   RegExpSymbols => regexp::REG_EXP_SYMBOLS,
   StringPrototypeBeforeAliases => string::STRING_PROTOTYPE_BEFORE_ALIASES,
   StringPrototypeAfterAliases => string::STRING_PROTOTYPE_AFTER_ALIASES,
   ArrayConstructor => array::ARRAY_CONSTRUCTOR,
   ArrayPrototype => array::ARRAY_PROTOTYPE,
   ArrayBufferPrototypeMutable => binary_data::ARRAY_BUFFER_PROTOTYPE_MUTABLE,
   DataViewPrototype => binary_data::DATA_VIEW_PROTOTYPE,
   MapPrototype => collections::MAP_PROTOTYPE,
   WeakMapPrototype => collections::WEAK_MAP_PROTOTYPE,
   WeakSetPrototype => collections::WEAK_SET_PROTOTYPE,
   AsyncDisposableStackPrototype => collections::ASYNC_DISPOSABLE_STACK_PROTOTYPE,
   SetPrototype => collections::SET_PROTOTYPE,
   DateConstructor => date::DATE_CONSTRUCTOR,
   DatePrototype => date::DATE_PROTOTYPE,
   FunctionPrototype => function::FUNCTION_PROTOTYPE,
   IteratorConstructor => iterator::ITERATOR_CONSTRUCTOR,
   IteratorPrototype => iterator::ITERATOR_PROTOTYPE,
   BigIntPrototype => numeric::BIG_INT_PROTOTYPE,
   BigIntConstructor => numeric::BIG_INT_CONSTRUCTOR,
   BooleanPrototype => numeric::BOOLEAN_PROTOTYPE,
   ObjectConstructorInitial => object::OBJECT_CONSTRUCTOR_INITIAL,
   ObjectPrototypeBeforeAccessor => object::OBJECT_PROTOTYPE_BEFORE_ACCESSOR,
   ObjectPrototypeAfterAccessor => object::OBJECT_PROTOTYPE_AFTER_ACCESSOR,
   ObjectConstructorRemaining => object::OBJECT_CONSTRUCTOR_REMAINING,
   RegExpPrototypeBeforeAccessors => regexp::REG_EXP_PROTOTYPE_BEFORE_ACCESSORS,
   DisposableStackPrototype => resource_management::DISPOSABLE_STACK_PROTOTYPE,
   Reflect => bootstrap::REFLECT,
   Math => bootstrap::MATH,
   Json => bootstrap::JSON,
   TypedArrayPrototypeBeforeAliases => bootstrap::TYPED_ARRAY_PROTOTYPE_BEFORE_ALIASES,
   TypedArrayPrototypeAfterAliases => bootstrap::TYPED_ARRAY_PROTOTYPE_AFTER_ALIASES,
   TypedArrayConstructor => bootstrap::TYPED_ARRAY_CONSTRUCTOR,
}

#[derive(Clone, Copy)]
pub(crate) struct IntrinsicMethodSpec {
    pub(crate) key: IntrinsicKey<'static>,
    pub(crate) builtin: StandardBuiltinId,
    pub(crate) writable: bool,
    pub(crate) configurable: bool,
}

impl IntrinsicMethodSpec {
    const fn native(builtin: StandardBuiltinId) -> Self {
        let name = match builtin.native_function_name() {
            Some(name) => name,
            None => panic!("intrinsic method plan requires a native name"),
        };
        Self::named(name, builtin)
    }

    const fn string_method(builtin: StandardBuiltinId) -> Self {
        let name = match builtin.string_prototype_method_name() {
            Some(name) => name,
            None => panic!("String intrinsic method plan requires a prototype name"),
        };
        Self::named(name, builtin)
    }

    const fn symbol(
        symbol: scriptw_ir::WellKnownSymbol,
        builtin: StandardBuiltinId,
        writable: bool,
    ) -> Self {
        Self {
            key: IntrinsicKey::Symbol(symbol),
            builtin,
            writable,
            configurable: true,
        }
    }

    const fn named(name: &'static str, builtin: StandardBuiltinId) -> Self {
        Self {
            key: IntrinsicKey::Name(name),
            builtin,
            writable: true,
            configurable: true,
        }
    }
}
