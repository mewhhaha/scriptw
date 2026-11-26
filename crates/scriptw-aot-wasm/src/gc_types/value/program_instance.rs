//! Frame-local code, hook and immutable data ownership for compiled programs.

use super::*;
mod native_payload;
use crate::data::{PooledStringIndex, StringPool};
use crate::program_hooks::ProvidedHooks;
use crate::runtime_artifact::ModuleKind;
use crate::runtime_helpers::RuntimeHelperFunctionBase;
use crate::EmitError;

mod language_features;
mod literal_index;
mod metadata;
pub(crate) use metadata::ProgramMetadataView;

/// The final P pool, checked against its actual imported runtime boundary.
/// No scalar linear-memory address is accepted as a metadata image.
pub(crate) struct ProgramInstanceImage<'pool> {
    strings: &'pool StringPool,
    literal_base: u32,
    literal_count: u32,
    metadata_bytes: u32,
    language_features: scriptw_ir::LanguageFeaturePolicy,
}

impl<'pool> ProgramInstanceImage<'pool> {
    pub(crate) fn from_program_pool(
        strings: &'pool StringPool,
        kind: ModuleKind<'_>,
        language_features: scriptw_ir::LanguageFeaturePolicy,
    ) -> Result<Self, EmitError> {
        let ModuleKind::Program(runtime) = kind else {
            return Err(EmitError::unsupported(
                "a ProgramInstance image requires an actual linked P module",
            ));
        };
        runtime.layout().require_pool(strings)?;
        let boundary = strings.compiler_owned_boundary();
        let literal_base = boundary.strings();
        let literal_count = strings
            .pooled_string_count()
            .checked_sub(literal_base)
            .ok_or_else(|| EmitError::unsupported("P literals do not retain the runtime prefix"))?;
        let metadata_bytes = strings
            .bytes
            .len()
            .checked_sub(boundary.static_len())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| EmitError::unsupported("P metadata exceeds its GC byte image"))?;
        Ok(Self {
            strings,
            literal_base,
            literal_count,
            metadata_bytes,
            language_features,
        })
    }

    pub(crate) fn literal_count(&self) -> u32 {
        self.literal_count
    }

    pub(crate) fn literal_count_operand(&self) -> ProgramLiteralCount {
        ProgramLiteralCount(self.literal_count)
    }
}

/// A table length minted only by the checked P image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramLiteralCount(u32);
impl ProgramLiteralCount {
    pub(crate) const fn value(self) -> u32 {
        self.0
    }

    /// A count for code-sink tests, never an emission operand.
    #[cfg(test)]
    pub(crate) const fn for_test(value: u32) -> Self {
        Self(value)
    }
}

/// A P-local ordinal. Runtime/compiler literals use their separate R pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramLiteralIndex(u32);

/// The producer hands over its sole table and descriptor roots. Every slot
/// of the fresh table is null: a literal is published only by this P's
/// resolver at its first use, and no capability writes the table otherwise.
pub(crate) struct ProgramLiteralRoots<'image, 'pool> {
    image: &'image ProgramInstanceImage<'pool>,
    table: GcLocal<PooledStringTable>,
    descriptors: ProgramStringDescriptorTable,
}

impl<'image, 'pool> ProgramLiteralRoots<'image, 'pool> {
    /// The table's length is the checked P literal count, which is also the
    /// descriptor image's row count, so an ordinal bounded by one bounds both.
    pub(crate) fn unresolved(
        schema: &RuntimeSchema,
        image: &'image ProgramInstanceImage<'pool>,
        descriptors: ProgramStringDescriptorTable,
        function: &mut Function,
    ) -> Self {
        let length = schema.reserve_i32_local(function);
        function.program_literal_count(image.literal_count_operand());
        length.store(function);
        let table = schema
            .reserve_gc_local::<PooledStringTable, NonNullable>(function)
            .initialize(
                schema.array_type::<PooledStringTable>().filled(
                    GcOperand::null(schema),
                    length,
                    function,
                ),
                function,
            );
        schema.release_i32_local(length, function);
        Self {
            image,
            table,
            descriptors,
        }
    }
}

/// Copies the P passive metadata segment into the existing read-only GC byte
/// owner. Contained records must be P-relative before consumers migrate; this
/// allocation does not turn old absolute-pointer records into safe readers.
pub(crate) struct ProgramMetadataBytes<'image, 'pool> {
    image: &'image ProgramInstanceImage<'pool>,
    bytes: Option<GcLocal<ImmutableByteArray>>,
}

impl<'image, 'pool> ProgramMetadataBytes<'image, 'pool> {
    pub(crate) fn from_program_segment(
        schema: &RuntimeSchema,
        image: &'image ProgramInstanceImage<'pool>,
        function: &mut Function,
    ) -> Self {
        // Both fields can retain the same read-only byte array when there are
        // no metadata bytes and no literal descriptor rows.
        if image.metadata_bytes == 0 && image.literal_count() == 0 {
            return Self { image, bytes: None };
        }
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Const(image.metadata_bytes as i32));
        function.instruction(&Instruction::ArrayNewData {
            array_type_index: schema.array_type::<ImmutableByteArray>().type_index().raw(),
            array_data_index: StringPool::PROGRAM_STATIC_DATA_SEGMENT,
        });
        Self {
            image,
            bytes: Some(
                schema
                    .reserve_gc_local(function)
                    .initialize(GcStackReference::new(), function),
            ),
        }
    }
}

/// Each absent hook is a null of that exact helper type. The constructor uses
/// the existing ProvidedHooks authority and helper index plan for this P.
pub(crate) struct ProgramInstanceHooks<'image, 'pool> {
    image: &'image ProgramInstanceImage<'pool>,
    dynamic_function: FunctionLocal<PreparedDynamicFunctionHook, Nullable>,
    script: FunctionLocal<PreparedScriptHook, Nullable>,
    realm_module_import: FunctionLocal<RealmModuleImportHook, Nullable>,
    module_body_reaction: FunctionLocal<ModuleBodyReactionHook, Nullable>,
    regexp_candidate: FunctionLocal<RegExpProgramCandidateHook, Nullable>,
    literal_resolve: FunctionLocal<ProgramLiteralResolveHook, Nullable>,
}

impl<'image, 'pool> ProgramInstanceHooks<'image, 'pool> {
    pub(crate) fn from_program(
        schema: &RuntimeSchema,
        image: &'image ProgramInstanceImage<'pool>,
        functions: &crate::planning::FunctionMetaRegistry,
        base: RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> Self {
        let provided = ProvidedHooks::of(functions, image.strings);
        Self {
            image,
            dynamic_function: bind_hook(schema, provided, base, function),
            script: bind_hook(schema, provided, base, function),
            realm_module_import: bind_hook(schema, provided, base, function),
            module_body_reaction: bind_hook(schema, provided, base, function),
            regexp_candidate: bind_hook(schema, provided, base, function),
            literal_resolve: bind_hook(schema, provided, base, function),
        }
    }
}

fn bind_hook<R: GcProgramHookRole>(
    schema: &RuntimeSchema,
    provided: ProvidedHooks,
    base: RuntimeHelperFunctionBase,
    function: &mut Function,
) -> FunctionLocal<R, Nullable> {
    if provided.provides(R::HOOK) {
        function.instruction(&Instruction::RefFunc(R::HOOK.helper().index(base)));
    } else {
        function.instruction(&Instruction::RefNull(HeapType::Concrete(
            R::SIGNATURE.type_index(),
        )));
    }
    schema
        .reserve_function_local(function)
        .initialize(FunctionStackReference { shape: PhantomData }, function)
}

/// One explicit live code/data owner. It is a frame-local GC root, not a
/// process/global current-P selector and not the execution Realm.
#[must_use = "retain the program owner through its frame/continuation and clear the local on exit"]
pub(crate) struct ProgramInstanceLocals {
    owner: GcLocal<ProgramInstance>,
}

// Fixed 64-slot leaf pages keep dormant shape sites from allocating one
// metadata reference each. Only the directory scales with highest_site / 64.
const PROGRAM_CACHE_PAGE_SHIFT: i32 = 6;
const PROGRAM_CACHE_PAGE_SIZE: i32 = 1 << PROGRAM_CACHE_PAGE_SHIFT;
// The flat per-site record table starts with this many slots and doubles.
const OWN_DATA_RECORDS_MINIMUM: i32 = 64;

macro_rules! program_site_table {
    ($method:ident, $table:ident, $pages:ident, $field:ident) => {
        pub(crate) fn $method(
            &self,
            site: I32Local,
            schema: &RuntimeSchema,
            function: &mut Function,
        ) -> (GcLocal<$table>, I32Local) {
            let directory = schema.array_type::<$pages>();
            let pages = schema.reserve_gc_local(function).initialize(
                schema
                    .field(ProgramInstanceSchema::$field)
                    .read(&self.owner, schema, function)
                    .reference(),
                function,
            );
            let page_index = schema.reserve_i32_local(function);
            let length = schema.reserve_i32_local(function);
            let capacity = schema.reserve_i32_local(function);
            let required = schema.reserve_i32_local(function);
            site.load(function);
            function.instruction(&Instruction::I32Const(PROGRAM_CACHE_PAGE_SHIFT));
            function.instruction(&Instruction::I32ShrU);
            page_index.store(function);
            function.instruction(&Instruction::I32Const(0));
            length.store(function);
            pages.load(schema, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            directory.length(&pages, schema, function);
            length.store(function);
            function.instruction(&Instruction::End);
            page_index.load(function);
            length.load(function);
            function.instruction(&Instruction::I32GeU);
            function.instruction(&Instruction::If(BlockType::Empty));
            page_index.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            required.store(function);
            length.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Shl);
            capacity.store(function);
            capacity.load(function);
            function.instruction(&Instruction::I32Const(8));
            function.instruction(&Instruction::I32LtU);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I32Const(8));
            capacity.store(function);
            function.instruction(&Instruction::End);
            capacity.load(function);
            required.load(function);
            function.instruction(&Instruction::I32LtU);
            function.instruction(&Instruction::If(BlockType::Empty));
            required.load(function);
            capacity.store(function);
            function.instruction(&Instruction::End);
            let replacement = schema.reserve_gc_local(function).initialize(
                directory.filled(GcOperand::null(schema), capacity, function),
                function,
            );
            length.load(function);
            function.instruction(&Instruction::If(BlockType::Empty));
            let previous = schema.reserve_gc_local(function).initialize(
                pages.load(schema, function).require_non_null(function),
                function,
            );
            directory.copy_prefix_from(&replacement, &previous, length, schema, function);
            previous.clear(function);
            function.instruction(&Instruction::End);
            schema.field(ProgramInstanceSchema::$field).write(
                &self.owner,
                GcOperand::nullable_reference(&replacement, schema),
                schema,
                function,
            );
            pages.replace(replacement.load(schema, function).nullable(), function);
            replacement.clear(function);
            function.instruction(&Instruction::End);
            let page = schema.reserve_gc_local(function).initialize(
                directory
                    .read(&pages, page_index, schema, function)
                    .reference(),
                function,
            );
            page.load(schema, function).is_null(function);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I32Const(PROGRAM_CACHE_PAGE_SIZE));
            capacity.store(function);
            let fresh = schema.reserve_gc_local(function).initialize(
                schema
                    .array_type::<$table>()
                    .filled(GcOperand::null(schema), capacity, function),
                function,
            );
            directory.write(
                &pages,
                page_index,
                GcOperand::nullable_reference(&fresh, schema),
                schema,
                function,
            );
            page.replace(fresh.load(schema, function).nullable(), function);
            fresh.clear(function);
            function.instruction(&Instruction::End);
            let result = schema.reserve_gc_local(function).initialize(
                page.load(schema, function).require_non_null(function),
                function,
            );
            let slot = schema.reserve_i32_local(function);
            site.load(function);
            function.instruction(&Instruction::I32Const(PROGRAM_CACHE_PAGE_SIZE - 1));
            function.instruction(&Instruction::I32And);
            slot.store(function);
            page.clear(function);
            pages.clear(function);
            schema.release_i32_local(required, function);
            schema.release_i32_local(capacity, function);
            schema.release_i32_local(length, function);
            schema.release_i32_local(page_index, function);
            (result, slot)
        }
    };
}

impl ProgramInstanceLocals {
    pub(crate) fn publish(
        schema: &RuntimeSchema,
        image: &ProgramInstanceImage<'_>,
        hooks: ProgramInstanceHooks<'_, '_>,
        literals: ProgramLiteralRoots<'_, '_>,
        metadata: ProgramMetadataBytes<'_, '_>,
        function: &mut Function,
    ) -> Result<Self, EmitError> {
        if !std::ptr::eq(image, hooks.image)
            || !std::ptr::eq(image, literals.image)
            || !std::ptr::eq(image, metadata.image)
        {
            return Err(EmitError::unsupported(
                "ProgramInstance resources belong to different P images",
            ));
        }
        let source_payload = Self::absent_native_payload_slot(schema, function);
        let metadata_bytes = metadata
            .bytes
            .as_ref()
            .unwrap_or_else(|| literals.descriptors.bytes());
        let owner = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ProgramInstance>().construct(
                (
                    GcOperand::nullable_function_reference(&hooks.dynamic_function),
                    GcOperand::nullable_function_reference(&hooks.script),
                    GcOperand::nullable_function_reference(&hooks.realm_module_import),
                    GcOperand::nullable_function_reference(&hooks.module_body_reaction),
                    GcOperand::nullable_function_reference(&hooks.regexp_candidate),
                    GcOperand::nullable_function_reference(&hooks.literal_resolve),
                    GcOperand::reference(&literals.table, schema),
                    GcOperand::reference(literals.descriptors.bytes(), schema),
                    GcOperand::i32(image.literal_base as i32),
                    GcOperand::program_literal_count(image.literal_count_operand()),
                    GcOperand::reference(metadata_bytes, schema),
                    GcOperand::constant(image.language_features),
                    GcOperand::reference(&source_payload, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        source_payload.clear(function);
        hooks.dynamic_function.clear(function);
        hooks.script.clear(function);
        hooks.realm_module_import.clear(function);
        hooks.module_body_reaction.clear(function);
        hooks.regexp_candidate.clear(function);
        hooks.literal_resolve.clear(function);
        literals.table.clear(function);
        literals.descriptors.clear(function);
        if let Some(bytes) = metadata.bytes {
            bytes.clear(function);
        }
        Ok(Self { owner })
    }

    program_site_table!(
        compact_shapes,
        CompactShapeTable,
        CompactShapePageTable,
        COMPACT_SHAPES
    );
    /// The per-site record table: one flat array indexed by site, so a use
    /// reaches its record with one array read. Null until the first record.
    pub(crate) fn own_data_records(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<OwnDataReadCacheTable, Nullable> {
        schema.reserve_gc_local(function).initialize(
            schema
                .field(ProgramInstanceSchema::OWN_DATA_READ_CACHES)
                .read(&self.owner, schema, function)
                .reference(),
            function,
        )
    }

    /// The record table, grown to cover `site`. Growth at least doubles the
    /// table, so registering every site of a P costs amortized constant work.
    pub(crate) fn own_data_records_covering(
        &self,
        site: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<OwnDataReadCacheTable> {
        let table = schema.array_type::<OwnDataReadCacheTable>();
        let records = self.own_data_records(schema, function);
        let length = schema.reserve_i32_local(function);
        let capacity = schema.reserve_i32_local(function);
        length.set_constant(0, function);
        records.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        table.length(&records, schema, function);
        length.store(function);
        function.instruction(&Instruction::End);
        site.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        // The capacity is max(site + 1, 2 * length, the minimum).
        site.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        capacity.store(function);
        for doubled in [true, false] {
            let candidate = |function: &mut Function| {
                if doubled {
                    length.load(function);
                    function.instruction(&Instruction::I32Const(1));
                    function.instruction(&Instruction::I32Shl);
                } else {
                    function.instruction(&Instruction::I32Const(OWN_DATA_RECORDS_MINIMUM));
                }
            };
            candidate(function);
            capacity.load(function);
            function.instruction(&Instruction::I32GtU);
            function.instruction(&Instruction::If(BlockType::Empty));
            candidate(function);
            capacity.store(function);
            function.instruction(&Instruction::End);
        }
        let replacement = schema.reserve_gc_local(function).initialize(
            table.filled(GcOperand::null(schema), capacity, function),
            function,
        );
        length.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let previous = schema.reserve_gc_local(function).initialize(
            records.load(schema, function).require_non_null(function),
            function,
        );
        table.copy_prefix_from(&replacement, &previous, length, schema, function);
        previous.clear(function);
        function.instruction(&Instruction::End);
        schema
            .field(ProgramInstanceSchema::OWN_DATA_READ_CACHES)
            .write(
                &self.owner,
                GcOperand::nullable_reference(&replacement, schema),
                schema,
                function,
            );
        records.replace(replacement.load(schema, function).nullable(), function);
        replacement.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(capacity, function);
        schema.release_i32_local(length, function);
        let covering = schema.reserve_gc_local(function).initialize(
            records.load(schema, function).require_non_null(function),
            function,
        );
        records.clear(function);
        covering
    }

    pub(crate) fn bind(
        schema: &RuntimeSchema,
        reference: GcStackReference<ProgramInstance>,
        function: &mut Function,
    ) -> Self {
        Self {
            owner: schema.hold_gc_local(reference, function),
        }
    }

    /// A source function enters its defining P. An R-native function has no
    /// defining P and inherits the caller operand, independently of its Realm.
    pub(crate) fn for_function_entry(
        schema: &RuntimeSchema,
        context: &GcLocal<FunctionContext>,
        calling_program: &Self,
        function: &mut Function,
    ) -> Self {
        let defining = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::DEFINING_PROGRAM)
                .read(context, schema, function)
                .reference(),
            function,
        );
        let effective = Self::bind(
            schema,
            calling_program.owner.load(schema, function),
            function,
        );
        defining.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        effective.owner.replace(
            defining.load(schema, function).require_non_null(function),
            function,
        );
        function.instruction(&Instruction::End);
        defining.clear(function);
        effective
    }

    pub(crate) fn reference(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ProgramInstance> {
        self.owner.load(schema, function)
    }

    pub(crate) fn hook<R: GcProgramHookRole>(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> FunctionLocal<R, Nullable> {
        schema.reserve_function_local(function).initialize(
            schema
                .struct_type::<ProgramInstance>()
                .field(R::field())
                .read(&self.owner, schema, function)
                .callable(),
            function,
        )
    }

    /// A checked literal of this P. A published slot is read in place; only
    /// its first use calls this P's own resolver, which publishes it.
    pub(crate) fn literal(
        &self,
        schema: &RuntimeSchema,
        index: ProgramLiteralIndex,
        base: RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let output = schema.reference_type::<StringValue>(GcNullability::NonNullable);
        // One relocated ordinal site serves both the read and the miss call.
        let ordinal = schema.reserve_i32_local(function);
        function.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(output))));
        // `array.get` traps on a null table itself; LITERAL_STRINGS is never null.
        let _ = schema
            .struct_type::<ProgramInstance>()
            .field(ProgramInstanceSchema::LITERAL_STRINGS)
            .read(&self.owner, schema, function);
        function.program_literal_index(index);
        function.instruction(&Instruction::LocalTee(ordinal.index));
        function.instruction(&Instruction::ArrayGet(
            schema.array_type::<PooledStringTable>().type_index().raw(),
        ));
        // Null only until the literal's first use in this P instance.
        function.likely(&Instruction::BrOnNonNull(0));
        let _ = self.resolve_literal(schema, GcIndex::Local(ordinal), base, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(ordinal, function);
        GcStackReference::new()
    }

    /// Calls this P's resolver body directly. P code only: R reaches the same
    /// body through the owner's LITERAL_RESOLVE field.
    fn resolve_literal(
        &self,
        schema: &RuntimeSchema,
        ordinal: GcIndex,
        base: RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let _ = self.owner.load(schema, function);
        ordinal.emit(function);
        function.instruction(&Instruction::Call(
            crate::runtime_helpers::RuntimeHelperId::ProgramLiteralResolve.index(base),
        ));
        GcStackReference::new()
    }

    /// The published slot of a runtime ordinal, or null before its first use.
    /// An ordinal outside this P's literal table traps.
    pub(crate) fn cached_literal_at(
        &self,
        schema: &RuntimeSchema,
        ordinal: I32Local,
        function: &mut Function,
    ) -> GcLocal<StringValue, Nullable> {
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ProgramInstance>()
                .field(ProgramInstanceSchema::LITERAL_STRINGS)
                .read(&self.owner, schema, function)
                .reference(),
            function,
        );
        ordinal.load(function);
        schema
            .array_type::<PooledStringTable>()
            .length(&table, schema, function);
        function.instruction(&Instruction::I32GeU);
        trap_if_true(function);
        let cached = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PooledStringTable>()
                .read(&table, ordinal, schema, function)
                .reference(),
            function,
        );
        table.clear(function);
        cached
    }

    /// The resolver's single write: build literal `ordinal` from this P's own
    /// descriptor row and passive UTF-16 image and publish it in its slot.
    /// The String cannot come from anywhere but that row.
    pub(crate) fn publish_literal_from_descriptor(
        &self,
        schema: &RuntimeSchema,
        ordinal: I32Local,
        function: &mut Function,
    ) -> GcLocal<StringValue> {
        let descriptors = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ProgramInstance>()
                .field(ProgramInstanceSchema::LITERAL_DESCRIPTORS)
                .read(&self.owner, schema, function)
                .reference(),
            function,
        );
        let row = ProgramStringDescriptorRowLocals::read(schema, &descriptors, ordinal, function);
        descriptors.clear(function);
        let construction = StringConstruction::from_program_pooled_row(
            schema,
            schema.reserve_gc_local(function),
            &row,
            function,
        );
        row.release(schema, function);
        let string = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(construction.publish(schema, function), function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ProgramInstance>()
                .field(ProgramInstanceSchema::LITERAL_STRINGS)
                .read(&self.owner, schema, function)
                .reference(),
            function,
        );
        schema.array_type::<PooledStringTable>().write(
            &table,
            ordinal,
            GcOperand::nullable_reference(&string, schema),
            schema,
            function,
        );
        table.clear(function);
        string
    }

    /// A runtime ordinal of this P's literals, from P code.
    pub(crate) fn literal_at(
        &self,
        schema: &RuntimeSchema,
        ordinal: I32Local,
        base: RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let string = self.cached_literal_at(schema, ordinal, function);
        string.load(schema, function).is_null(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        string.replace(
            self.resolve_literal(schema, GcIndex::Local(ordinal), base, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        let result = string.load(schema, function).require_non_null(function);
        string.clear(function);
        result
    }

    pub(crate) fn metadata(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ImmutableByteArray> {
        schema
            .struct_type::<ProgramInstance>()
            .field(ProgramInstanceSchema::METADATA)
            .read(&self.owner, schema, function)
            .reference()
    }

    pub(crate) fn clear(self, function: &mut Function) {
        self.owner.clear(function);
    }
}

/// Required FunctionContext capture input; absence means native inheritance,
/// never "infer the program from the Realm". The allocation owner must check
/// this mode against the actual executable kind before emitting the capture.
pub(crate) enum FunctionProgramOwner<'a> {
    Defining(&'a ProgramInstanceLocals),
    InheritCaller,
}

impl FunctionProgramOwner<'_> {
    pub(crate) fn capture_operand<'a>(
        &'a self,
        kind: ExecutableCodeKind,
        schema: &RuntimeSchema,
    ) -> Result<GcOperand<'a, GcRef<ProgramInstance>, Nullable>, EmitError> {
        match (kind, self) {
            (ExecutableCodeKind::JavaScript, Self::Defining(program)) => {
                Ok(GcOperand::nullable_reference(&program.owner, schema))
            }
            (
                ExecutableCodeKind::StandardBuiltin | ExecutableCodeKind::HostBuiltin,
                Self::InheritCaller,
            ) => Ok(GcOperand::null(schema)),
            (ExecutableCodeKind::JavaScript, Self::InheritCaller)
            | (
                ExecutableCodeKind::StandardBuiltin | ExecutableCodeKind::HostBuiltin,
                Self::Defining(_),
            ) => Err(EmitError::unsupported(
                "FunctionContext program capture differs from its executable kind",
            )),
        }
    }
}

fn trap_if_true(function: &mut Function) {
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Unreachable);
    function.instruction(&Instruction::End);
}
