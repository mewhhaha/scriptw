//! Source read plans retain guarded own-data loads and complete Get fallbacks.
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
    RUNTIME_IMPORT_NAMESPACE,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, ExprIr, PropertyKeyIr, SpecOperationIr, StatementIr, ValueKind};
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
                                    assert!(module
                                        .names
                                        .insert(name.index, name.name.into())
                                        .is_none());
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

fn assert_numeric_probe_before_conversion(body: &[Operator<'_>], conversion: u32) {
    let conversion = body
        .iter()
        .position(|operator| {
            matches!(operator, Operator::Call { function_index } if *function_index == conversion)
        })
        .expect("the original property-key conversion remains available on a miss");
    let prefix = &body[..conversion];
    let cast = prefix
        .iter()
        .position(|operator| matches!(operator, Operator::I64TruncF64U))
        .expect("the admitted numeric key becomes an integer before conversion");
    let guard = &prefix[..cast];
    assert!(
        guard.windows(4).any(|ops| matches!(
            ops,
            [
                Operator::I32Eq,
                Operator::If { .. },
                Operator::LocalGet { .. },
                Operator::RefTestNonNull { .. }
            ]
        )),
        "nonnumeric keys skip the GC subtype test, not just the numeric backing load"
    );
    assert!(
        guard
            .iter()
            .any(|op| matches!(op, Operator::RefTestNonNull { .. }))
            && guard.iter().any(|op| matches!(op, Operator::F64Ge))
            && guard.iter().any(|op| matches!(op, Operator::F64Lt))
            && guard.iter().any(|op| matches!(op, Operator::F64Trunc))
            && guard.iter().any(|op| matches!(op, Operator::F64Eq)),
        "concrete Array and finite integral index guards precede the trapping cast"
    );
    let load = prefix
        .iter()
        .rposition(|op| matches!(op, Operator::ArrayGet { .. }))
        .expect("the source path loads existing packed data before allocating a key");
    assert!(
        load > cast,
        "the guarded index addresses the packed backing"
    );
    assert!(
        prefix[load..]
            .iter()
            .any(|op| matches!(op, Operator::BrIf { .. })),
        "a successful own-data load bypasses property-key conversion"
    );
    assert!(
        !prefix[cast..].iter().any(|op| matches!(
            op,
            Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::Call { .. }
                | Operator::CallRef { .. }
        )),
        "the packed read arm allocates no descriptors or keys and calls no user code"
    );
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

fn assert_inline_cache_hit_bypasses_runtime(body: &[Operator<'_>], cache: u32) {
    let call = body
        .iter()
        .position(|op| {
            matches!(op,
        Operator::Call { function_index } if *function_index == cache)
        })
        .unwrap();
    let (miss, found) = body
        .windows(3)
        .enumerate()
        .filter_map(|(start, ops)| {
            let [Operator::LocalGet { local_index }, Operator::I32Eqz, Operator::If { .. }] = ops
            else {
                return None;
            };
            let end = frame_end(body, start + 2);
            (start < call && call < end).then_some((start, *local_index))
        })
        .last()
        .expect("the runtime cache call is dominated by the inline hit flag's miss branch");
    let publication = body[..miss]
        .windows(2)
        .rposition(|ops| {
            matches!(ops,
        [Operator::I32Const { value: 1 }, Operator::LocalSet { local_index }]
            if *local_index == found)
        })
        .expect("the direct typed load publishes its hit flag");
    let proof = body[..publication]
        .windows(3)
        .rposition(|ops| {
            matches!(
                ops,
                [Operator::RefEq, Operator::I32And, Operator::If { .. }]
            )
        })
        .expect("actual shape and static key identity admit the typed slot load");
    let hit = &body[proof + 3..publication];
    assert!(
        hit.iter()
            .any(|op| matches!(op, Operator::StructGet { .. }))
            && hit.iter().any(|op| matches!(op, Operator::ArrayGet { .. })),
        "P directly loads live inline and array-backed compact slots"
    );
    assert!(
        !hit.iter().any(|op| matches!(
            op,
            Operator::Call { .. }
                | Operator::ReturnCall { .. }
                | Operator::CallRef { .. }
                | Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
        )),
        "the admitted own-data region calls no runtime/user code and creates no snapshot"
    );
}

#[test]
fn own_data_cache_and_numeric_array_reads_keep_guarded_loads_and_get_fallbacks() {
    std::thread::Builder::new()
        .name("property-read-plan-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Unknown arguments prevent replacing the loaded value with a
            // constant. Publishing the functions retains one unspecialized
            // body each; controls isolate ordinary function/literal plumbing.
            let source = r#"
function ownCandidate(value) { var record = {x: value}; return record.x; }
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
[ownCandidate, ownControl, indexRead, arrayControl, lengthRead, unresolvedRead, computedRead, repeatedRead];
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
            let (cached_definition, cached_body) = runtime.named("helper::cached_property_read");
            let cache = program.imported_runtime_function(&runtime, cached_definition);
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
            for body in [own, unresolved, repeated] {
                assert_inline_cache_hit_bypasses_runtime(body, cache);
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
            );
            assert_numeric_probe_before_conversion(dynamic_read, key_conversion);
            assert!(
                calls(cached_body, object_read_definition),
                "cache miss retains full Get with its canonical key and explicit receiver"
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
            let shape = runtime.witness_type(GcSnapshotLayout::CompactShape);
            let string = runtime.witness_type(GcSnapshotLayout::String);
            let cache_types = runtime.types.iter().enumerate().filter_map(|(index, ty)| {
                let CompositeInnerType::Struct(fields) = &ty.composite_type.inner else { return None; };
                if fields.fields.len() != 3 { return None; }
                let reference = |field: usize, target, nullable| matches!(fields.fields[field].element_type,
                    StorageType::Val(ValType::Ref(r)) if r.is_nullable() == nullable
                        && r.heap_type() == HeapType::Concrete(UnpackedIndex::Module(target)));
                (reference(0, string, false) && reference(1, shape, true)
                    && fields.fields[2].element_type == StorageType::Val(ValType::I32))
                    .then_some(index as u32)
            }).collect::<Vec<_>>();
            assert_eq!(cache_types.len(), 1, "one canonical primitive key/shape/slot cache layout");
            let (initialize, retained) = repeated.windows(3).enumerate().find_map(|(start, ops)| {
                let [Operator::LocalGet { local_index }, Operator::RefIsNull, Operator::If { .. }] = ops else { return None; };
                let end = frame_end(repeated, start + 2);
                repeated[start + 3..end].iter().any(|op| matches!(op,
                    Operator::StructNew { struct_type_index } if *struct_type_index == cache_types[0]))
                    .then_some((start, *local_index))
            }).expect("an invocation-local metadata root initializes lazily at the source site");
            assert_eq!(repeated.iter().filter(|op| matches!(op,
                Operator::LocalSet { local_index } if *local_index == retained)).count(), 1,
                "the retained proof root is never cleared or reinitialized on a loop backedge");
            let loop_start = repeated.iter().enumerate().find_map(|(start, op)| {
                (matches!(op, Operator::Loop { .. }) && start < initialize
                    && initialize < frame_end(repeated, start)).then_some(start)
            }).expect("the cache proof is retained within the actual emitted source loop");
            let (call, _) = runtime.named("helper::proxy_call");
            let call = program.imported_runtime_function(&runtime, call);
            let object = runtime.witness_type(GcSnapshotLayout::Object);
            let live_storage = repeated[loop_start..initialize].iter().rposition(|op| matches!(op,
                Operator::StructGet { struct_type_index, field_index }
                    if *struct_type_index == object && *field_index == GcSnapshotField::ObjectProperties.index()))
                .map(|offset| loop_start + offset).expect("the loop reloads actual current storage");
            assert!(calls(&repeated[loop_start..live_storage], call),
                "current storage admission remains after the observable source callback");
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
            assert!(length[..live_length].iter().any(|op| matches!(op,
                Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                    if *index == array)), "the actual Array type dominates the intrinsic field load");
            assert!(calls(&length[live_length..], fallback),
                "other actual receivers still reach the complete saved-value Get fallback");
        })
        .expect("compiler worker starts")
        .join()
        .expect("property-read emission does not panic");
}
