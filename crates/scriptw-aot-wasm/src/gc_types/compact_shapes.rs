//! Private constructors for the compact data shape graph. Shapes retain only
//! primitive string keys and immutable attributes; their transition caches
//! retain only further shapes. Nothing here can observe a value or a Realm.
use super::*;

/// Shapes one runtime may create by transitions. Transition tables are
/// strong, so this bounds the runtime-lifetime shape graph even for programs
/// whose adds keep producing novel key sequences; once it is spent, further
/// novel adds promote to dictionaries while existing transitions still apply.
const COMPACT_SHAPE_BUDGET: i32 = 1 << 14;

/// One immutable root per runtime: the canonical empty default-data shape and
/// the zero-slot storage that every fresh shaped object shares until its first
/// own property. A zero-length tag array has no valid slot, so no store can
/// reach this storage; adding a property always publishes a new storage.
/// The mutable budget counts down the shapes transitions may still create.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct CompactShapeGlobals {
    empty_storage: u32,
    budget: u32,
}

impl CompactShapeGlobals {
    pub(super) fn declare(layouts: &GcLayoutRegistry, globals: &mut GlobalLedger) -> Self {
        let empty_storage = globals.len();
        let attributes = crate::heap::StoredPropertyAttributes::Data {
            writable: true,
            enumerable: true,
            configurable: true,
        }
        .descriptor_word();
        let null = |layout| Instruction::RefNull(layouts.reference(layout, GcNullability::Nullable).heap_type);
        globals.global(
            GlobalType {
                val_type: ValType::Ref(
                    layouts.reference(ArrayCompactDataStorage::LAYOUT, GcNullability::NonNullable),
                ),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                Instruction::ArrayNewFixed {
                    array_type_index: layouts.array_type::<CompactShapeKeys>().type_index().raw(),
                    array_size: 0,
                },
                Instruction::I32Const(attributes.as_i32()),
                Instruction::I32Const(1),
                null(CompactShapeTable::LAYOUT),
                null(OrdinaryPropertyIndex::LAYOUT),
                Instruction::StructNew(layouts.struct_type::<CompactDataShape>().type_index().raw()),
                Instruction::ArrayNewFixed {
                    array_type_index: layouts.array_type::<CompactTagArray>().type_index().raw(),
                    array_size: 0,
                },
                null(CompactScalarArray::LAYOUT),
                null(CompactReferenceArray::LAYOUT),
                Instruction::StructNew(
                    layouts
                        .struct_type::<ArrayCompactDataStorage>()
                        .type_index()
                        .raw(),
                ),
            ]),
        );
        let budget = globals.len();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(COMPACT_SHAPE_BUDGET),
        );
        Self {
            empty_storage,
            budget,
        }
    }
}

impl RuntimeSchema {
    pub(crate) fn empty_compact_storage(
        &self,
        function: &mut Function,
    ) -> GcStackReference<ArrayCompactDataStorage> {
        function.instruction(&Instruction::GlobalGet(self.compact_shapes.empty_storage));
        GcStackReference::new()
    }

    /// Pushes the number of shapes transitions may still create.
    pub(crate) fn compact_shape_budget(&self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.compact_shapes.budget));
    }

    /// Records one created shape; callers first observed a positive budget.
    pub(crate) fn spend_compact_shape_budget(&self, function: &mut Function) {
        function.instruction(&Instruction::GlobalGet(self.compact_shapes.budget));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::GlobalSet(self.compact_shapes.budget));
    }

    /// `parent[0..count)` followed by `key`. The fresh key array is written
    /// only here, before any shape can publish it.
    pub(crate) fn compact_shape_keys_extended(
        &self,
        slot: GcLocalSlot<CompactShapeKeys>,
        parent: &GcLocal<CompactShapeKeys>,
        count: I32Local,
        key: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> GcLocal<CompactShapeKeys> {
        let keys = self.array_type::<CompactShapeKeys>();
        key.load(self, function);
        count.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::ArrayNew(keys.type_index().raw()));
        let extended = slot.initialize(GcStackReference::new(), function);
        extended.load(self, function);
        function.instruction(&Instruction::I32Const(0));
        parent.load(self, function);
        function.instruction(&Instruction::I32Const(0));
        count.load(function);
        function.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: keys.type_index().raw(),
            array_type_index_src: keys.type_index().raw(),
        });
        extended
    }
}
