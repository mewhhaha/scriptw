//! The one authority that may resolve a property name to a catalogued method
//! of an intrinsic prototype.
//!
//! Static call lowering maps `(receiver kind, property name)` to a builtin so
//! it can pick a fast path and a result kind. Each of those tables used to
//! trust the catalogue unconditionally, so a program that had replaced
//! `String.prototype.toUpperCase` or `Function.prototype.apply` got the
//! builtin's result kind — or the builtin itself — instead of its own
//! function. The catalogue is only a claim about a fresh realm; whether the
//! claim still holds at a program point is a flow fact.
//!
//! [`IntrinsicMethod`] is that fact. Its field is private to this module, so
//! one private live-property factory is its only constructor, and every
//! static resolution that wants the exact builtin has to go through the same
//! proof. A catalogued name without a proof is [`IntrinsicMethodLookup::Unproven`]:
//! the builtin stays a possible (and therefore still emitted) callee, but the
//! read is an ordinary property read and licenses no result kind.
use super::*;

/// An intrinsic prototype object that static lowering resolves methods on.
///
/// Every variant is `%C.prototype%` for a global constructor `C` whose
/// catalogued shape carries a `prototype` data property, which is where
/// writes to the prototype are recorded (`IntrinsicPrototypeShape`
/// publication) and where aliasing writes, deletes and unknown effects erase
/// what was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IntrinsicPrototype {
    Object,
    Function,
    Array,
    String,
    Number,
    Boolean,
    BigInt,
    Symbol,
    Iterator,
    RegExp,
}

impl IntrinsicPrototype {
    const fn constructor(self) -> StandardBuiltinId {
        match self {
            Self::Object => StandardBuiltinId::ObjectConstructor,
            Self::Function => StandardBuiltinId::FunctionConstructor,
            Self::Array => StandardBuiltinId::ArrayConstructor,
            Self::String => StandardBuiltinId::StringConstructor,
            Self::Number => StandardBuiltinId::NumberConstructor,
            Self::Boolean => StandardBuiltinId::BooleanConstructor,
            Self::BigInt => StandardBuiltinId::BigIntConstructor,
            Self::Symbol => StandardBuiltinId::SymbolConstructor,
            Self::Iterator => StandardBuiltinId::IteratorConstructor,
            Self::RegExp => StandardBuiltinId::RegExpConstructor,
        }
    }

    /// The builtin `%C.prototype%[name]` holds in a fresh realm, for the names
    /// static lowering resolves. Any other name is `None`, including names the
    /// prototype does have but that no lowering path specializes.
    pub(super) fn catalogued_method(self, name: &str) -> Option<StandardBuiltinId> {
        use StandardBuiltinId as B;
        Some(match (self, name) {
            (Self::Object, "toString") => B::ObjectPrototypeToString,
            (Self::Object, "toLocaleString") => B::ObjectPrototypeToLocaleString,
            (Self::Object, "valueOf") => B::ObjectPrototypeValueOf,
            (Self::Object, "hasOwnProperty") => B::ObjectPrototypeHasOwnProperty,
            (Self::Object, "isPrototypeOf") => B::ObjectPrototypeIsPrototypeOf,
            (Self::Object, "propertyIsEnumerable") => B::ObjectPrototypePropertyIsEnumerable,
            (Self::Function, "call") => B::FunctionPrototypeCall,
            (Self::Function, "apply") => B::FunctionPrototypeApply,
            (Self::Function, "bind") => B::FunctionPrototypeBind,
            (Self::Function, "toString") => B::FunctionPrototypeToString,
            (Self::String, "charAt") => B::StringPrototypeCharAt,
            (Self::String, "concat") => B::StringPrototypeConcat,
            (Self::String, "charCodeAt") => B::StringPrototypeCharCodeAt,
            (Self::String, "codePointAt") => B::StringPrototypeCodePointAt,
            (Self::String, "at") => B::StringPrototypeAt,
            (Self::String, "anchor") => B::StringPrototypeAnchor,
            (Self::String, "big") => B::StringPrototypeBig,
            (Self::String, "blink") => B::StringPrototypeBlink,
            (Self::String, "bold") => B::StringPrototypeBold,
            (Self::String, "fixed") => B::StringPrototypeFixed,
            (Self::String, "fontcolor") => B::StringPrototypeFontcolor,
            (Self::String, "fontsize") => B::StringPrototypeFontsize,
            (Self::String, "italics") => B::StringPrototypeItalics,
            (Self::String, "link") => B::StringPrototypeLink,
            (Self::String, "small") => B::StringPrototypeSmall,
            (Self::String, "strike") => B::StringPrototypeStrike,
            (Self::String, "sub") => B::StringPrototypeSub,
            (Self::String, "substr") => B::StringPrototypeSubstr,
            (Self::String, "substring") => B::StringPrototypeSubstring,
            (Self::String, "sup") => B::StringPrototypeSup,
            (Self::String, "match") => B::StringPrototypeMatch,
            (Self::String, "matchAll") => B::StringPrototypeMatchAll,
            (Self::String, "replace") => B::StringPrototypeReplace,
            (Self::String, "replaceAll") => B::StringPrototypeReplaceAll,
            (Self::String, "search") => B::StringPrototypeSearch,
            (Self::String, "indexOf") => B::StringPrototypeIndexOf,
            (Self::String, "lastIndexOf") => B::StringPrototypeLastIndexOf,
            (Self::String, "slice") => B::StringPrototypeSlice,
            (Self::String, "split") => B::StringPrototypeSplit,
            (Self::String, "padStart") => B::StringPrototypePadStart,
            (Self::String, "padEnd") => B::StringPrototypePadEnd,
            (Self::String, "repeat") => B::StringPrototypeRepeat,
            (Self::String, "endsWith") => B::StringPrototypeEndsWith,
            (Self::String, "includes") => B::StringPrototypeIncludes,
            (Self::String, "startsWith") => B::StringPrototypeStartsWith,
            (Self::String, "normalize") => B::StringPrototypeNormalize,
            (Self::String, "localeCompare") => B::StringPrototypeLocaleCompare,
            (Self::String, "toLocaleLowerCase") => B::StringPrototypeToLocaleLowerCase,
            (Self::String, "toLocaleUpperCase") => B::StringPrototypeToLocaleUpperCase,
            (Self::String, "toLowerCase") => B::StringPrototypeToLowerCase,
            (Self::String, "toUpperCase") => B::StringPrototypeToUpperCase,
            (Self::String, "toString") => B::StringPrototypeToString,
            (Self::String, "valueOf") => B::StringPrototypeValueOf,
            (Self::String, "trim") => B::StringPrototypeTrim,
            // B.2.2.15-16: the Annex B names are the same function objects.
            (Self::String, "trimStart" | "trimLeft") => B::StringPrototypeTrimStart,
            (Self::String, "trimEnd" | "trimRight") => B::StringPrototypeTrimEnd,
            (Self::String, "isWellFormed") => B::StringPrototypeIsWellFormed,
            (Self::String, "toWellFormed") => B::StringPrototypeToWellFormed,
            (Self::Number, "toExponential") => B::NumberPrototypeToExponential,
            (Self::Number, "toFixed") => B::NumberPrototypeToFixed,
            (Self::Number, "toLocaleString") => B::NumberPrototypeToLocaleString,
            (Self::Number, "toPrecision") => B::NumberPrototypeToPrecision,
            (Self::Number, "toString") => B::NumberPrototypeToString,
            (Self::Number, "valueOf") => B::NumberPrototypeValueOf,
            (Self::Boolean, "toString") => B::BooleanPrototypeToString,
            (Self::Boolean, "valueOf") => B::BooleanPrototypeValueOf,
            (Self::BigInt, "toString") => B::BigIntPrototypeToString,
            (Self::BigInt, "toLocaleString") => B::BigIntPrototypeToLocaleString,
            (Self::BigInt, "valueOf") => B::BigIntPrototypeValueOf,
            (Self::Symbol, "toString") => B::SymbolPrototypeToString,
            (Self::Symbol, "valueOf") => B::SymbolPrototypeValueOf,
            (Self::Symbol, "constructor") => B::SymbolConstructor,
            (Self::Iterator, "toArray") => B::IteratorPrototypeToArray,
            (Self::Iterator, "includes") => B::IteratorPrototypeIncludes,
            (Self::Iterator, "join") => B::IteratorPrototypeJoin,
            (Self::Iterator, "forEach") => B::IteratorPrototypeForEach,
            (Self::Iterator, "every") => B::IteratorPrototypeEvery,
            (Self::Iterator, "some") => B::IteratorPrototypeSome,
            (Self::Iterator, "find") => B::IteratorPrototypeFind,
            (Self::Iterator, "reduce") => B::IteratorPrototypeReduce,
            (Self::Iterator, "map") => B::IteratorPrototypeMap,
            (Self::Iterator, "filter") => B::IteratorPrototypeFilter,
            (Self::Iterator, "flatMap") => B::IteratorPrototypeFlatMap,
            (Self::Iterator, "take") => B::IteratorPrototypeTake,
            (Self::Iterator, "drop") => B::IteratorPrototypeDrop,
            (Self::Iterator, "chunks") => B::IteratorPrototypeChunks,
            (Self::Iterator, "windows") => B::IteratorPrototypeWindows,
            (Self::RegExp, "exec") => B::RegExpPrototypeExec,
            (Self::RegExp, "test") => B::RegExpPrototypeTest,
            _ => return None,
        })
    }
}

/// Proof that, at the current program point, `%C.prototype%[name]` still
/// holds the builtin the catalogue names for it.
///
/// It says nothing about the receiver: a caller that reads the method from a
/// receiver must separately know that the receiver neither has an own property
/// of that name nor a different prototype chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IntrinsicMethod {
    builtin: StandardBuiltinId,
}

impl IntrinsicMethod {
    pub(super) fn builtin(self) -> StandardBuiltinId {
        self.builtin
    }

    /// The exact callee fact the proof licenses.
    pub(super) fn callee_info(self) -> ValueInfo {
        ScriptLowerer::standard_builtin_value_info(self.builtin)
    }
}

/// The outcome of resolving one property name on one intrinsic prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IntrinsicMethodLookup {
    Proven(IntrinsicMethod),
    /// The name is catalogued, but a write, delete or unknown effect may have
    /// replaced the builtin.
    Unproven(StandardBuiltinId),
    /// No lowering path specializes this name on this prototype.
    Uncatalogued,
}

impl IntrinsicMethodLookup {
    pub(super) fn proven(self) -> Option<IntrinsicMethod> {
        match self {
            Self::Proven(method) => Some(method),
            Self::Unproven(_) | Self::Uncatalogued => None,
        }
    }

    /// The same lookup for a receiver whose own properties are unknown: the
    /// prototype may still hold the builtin, but the receiver may shadow it.
    pub(super) fn unclaimed(self) -> Self {
        match self {
            Self::Proven(method) => Self::Unproven(method.builtin),
            Self::Unproven(builtin) => Self::Unproven(builtin),
            Self::Uncatalogued => Self::Uncatalogued,
        }
    }

    /// The callee fact a read of the looked-up name may carry: exact under a
    /// proof, and otherwise an open target set that keeps the builtin a
    /// possible — and so still emitted — callee without claiming it.
    pub(super) fn callee_info(self) -> Option<ValueInfo> {
        match self {
            Self::Proven(method) => Some(method.callee_info()),
            Self::Unproven(builtin) => Some(ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::all_runtime_tags(),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::Open(BTreeSet::from([
                    builtin.function_id()
                ])),
            }),
            Self::Uncatalogued => None,
        }
    }
}

impl ScriptLowerer<'_> {
    /// Resolve `name` on `prototype`, proving the catalogued builtin is still
    /// installed. The only constructor of [`IntrinsicMethod`].
    pub(super) fn intrinsic_method(
        &self,
        prototype: IntrinsicPrototype,
        name: &str,
    ) -> IntrinsicMethodLookup {
        let Some(builtin) = prototype.catalogued_method(name) else {
            return IntrinsicMethodLookup::Uncatalogued;
        };
        let deleted_to_string = name == "toString"
            && match prototype {
                IntrinsicPrototype::Number => {
                    self.number_prototype_to_string_state != PrototypeToStringState::Intrinsic
                }
                IntrinsicPrototype::Boolean => {
                    self.boolean_prototype_to_string_state != PrototypeToStringState::Intrinsic
                }
                _ => false,
            };
        self.intrinsic_property_method(prototype, name, builtin, deleted_to_string)
    }

    fn intrinsic_property_method(
        &self,
        prototype: IntrinsicPrototype,
        name: &str,
        builtin: StandardBuiltinId,
        invalidated: bool,
    ) -> IntrinsicMethodLookup {
        if !invalidated && self.intrinsic_prototype_still_holds(prototype, name, builtin) {
            IntrinsicMethodLookup::Proven(IntrinsicMethod { builtin })
        } else {
            IntrinsicMethodLookup::Unproven(builtin)
        }
    }

    /// `receiver[name]` for an object receiver: its shape must resolve the
    /// name, through its own prototype chain, to the builtin `prototype`
    /// catalogues, and that prototype must still hold it. The shape alone is a
    /// snapshot taken when the object was created and says nothing about
    /// writes to the shared prototype since.
    pub(super) fn shaped_receiver_intrinsic_method(
        &self,
        receiver: &TypedExpr,
        prototype: IntrinsicPrototype,
        name: &str,
    ) -> Option<IntrinsicMethod> {
        let method = self.intrinsic_method(prototype, name).proven()?;
        match self.read_current_object_shape_property(receiver, name)? {
            ObjectShapeProperty::Data(info) => (info.function_targets.exact_single_target()
                == Some(&method.builtin.function_id()))
            .then_some(method),
            ObjectShapeProperty::Accessor { .. } => None,
        }
    }

    /// A read of `name` from a primitive (or otherwise own-property-free)
    /// receiver whose prototype is `prototype`, carrying only the callee fact
    /// the lookup licenses. `None` for an uncatalogued name.
    pub(super) fn intrinsic_method_read(
        &mut self,
        prototype: IntrinsicPrototype,
        receiver: &TypedExpr,
        name: &str,
    ) -> Option<TypedExpr> {
        let lookup = self.intrinsic_method(prototype, name);
        let key = PropertyKeyIr::StaticString(name.to_string());
        self.read_intrinsic_method_lookup(lookup, receiver, key)
    }

    /// Symbol-keyed methods obey the same live-prototype proof as named
    /// methods. The namespace key is used only for shape lookup; the emitted
    /// property key retains the actual Symbol operand.
    pub(super) fn intrinsic_symbol_method_read(
        &mut self,
        prototype: IntrinsicPrototype,
        receiver: &TypedExpr,
        symbol: WellKnownSymbol,
        key: PropertyKeyIr,
    ) -> Option<TypedExpr> {
        let builtin = match (prototype, symbol) {
            (IntrinsicPrototype::Symbol, WellKnownSymbol::ToPrimitive) => {
                StandardBuiltinId::SymbolPrototypeToPrimitive
            }
            (IntrinsicPrototype::String, WellKnownSymbol::Iterator) => {
                StandardBuiltinId::StringPrototypeIterator
            }
            _ => return None,
        };
        let lookup =
            self.intrinsic_property_method(prototype, &shape_namespace_key(symbol), builtin, false);
        self.read_intrinsic_method_lookup(lookup, receiver, key)
    }

    /// Object calls must acquire their actual property even when a catalogue
    /// method remains possible. Own descriptors are authoritative; an absent
    /// descriptor cannot prove that a prototype lookup has no getter.
    pub(super) fn intrinsic_object_method_read(
        &mut self,
        prototype: IntrinsicPrototype,
        receiver: &TypedExpr,
        name: &str,
        key: PropertyKeyIr,
    ) -> Option<TypedExpr> {
        let lookup = self.intrinsic_method(prototype, name);
        if matches!(lookup, IntrinsicMethodLookup::Uncatalogued) {
            return None;
        }
        if self
            .read_own_object_shape_property(receiver, name)
            .is_some()
            && !Self::has_regexp_prototype_shape(receiver)
        {
            return None;
        }
        let property = self.read_current_object_shape_property(receiver, name);
        self.read_intrinsic_object_property_lookup(lookup, receiver, key, property.as_ref())
    }

    pub(super) fn intrinsic_object_symbol_method_read(
        &mut self,
        receiver: &TypedExpr,
        symbol: WellKnownSymbol,
        key: PropertyKeyIr,
    ) -> Option<TypedExpr> {
        let name = shape_namespace_key(symbol);
        if receiver
            .heap_shape
            .as_deref()
            .and_then(|shape| own_shape_property(shape, &name))
            .is_some()
            && !Self::has_regexp_prototype_shape(receiver)
            && !Self::has_array_prototype_shape(receiver)
        {
            return None;
        }
        let property = self.read_current_object_symbol_shape_property(receiver, symbol);
        let array_iterator = StandardBuiltinId::ArrayPrototypeValues;
        let (prototype, builtin) = match symbol {
            WellKnownSymbol::Iterator
                if receiver.possible_kinds.contains(ValueKind::Array)
                    || matches!(&property, Some(ObjectShapeProperty::Data(info))
                        if info.function_targets.exact_single_target()
                            == Some(&array_iterator.function_id())) =>
            {
                (IntrinsicPrototype::Array, array_iterator)
            }
            WellKnownSymbol::Match => (
                IntrinsicPrototype::RegExp,
                StandardBuiltinId::RegExpPrototypeSymbolMatch,
            ),
            WellKnownSymbol::MatchAll => (
                IntrinsicPrototype::RegExp,
                StandardBuiltinId::RegExpPrototypeSymbolMatchAll,
            ),
            WellKnownSymbol::Replace => (
                IntrinsicPrototype::RegExp,
                StandardBuiltinId::RegExpPrototypeSymbolReplace,
            ),
            WellKnownSymbol::Search => (
                IntrinsicPrototype::RegExp,
                StandardBuiltinId::RegExpPrototypeSymbolSearch,
            ),
            WellKnownSymbol::Split => (
                IntrinsicPrototype::RegExp,
                StandardBuiltinId::RegExpPrototypeSymbolSplit,
            ),
            _ => return None,
        };
        let lookup = self.intrinsic_property_method(prototype, &name, builtin, false);
        self.read_intrinsic_object_property_lookup(lookup, receiver, key, property.as_ref())
    }

    fn read_intrinsic_object_property_lookup(
        &mut self,
        lookup: IntrinsicMethodLookup,
        receiver: &TypedExpr,
        key: PropertyKeyIr,
        property: Option<&ObjectShapeProperty>,
    ) -> Option<TypedExpr> {
        let builtin = match lookup {
            IntrinsicMethodLookup::Proven(method) => method.builtin(),
            IntrinsicMethodLookup::Unproven(builtin) => builtin,
            IntrinsicMethodLookup::Uncatalogued => return None,
        };
        let lookup = match property {
            Some(ObjectShapeProperty::Data(info))
                if info.function_targets.exact_single_target() == Some(&builtin.function_id()) =>
            {
                lookup
            }
            Some(_) => return None,
            None => lookup.unclaimed(),
        };
        let candidates = property
            .is_none()
            .then(|| {
                let name = match &key {
                    PropertyKeyIr::StaticString(name) => Some(name.clone()),
                    PropertyKeyIr::StringExpr(key) => match key.expr {
                        ExprIr::WellKnownSymbol(symbol) => Some(shape_namespace_key(symbol)),
                        _ => None,
                    },
                    PropertyKeyIr::ArrayIndex(_) | PropertyKeyIr::ArrayLength => None,
                };
                name.map(|name| {
                    self.unproven_object_property_info(receiver, &name)
                        .function_targets
                })
            })
            .flatten();
        let mut read = self.read_intrinsic_method_lookup(lookup, receiver, key)?;
        if let Some(candidates) = candidates {
            // An unproven fallback does not disprove the receiver's earlier
            // descriptor. Retain both native bodies without claiming either
            // one is the property that the runtime Get will acquire.
            read.function_targets = read.function_targets.join(candidates);
        }
        Some(read)
    }

    /// Instance shapes retain prototype snapshots, not the mutable prototype's
    /// current contents. Provenance requires every catalogue snapshot to agree
    /// with its live intrinsic before it licenses a descriptor. Never substitute
    /// a changed live descriptor: an alias may refer to a different Realm.
    pub(super) fn read_current_object_shape_property(
        &self,
        receiver: &TypedExpr,
        name: &str,
    ) -> Option<ObjectShapeProperty> {
        if shape_property_name_is_symbol_keyed(name) {
            return None;
        }
        self.read_current_heap_shape_property(receiver.heap_shape.as_deref()?, name)
    }

    pub(super) fn read_current_object_symbol_shape_property(
        &self,
        receiver: &TypedExpr,
        symbol: WellKnownSymbol,
    ) -> Option<ObjectShapeProperty> {
        self.read_current_heap_shape_property(
            receiver.heap_shape.as_deref()?,
            &shape_namespace_key(symbol),
        )
    }

    pub(super) fn read_current_heap_shape_property(
        &self,
        mut shape: &HeapShape,
        name: &str,
    ) -> Option<ObjectShapeProperty> {
        loop {
            let provenance = match shape {
                HeapShape::Object(object) => object.provenance,
                HeapShape::Array(array) => array.provenance,
            };
            match provenance {
                HeapShapeProvenance::Program => {}
                HeapShapeProvenance::IntrinsicPrototype(constructor) => {
                    if constructor == StandardBuiltinId::ArrayConstructor
                        && self.array_prototype_mutated
                    {
                        return None;
                    }
                    let live = self.live_intrinsic_prototype(constructor)?;
                    let live_shape = live.heap_shape.as_deref()?;
                    let expected = own_shape_property(shape, name);
                    if own_shape_property(live_shape, name) != expected {
                        return None;
                    }
                    if let Some(expected) = expected {
                        return Some(expected.clone());
                    }
                    // A descriptor reached through a parent must validate that
                    // parent separately. Comparing an entire stale chain with
                    // another stale chain is not a proof of current contents.
                    if shape_prototype(live_shape) != shape_prototype(shape) {
                        return None;
                    }
                }
                HeapShapeProvenance::UntrackedIntrinsicPrototype => return None,
            }
            if let Some(property) = own_shape_property(shape, name) {
                return Some(property.clone());
            }
            let prototype = match shape {
                HeapShape::Object(object) => object.prototype.as_deref(),
                HeapShape::Array(array) => {
                    if array.prototype.is_none() {
                        return self.read_current_heap_shape_property(
                            &Self::array_prototype_shape(),
                            name,
                        );
                    }
                    array.prototype.as_deref()
                }
            };
            shape = prototype?;
        }
    }

    /// Stale descriptors still identify possible callees that codegen must emit,
    /// but they prove neither the result kind nor that Get avoids user code.
    pub(super) fn unproven_object_property_info(
        &self,
        receiver: &TypedExpr,
        name: &str,
    ) -> ValueInfo {
        let property = receiver.heap_shape.as_deref().and_then(|shape| {
            shape_property(shape, name).or_else(|| {
                matches!(shape, HeapShape::Array(array) if array.prototype.is_none())
                    .then(|| shape_property(&Self::array_prototype_shape(), name))
                    .flatten()
            })
        });
        let mut info = match property {
            Some(ObjectShapeProperty::Data(info)) => info,
            Some(ObjectShapeProperty::Accessor {
                getter: Some(getter),
                ..
            }) => self.accessor_return_info(&getter.function_id),
            Some(ObjectShapeProperty::Accessor { getter: None, .. }) | None => {
                unknown_runtime_value_info()
            }
        };
        info.widen_for_possible_replacement();
        info
    }

    fn read_intrinsic_method_lookup(
        &mut self,
        lookup: IntrinsicMethodLookup,
        receiver: &TypedExpr,
        key: PropertyKeyIr,
    ) -> Option<TypedExpr> {
        let info = lookup.callee_info()?;
        if matches!(lookup, IntrinsicMethodLookup::Unproven(_)) {
            // A replacement can be an accessor. Its Get runs before the
            // following argument, assignment, or expression is lowered.
            self.observe_all_planned_source_as_unknown_property_hooks();
            self.invalidate_unknown_user_code_effects();
        }
        Some(TypedExpr::from_info(
            info,
            ExprIr::PropertyRead {
                target: Box::new(receiver.clone()),
                key,
            },
        ))
    }

    pub(super) fn intrinsic_symbol_description_read(
        &self,
        receiver: &TypedExpr,
    ) -> Option<TypedExpr> {
        let prototype = self.live_intrinsic_prototype(StandardBuiltinId::SymbolConstructor)?;
        let property = own_shape_property(prototype.heap_shape.as_deref()?, "description")?;
        let ObjectShapeProperty::Accessor {
            getter: Some(getter),
            ..
        } = property
        else {
            return None;
        };
        if getter.function_id != StandardBuiltinId::SymbolPrototypeDescriptionGetter.function_id() {
            return None;
        }
        let info = if matches!(receiver.expr, ExprIr::WellKnownSymbol(_)) {
            ValueInfo::new(ValueKind::String)
        } else {
            ValueInfo {
                kind: ValueKind::Dynamic,
                possible_kinds: KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            }
        };
        Some(TypedExpr::from_info(
            info,
            ExprIr::PropertyRead {
                target: Box::new(receiver.clone()),
                key: PropertyKeyIr::StaticString("description".to_string()),
            },
        ))
    }

    /// The recorded value of `%C%.prototype` when the global binding `C` still
    /// denotes the intrinsic `constructor`.
    ///
    /// The global property is the only place a write to the intrinsic
    /// prototype is published, and every event that could change the
    /// prototype without publishing — an aliasing write, a delete, a
    /// `defineProperty`, unknown user code — erases this recorded shape. A
    /// snapshot of the prototype taken from anywhere else, such as the
    /// catalogue's fresh-realm shape, is not evidence about the current
    /// program point.
    pub(super) fn live_intrinsic_prototype(
        &self,
        constructor: StandardBuiltinId,
    ) -> Option<&ValueInfo> {
        let constructor_info = if let Some(name) = constructor.global_name() {
            &self
                .global_properties
                .get(name)
                .filter(|property| {
                    property.proven_present && property.source == GlobalPropertySource::Builtin
                })?
                .value_info
        } else {
            let (namespace, member) = [
                (INTL_NAME, INTL_NAMESPACE_CONSTRUCTORS),
                (TEMPORAL_NAME, TEMPORAL_NAMESPACE_CONSTRUCTORS),
            ]
            .into_iter()
            .find_map(|(namespace, members)| {
                members
                    .iter()
                    .find(|(_, builtin)| *builtin == constructor)
                    .map(|(member, _)| (namespace, *member))
            })?;
            let namespace = &self
                .global_properties
                .get(namespace)
                .filter(|property| {
                    property.proven_present && property.source == GlobalPropertySource::Builtin
                })?
                .value_info;
            match own_shape_property(namespace.heap_shape.as_deref()?, member)? {
                ObjectShapeProperty::Data(info) => info,
                ObjectShapeProperty::Accessor { .. } => return None,
            }
        };
        if constructor_info.function_targets.exact_single_target()
            != Some(&constructor.function_id())
        {
            return None;
        }
        let HeapShape::Object(constructor_shape) = constructor_info.heap_shape.as_deref()? else {
            return None;
        };
        match constructor_shape.properties.get("prototype")? {
            ObjectShapeProperty::Data(prototype) => Some(prototype),
            ObjectShapeProperty::Accessor { .. } => None,
        }
    }

    fn intrinsic_prototype_still_holds(
        &self,
        prototype: IntrinsicPrototype,
        name: &str,
        builtin: StandardBuiltinId,
    ) -> bool {
        let Some(live) = self.live_intrinsic_prototype(prototype.constructor()) else {
            return false;
        };
        // `%Function.prototype%` is itself callable; the other prototypes are
        // ordinary objects. A recorded value of another identity is not the
        // intrinsic whatever its shape says.
        let expected_identity = (prototype == IntrinsicPrototype::Function)
            .then(|| StandardBuiltinId::FunctionPrototype.function_id());
        if live.function_targets.exact_single_target() != expected_identity.as_ref() {
            return false;
        }
        let Some(live_shape) = live.heap_shape.as_deref() else {
            return false;
        };
        match own_shape_property(live_shape, name) {
            Some(ObjectShapeProperty::Data(method)) => {
                method.function_targets.exact_single_target() == Some(&builtin.function_id())
            }
            Some(ObjectShapeProperty::Accessor { .. }) => false,
            // The fresh-realm shape omits some catalogued methods. A write
            // publishes an explicit fact, while delete and unknown effects
            // erase the shape, so absence is the intrinsic state exactly when
            // the fresh-realm shape omits the name too.
            None => {
                let pristine = Self::standard_builtin_function_shape(prototype.constructor());
                match own_shape_property(&pristine, "prototype") {
                    Some(ObjectShapeProperty::Data(prototype)) => prototype
                        .heap_shape
                        .as_deref()
                        .is_some_and(|shape| own_shape_property(shape, name).is_none()),
                    Some(ObjectShapeProperty::Accessor { .. }) | None => false,
                }
            }
        }
    }
}

fn own_shape_property<'shape>(
    shape: &'shape HeapShape,
    name: &str,
) -> Option<&'shape ObjectShapeProperty> {
    match shape {
        HeapShape::Object(object) => object.properties.get(name),
        HeapShape::Array(array) => array.properties.get(name),
    }
}

fn shape_property(shape: &HeapShape, name: &str) -> Option<ObjectShapeProperty> {
    own_shape_property(shape, name)
        .cloned()
        .or_else(|| shape_property(shape_prototype(shape)?, name))
}

fn shape_prototype(shape: &HeapShape) -> Option<&HeapShape> {
    match shape {
        HeapShape::Object(object) => object.prototype.as_deref(),
        HeapShape::Array(array) => array.prototype.as_deref(),
    }
}
