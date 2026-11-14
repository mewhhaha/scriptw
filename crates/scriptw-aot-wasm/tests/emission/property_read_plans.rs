//! Source reads call the typed runtime owner for live cache hits and complete Get.
use scriptw_aot_wasm::{
    GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy, RUNTIME_IMPORT_NAMESPACE,
    emit_with_rooted_snapshot,
};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{ExprIr, PropertyKeyIr, SpecOperationIr, StatementIr, ValueKind, lower};
use std::collections::BTreeMap;
use wasmparser::{
    CompositeInnerType, ExternalKind, HeapType, KnownCustom, Name, Operator, Parser, Payload,
    StorageType, TypeRef, UnpackedIndex, ValType, Validator, WasmFeatures,
};

#[derive(Default)]
pub(super) struct Module<'a> {
    pub(super) types: Vec<wasmparser::SubType>,
    names: BTreeMap<u32, String>,
    imports: BTreeMap<u32, (String, String)>,
    function_types: BTreeMap<u32, u32>,
    exports: BTreeMap<String, u32>,
    bodies: BTreeMap<u32, Vec<Operator<'a>>>,
    locals: BTreeMap<u32, Vec<ValType>>,
    globals: Vec<wasmparser::GlobalType>,
    global_exports: BTreeMap<String, u32>,
}

impl<'a> Module<'a> {
    pub(super) fn read(bytes: &'a [u8], operators: bool) -> Self {
        let mut module = Self::default();
        let mut next_function = 0;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.expect("property-read module decodes") {
                Payload::TypeSection(reader) => {
                    for group in reader {
                        module.types.extend(group.unwrap().types().cloned());
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import.expect("property-read import decodes");
                        if let TypeRef::Func(ty) | TypeRef::FuncExact(ty) = import.ty {
                            module.function_types.insert(next_function, ty);
                            module.imports.insert(
                                next_function,
                                (import.module.to_owned(), import.name.to_owned()),
                            );
                            next_function += 1;
                        } else if let TypeRef::Global(ty) = import.ty {
                            module.globals.push(ty);
                        }
                    }
                }
                Payload::FunctionSection(reader) => {
                    for (offset, ty) in reader.into_iter().enumerate() {
                        module.function_types.insert(
                            next_function + offset as u32,
                            ty.expect("property-read function type decodes"),
                        );
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.expect("property-read export decodes");
                        match export.kind {
                            ExternalKind::Func => {
                                module.exports.insert(export.name.to_owned(), export.index);
                            }
                            ExternalKind::Global => {
                                module
                                    .global_exports
                                    .insert(export.name.to_owned(), export.index);
                            }
                            _ => {}
                        }
                    }
                }
                Payload::GlobalSection(reader) => module
                    .globals
                    .extend(reader.into_iter().map(|global| global.unwrap().ty)),
                Payload::CodeSectionEntry(body) => {
                    module.locals.insert(
                        next_function,
                        body.get_locals_reader()
                            .expect("property-read locals decode")
                            .into_iter()
                            .map(|local| local.expect("property-read local decodes").1)
                            .collect(),
                    );
                    let body = if operators {
                        body.get_operators_reader()
                            .expect("property-read function opens")
                            .into_iter()
                            .map(|operator| operator.expect("property-read operator decodes"))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    module.bodies.insert(next_function, body);
                    next_function += 1;
                }
                Payload::CustomSection(section) => {
                    if let KnownCustom::Name(subsections) = section.as_known() {
                        for subsection in subsections {
                            if let Name::Function(map) =
                                subsection.expect("name subsection decodes")
                            {
                                for name in map {
                                    let name = name.expect("property-read function name decodes");
                                    assert!(
                                        module.names.insert(name.index, name.name.into()).is_none()
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        module
    }

    pub(super) fn named(&self, prefix: &str) -> (u32, &[Operator<'a>]) {
        let matches = self
            .names
            .iter()
            .filter(|(_, name)| {
                if prefix.ends_with('#') {
                    assert!(
                        prefix.starts_with("js::"),
                        "only an explicit source function prefix may end in '#': {prefix}"
                    );
                    name.starts_with(prefix)
                } else {
                    name.as_str() == prefix
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "one encoded body matches {prefix}; matching names: {matches:?}"
        );
        let index = *matches[0].0;
        (
            index,
            self.bodies
                .get(&index)
                .expect("the named function has an actual encoded definition"),
        )
    }

    pub(super) fn imported_runtime_function(&self, runtime: &Self, definition: u32) -> u32 {
        let matches = self
            .imports
            .iter()
            .filter_map(|(&index, (namespace, name))| {
                (namespace == RUNTIME_IMPORT_NAMESPACE
                    && runtime.exports.get(name) == Some(&definition))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "P imports the actual runtime definition");
        matches[0]
    }

    pub(super) fn witness_type(&self, layout: GcSnapshotLayout) -> u32 {
        let index = self.global_exports[layout.witness_export()];
        let ValType::Ref(reference) = self.globals[index as usize].content_type else {
            panic!("snapshot witness retains a GC reference");
        };
        let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
            panic!("snapshot witness retains a concrete GC type");
        };
        index
    }

    pub(super) fn function_type(&self, index: u32) -> &wasmparser::FuncType {
        let ty = self.function_types[&index];
        let CompositeInnerType::Func(signature) = &self.types[ty as usize].composite_type.inner
        else {
            panic!("the actual function index resolves to a function signature");
        };
        signature
    }
}

fn count(body: &[Operator<'_>], predicate: impl Fn(&Operator<'_>) -> bool) -> usize {
    body.iter().filter(|operator| predicate(operator)).count()
}

pub(super) fn calls(body: &[Operator<'_>], target: u32) -> bool {
    body.iter().any(|operator| {
        matches!(operator,
            Operator::Call { function_index } | Operator::ReturnCall { function_index }
                if *function_index == target)
    })
}

fn assert_numeric_probe_before_conversion(
    body: &[Operator<'_>],
    conversion: u32,
    module: &Module<'_>,
    complete_get_v: Option<(u32, u32)>,
) {
    let calls_to = |target| {
        body.iter()
            .enumerate()
            .filter_map(|(index, op)| {
                matches!(op, Operator::Call { function_index } if *function_index == target)
                    .then_some(index)
            })
            .collect::<Vec<_>>()
    };
    let conversion_calls = calls_to(conversion);
    assert_eq!(
        conversion_calls.len(),
        1,
        "one property-key conversion on a miss retains its completed String/Symbol key"
    );
    let conversion = conversion_calls[0];
    let cast = body[..conversion]
        .iter()
        .position(|op| matches!(op, Operator::I64TruncF64U))
        .expect("the admitted numeric key becomes an integer before conversion");
    let candidates = body[..cast].windows(4).enumerate().filter_map(|(start, ops)| {
        if !matches!(ops,
            [Operator::LocalGet { .. }, Operator::I32Const { value }, Operator::I32Eq, Operator::If { .. }]
                if *value == scriptw_aot_wasm::WasmRuntimeValueTag::Number as i32)
        { return None; }
        let end = frame_end(body, start + 3);
        (cast < end && end < conversion).then_some((start, end))
    }).collect::<Vec<_>>();
    assert_eq!(
        candidates.len(),
        1,
        "one primitive Number probe contains the guarded index cast"
    );
    let (start, end) = candidates[0];
    let array = module.witness_type(GcSnapshotLayout::Array);
    assert!(body[start + 4..cast].windows(2).any(|ops| matches!(ops,
        [Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }, Operator::If { .. }]
            if *index == array)), "the Number arm tests the actual concrete Array before the trapping cast");
    let guard = &body[start..cast];
    assert!(
        guard.iter().any(|op| matches!(op, Operator::F64Ge))
            && guard.iter().any(|op| matches!(op, Operator::F64Lt))
            && guard.iter().any(|op| matches!(op, Operator::I64TruncSatF64S))
            && guard.iter().any(|op| matches!(op, Operator::F64ConvertI64S))
            && guard.iter().any(|op| matches!(op, Operator::F64Eq)),
        "finite integral index guards precede the trapping cast"
    );
    let [
        Operator::LocalGet { .. },
        Operator::BrIf { relative_depth },
        ..,
    ] = &body[end + 1..]
    else {
        panic!("the completed probe's handled flag controls the property exit");
    };
    let hit = end + 2;
    let mut frames = Vec::new();
    for (index, op) in body[..hit].iter().enumerate() {
        match op {
            Operator::If { .. }
            | Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. }
            | Operator::Try { .. } => frames.push(index),
            Operator::End => {
                frames
                    .pop()
                    .expect("the hit remains inside the source body");
            }
            _ => {}
        }
    }
    let target = frames[frames.len() - 1 - *relative_depth as usize];
    let exit = frame_end(body, target);
    assert!(
        matches!(body[target], Operator::Block { .. }) && exit > conversion,
        "the actual hit target skips the complete property-key conversion fallback"
    );
    // Operand evaluation and identity boxing may precede this guard. Only the
    // admitted Number/concrete-Array probe and its hit must stay call-free.
    let region = &body[start..=hit];
    assert!(
        !region.iter().any(|op| matches!(
            op,
            Operator::StructNew { .. }
                | Operator::StructNewDefault { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArrayNewData { .. }
                | Operator::ArrayNewElem { .. }
                | Operator::Call { .. }
                | Operator::CallRef { .. }
                | Operator::CallIndirect { .. }
                | Operator::ReturnCall { .. }
                | Operator::ReturnCallRef { .. }
                | Operator::ReturnCallIndirect { .. }
        )),
        "the complete numeric hit path allocates nothing and calls no runtime or user code"
    );
    for layout in [
        GcSnapshotLayout::ArrayValues,
        GcSnapshotLayout::ArrayNumbers,
        GcSnapshotLayout::ArrayNumberPresence,
    ] {
        let backing = module.witness_type(layout);
        assert!(
            region.iter().any(|op| matches!(op,
            Operator::ArrayGet { array_type_index } if *array_type_index == backing)),
            "the hit reads each actual live packed backing: {layout:?}"
        );
    }
    if let Some((boxing, get)) = complete_get_v {
        let boxing = calls_to(boxing);
        let get = calls_to(get);
        assert_eq!(boxing.len(), 1, "one saved-operand ToObject fallback");
        assert_eq!(get.len(), 1, "one saved-receiver ObjectRead fallback");
        assert!(
            hit < conversion && boxing[0] < conversion && conversion < get[0] && get[0] < exit,
            "the actual hit skips the ordered key-conversion/Get fallback; boxing precedes key conversion"
        );
    }
}

fn frame_end(body: &[Operator<'_>], start: usize) -> usize {
    let mut depth = 1;
    for (offset, op) in body[start + 1..].iter().enumerate() {
        match op {
            Operator::If { .. }
            | Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. }
            | Operator::Try { .. } => depth += 1,
            Operator::End => {
                depth -= 1;
                if depth == 0 {
                    return start + offset + 1;
                }
            }
            _ => {}
        }
    }
    panic!("encoded control frame is balanced");
}

fn concrete_reference_type(value: ValType) -> u32 {
    let ValType::Ref(reference) = value else {
        panic!("the actual helper/storage edge is a GC reference");
    };
    let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
        panic!("the actual helper/storage edge retains a concrete type");
    };
    index
}

fn own_data_cache_type(module: &Module<'_>, storage: u32) -> u32 {
    let CompositeInnerType::Struct(storage) = &module.types[storage as usize].composite_type.inner
    else {
        panic!("the compact store helper accepts a GC record");
    };
    assert_eq!(
        storage.fields.len(),
        1,
        "the compact base owns its immutable shape"
    );
    let shape = concrete_reference_type(storage.fields[0].element_type.unpack());
    let CompositeInnerType::Struct(shape_fields) =
        &module.types[shape as usize].composite_type.inner
    else {
        panic!("the live storage shape is a GC record");
    };
    assert!(
        shape_fields.fields.len() >= 2,
        "shape owns primitive keys and attributes"
    );
    assert!(shape_fields.fields[..2].iter().all(|field| !field.mutable));
    let keys = concrete_reference_type(shape_fields.fields[0].element_type.unpack());
    let CompositeInnerType::Array(keys) = &module.types[keys as usize].composite_type.inner else {
        panic!("the shape owns its key array");
    };
    let string = concrete_reference_type(keys.0.element_type.unpack());
    let cache_types = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| {
            let CompositeInnerType::Struct(fields) = &ty.composite_type.inner else {
                return None;
            };
            if fields.fields.len() < 3 {
                return None;
            }
            let reference = |field: usize, target, nullable| {
                matches!(fields.fields[field].element_type,
            StorageType::Val(ValType::Ref(r)) if r.is_nullable() == nullable
                && r.heap_type() == HeapType::Concrete(UnpackedIndex::Module(target)))
            };
            // After the key, shape and slot the record adds only further
            // nullable shapes and integers: no object, value or callable.
            (reference(0, string, false)
                && reference(1, shape, true)
                && fields.fields[2].element_type == StorageType::Val(ValType::I32)
                && (3..fields.fields.len()).all(|field| {
                    reference(field, shape, true)
                        || fields.fields[field].element_type == StorageType::Val(ValType::I32)
                }))
            .then_some(index as u32)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cache_types.len(),
        1,
        "one canonical read record: a primitive key, then shapes and integers only"
    );
    cache_types[0]
}

#[test]
fn own_data_cache_reads_use_runtime_guards_and_numeric_reads_keep_get_fallbacks() {
    std::thread::Builder::new()
        .name("property-read-plan-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Unknown arguments prevent replacing the loaded value with a
            // constant. Publishing the functions retains one unspecialized
            // body each; controls isolate ordinary function/literal plumbing.
            let source = r#"
Object.prototype;
function ownCandidate(value) { var record = {x: value}; return record.x; }
function absentCandidate(value) { var record = {x: value}; return record.then; }
function ownControl(value) { var record = {x: value}; return record; }
function indexRead(value) { var array = [value]; return array[0]; }
function arrayControl(value) { var array = [value]; return array; }
function lengthRead(value) { var array = [value]; return array.length; }
function unresolvedRead(record) { return record.x; }
function computedRead(record, key) { return record[key]; }
function repeatedRead(record, count, callback) {
  var sum = 0;
  for (var i = 0; i < count; i++) { callback(record, i); sum += record.x; }
  return sum;
}
[absentCandidate, ownCandidate, ownControl, indexRead, arrayControl, lengthRead, unresolvedRead, computedRead, repeatedRead];
"#;
            let parsed = parse(source, ParseOptions::script()).expect("property fixtures parse");
            let lowered = lower(&parsed);
            let script = lowered
                .script
                .as_ref()
                .expect("property fixtures retain source IR");
            let returned = |name: &str| {
                let function = script
                    .functions
                    .iter()
                    .find(|function| function.name == name)
                    .expect("published source function retains its IR");
                let StatementIr::Return(value) = function
                    .body
                    .statements
                    .last()
                    .expect("source read function returns its loaded value")
                else {
                    panic!("unexpected return staging for {name}: {:?}", function.body);
                };
                value
            };
            assert!(
                matches!(&returned("ownCandidate").expr,
                ExprIr::PropertyRead { target, key: PropertyKeyIr::StaticString(name) }
                    if name == "x" && target.possible_kinds.contains(ValueKind::Object)),
                "shaped own source read remains a real PropertyRead: {:?}",
                returned("ownCandidate")
            );
            assert!(
                matches!(&returned("unresolvedRead").expr,
                ExprIr::SpecOperation { operation: SpecOperationIr::GetV, operands }
                    if operands.len() == 2 && operands[0].possible_kinds.contains(ValueKind::Object)
                    && matches!(&operands[1].expr, ExprIr::String(name) if name == "x")),
                "unresolved static source read takes canonical GetV: {:?}",
                returned("unresolvedRead")
            );
            assert!(
                matches!(&returned("computedRead").expr,
                ExprIr::SpecOperation { operation: SpecOperationIr::GetV, operands }
                    if operands.len() == 2
                    && operands[1].possible_kinds.contains(ValueKind::Number)),
                "the indexed-read control exercises canonical GetV, not only PropertyRead"
            );
            let artifact = emit_with_rooted_snapshot(
                &lowered, PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).expect("property fixtures emit compiled Wasm");
            for bytes in [&artifact.bytes[..], artifact.runtime().unwrap().bytes()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let program = Module::read(&artifact.bytes, true);
            let runtime = Module::read(
                artifact
                    .runtime()
                    .expect("property reads link their runtime")
                    .bytes(),
                true,
            );
            let (definition, dynamic_read) = runtime.named("helper::dynamic_property_read");
            let fallback = program.imported_runtime_function(&runtime, definition);
            let (object_read_definition, _) = runtime.named("helper::object_read");
            let canonical_get = program.imported_runtime_function(&runtime, object_read_definition);
            let (cached_definition, entry_body) = runtime.named("helper::cached_property_read");
            // The entry answers record hits; everything else is the miss
            // kernel's, which registers records and keeps the complete Get.
            let (miss_definition, cached_body) =
                runtime.named("helper::cached_property_read_miss");
            let cache = program.imported_runtime_function(&runtime, cached_definition);
            let (maker_definition, maker_body) = runtime.named("helper::cached_property_record");
            let record_maker = program.imported_runtime_function(&runtime, maker_definition);
            let (_, own) = program.named("js::ownCandidate#");
            let (_, unresolved) = program.named("js::unresolvedRead#");
            let (_, repeated) = program.named("js::repeatedRead#");
            let (_, computed) = program.named("js::computedRead#");
            let (_, own_control) = program.named("js::ownControl#");
            let (_, indexed) = program.named("js::indexRead#");
            let (_, array_control) = program.named("js::arrayControl#");
            let (_, length) = program.named("js::lengthRead#");

            for (name, candidate, control) in [
                ("numeric index", indexed, array_control),
                ("array length", length, array_control),
            ] {
                assert!(
                    calls(candidate, fallback),
                    "{name} reaches the actual imported runtime Get"
                );
                assert!(
                    !calls(control, fallback),
                    "the matching construction control does not perform Get"
                );
            }
            assert!(
                calls(own, cache),
                "shaped PropertyRead reaches the actual cache kernel"
            );
            assert!(
                calls(unresolved, cache),
                "unresolved literal-key GetV reaches the actual cache kernel"
            );
            let (store, _) = runtime.named("helper::compact_data_store");
            let program_store = program.imported_runtime_function(&runtime, store);
            let program_storage = concrete_reference_type(program.function_type(program_store).params()[0]);
            let program_cache_type = own_data_cache_type(&program, program_storage);
            assert_eq!(program.function_type(cache), runtime.function_type(cached_definition),
                "P calls the actual typed R cache signature with the shared GC prefix");
            for source in ["js::ownCandidate#", "js::absentCandidate#", "js::unresolvedRead#", "js::repeatedRead#"] {
                let (_, body) = program.named(source);
                assert_eq!(count(body, |op| matches!(op,
                    Operator::Call { function_index } if *function_index == cache)), 1,
                    "each source read calls the actual typed R cache kernel once");
                assert!(!body.iter().any(|op| matches!(op,
                    Operator::StructSet { struct_type_index, .. }
                    | Operator::StructNew { struct_type_index }
                        if *struct_type_index == program_cache_type)),
                    "P writes no record field and allocates no record; R owns registration");
                assert!(body.iter().all(|op| !matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == program_cache_type && !matches!(field_index, 1 | 2))),
                    "P reads only a record's shape and slot");
                assert!(body.iter().any(|op| matches!(op,
                    Operator::GlobalGet { .. })) && body.iter().any(|op| matches!(op,
                    Operator::Call { function_index } if *function_index == record_maker)),
                    "the site keeps its record in a global and asks R to make it on first use");
            }
            let (boxing, _) = runtime.named("helper::value_to_object");
            assert!(!calls(unresolved, program.imported_runtime_function(&runtime, boxing)),
                "static GetV keeps primitive boxing entirely inside the complete miss kernel");
            assert!(
                !calls(own_control, cache),
                "construction alone does not request a read cache"
            );
            assert!(
                !calls(computed, cache) && calls(computed, canonical_get),
                "computed GetV retains the complete canonical operation for guarded misses"
            );
            let (key_conversion, _) = runtime.named("helper::value_to_property_key");
            assert!(!calls(unresolved, program.imported_runtime_function(&runtime, key_conversion)),
                "a compiler-owned primitive static key needs no general property-key conversion");
            assert_numeric_probe_before_conversion(
                computed,
                program.imported_runtime_function(&runtime, key_conversion),
                &runtime,
                Some((program.imported_runtime_function(&runtime, boxing), canonical_get)),
            );
            assert_numeric_probe_before_conversion(dynamic_read, key_conversion, &runtime, None);
            assert!(
                calls(cached_body, object_read_definition),
                "cache miss retains full Get with its canonical key and explicit receiver"
            );
            assert_eq!(
                runtime.function_type(cached_definition),
                runtime.function_type(miss_definition),
                "the entry forwards its exact operands to the miss kernel"
            );
            assert_eq!(
                entry_body
                    .iter()
                    .filter(|op| matches!(op, Operator::Call { .. } | Operator::CallRef { .. }
                        | Operator::ReturnCall { .. } | Operator::ReturnCallRef { .. }))
                    .collect::<Vec<_>>(),
                [&Operator::ReturnCall { function_index: miss_definition }],
                "the hit entry makes no call but its tail call to the miss kernel"
            );
            assert!(
                !entry_body.iter().any(|op| matches!(op, Operator::StructNew { .. }
                    | Operator::StructNewDefault { .. } | Operator::ArrayNew { .. }
                    | Operator::ArrayNewDefault { .. } | Operator::ArrayNewFixed { .. }
                    | Operator::StructSet { .. } | Operator::ArraySet { .. })),
                "a read hit allocates and registers nothing"
            );
            assert!(
                count(cached_body, |operator| matches!(
                    operator,
                    Operator::RefTestNonNull { .. }
                )) >= 2,
                "R checks concrete ordinary receiver and compact storage"
            );
            assert!(
                count(cached_body, |operator| matches!(operator, Operator::RefEq)) >= 2,
                "R checks the primitive key and live immutable shape identity"
            );
            assert!(
                count(cached_body, |operator| matches!(
                    operator,
                    Operator::ArrayGet { .. }
                )) >= 2,
                "R loads current scalar/reference values directly from guarded storage"
            );
            let storage = concrete_reference_type(runtime.function_type(store).params()[0]);
            let cache_type = own_data_cache_type(&runtime, storage);
            assert_eq!(own_data_cache_type(&program, program_storage), cache_type,
                "P and R use the same canonical cache type in the shared GC prefix");
            let storage_definition = &runtime.types[storage as usize];
            assert_eq!(&program.types[program_storage as usize], storage_definition,
                "the compact store boundary retains R's exact shared type");
            for mode in [-1, -2] {
                assert!(cached_body.iter().any(|op| matches!(op, Operator::I32Const { value } if *value == mode)),
                    "R registers the same closed negative mode {mode}");
            }
            let (find, _) = runtime.named("helper::intrinsic_property_find");
            let intrinsic_storage = concrete_reference_type(runtime.function_type(find).params()[0]);
            let intrinsic_count = GcSnapshotField::IntrinsicCount.index();
            for field in [intrinsic_count - 1, intrinsic_count] {
                assert!(cached_body.iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == intrinsic_storage && *field_index == field)),
                    "R negative hits guard both sealed intrinsic ADDRESS and COUNT");
            }
            assert!(count(cached_body, |op| matches!(op,
                Operator::StructGet { struct_type_index, field_index }
                    if *struct_type_index == runtime.witness_type(GcSnapshotLayout::Object)
                        && *field_index == GcSnapshotField::ObjectPrototype.index())) >= 2,
                "R negative hits reload the lookup prototype and live null parent");
            assert!(calls(cached_body, find),
                "negative registration proves absence in sealed Object.prototype rows");
            assert_eq!(count(cached_body, |op| matches!(op,
                Operator::StructNew { struct_type_index } if *struct_type_index == cache_type)), 0,
                "the miss kernel fills the site's record and allocates none");
            assert_eq!(count(maker_body, |op| matches!(op,
                Operator::StructNew { struct_type_index } if *struct_type_index == cache_type)), 1,
                "R's record helper alone allocates the canonical record");
            let (allocate, _) = runtime.named("helper::compact_literal_allocate");
            let object = concrete_reference_type(runtime.function_type(allocate).results()[0]);
            let live_storage = cached_body.iter().position(|op| matches!(op,
                Operator::StructGet { struct_type_index, field_index }
                    if *struct_type_index == object && *field_index == GcSnapshotField::ObjectProperties.index()))
                .expect("every helper call reloads the lookup's current own storage");
            let admission = cached_body.windows(3).position(|ops| matches!(ops,
                [Operator::RefEq, Operator::I32And, Operator::If { .. }]))
                .expect("actual key and shape identity admit the cached slot");
            assert!(live_storage < admission);
            for sealed in [GcSnapshotLayout::InlineCompactStorage, GcSnapshotLayout::ArrayCompactStorage] {
                let sealed = runtime.witness_type(sealed);
                assert!(cached_body[live_storage..admission].iter().any(|op| matches!(op,
                    Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                        if *index == sealed)), "current compact storage is proven before using cache metadata");
            }
            assert!(!cached_body.iter().any(|op| matches!(op,
                Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                    if *index == storage)), "a test against the open compact base would call the host subtype query");
            // The entry identifies each sealed storage with one cast test
            // after loading the receiver's current storage, and only then
            // compares the record's shape; it never tests the open base.
            let casts_to = |op: &Operator<'_>, layout: u32| matches!(op,
                Operator::BrOnCast { to_ref_type, .. } | Operator::BrOnCastFail { to_ref_type, .. }
                    if matches!(to_ref_type.heap_type(), HeapType::Concrete(index)
                        if index.as_module_index() == Some(layout)));
            let entry_storage = entry_body.iter().position(|op| matches!(op,
                Operator::StructGet { struct_type_index, field_index }
                    if *struct_type_index == object && *field_index == GcSnapshotField::ObjectProperties.index()))
                .expect("the hit entry reloads the receiver's current own storage");
            let array_storage = runtime.witness_type(GcSnapshotLayout::ArrayCompactStorage);
            let array_cast = entry_storage + entry_body[entry_storage..].iter()
                .position(|op| casts_to(op, array_storage))
                .expect("the hit entry proves sealed Array storage by one cast test");
            let shape_admission = array_cast + entry_body[array_cast..].iter()
                .position(|op| matches!(op, Operator::RefEq))
                .expect("the hit entry compares the live shape with the record's");
            assert!(entry_body[array_cast..shape_admission].iter().any(|op| matches!(op,
                Operator::StructGet { struct_type_index, .. } if *struct_type_index == cache_type)),
                "current storage, then its sealed layout, then the record's shape");
            assert!(entry_body[entry_storage..].iter().any(|op| casts_to(
                op, runtime.witness_type(GcSnapshotLayout::InlineCompactStorage))),
                "the hit entry proves sealed inline storage by one cast test");
            assert!(!entry_body.iter().any(|op| casts_to(op, storage) || matches!(op,
                Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                | Operator::RefCastNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                    if *index == storage)),
                "the hit entry never tests the open compact base");
            let cache_call = repeated.iter().position(|op| matches!(op,
                Operator::Call { function_index } if *function_index == cache)).unwrap();
            let loop_start = repeated.iter().enumerate().find_map(|(start, op)| {
                (matches!(op, Operator::Loop { .. }) && start < cache_call
                    && cache_call < frame_end(repeated, start)).then_some(start)
            }).expect("the runtime read remains inside the actual source loop");
            let (call, _) = runtime.named("helper::proxy_call");
            let call = program.imported_runtime_function(&runtime, call);
            assert!(calls(&repeated[loop_start..cache_call], call),
                "the guarded read remains after the observable callback");
            for (name, read) in [("numeric index", indexed), ("array length", length)] {
                assert!(
                    !read.iter().any(|operator| matches!(
                        operator,
                        Operator::I64TruncSatF64U | Operator::F64Eq
                    )),
                    "{name} keeps numeric index classification in R"
                );
            }
            assert_eq!(
                count(indexed, |op| matches!(op, Operator::RefTestNonNull { .. })),
                count(array_control, |op| matches!(op, Operator::RefTestNonNull { .. })),
                "a static numeric index retains its representation guard in R"
            );
            let array = runtime.witness_type(GcSnapshotLayout::Array);
            let live_length = length.iter().position(|op| matches!(op,
                Operator::StructGet { struct_type_index, field_index }
                    if *struct_type_index == array && *field_index == GcSnapshotField::ArrayLength.index()))
                .expect("the exact length consumer loads the actual Array's live intrinsic field");
            // One br_on_cast both proves and casts the actual Array.
            assert!(length[..live_length].iter().any(|op| matches!(op,
                Operator::BrOnCast { to_ref_type, .. }
                    if to_ref_type.heap_type() == HeapType::Concrete(UnpackedIndex::Module(array)))),
                "the actual Array type dominates the intrinsic field load");
            assert!(calls(&length[live_length..], fallback),
                "other actual receivers still reach the complete saved-value Get fallback");
        })
        .expect("compiler worker starts")
        .join()
        .expect("property-read emission does not panic");
}
