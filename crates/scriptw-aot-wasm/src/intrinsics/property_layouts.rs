//! Complete initial ordinary-property layouts. These are compiler metadata,
//! not JavaScript instructions; mutation leaves them for the ordinary dictionary.
use super::{IntrinsicKey, IntrinsicMethodPlan, IntrinsicMethodSpec};
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::heap::{DescriptorWord, StoredPropertyAttributes};
use crate::EmitError;
use scriptw_ir::{StandardBuiltinId, WellKnownSymbol};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub(crate) enum IntrinsicPropertyLayoutId {
    Reflect,
    Math,
    Json,
    Atomics,
    ObjectPrototype,
    ArrayPrototype,
    StringPrototype,
    ArrayUnscopables,
}

impl IntrinsicPropertyLayoutId {
    pub(crate) const ALL: [Self; 8] = [
        Self::Reflect,
        Self::Math,
        Self::Json,
        Self::Atomics,
        Self::ObjectPrototype,
        Self::ArrayPrototype,
        Self::StringPrototype,
        Self::ArrayUnscopables,
    ];

    pub(crate) fn layout(self) -> Result<ValidatedIntrinsicPropertyLayout, EmitError> {
        let mut plan = LayoutBuilder::default();
        match self {
            Self::Reflect => {
                plan.methods(IntrinsicMethodPlan::Reflect)?;
                plan.tag("Reflect");
            }
            Self::Math => {
                for (name, number) in [
                    ("E", std::f64::consts::E),
                    ("LN10", std::f64::consts::LN_10),
                    ("LN2", std::f64::consts::LN_2),
                    ("LOG10E", std::f64::consts::LOG10_E),
                    ("LOG2E", std::f64::consts::LOG2_E),
                    ("PI", std::f64::consts::PI),
                    ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
                    ("SQRT2", std::f64::consts::SQRT_2),
                ] {
                    plan.data(
                        IntrinsicKey::Name(name),
                        IntrinsicPropertyRecipe::Number(number.to_bits()),
                        false,
                        false,
                        false,
                    );
                }
                plan.methods(IntrinsicMethodPlan::Math)?;
                plan.tag("Math");
            }
            Self::Json => {
                plan.tag("JSON");
                plan.methods(IntrinsicMethodPlan::Json)?;
            }
            Self::Atomics => {
                plan.tag("Atomics");
                for builtin in crate::builtins::ATOMICS_PUBLICATION_ORDER {
                    let name = builtin.native_function_name().ok_or_else(|| {
                        EmitError::unsupported("Atomics layout method has no native name")
                    })?;
                    plan.method(IntrinsicMethodSpec {
                        key: IntrinsicKey::Name(name),
                        builtin,
                        writable: true,
                        configurable: true,
                    });
                }
            }
            Self::ObjectPrototype => {
                plan.constructor(NonArrayRealmIntrinsicSlot::ObjectConstructor);
                plan.methods(IntrinsicMethodPlan::ObjectPrototypeBeforeAccessor)?;
                plan.accessor(
                    IntrinsicKey::Name("__proto__"),
                    StandardBuiltinId::ObjectPrototypeProtoGetter,
                    StandardBuiltinId::ObjectPrototypeProtoSetter,
                );
                plan.methods(IntrinsicMethodPlan::ObjectPrototypeAfterAccessor)?;
            }
            Self::ArrayPrototype => {
                // Array length remains in the Array exotic storage.
                plan.constructor(NonArrayRealmIntrinsicSlot::ArrayConstructor);
                plan.methods(IntrinsicMethodPlan::ArrayPrototype)?;
                plan.aliases(
                    &[
                        IntrinsicKey::Name("values"),
                        IntrinsicKey::Symbol(WellKnownSymbol::Iterator),
                    ],
                    IntrinsicPropertyRecipe::RealmIntrinsic(
                        NonArrayRealmIntrinsicSlot::ArrayPrototypeValues,
                    ),
                );
                plan.data(
                    IntrinsicKey::Symbol(WellKnownSymbol::Unscopables),
                    IntrinsicPropertyRecipe::LayoutObject(Self::ArrayUnscopables),
                    false,
                    false,
                    true,
                );
            }
            Self::StringPrototype => {
                // The current boxed String prototype owns length before constructor.
                plan.data(
                    IntrinsicKey::Name("length"),
                    IntrinsicPropertyRecipe::Number(0.0f64.to_bits()),
                    false,
                    false,
                    false,
                );
                plan.constructor(NonArrayRealmIntrinsicSlot::StringConstructor);
                plan.methods(IntrinsicMethodPlan::StringPrototypeBeforeAliases)?;
                for (names, builtin) in [
                    (
                        ["trimStart", "trimLeft"],
                        StandardBuiltinId::StringPrototypeTrimStart,
                    ),
                    (
                        ["trimEnd", "trimRight"],
                        StandardBuiltinId::StringPrototypeTrimEnd,
                    ),
                ] {
                    plan.aliases(
                        &[IntrinsicKey::Name(names[0]), IntrinsicKey::Name(names[1])],
                        IntrinsicPropertyRecipe::Method(builtin),
                    );
                }
                plan.methods(IntrinsicMethodPlan::StringPrototypeAfterAliases)?;
            }
            Self::ArrayUnscopables => {
                for name in [
                    "at",
                    "copyWithin",
                    "entries",
                    "fill",
                    "find",
                    "findIndex",
                    "findLast",
                    "findLastIndex",
                    "flat",
                    "flatMap",
                    "includes",
                    "keys",
                    "toReversed",
                    "toSorted",
                    "toSpliced",
                    "values",
                ] {
                    plan.data(
                        IntrinsicKey::Name(name),
                        IntrinsicPropertyRecipe::Boolean(true),
                        true,
                        true,
                        true,
                    );
                }
            }
        }
        ValidatedIntrinsicPropertyLayout::new(plan.properties, plan.groups)
    }
}

/// The wire discriminant is shared by row sealing and the exhaustive emitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum IntrinsicPropertyRecipeKind {
    Number,
    Boolean,
    String,
    Method,
    RealmIntrinsic,
    Accessor,
    LayoutObject,
}
impl IntrinsicPropertyRecipeKind {
    pub(crate) const ALL: [Self; 7] = [
        Self::Number,
        Self::Boolean,
        Self::String,
        Self::Method,
        Self::RealmIntrinsic,
        Self::Accessor,
        Self::LayoutObject,
    ];
}

#[derive(Clone, Copy)]
pub(crate) enum IntrinsicPropertyRecipe {
    Number(u64),
    Boolean(bool),
    String(&'static str),
    Method(StandardBuiltinId),
    RealmIntrinsic(NonArrayRealmIntrinsicSlot),
    Accessor {
        getter: StandardBuiltinId,
        setter: StandardBuiltinId,
    },
    LayoutObject(IntrinsicPropertyLayoutId),
}
impl IntrinsicPropertyRecipe {
    pub(crate) fn kind(self) -> IntrinsicPropertyRecipeKind {
        match self {
            Self::Number(_) => IntrinsicPropertyRecipeKind::Number,
            Self::Boolean(_) => IntrinsicPropertyRecipeKind::Boolean,
            Self::String(_) => IntrinsicPropertyRecipeKind::String,
            Self::Method(_) => IntrinsicPropertyRecipeKind::Method,
            Self::RealmIntrinsic(_) => IntrinsicPropertyRecipeKind::RealmIntrinsic,
            Self::Accessor { .. } => IntrinsicPropertyRecipeKind::Accessor,
            Self::LayoutObject(_) => IntrinsicPropertyRecipeKind::LayoutObject,
        }
    }
    fn identity(self) -> (u32, u64, u64) {
        let (first, second) = match self {
            Self::Number(bits) => (bits, 0),
            Self::Boolean(value) => (u64::from(value), 0),
            Self::String(_) => (0, 0),
            Self::Method(builtin) => (builtin as u64, 0),
            Self::RealmIntrinsic(slot) => (u64::from(slot.gc_index()), 0),
            Self::Accessor { getter, setter } => (getter as u64, setter as u64),
            Self::LayoutObject(layout) => (layout as u64, 0),
        };
        (self.kind() as u32, first, second)
    }
}

/// A cache slot is local to one complete object's installation, never a builtin ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IntrinsicIdentityGroup(u32);
impl IntrinsicIdentityGroup {
    pub(crate) fn index(self) -> u32 {
        self.0
    }
}
#[derive(Clone, Copy)]
pub(crate) struct IntrinsicPropertySpec {
    pub(crate) key: IntrinsicKey<'static>,
    pub(crate) flags: DescriptorWord,
    pub(crate) group: IntrinsicIdentityGroup,
    pub(crate) recipe: IntrinsicPropertyRecipe,
}
/// Complete immutable named properties, with no canonical Array-index keys.
/// Packed append can therefore skip indexed descriptor probes for this storage.
pub(crate) struct ValidatedIntrinsicPropertyLayout {
    properties: Vec<IntrinsicPropertySpec>,
    group_count: u32,
}
impl ValidatedIntrinsicPropertyLayout {
    fn new(properties: Vec<IntrinsicPropertySpec>, group_count: u32) -> Result<Self, EmitError> {
        let error = || EmitError::unsupported("invalid complete intrinsic property layout");
        if properties.is_empty() || group_count == 0 {
            return Err(error());
        }
        let mut names = BTreeSet::new();
        let mut symbols = BTreeSet::new();
        let mut groups = BTreeMap::new();
        for property in &properties {
            let unique = match property.key {
                IntrinsicKey::Name(name) => {
                    let units = scriptw_ir::decode_js_string_utf16(name);
                    if is_array_index(&units) {
                        return Err(error());
                    }
                    names.insert(units)
                }
                IntrinsicKey::Symbol(symbol) => symbols.insert(symbol),
            };
            if !unique || property.group.0 >= group_count {
                return Err(error());
            }
            if let Some(previous) = groups.insert(property.group.0, property.recipe) {
                if previous.identity() != property.recipe.identity()
                    || matches!((previous, property.recipe), (IntrinsicPropertyRecipe::String(a), IntrinsicPropertyRecipe::String(b)) if a != b)
                {
                    return Err(error());
                }
            }
            if let IntrinsicPropertyRecipe::Accessor { .. } = property.recipe {
                let setter = property.group.0.checked_add(1).ok_or_else(error)?;
                if setter >= group_count || groups.insert(setter, property.recipe).is_some() {
                    return Err(error());
                }
            }
        }
        if groups.len() != group_count as usize {
            return Err(error());
        }
        Ok(Self {
            properties,
            group_count,
        })
    }
    pub(crate) fn properties(&self) -> &[IntrinsicPropertySpec] {
        &self.properties
    }
    pub(crate) fn group_count(&self) -> u32 {
        self.group_count
    }
}

// Classify the semantic UTF-16 key, just as the runtime indexed-key owner does.
fn is_array_index(units: &[u16]) -> bool {
    if units.is_empty() || units.len() > 10 || (units.len() > 1 && units[0] == 48) {
        return false;
    }
    let value = units.iter().try_fold(0_u32, |value, &unit| {
        let digit = unit.checked_sub(48).filter(|digit| *digit < 10)?;
        value.checked_mul(10)?.checked_add(u32::from(digit))
    });
    matches!(value, Some(value) if value != u32::MAX)
}

#[derive(Default)]
struct LayoutBuilder {
    properties: Vec<IntrinsicPropertySpec>,
    groups: u32,
}
impl LayoutBuilder {
    fn data(
        &mut self,
        key: IntrinsicKey<'static>,
        recipe: IntrinsicPropertyRecipe,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) {
        let group = IntrinsicIdentityGroup(self.groups);
        self.groups += 1;
        self.properties.push(IntrinsicPropertySpec {
            key,
            recipe,
            group,
            flags: StoredPropertyAttributes::Data {
                writable,
                enumerable,
                configurable,
            }
            .descriptor_word(),
        });
    }
    fn method(&mut self, method: IntrinsicMethodSpec) {
        let recipe = match crate::module::standard_builtin_function_realm_slot(method.builtin) {
            Some(slot) => IntrinsicPropertyRecipe::RealmIntrinsic(slot),
            None => IntrinsicPropertyRecipe::Method(method.builtin),
        };
        self.data(
            method.key,
            recipe,
            method.writable,
            false,
            method.configurable,
        );
    }
    fn methods(&mut self, plan: IntrinsicMethodPlan) -> Result<(), EmitError> {
        for &method in plan.methods()?.iter() {
            self.method(method);
        }
        Ok(())
    }
    fn tag(&mut self, text: &'static str) {
        self.data(
            IntrinsicKey::Symbol(WellKnownSymbol::ToStringTag),
            IntrinsicPropertyRecipe::String(text),
            false,
            false,
            true,
        );
    }
    fn constructor(&mut self, slot: NonArrayRealmIntrinsicSlot) {
        self.data(
            IntrinsicKey::Name("constructor"),
            IntrinsicPropertyRecipe::RealmIntrinsic(slot),
            true,
            false,
            true,
        );
    }
    fn aliases(&mut self, keys: &[IntrinsicKey<'static>], recipe: IntrinsicPropertyRecipe) {
        let group = IntrinsicIdentityGroup(self.groups);
        self.groups += 1;
        for &key in keys {
            self.properties.push(IntrinsicPropertySpec {
                key,
                recipe,
                group,
                flags: StoredPropertyAttributes::Data {
                    writable: true,
                    enumerable: false,
                    configurable: true,
                }
                .descriptor_word(),
            });
        }
    }
    fn accessor(
        &mut self,
        key: IntrinsicKey<'static>,
        getter: StandardBuiltinId,
        setter: StandardBuiltinId,
    ) {
        let group = IntrinsicIdentityGroup(self.groups);
        self.groups += 2;
        self.properties.push(IntrinsicPropertySpec {
            key,
            group,
            flags: StoredPropertyAttributes::Accessor {
                enumerable: false,
                configurable: true,
            }
            .descriptor_word(),
            recipe: IntrinsicPropertyRecipe::Accessor { getter, setter },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_layouts_keep_seed_order_and_explicit_alias_identity() {
        for id in IntrinsicPropertyLayoutId::ALL {
            id.layout().unwrap();
        }
        let string = IntrinsicPropertyLayoutId::StringPrototype.layout().unwrap();
        assert!(matches!(
            string.properties()[0].key,
            IntrinsicKey::Name("length")
        ));
        assert!(matches!(
            string.properties()[1].key,
            IntrinsicKey::Name("constructor")
        ));
        let named = |name| {
            string
                .properties()
                .iter()
                .find(|p| matches!(p.key, IntrinsicKey::Name(key) if key == name))
                .unwrap()
        };
        assert_eq!(named("trimStart").group, named("trimLeft").group);
        assert_eq!(named("trimEnd").group, named("trimRight").group);
        assert_ne!(named("trimStart").group, named("trimEnd").group);
        let array = IntrinsicPropertyLayoutId::ArrayPrototype.layout().unwrap();
        let values = array
            .properties()
            .iter()
            .find(|p| matches!(p.key, IntrinsicKey::Name("values")))
            .unwrap();
        let iterator = array
            .properties()
            .iter()
            .find(|p| matches!(p.key, IntrinsicKey::Symbol(WellKnownSymbol::Iterator)))
            .unwrap();
        assert_eq!(values.group, iterator.group);
        assert!(
            matches!(values.recipe, IntrinsicPropertyRecipe::RealmIntrinsic(slot) if slot.gc_index() == NonArrayRealmIntrinsicSlot::ArrayPrototypeValues.gc_index())
        );
    }
    #[test]
    fn validation_rejects_duplicate_keys_and_aliases_to_different_recipes() {
        let mut duplicate = LayoutBuilder::default();
        duplicate.tag("a");
        duplicate.tag("b");
        assert!(
            ValidatedIntrinsicPropertyLayout::new(duplicate.properties, duplicate.groups).is_err()
        );
        for (first, second) in [
            ("push", "push"),
            ("\u{F0000}", "\u{F0000}\u{F0000}"),
            ("\u{F0000}D800", "\u{F0000}d800"),
        ] {
            let mut duplicate = LayoutBuilder::default();
            for name in [first, second] {
                duplicate.data(
                    IntrinsicKey::Name(name),
                    IntrinsicPropertyRecipe::Number(0),
                    true,
                    true,
                    true,
                );
            }
            assert!(
                ValidatedIntrinsicPropertyLayout::new(duplicate.properties, duplicate.groups)
                    .is_err(),
                "duplicate decoded key: {first:?}, {second:?}",
            );
        }
        let mut aliases = LayoutBuilder::default();
        aliases.data(
            IntrinsicKey::Name("a"),
            IntrinsicPropertyRecipe::Number(0),
            true,
            false,
            true,
        );
        aliases.data(
            IntrinsicKey::Name("b"),
            IntrinsicPropertyRecipe::Boolean(false),
            true,
            false,
            true,
        );
        aliases.properties[1].group = aliases.properties[0].group;
        assert!(ValidatedIntrinsicPropertyLayout::new(aliases.properties, 1).is_err());
    }
    #[test]
    fn validation_excludes_only_canonical_array_index_strings() {
        for (names, valid) in [
            (&["0", "1", "9", "10", "4294967294"][..], false),
            (
                &[
                    "",
                    "00",
                    "01",
                    "-0",
                    "+1",
                    "1.0",
                    "1e0",
                    "4294967295",
                    "4294967296",
                    "١",
                    "\0",
                    "0\0",
                    "0\u{F0000}D800",
                    "\u{F0000}D800",
                    "\u{F0000}DC00",
                    "\u{F0000}DFFF",
                    "\u{10000}",
                    "\u{F0000}",
                    "\u{F0000}\u{F0000}",
                    r"\u0030",
                    r"\x30",
                    r"\u{30}",
                ][..],
                true,
            ),
        ] {
            for &name in names {
                let mut plan = LayoutBuilder::default();
                plan.data(
                    IntrinsicKey::Name(name),
                    IntrinsicPropertyRecipe::Number(0),
                    true,
                    true,
                    true,
                );
                assert_eq!(
                    ValidatedIntrinsicPropertyLayout::new(plan.properties, plan.groups).is_ok(),
                    valid,
                    "{name:?}",
                );
            }
        }
    }
}
