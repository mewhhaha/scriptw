//! P calls shared R publication, physical Number/bitmap and descriptor owners.
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy, RUNTIME_IMPORT_NAMESPACE,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, ExprIr, SpecOperationIr, StatementIr};
use std::collections::BTreeMap;
use wasmparser::{
    ExternalKind, HeapType, KnownCustom, Name, Operator, Parser, Payload, TypeRef, UnpackedIndex,
    ValType, Validator, WasmFeatures,
};

#[derive(Default)]
struct Module<'a> {
    types: Vec<wasmparser::SubType>,
    type_group_sizes: Vec<usize>,
    names: BTreeMap<u32, String>,
    imports: BTreeMap<u32, (String, String)>,
    functions: BTreeMap<u32, Vec<Operator<'a>>>,
    body_bytes: BTreeMap<u32, usize>,
    exports: BTreeMap<String, u32>,
    globals: Vec<wasmparser::GlobalType>,
    global_exports: BTreeMap<String, u32>,
}

impl<'a> Module<'a> {
    fn read(bytes: &'a [u8]) -> Self {
        let mut output = Self::default();
        let mut next = 0;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.unwrap() {
                Payload::TypeSection(reader) => {
                    for group in reader {
                        let group = group.unwrap();
                        output.type_group_sizes.push(group.types().len());
                        output.types.extend(group.types().cloned());
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import.unwrap();
                        match import.ty {
                            TypeRef::Func(_) | TypeRef::FuncExact(_) => {
                                output
                                    .imports
                                    .insert(next, (import.module.into(), import.name.into()));
                                next += 1;
                            }
                            TypeRef::Global(ty) => output.globals.push(ty),
                            _ => {}
                        }
                    }
                }
                Payload::GlobalSection(reader) => output
                    .globals
                    .extend(reader.into_iter().map(|global| global.unwrap().ty)),
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.unwrap();
                        match export.kind {
                            ExternalKind::Func => {
                                output.exports.insert(export.name.into(), export.index);
                            }
                            ExternalKind::Global => {
                                output
                                    .global_exports
                                    .insert(export.name.into(), export.index);
                            }
                            _ => {}
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    output.body_bytes.insert(next, body.range().len());
                    output.functions.insert(
                        next,
                        body.get_operators_reader()
                            .unwrap()
                            .into_iter()
                            .map(Result::unwrap)
                            .collect(),
                    );
                    next += 1;
                }
                Payload::CustomSection(section) => {
                    if let KnownCustom::Name(reader) = section.as_known() {
                        for subsection in reader {
                            if let Name::Function(names) = subsection.unwrap() {
                                for name in names {
                                    let name = name.unwrap();
                                    output.names.insert(name.index, name.name.into());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        output
    }

    fn named(&self, prefix: &str) -> (u32, &[Operator<'a>]) {
        let names = self
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
            names.len(),
            1,
            "one actual body for {prefix}; matching names: {names:?}"
        );
        let index = *names[0].0;
        (index, self.functions.get(&index).unwrap())
    }

    fn witness_type(&self, layout: GcSnapshotLayout) -> u32 {
        let index = *self
            .global_exports
            .get(layout.witness_export())
            .unwrap_or_else(|| panic!("observation owner must export {:?} witness", layout));
        let ValType::Ref(reference) = self.globals[index as usize].content_type else {
            panic!("typed GC witness");
        };
        let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
            panic!("concrete witness");
        };
        index
    }

    fn imported_runtime(&self, runtime: &Self, definition: u32) -> u32 {
        let indices = self
            .imports
            .iter()
            .filter_map(|(&index, (namespace, name))| {
                (namespace == RUNTIME_IMPORT_NAMESPACE
                    && runtime.exports.get(name) == Some(&definition))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            indices.len(),
            1,
            "actual exported R definition is linked into P"
        );
        indices[0]
    }
}

fn calls(body: &[Operator<'_>], target: u32) -> bool {
    body.iter().any(|operator| {
        matches!(operator,
        Operator::Call { function_index } | Operator::ReturnCall { function_index }
            if *function_index == target)
    })
}

#[test]
fn real_push_and_join_builtins_reach_guarded_canonical_packed_kernels() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let lowered = lower(
                &parse(
                    "[Array.prototype.push, Array.prototype.join];",
                    ParseOptions::script(),
                )
                .unwrap(),
            );
            let artifact = emit_with_rooted_snapshot(
                &lowered,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let runtime = artifact.runtime().unwrap();
            for bytes in [&artifact.bytes[..], runtime.bytes()] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .expect("actual packed builtin modules validate");
            }
            let runtime = Module::read(runtime.bytes());
            let array = runtime.witness_type(GcSnapshotLayout::Array);
            let storage = runtime.witness_type(GcSnapshotLayout::ArrayStorage);
            let values = runtime.witness_type(GcSnapshotLayout::ArrayValues);
            let units = runtime.witness_type(GcSnapshotLayout::CodeUnits);
            let (publish, _) = runtime.named("helper::array_indexed_publish_default_data");
            let (own_descriptor, _) = runtime.named("helper::ordinary_own_descriptor");
            let (ordinary_set, _) = runtime.named("helper::ordinary_set");
            let (object_write, object_write_body) = runtime.named("helper::object_write");
            let (_, push) = runtime.named("builtin::Array.prototype.push");
            let publication = push
                .iter()
                .position(|op| matches!(op, Operator::Call { function_index } if *function_index == publish))
                .expect("the actual push builtin reaches the sole canonical growth/publication helper");
            for (owner, field) in [
                (array, scriptw_aot_wasm::GcSnapshotField::ArrayLengthWritable),
                (storage, scriptw_aot_wasm::GcSnapshotField::ArrayCount),
            ] {
                assert!(push[..publication].iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == owner && *field_index == field.index())),
                    "push checks {field:?} before any canonical publication");
            }
            assert!(calls(&push[..publication], own_descriptor),
                "live ordinary prototype descriptors are checked before publication");
            // emit_object_write_strict enters ObjectWrite with its strict flag;
            // ObjectWrite owns rejection around the canonical [[Set]] result.
            // Follow both real encoded edges instead of assuming push calls
            // the lower OrdinarySet owner directly.
            assert_eq!(push[publication..].iter().filter(|op| matches!(op,
                Operator::Call { function_index } | Operator::ReturnCall { function_index }
                    if *function_index == object_write)).count(), 2,
                "declined push retains indexed Set and the final length Set, including zero arguments");
            assert!(calls(object_write_body, ordinary_set),
                "the rejection owner reaches the canonical ordinary Set dispatcher");
            assert!(push[publication..].iter().any(|op| matches!(op,
                Operator::StructSet { struct_type_index, field_index }
                    if *struct_type_index == array && *field_index == scriptw_aot_wasm::GcSnapshotField::ArrayLength.index())),
                "successful packed append publishes the canonical receiver length");
            let (_, join) = runtime.named("builtin::Array.prototype.join");
            let (packed_join, kernel) = runtime.named("helper::array_packed_join");
            assert!(calls(join, packed_join),
                "actual builtin join calls the sole shared String-producing kernel");
            let first_value_read = kernel.iter().position(|op| matches!(op,
                Operator::ArrayGet { array_type_index } if *array_type_index == values))
                .expect("shared join kernel validates packed immutable value slots");
            let exact_allocation = kernel.iter().position(|op| matches!(op,
                Operator::ArrayNew { array_type_index } if *array_type_index == units))
                .expect("shared join kernel allocates a result backing");
            assert!(first_value_read < exact_allocation,
                "packed value validation precedes the first result allocation");
            assert!(kernel[exact_allocation..].iter().any(|op| matches!(op,
                Operator::ArrayGet { array_type_index } if *array_type_index == values)),
                "the exact-size result consumes the validated packed backing");
            assert!(kernel[exact_allocation..].iter().any(|op| matches!(op,
                Operator::ArrayCopy { array_type_index_dst, array_type_index_src }
                    if *array_type_index_dst == units && *array_type_index_src == units)),
                "join copies exact UTF16 units through the private String construction owner");
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn source_join_calls_reach_shared_kernel_only_after_actual_code_realm_and_array_guards() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let lowered = lower(&parse(
                "function joinOne(){var a=['x',null,'y'];var text=a.join(':');return text;} \
                 function joinZero(){var a=['x'];var text=a.join();return text;} \
                 function joinExtra(){var a=['x'];var text=a.join(':',undefined);return text;} \
                 [joinOne,joinZero,joinExtra];",
                ParseOptions::script(),
            ).unwrap());
            let artifact = emit_with_rooted_snapshot(&lowered, PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [&artifact.bytes[..], runtime_bytes] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let runtime = Module::read(runtime_bytes);
            let program = Module::read(&artifact.bytes);
            let (kernel, _) = runtime.named("helper::array_packed_join");
            let kernel = program.imported_runtime(&runtime, kernel);
            let (call, _) = runtime.named("helper::proxy_call");
            let call = program.imported_runtime(&runtime, call);
            let array = runtime.witness_type(GcSnapshotLayout::Array);
            let code = runtime.witness_type(GcSnapshotLayout::Code);
            let context = runtime.witness_type(GcSnapshotLayout::Context);
            for name in ["js::joinOne#", "js::joinZero#"] {
                let (_, body) = program.named(name);
                let direct = body.iter().position(|op| matches!(op,
                    Operator::Call { function_index } if *function_index == kernel))
                    .expect("ordinary source join reaches the shared packed kernel");
                assert!(body[..direct].iter().any(|op| matches!(op,
                    Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                        if *index == array)), "actual concrete Array guard precedes kernel call");
                assert!(body[..direct].iter().filter(|op| matches!(op,
                    Operator::StructGet { struct_type_index, .. } if *struct_type_index == code)).count() >= 2,
                    "actual immutable code kind and native builtin identity precede kernel call");
                assert!(body[..direct].iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == context && *field_index == scriptw_aot_wasm::GcSnapshotField::ContextRealm.index())),
                    "actual defining Realm is read before the direct kernel route");
                assert!(body[..direct].iter().any(|op| matches!(op, Operator::RefEq)),
                    "actual defining Realm identity is compared before the direct route");
                assert!(calls(&body[direct..], call),
                    "unhandled kernel/identity checks retain the saved-operand ordinary call");
            }
            let (_, extra) = program.named("js::joinExtra#");
            assert!(!calls(extra, kernel), "extra arguments decline before direct-call argument evaluation");
            assert!(calls(extra, call), "extra arguments retain ordinary invocation");
        }).unwrap().join().unwrap();
}

#[test]
fn compiled_numbers_use_bitmap_arrays_and_keep_real_descriptor_fallback() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        // Publish callable values without directly calling them. This retains
        // their generic bodies and avoids direct-call-specialized clones; the
        // operand assertions must describe that one declared generic body.
        let parsed = parse("function make(value){var a=[value];return a;}function read(array,index){return array[index];}[make,read];", ParseOptions::script()).unwrap();
        let lowered = lower(&parsed);
        let read_sources = lowered.script.as_ref().unwrap().functions.iter()
            .filter(|function| function.name == "read").collect::<Vec<_>>();
        assert_eq!(read_sources.len(), 1, "fixture retains one generic read IR carrier");
        let read_result = read_sources[0].body.statements.iter().find_map(|statement| match statement {
            StatementIr::Return(value) => Some(value),
            _ => None,
        }).expect("read source has its actual return expression");
        let ExprIr::SpecOperation { operation: SpecOperationIr::GetV, operands } = &read_result.expr else {
            panic!("unknown receiver/computed key must retain canonical GetV, got {read_result:?}");
        };
        assert_eq!(operands.len(), 2, "GetV retains target and key operands");
        assert!(!matches!(&operands[1].expr, ExprIr::String(_)),
            "this nonliteral key reaches ObjectRead after GetV's operand/ToObject/ToPropertyKey steps");
        let artifact = emit_with_rooted_snapshot(&lowered, PromiseRejectionPolicy::Ignore,
            &scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
        // Preserve the same generic signature, locals, argument read and
        // callable publication protocol. Its empty literal owns any ordinary
        // activation descriptors independently of element publication.
        let control = lower(&parse("function make(value){var a=[];value;return a;}function read(array,index){return array[index];}[make,read];", ParseOptions::script()).unwrap());
        let control_artifact = emit_with_rooted_snapshot(&control, PromiseRejectionPolicy::Ignore,
            &scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
        let runtime = artifact.runtime().unwrap();
        let features = WasmFeatures::default() | WasmFeatures::THREADS | WasmFeatures::MULTI_MEMORY
            | WasmFeatures::REFERENCE_TYPES | WasmFeatures::FUNCTION_REFERENCES | WasmFeatures::GC
            | WasmFeatures::EXCEPTIONS | WasmFeatures::TAIL_CALL;
        for bytes in [&artifact.bytes[..], &runtime.bytes()[..], &control_artifact.bytes[..]] {
            Validator::new_with_features(features).validate_all(bytes).unwrap();
        }
        let program = Module::read(&artifact.bytes);
        let control_program = Module::read(&control_artifact.bytes);
        let runtime = Module::read(runtime.bytes());
        // Linked R owns snapshot globals, and the Engine observes that R
        // instance. P imports R's core and appends program-only signatures.
        // Verify the actual encoded index/type/recursion-group agreement before
        // using R's witness indices to classify any instruction in P.
        assert!(program.types.len() >= runtime.types.len());
        assert_eq!(&program.types[..runtime.types.len()], &runtime.types,
            "P and R declare the exact same core types at the same indices");
        assert!(control_program.types.len() >= runtime.types.len());
        assert_eq!(&control_program.types[..runtime.types.len()], &runtime.types,
            "the matched control uses the same concrete descriptor index");
        assert!(program.type_group_sizes.len() >= runtime.type_group_sizes.len());
        assert_eq!(&program.type_group_sizes[..runtime.type_group_sizes.len()], &runtime.type_group_sizes,
            "P's program-only type suffix must not alter shared recursive groups");
        let numbers = runtime.witness_type(GcSnapshotLayout::ArrayNumbers);
        let presence = runtime.witness_type(GcSnapshotLayout::ArrayNumberPresence);
        let values = runtime.witness_type(GcSnapshotLayout::ArrayValues);
        let stored = runtime.witness_type(GcSnapshotLayout::Stored);
        let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
        let dense = runtime.witness_type(GcSnapshotLayout::ArrayDense);
        assert_ne!(numbers, presence, "Number payload and hole presence are distinct arrays");
        let (_, make) = program.named("js::make#");
        let (_, control_make) = control_program.named("js::make#");
        let (default_publish, default_publish_body) = runtime.named("helper::array_indexed_publish_default_data");
        assert!(calls(make, program.imported_runtime(&runtime, default_publish)),
            "the fresh producer calls the actual linked R default-data publication owner");
        assert!(!calls(control_make, control_program.imported_runtime(&runtime, default_publish)),
            "the empty-array control has no element publication");
        for array in [numbers, presence] {
            assert!(default_publish_body.iter().any(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == array)), "shared R publication writes its physical packed backing");
            assert!(!make.iter().any(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == array)), "P never duplicates the shared packed publication CFG");
        }
        assert!(default_publish_body.iter().any(|op| matches!(op, Operator::ArrayNew { array_type_index } if *array_type_index == values)), "the shared owner grows a nullable immutable-value table for general ordinary data");
        assert!(default_publish_body.iter().any(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == values)), "general data publication writes the actual typed slot table");
        assert!(default_publish_body.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == stored)), "numeric promotion and incoming general values publish real immutable StoredValues");
        let (publish, publish_body) = runtime.named("helper::array_indexed_publish");
        assert!(calls(default_publish_body, publish), "ineligible default-data values retain R's actual descriptor publication fallback");
        assert!(default_publish_body.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)), "the shared default-data owner boxes the incoming descriptor before its raw publication fallback");
        let descriptor_count = |body: &[Operator<'_>]| body.iter().filter(|op|
            matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)).count();
        assert_eq!(descriptor_count(make), descriptor_count(control_make),
            "element publication adds no P descriptor allocation beyond the matched empty-array activation");
        assert!(publish_body.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)), "R's cold promotion constructs real descriptors for previously packed entries");
        assert!(publish_body.iter().any(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == dense)), "cold descriptors enter the actual dense store");
        let (_, source_read) = program.named("js::read#");
        let (object_read, read) = runtime.named("helper::object_read");
        assert!(calls(source_read, program.imported_runtime(&runtime, object_read)),
            "the canonical nonliteral GetV source body calls the actual linked R ObjectRead definition");
        for array in [numbers, presence, values] {
            assert!(read.iter().any(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == array)), "ordinary read reaches encoded packed value/presence loads");
        }
        let (_, delete) = runtime.named("helper::array_indexed_delete");
        assert!(delete.iter().any(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == values)), "general deletion clears the actual nullable value edge");
        assert!(delete.iter().any(|op| matches!(op, Operator::ArraySet { array_type_index } if *array_type_index == presence)), "deletion clears explicit presence rather than inventing a Number sentinel");
    }).unwrap().join().unwrap();
}

#[test]
fn many_short_array_literals_keep_publication_cfg_in_one_runtime_body() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            const LITERALS: usize = 256;
            const ELEMENTS: usize = 3;
            // This fixed source stays below the current Engine's 1-MiB
            // large-body native-policy boundary. Unrelated lowering may
            // change exact operator counts without duplicating publication.
            const SOURCE_BODY_BYTE_CEILING: usize = 1024 * 1024;
            // Each literal is far below passive-recipe admission. One source
            // body must remain cheap even when it contains many such literals.
            let mut source = String::from("function make(consume){");
            for ordinal in 0..LITERALS {
                source.push_str(&format!(
                    "consume([{ordinal},{},{}]);",
                    ordinal + 1,
                    ordinal + 2
                ));
            }
            source.push_str("}make;");
            let lowered = lower(&parse(&source, ParseOptions::script()).unwrap());
            let artifact = emit_with_rooted_snapshot(
                &lowered,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let program = Module::read(&artifact.bytes);
            let runtime = Module::read(artifact.runtime().unwrap().bytes());
            let (publish, publication) =
                runtime.named("helper::array_indexed_publish_default_data");
            let imported_publish = program.imported_runtime(&runtime, publish);
            let (make_index, make) = program.named("js::make#");
            assert!(program.types.len() >= runtime.types.len());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types,
                "backing witnesses retain their actual shared core indices");
            for array in [
                runtime.witness_type(GcSnapshotLayout::ArrayNumbers),
                runtime.witness_type(GcSnapshotLayout::ArrayNumberPresence),
            ] {
                assert!(!make.iter().any(|op| matches!(op,
                    Operator::ArraySet { array_type_index } if *array_type_index == array)),
                    "the many-literal P body has no duplicated packed backing publication");
            }
            let publications = make
                .iter()
                .filter(|operator| {
                    matches!(operator,
                        Operator::Call { function_index }
                        | Operator::ReturnCall { function_index }
                        if *function_index == imported_publish)
                })
                .count();
            assert_eq!(
                publications,
                LITERALS * ELEMENTS,
                "every occupied source element reaches the actual R publication owner"
            );
            assert!(
                publication
                    .iter()
                    .any(|op| matches!(op, Operator::Loop { .. })),
                "the one R publication body owns geometric growth rather than deleting it"
            );
            assert!(
                program.body_bytes[&make_index] < SOURCE_BODY_BYTE_CEILING,
                "the fixed many-short-literal control stays below the large-body native-policy boundary, got {} bytes",
                program.body_bytes[&make_index]
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn bounded_dynamic_literals_guard_one_complete_number_backing_and_retain_the_runtime_fallback() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // Publishing the functions keeps their original generic argument
            // reads and prevents direct-call specialization from proving tags.
            let source = "function three(a,b,c){return[a,b,c];}\
                function four(a,b,c,d){return[a,b,c,d];}\
                function eight(x){return[x,x,x,x,x,x,x,x];}\
                function hole(x){return[x,,x,x];}\
                function long(x){return[x,x,x,x,x,x,x,x,x];}\
                function empty(){return[];}[three,four,eight,hole,long,empty];";
            let lowered = lower(&parse(source, ParseOptions::script()).unwrap());
            let artifact = emit_with_rooted_snapshot(
                &lowered,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let features = WasmFeatures::default()
                | WasmFeatures::THREADS
                | WasmFeatures::MULTI_MEMORY
                | WasmFeatures::REFERENCE_TYPES
                | WasmFeatures::FUNCTION_REFERENCES
                | WasmFeatures::GC
                | WasmFeatures::EXCEPTIONS
                | WasmFeatures::TAIL_CALL;
            for bytes in [&artifact.bytes[..], artifact.runtime().unwrap().bytes()] {
                Validator::new_with_features(features)
                    .validate_all(bytes)
                    .unwrap();
            }
            let program = Module::read(&artifact.bytes);
            let runtime = Module::read(artifact.runtime().unwrap().bytes());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let numbers = runtime.witness_type(GcSnapshotLayout::ArrayNumbers);
            let presence = runtime.witness_type(GcSnapshotLayout::ArrayNumberPresence);
            let storage = runtime.witness_type(GcSnapshotLayout::ArrayStorage);
            let (publish, _) = runtime.named("helper::array_indexed_publish_default_data");
            let publish = program.imported_runtime(&runtime, publish);
            for (name, length, capacity) in [("js::three#", 3, 4), ("js::four#", 4, 4), ("js::eight#", 8, 8)] {
                let (_, body) = program.named(name);
                let backing = body
                    .iter()
                    .position(|op| matches!(op, Operator::ArrayNewFixed { array_type_index, array_size }
                        if *array_type_index == numbers && *array_size == capacity))
                    .expect("one power-of-two Number backing in the original generic source body");
                let guard = body[..backing]
                    .iter()
                    .rposition(|op| matches!(op, Operator::If { .. }))
                    .expect("the Number backing is guarded");
                // One combined run-time test checks actual captured Value tags;
                // unknown parameters may subsequently be strings, BigInts or refs.
                let condition_ops = (length * 4 - 1) as usize;
                let condition = &body[guard - condition_ops..guard];
                assert_eq!(condition.iter().filter(|op| matches!(op,
                    Operator::I32Const { value } if *value == scriptw_ir::ValueKind::Number.tag() as i32)).count(), length as usize);
                assert_eq!(condition.iter().filter(|op| matches!(op, Operator::I32Eq)).count(), length as usize);
                assert_eq!(condition.iter().filter(|op| matches!(op, Operator::I32And)).count(), length as usize - 1);
                assert!(body[backing..].iter().any(|op| matches!(op,
                    Operator::ArrayNewFixed { array_type_index, array_size: 1 }
                        if *array_type_index == presence)));
                for field in [
                    scriptw_aot_wasm::GcSnapshotField::ArrayNumbers,
                    scriptw_aot_wasm::GcSnapshotField::ArrayNumberPresent,
                    scriptw_aot_wasm::GcSnapshotField::ArrayCount,
                ] {
                    assert!(body[backing..].iter().any(|op| matches!(op,
                        Operator::StructSet { struct_type_index, field_index }
                            if *struct_type_index == storage && *field_index == field.index())),
                        "complete Number backing publication writes {field:?}");
                }
                let alternative = body[backing..]
                    .iter()
                    .position(|op| matches!(op, Operator::Else))
                    .map(|offset| backing + offset)
                    .expect("non-Numbers retain their ordinary fallback");
                let publications = body.iter().filter(|op| matches!(op,
                    Operator::Call { function_index } | Operator::ReturnCall { function_index }
                        if *function_index == publish)).count();
                assert_eq!(publications, length as usize);
                assert!(!calls(&body[..alternative], publish));
                assert!(calls(&body[alternative..], publish));
                for array in [numbers, presence] {
                    assert!(!body.iter().any(|op| matches!(op,
                        Operator::ArraySet { array_type_index } if *array_type_index == array)),
                        "bounded source bodies never duplicate the backing-growth CFG");
                }
            }
            for (name, occupied) in [("js::hole#", 3), ("js::long#", 9), ("js::empty#", 0)] {
                let (_, body) = program.named(name);
                assert!(!body.iter().any(|op| matches!(op,
                    Operator::ArrayNewFixed { array_type_index, .. }
                        if *array_type_index == numbers || *array_type_index == presence)),
                    "holes, empty and larger literals keep their existing path");
                assert_eq!(body.iter().filter(|op| matches!(op,
                    Operator::Call { function_index } | Operator::ReturnCall { function_index }
                        if *function_index == publish)).count(), occupied);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
