//! Opt-in, declaration-derived host projections. No JavaScript operation is
//! emitted by these witnesses or by the retained Realm inventory.
use super::*;
use scriptw_runtime::rooted_snapshot::{SnapshotIntrinsic, SnapshotWellKnownSymbol};

macro_rules! snapshot_layouts {
    ($($name:ident => $target:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum GcSnapshotLayout { $($name,)+ }
        impl GcSnapshotLayout {
            pub const ALL: &'static [Self] = &[$(Self::$name,)+];
            pub const fn witness_export(self) -> &'static str {
                match self { $(Self::$name => concat!("scriptw_snapshot_type_", stringify!($name)),)+ }
            }
            fn layout(self) -> GcLayout { match self { $(Self::$name => $target::LAYOUT,)+ } }
        }
    };
}
snapshot_layouts!(PropertyValue => PropertyValue, Stored => StoredValue, String => StringValue, CodeUnits => CodeUnitArray,
    BigInt => BigIntValue, Limbs => BigIntLimbArray, Symbol => SymbolValue,
    Object => OrdinaryObject, PropertyStorage => OrdinaryPropertyStorage,
    CompactStorage => CompactDataStorage, CompactShape => CompactDataShape,
    CompactKeys => CompactShapeKeys, CompactTags => CompactTagArray,
    CompactScalars => CompactScalarArray, CompactReferences => CompactReferenceArray,
    Properties => PropertyTable, Property => PropertyEntry,
    Descriptor => PropertyDescriptor, PrivateElements => PrivateElementTable,
    Array => ArrayObject, ArrayStorage => ArrayIndexStorage, ArrayBuckets => ArrayIndexBucketTable,
    ArrayEntry => ArrayIndexEntry, ArrayDense => IndexedTable,
    ArrayValues => PackedValueTable,
    ArrayNumbers => NumberBitsI64Array, ArrayNumberPresence => NumberPresenceU64Array,
    Function => FunctionObject, Code => ExecutableCode,
    Context => FunctionContext, Realm => RealmRecord, Intrinsics => IntrinsicTable,
    Realms => SnapshotRealmInventory, WellKnownSymbols => WellKnownSymbolTable,
    Proxy => ProxyObject, BoundFunction => BoundFunction, Arguments => ArgumentsObject, Boxed => PrimitiveBox,
    Promise => PromiseObject, Error => NativeErrorObject);

pub const SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT: &str = "scriptw_snapshot_resolve_property_value";

pub const SNAPSHOT_ENTRY_REALM_EXPORT: &str = "scriptw_snapshot_entry_realm";
pub const SNAPSHOT_REALMS_EXPORT: &str = "scriptw_snapshot_realms";
pub const SNAPSHOT_SYMBOLS_EXPORT: &str = "scriptw_snapshot_well_known_symbols";

macro_rules! snapshot_fields {
    ($($name:ident => $owner:ident, $schema:ident::$field:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy)]
        pub enum GcSnapshotField { $($name,)+ }
        impl GcSnapshotField {
            pub const fn layout(self) -> GcSnapshotLayout { match self { $(Self::$name => GcSnapshotLayout::$owner,)+ } }
            pub const fn index(self) -> u32 { match self { $(Self::$name => $schema::$field.ordinal().raw(),)+ } }
        }
    };
}
snapshot_fields!(StoredTag => Stored, StoredValueSchema::TAG,
    StoredScalar => Stored, StoredValueSchema::SCALAR, StoredRef => Stored, StoredValueSchema::REFERENCE,
    StringUnits => String, StringValueSchema::CODE_UNITS, BigIntSign => BigInt, BigIntValueSchema::NEGATIVE,
    BigIntLimbs => BigInt, BigIntValueSchema::LIMBS,
    SymbolDescription => Symbol, SymbolValueSchema::DESCRIPTION, SymbolRegistry => Symbol, SymbolValueSchema::REGISTRY_KEY,
    ObjectPrototype => Object, OrdinaryObjectSchema::PROTOTYPE, ObjectProperties => Object, OrdinaryObjectSchema::PROPERTIES,
    ObjectPrivate => Object, OrdinaryObjectSchema::PRIVATE_ELEMENTS, ObjectExtensible => Object, OrdinaryObjectSchema::EXTENSIBLE,
    CompactShape => CompactStorage, CompactDataStorageSchema::SHAPE,
    CompactTags => CompactStorage, CompactDataStorageSchema::TAGS,
    CompactScalars => CompactStorage, CompactDataStorageSchema::SCALARS,
    CompactReferences => CompactStorage, CompactDataStorageSchema::REFERENCES,
    CompactKeys => CompactShape, CompactDataShapeSchema::KEYS,
    CompactAttributes => CompactShape, CompactDataShapeSchema::ATTRIBUTES,
    PropertiesEntries => PropertyStorage, OrdinaryPropertyStorageSchema::ENTRIES,
    PropertiesLength => PropertyStorage, OrdinaryPropertyStorageSchema::LENGTH,
    PropertyKey => Property, PropertyEntrySchema::KEY, PropertyDescriptor => Property, PropertyEntrySchema::DESCRIPTOR,
    DescriptorFlags => Descriptor, PropertyDescriptorSchema::FLAGS, DescriptorValue => Descriptor, PropertyDescriptorSchema::VALUE,
    DescriptorGet => Descriptor, PropertyDescriptorSchema::GETTER, DescriptorSet => Descriptor, PropertyDescriptorSchema::SETTER,
    ArrayObject => Array, ArrayObjectSchema::OBJECT, ArrayStorage => Array, ArrayObjectSchema::ELEMENTS,
    ArrayLength => Array, ArrayObjectSchema::LENGTH, ArrayLengthWritable => Array, ArrayObjectSchema::LENGTH_WRITABLE,
    ArrayBuckets => ArrayStorage, ArrayIndexStorageSchema::BUCKETS, ArrayCount => ArrayStorage, ArrayIndexStorageSchema::COUNT,
    ArrayDense => ArrayStorage, ArrayIndexStorageSchema::DENSE,
    ArrayMode => ArrayStorage, ArrayIndexStorageSchema::MODE,
    ArrayValues => ArrayStorage, ArrayIndexStorageSchema::VALUES,
    ArrayNumbers => ArrayStorage, ArrayIndexStorageSchema::NUMBERS,
    ArrayNumberPresent => ArrayStorage, ArrayIndexStorageSchema::NUMBER_PRESENT,
    ArrayIndex => ArrayEntry, ArrayIndexEntrySchema::INDEX, ArrayDescriptor => ArrayEntry, ArrayIndexEntrySchema::DESCRIPTOR,
    ArrayNext => ArrayEntry, ArrayIndexEntrySchema::NEXT,
    FunctionObject => Function, FunctionObjectSchema::OBJECT, FunctionCode => Function, FunctionObjectSchema::CODE,
    FunctionContext => Function, FunctionObjectSchema::CONTEXT, FunctionHtmlDda => Function, FunctionObjectSchema::IS_HTMLDDA,
    ErrorObject => Error, NativeErrorObjectSchema::OBJECT,
    CodeProtocol => Code, ExecutableCodeSchema::PROTOCOL, ContextRealm => Context, FunctionContextSchema::REALM,
    RealmIntrinsics => Realm, RealmRecordSchema::INTRINSICS,
    InventoryRealm => Realms, SnapshotRealmInventorySchema::REALM, InventoryPrevious => Realms, SnapshotRealmInventorySchema::PREVIOUS);

/// Decode the closed indexed-storage mode at the opt-in host projection.
pub fn snapshot_array_storage_mode(code: i32) -> Option<ArrayIndexedStorageMode> {
    [
        ArrayIndexedStorageMode::DenseDescriptors,
        ArrayIndexedStorageMode::SparseDescriptors,
        ArrayIndexedStorageMode::PackedNumbers,
        ArrayIndexedStorageMode::PackedValues,
    ]
    .into_iter()
    .find(|mode| mode.encode() == code)
}

pub fn snapshot_descriptor_flags(word: u32) -> Option<(bool, bool, bool, bool)> {
    use crate::heap::DescriptorMask;
    if word & !DescriptorMask::KIND_AND_ATTRIBUTES.bits() != 0 {
        return None;
    }
    Some((
        word & DescriptorMask::ACCESSOR.bits() != 0,
        word & DescriptorMask::WRITABLE.bits() != 0,
        word & DescriptorMask::ENUMERABLE.bits() != 0,
        word & DescriptorMask::CONFIGURABLE.bits() != 0,
    ))
}

pub fn snapshot_function_protocol(code: i32) -> Option<(scriptw_ir::FunctionExecutionKind, bool)> {
    let protocol = FunctionProtocolCode::ALL
        .iter()
        .find(|value| value.encoding() == code)?
        .protocol();
    Some((protocol.execution_kind(), protocol.is_constructable()))
}

pub fn snapshot_intrinsic_index(intrinsic: SnapshotIntrinsic) -> usize {
    use crate::functions::NonArrayRealmIntrinsicSlot as S;
    use SnapshotIntrinsic::*;
    (match intrinsic {
        ArrayPrototype => S::ARRAY_INDEX,
        ObjectConstructor => S::ObjectConstructor.gc_index(),
        ObjectPrototype => S::ObjectPrototype.gc_index(),
        FunctionConstructor => S::FunctionConstructor.gc_index(),
        FunctionPrototype => S::FunctionPrototype.gc_index(),
        ArrayConstructor => S::ArrayConstructor.gc_index(),
        ErrorConstructor => S::ErrorConstructor.gc_index(),
        ErrorPrototype => S::ErrorPrototype.gc_index(),
        EvalErrorConstructor => S::EvalErrorConstructor.gc_index(),
        EvalErrorPrototype => S::EvalErrorPrototype.gc_index(),
        RangeErrorConstructor => S::RangeErrorConstructor.gc_index(),
        RangeErrorPrototype => S::RangeErrorPrototype.gc_index(),
        ReferenceErrorConstructor => S::ReferenceErrorConstructor.gc_index(),
        ReferenceErrorPrototype => S::ReferenceErrorPrototype.gc_index(),
        SyntaxErrorConstructor => S::SyntaxErrorConstructor.gc_index(),
        SyntaxErrorPrototype => S::SyntaxErrorPrototype.gc_index(),
        TypeErrorConstructor => S::TypeErrorConstructor.gc_index(),
        TypeErrorPrototype => S::TypeErrorPrototype.gc_index(),
        UriErrorConstructor => S::URIErrorConstructor.gc_index(),
        UriErrorPrototype => S::URIErrorPrototype.gc_index(),
        AggregateErrorConstructor => S::AggregateErrorConstructor.gc_index(),
        AggregateErrorPrototype => S::AggregateErrorPrototype.gc_index(),
    }) as usize
}
pub fn snapshot_symbol_index(symbol: SnapshotWellKnownSymbol) -> usize {
    use scriptw_ir::WellKnownSymbol as S;
    let symbol = match symbol {
        SnapshotWellKnownSymbol::AsyncIterator => S::AsyncIterator,
        SnapshotWellKnownSymbol::HasInstance => S::HasInstance,
        SnapshotWellKnownSymbol::IsConcatSpreadable => S::IsConcatSpreadable,
        SnapshotWellKnownSymbol::Iterator => S::Iterator,
        SnapshotWellKnownSymbol::Match => S::Match,
        SnapshotWellKnownSymbol::MatchAll => S::MatchAll,
        SnapshotWellKnownSymbol::Replace => S::Replace,
        SnapshotWellKnownSymbol::Search => S::Search,
        SnapshotWellKnownSymbol::Species => S::Species,
        SnapshotWellKnownSymbol::Split => S::Split,
        SnapshotWellKnownSymbol::ToPrimitive => S::ToPrimitive,
        SnapshotWellKnownSymbol::ToStringTag => S::ToStringTag,
        SnapshotWellKnownSymbol::Unscopables => S::Unscopables,
        SnapshotWellKnownSymbol::Dispose => S::Dispose,
        SnapshotWellKnownSymbol::AsyncDispose => S::AsyncDispose,
    };
    symbol as usize
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SnapshotRoots {
    entry: GcRootGlobal<RealmRecord>,
    inventory: GcRootGlobal<SnapshotRealmInventory>,
    witnesses: Vec<(GcSnapshotLayout, u32)>,
}
impl SnapshotRoots {
    pub(super) fn declare(layouts: &GcLayoutRegistry, globals: &mut GlobalLedger) -> Self {
        let entry = GcRootGlobal::declare(layouts, globals);
        let inventory = GcRootGlobal::declare(layouts, globals);
        let witnesses = GcSnapshotLayout::ALL
            .iter()
            .map(|layout| {
                let index = globals.len();
                let reference = layouts.reference(layout.layout(), GcNullability::Nullable);
                globals.global(
                    GlobalType {
                        val_type: ValType::Ref(reference),
                        mutable: false,
                        shared: false,
                    },
                    &ConstExpr::ref_null(reference.heap_type),
                );
                (*layout, index)
            })
            .collect();
        Self {
            entry,
            inventory,
            witnesses,
        }
    }
}
impl RuntimeSchema {
    pub(crate) fn export_snapshot_roots(
        &self,
        exports: &mut wasm_encoder::ExportSection,
        helpers: crate::runtime_helpers::RuntimeHelperFunctionBase,
    ) {
        let Some(roots) = &self.snapshot else {
            return;
        };
        exports.export(
            SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT,
            wasm_encoder::ExportKind::Func,
            crate::runtime_helpers::RuntimeHelperId::PropertyValueResolve.index(helpers),
        );
        for (layout, index) in &roots.witnesses {
            exports.export(
                layout.witness_export(),
                wasm_encoder::ExportKind::Global,
                *index,
            );
        }
        exports.export(
            SNAPSHOT_ENTRY_REALM_EXPORT,
            wasm_encoder::ExportKind::Global,
            roots.entry.raw(),
        );
        exports.export(
            SNAPSHOT_REALMS_EXPORT,
            wasm_encoder::ExportKind::Global,
            roots.inventory.raw(),
        );
        exports.export(
            SNAPSHOT_SYMBOLS_EXPORT,
            wasm_encoder::ExportKind::Global,
            self.well_known_symbols.raw(),
        );
    }
    pub(crate) fn retain_snapshot_realm(
        &self,
        realm: GcStackReference<RealmRecord>,
        function: &mut Function,
    ) -> GcStackReference<RealmRecord> {
        let Some(roots) = &self.snapshot else {
            return realm;
        };
        let realm = self.reserve_gc_local(function).initialize(realm, function);
        emit_root_get(function, roots.entry);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        realm.load(self, function);
        emit_root_set(function, roots.entry);
        function.instruction(&Instruction::End);
        let previous = self
            .reserve_gc_local::<SnapshotRealmInventory, Nullable>(function)
            .initialize(roots.inventory.load_nullable(function), function);
        let inventory = self.struct_type::<SnapshotRealmInventory>().construct(
            (
                GcOperand::reference(&realm, self),
                GcOperand::reference(&previous, self),
            ),
            function,
        );
        roots.inventory.publish_snapshot_record(inventory, function);
        previous.clear(function);
        let result = realm.load(self, function);
        realm.clear(function);
        result
    }
}

impl<T: GcHeapType> GcRootGlobal<T> {
    fn publish_snapshot_record(self, _record: GcStackReference<T>, function: &mut Function) {
        emit_root_set(function, self);
    }
}
