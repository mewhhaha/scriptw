use super::*;
use scriptw_ir::{ExprIr, StatementIr};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use wasmtime::{
    ArrayRef, ArrayRefPre, AsContextMut, GcHeapOutOfMemory, OwnedRooted, Rooted, StructRef,
};

fn append_uleb(bytes: &mut Vec<u8>, mut value: u32) {
    loop {
        let low = (value & 0x7f) as u8;
        value >>= 7;
        bytes.push(low | if value == 0 { 0 } else { 0x80 });
        if value == 0 {
            break;
        }
    }
}
fn read_uleb(bytes: &[u8], cursor: &mut usize) -> u32 {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
    }
    panic!("invalid test artifact length")
}

fn function_names(bytes: &[u8]) -> BTreeMap<u32, String> {
    let mut names = BTreeMap::new();
    for payload in WasmParser::new(0).parse_all(bytes) {
        let WasmPayload::CustomSection(section) = payload.unwrap() else {
            continue;
        };
        let wasmparser::KnownCustom::Name(subsections) = section.as_known() else {
            continue;
        };
        for subsection in subsections {
            if let wasmparser::Name::Function(functions) = subsection.unwrap() {
                for function in functions {
                    let function = function.unwrap();
                    names.insert(function.index, function.name.to_owned());
                }
            }
        }
    }
    names
}

/// Resolve the real R export by its physical name-section index. R already
/// exports its functions; no test alias or P-owned stand-in is needed.
fn runtime_helper_export(bytes: &[u8], helper: &str) -> String {
    let names = function_names(bytes);
    let indices: Vec<_> = names
        .iter()
        .filter_map(|(&index, name)| (name == helper).then_some(index))
        .collect();
    let [index] = indices.as_slice() else {
        panic!("one actual runtime helper {helper}: {indices:?}")
    };
    let name = format!("f{index}");
    let mut found = 0;
    for payload in WasmParser::new(0).parse_all(bytes) {
        if let WasmPayload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.name == name {
                    assert_eq!(export.kind, wasmparser::ExternalKind::Func);
                    assert_eq!(export.index, *index);
                    found += 1;
                }
            }
        }
    }
    assert_eq!(found, 1, "physical R helper export {helper}");
    name
}

/// Stop a copied main immediately before its one entry Evaluate call. The
/// original trusted IR and emitted artifact retain the mandatory entry owner;
/// private tests can inspect allocation and instantiation without executing source.
fn paused_before_entry_evaluation(bytes: &[u8]) -> (Vec<u8>, u32) {
    let evaluate = *function_names(bytes)
        .iter()
        .find(|(_, name)| name.as_str() == "helper::module_evaluate")
        .unwrap()
        .0;
    let mut types = Vec::new();
    let mut function_types = Vec::new();
    let mut imported = 0;
    let mut global_count = 0;
    let mut main = None;
    for payload in WasmParser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            WasmPayload::TypeSection(groups) => {
                for group in groups {
                    types.extend(group.unwrap().into_types());
                }
            }
            WasmPayload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    match import.unwrap().ty {
                        wasmparser::TypeRef::Func(index)
                        | wasmparser::TypeRef::FuncExact(index) => {
                            function_types.push(index);
                            imported += 1;
                        }
                        wasmparser::TypeRef::Global(_) => global_count += 1,
                        _ => {}
                    }
                }
            }
            WasmPayload::GlobalSection(globals) => global_count += globals.count(),
            WasmPayload::FunctionSection(functions) => {
                function_types.extend(functions.into_iter().map(Result::unwrap));
            }
            WasmPayload::ExportSection(exports) => {
                for export in exports {
                    let export = export.unwrap();
                    if export.name == "main" {
                        assert_eq!(export.kind, wasmparser::ExternalKind::Func);
                        assert!(main.replace(export.index).is_none());
                    }
                }
            }
            _ => {}
        }
    }
    let main = main.unwrap();
    let main_type = types[function_types[main as usize] as usize].unwrap_func();
    assert!(main_type.params().is_empty());
    assert_eq!(
        main_type.results(),
        &[
            wasmparser::ValType::I32,
            wasmparser::ValType::I64,
            wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
            wasmparser::ValType::I32,
            wasmparser::ValType::I32,
        ]
    );
    let evaluate_type = types[function_types[evaluate as usize] as usize].unwrap_func();
    assert!(
        matches!(evaluate_type.params(), [wasmparser::ValType::Ref(record), wasmparser::ValType::Ref(program)]
        if !record.is_nullable() && !program.is_nullable() && record != program),
        "typed ModuleRecord and its explicit ProgramInstance inputs"
    );
    assert_eq!(evaluate_type.results(), main_type.results());

    let mut defined = 0;
    let mut main_body = None;
    let mut calls = Vec::new();
    for payload in WasmParser::new(0).parse_all(bytes) {
        if let WasmPayload::CodeSectionEntry(body) = payload.unwrap() {
            let index = imported + defined;
            defined += 1;
            if index != main {
                continue;
            }
            main_body = Some(body.range());
            let mut operators = body.get_operators_reader().unwrap();
            while !operators.eof() {
                let start = operators.original_position();
                let operator = operators.read().unwrap();
                if matches!(operator, wasmparser::Operator::Call { function_index } if function_index == evaluate)
                {
                    calls.push(start..operators.original_position());
                }
            }
        }
    }
    assert_eq!(calls.len(), 1, "replace exactly the entry Evaluate call");
    let call = calls.pop().unwrap();
    let main_body = main_body.unwrap();
    assert!(main_body.start <= call.start && call.end <= main_body.end);

    let mut rewritten = bytes[..8].to_vec();
    let mut cursor = 8;
    let mut replacements = 0;
    let mut added_global = false;
    // Test-only (mut eqref), initialized null. The intercepted typed input is
    // its only writer; the product publishes no private ModuleRecord export.
    let root_global = [0x6d, 0x01, 0xd0, 0x6d, 0x0b];
    while cursor < bytes.len() {
        let section_start = cursor;
        let id = bytes[cursor];
        cursor += 1;
        let length = read_uleb(bytes, &mut cursor) as usize;
        let end = cursor + length;
        if id == 6 {
            let count = read_uleb(bytes, &mut cursor);
            let mut globals = Vec::new();
            append_uleb(&mut globals, count + 1);
            globals.extend_from_slice(&bytes[cursor..end]);
            globals.extend_from_slice(&root_global);
            rewritten.push(6);
            append_uleb(&mut rewritten, globals.len().try_into().unwrap());
            rewritten.extend_from_slice(&globals);
            added_global = true;
            cursor = end;
            continue;
        }
        if id == 7 && !added_global {
            rewritten.extend_from_slice(&[6, 6, 1]);
            rewritten.extend_from_slice(&root_global);
            added_global = true;
        }
        if id != 10 {
            rewritten.extend_from_slice(&bytes[section_start..end]);
            cursor = end;
            continue;
        }
        let count = read_uleb(bytes, &mut cursor);
        let mut code = Vec::new();
        append_uleb(&mut code, count);
        for ordinal in 0..count {
            let body_length_start = cursor;
            let body_length = read_uleb(bytes, &mut cursor) as usize;
            let body_end = cursor + body_length;
            if imported + ordinal == main {
                assert_eq!(cursor..body_end, main_body);
                let mut body = bytes[cursor..call.start].to_vec();
                // Retain the actual typed record, then return a normal
                // Undefined Completion without entering source or draining jobs.
                body.push(0x1a); // drop the final P operand; the graph retains it
                body.push(0x24); // global.set
                append_uleb(&mut body, global_count);
                body.extend_from_slice(&[0x41, 0, 0x42, 0, 0xd0, 0x6d, 0x41, 0, 0x41, 0, 0x0f]);
                body.extend_from_slice(&bytes[call.end..body_end]);
                append_uleb(&mut code, body.len().try_into().unwrap());
                code.extend_from_slice(&body);
                replacements += 1;
            } else {
                code.extend_from_slice(&bytes[body_length_start..body_end]);
            }
            cursor = body_end;
        }
        assert_eq!(cursor, end);
        rewritten.push(id);
        append_uleb(&mut rewritten, code.len().try_into().unwrap());
        rewritten.extend_from_slice(&code);
    }
    assert_eq!(replacements, 1);
    assert!(added_global);
    (rewritten, global_count)
}

/// Export private operations and the single module root from a copied artifact.
/// Product artifacts never publish these names or private memory mutation APIs.
fn with_private_exports(bytes: &[u8], module_root: u32) -> Vec<u8> {
    let names = function_names(bytes);
    let mut exports = vec![("test_module_record", 3, module_root)];
    for (export, helper) in [
        ("test_module_evaluate", "helper::module_evaluate"),
        ("test_module_execute", "helper::module_execute"),
        ("test_module_ready", "helper::module_ready"),
        ("test_module_initialize", "helper::module_initialize"),
    ] {
        let index = *names
            .iter()
            .find(|(_, name)| name.as_str() == helper)
            .unwrap()
            .0;
        exports.push((export, 0, index));
    }
    let mut rewritten = bytes[..8].to_vec();
    let mut cursor = 8;
    let mut found = false;
    while cursor < bytes.len() {
        let id = bytes[cursor];
        cursor += 1;
        let length = read_uleb(bytes, &mut cursor) as usize;
        let end = cursor + length;
        if id == 7 {
            found = true;
            let count = read_uleb(bytes, &mut cursor);
            let mut body = Vec::new();
            append_uleb(&mut body, count + exports.len() as u32);
            body.extend_from_slice(&bytes[cursor..end]);
            for &(name, kind, index) in &exports {
                append_uleb(&mut body, name.len() as u32);
                body.extend_from_slice(name.as_bytes());
                body.push(kind);
                append_uleb(&mut body, index);
            }
            rewritten.push(id);
            append_uleb(&mut rewritten, body.len() as u32);
            rewritten.extend_from_slice(&body);
        } else {
            rewritten.push(id);
            append_uleb(&mut rewritten, length as u32);
            rewritten.extend_from_slice(&bytes[cursor..end]);
        }
        cursor = end;
    }
    assert!(found);
    rewritten
}

// Ordinals of the actual typed GC schemas. ModuleRecord retains SOURCE_CELL
// at ordinal 6 after the two namespace cells. Reads/writes use Wasmtime's field
// APIs, so a field-kind mismatch fails rather than indexing unrelated bytes.
mod record_field {
    pub const GRAPH: usize = 0;
    pub const REALM: usize = 2;
    pub const ENVIRONMENT: usize = 3;
    pub const ACTIVATION_KIND: usize = 7;
    pub const ASYNC_ACTIVATION: usize = 9;
    pub const EVALUATION_PROMISE: usize = 12;
    pub const STATE: usize = 13;
    pub const COMPLETION: usize = 14;
    pub const BODY_STATE: usize = 15;
}
mod graph_field {
    pub const MODULES: usize = 0;
    pub const PROGRAM: usize = 4;
    pub const NEXT: usize = 5;
}
mod realm_field {
    pub const MODULES: usize = 7;
}
mod activation_field {
    pub const FRAME: usize = 0;
    pub const PROMISE: usize = 3;
    pub const RESUME_POINT: usize = 4;
    pub const COMPLETED: usize = 5;
    pub const MODULE_ENTRY_MODE: usize = 7;
}

fn structure(store: impl AsContext, value: WasmtimeVal) -> Rooted<StructRef> {
    let WasmtimeVal::AnyRef(Some(value)) = value else {
        panic!("non-null GC record")
    };
    value.as_struct(store).unwrap().expect("GC struct")
}
fn reference_field(
    store: &mut impl AsContextMut,
    record: Rooted<StructRef>,
    index: usize,
) -> Rooted<StructRef> {
    let value = record.field(&mut *store, index).unwrap();
    structure(store, value)
}
fn scalar_field(store: &mut impl AsContextMut, record: Rooted<StructRef>, index: usize) -> i32 {
    record
        .field(store, index)
        .unwrap()
        .i32()
        .expect("typed scalar field")
}
fn null_field(store: &mut impl AsContextMut, record: Rooted<StructRef>, index: usize) -> bool {
    matches!(
        record.field(store, index).unwrap(),
        WasmtimeVal::AnyRef(None)
    )
}
// ProgramInstance hooks are typed funcrefs, never GC anyrefs. Validate the
// actual role's complete leading owner/module shape before testing nullness.
fn null_module_body_reaction(
    store: &mut impl AsContextMut,
    program: Rooted<StructRef>,
    module: Rooted<StructRef>,
) -> bool {
    const MODULE_BODY_REACTION: usize = 3;
    let program_type = program.ty(&*store).unwrap();
    assert_eq!(
        program_type.fields().len(),
        scriptw_aot_wasm::GcHostLayout::ProgramInstance
            .field_count()
            .unwrap() as usize,
        "ProgramInstance schema",
    );
    let field = program_type.field(MODULE_BODY_REACTION).unwrap();
    assert_eq!(field.mutability(), wasmtime::Mutability::Const);
    let hook = field.element_type().unwrap_val_type().unwrap_ref();
    assert!(hook.is_nullable(), "absent hook retains its typed null");
    let signature = hook.heap_type().unwrap_concrete_func();
    let parameters: Vec<_> = signature.params().collect();
    assert_eq!(parameters.len(), 6, "P, ModuleRecord, rejected, Value");
    assert_eq!(signature.results().len(), 0, "ModuleBodyReaction is void");
    let owner = parameters[0].unwrap_ref();
    assert!(!owner.is_nullable());
    assert!(wasmtime::StructType::eq(
        owner.heap_type().unwrap_concrete_struct(),
        &program_type,
    ));
    let record = parameters[1].unwrap_ref();
    assert!(!record.is_nullable());
    assert!(wasmtime::StructType::eq(
        record.heap_type().unwrap_concrete_struct(),
        &module.ty(&*store).unwrap(),
    ));
    assert!(matches!(parameters[2], wasmtime::ValType::I32));
    assert!(matches!(parameters[3], wasmtime::ValType::I32));
    assert!(matches!(parameters[4], wasmtime::ValType::I64));
    parameters[5].unwrap_ref();
    matches!(
        program.field(store, MODULE_BODY_REACTION).unwrap(),
        WasmtimeVal::FuncRef(None)
    )
}

fn same_record(store: impl AsContext, left: Rooted<StructRef>, right: Rooted<StructRef>) {
    assert!(Rooted::ref_eq(store, &left, &right).unwrap());
}
fn call_completion(
    store: &mut impl AsContextMut,
    operation: wasmtime::Func,
    record: Rooted<StructRef>,
) -> wasmtime::Result<[WasmtimeVal; 5]> {
    let mut result = [
        WasmtimeVal::I32(0),
        WasmtimeVal::I64(0),
        WasmtimeVal::AnyRef(None),
        WasmtimeVal::I32(0),
        WasmtimeVal::I32(0),
    ];
    let arguments = record_arguments(store, record);
    operation.call(store, &arguments, &mut result)?;
    Ok(result)
}

fn record_arguments(store: &mut impl AsContextMut, record: Rooted<StructRef>) -> [WasmtimeVal; 2] {
    let graph = reference_field(store, record, record_field::GRAPH);
    let program = reference_field(store, graph, graph_field::PROGRAM);
    [WasmtimeVal::from(record), WasmtimeVal::from(program)]
}

struct ModuleStore {
    limits: WasmtimeStoreLimits,
    realm: Realm,
    monotonic_clock_origin: MonotonicClockInstant,
    source_allowed: bool,
    reentrant_calls: usize,
    record: Option<Rooted<StructRef>>,
    evaluate: Option<wasmtime::Func>,
    reentrant_promise: Option<OwnedRooted<StructRef>>,
    // The two-program control supplies the expected FIFO source-entry owners.
    // Each callback reenters its actual P helper and typed ModuleRecord, rather
    // than borrowing whichever program happened to initialize the fixture.
    reentry_schedule: VecDeque<(Rooted<StructRef>, wasmtime::Func)>,
}
struct ModuleFixture {
    engine: WasmtimeEngine,
    runtime: WasmtimeModule,
    module: WasmtimeModule,
    runtime_key: scriptw_aot_wasm::RuntimeArtifactKey,
    drain_export: String,
}
impl ModuleFixture {
    fn new() -> Self {
        Self::from_source("print('body'); await 0; print('resumed');")
    }
    fn from_source(source: &str) -> Self {
        configure_compilation_jobs(1).unwrap();
        let compiler = Engine::new(RealmBuilder::new().build());
        let unit = compiler
            .compile_module(
                source,
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
            )
            .unwrap();
        assert!(unit
            .ir
            .script
            .as_ref()
            .unwrap()
            .module_entry_evaluation()
            .is_some());
        let original = compiler.emit_wasm(&unit).unwrap();
        let runtime_key = original.runtime.as_ref().unwrap().key();
        let drain_export = runtime_helper_export(
            original.runtime.as_ref().unwrap().0.bytes(),
            "helper::promise_drain_jobs",
        );
        let (paused, root) = paused_before_entry_evaluation(&original.bytes);
        let bytes = with_private_exports(&paused, root);
        let engine = shared_wasm_engine().unwrap();
        let runtime = WasmtimeModule::new(
            &engine,
            original.runtime.as_ref().expect("linked runtime").0.bytes(),
        )
        .unwrap();
        let module = WasmtimeModule::new(&engine, bytes).unwrap();
        Self {
            engine,
            runtime,
            module,
            runtime_key,
            drain_export,
        }
    }
    fn instantiate(&self) -> PrivateModule {
        let realm = RealmBuilder::new().build();
        let monotonic_clock_origin = realm.host_clock().monotonic_instant();
        let mut store = WasmtimeStore::new(
            &self.engine,
            ModuleStore {
                limits: WasmtimeStoreLimitsBuilder::new()
                    .memory_size(64 * 1024 * 1024)
                    .build(),
                realm,
                monotonic_clock_origin,
                source_allowed: false,
                reentrant_calls: 0,
                record: None,
                evaluate: None,
                reentrant_promise: None,
                reentry_schedule: VecDeque::new(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let mut linker = WasmtimeLinker::<ModuleStore>::new(&self.engine);
        for import in self.runtime.imports() {
            match import.ty() {
                WasmtimeExternType::Func(signature) => {
                    let is_print = import.name() == WASM_HOST_IMPORT_PRINT_LINE_UTF8;
                    let host = format!("{}::{}", import.module(), import.name());
                    if host == "scriptw_host::monotonic_clock_nanos" {
                        assert_eq!(signature.params().len(), 0);
                        assert!(matches!(
                            signature.results().collect::<Vec<_>>().as_slice(),
                            [wasmtime::ValType::I64]
                        ));
                    }
                    let byte_array = if host == "scriptw_host::byte_array_allocate" {
                        assert!(matches!(
                            &signature.params().collect::<Vec<_>>()[..],
                            [wasmtime::ValType::I32]
                        ));
                        let results: Vec<_> = signature.results().collect();
                        let [result] = results.as_slice() else {
                            panic!("one byte-array result")
                        };
                        let result = result.unwrap_ref();
                        assert!(result.is_nullable(), "actual collector OOM is nullable");
                        let array = result.heap_type().unwrap_concrete_array();
                        assert_eq!(array.mutability(), wasmtime::Mutability::Var);
                        assert!(matches!(array.element_type(), wasmtime::StorageType::I8));
                        Some(array.clone())
                    } else {
                        None
                    };
                    linker.func_new(import.module(), import.name(), signature, move |mut caller, inputs, outputs| {
                        // These fixtures never create weak references or finalization
                        // registries: those callbacks still trap below. The genuine
                        // empty-state boundary returns exactly what Engine returns.
                        match host.as_str() {
                            "scriptw_host::monotonic_clock_nanos" => {
                                assert!(inputs.is_empty() && outputs.len() == 1);
                                let state = caller.data();
                                outputs[0] = WasmtimeVal::I64(
                                    state.realm.host_clock().monotonic_instant()
                                        .saturating_duration_since(state.monotonic_clock_origin)
                                        .as_i64_saturating(),
                                );
                                return Ok(());
                            }
                            "scriptw_host::weak_job_finish" => {
                                assert!(inputs.is_empty() && outputs.is_empty());
                                return Ok(());
                            }
                            "scriptw_host::finalization_next_registry" => {
                                assert!(inputs.is_empty() && outputs.len() == 1);
                                outputs[0] = WasmtimeVal::AnyRef(None);
                                return Ok(());
                            }
                            "scriptw_host::byte_array_allocate" => {
                                let ([WasmtimeVal::I32(length)], [output]) = (inputs, outputs) else {
                                    return Err(wasmtime::Error::msg("invalid byte allocation operands"));
                                };
                                let length = u32::try_from(*length).map_err(wasmtime::Error::new)?;
                                let allocator = ArrayRefPre::new(&mut caller, byte_array.as_ref().unwrap().clone());
                                *output = match ArrayRef::new(&mut caller, &allocator, &WasmtimeVal::I32(0), length) {
                                    Ok(bytes) => WasmtimeVal::from(bytes),
                                    Err(error) if error.is::<GcHeapOutOfMemory<()>>() => WasmtimeVal::AnyRef(None),
                                    Err(error) => return Err(error),
                                };
                                return Ok(());
                            }
                            _ => {}
                        }
                        if !is_print || !caller.data().source_allowed {
                            return Err(wasmtime::Error::msg(format!("module allocation/instantiation called source or unsupported host code: {host}")));
                        }
                        // This import belongs to R, so P's private handles are
                        // held in Store data instead of Caller::get_export.
                        let (module, evaluate) = caller.data_mut().reentry_schedule.pop_front()
                            .unwrap_or_else(|| (caller.data().record.expect("initialized program record"),
                                                caller.data().evaluate.unwrap()));
                        assert_eq!(scalar_field(&mut caller, module, record_field::BODY_STATE), 1,
                            "body is Executing before a reentrant call");
                        let promise = reference_field(&mut caller, module, record_field::EVALUATION_PROMISE);
                        let result = call_completion(&mut caller, evaluate, module)?;
                        assert_eq!(result[3].i32(), Some(0));
                        let returned = structure(&caller, result[2]);
                        same_record(&caller, promise, returned);
                        caller.data_mut().reentrant_calls += 1;
                        let retained = promise.to_owned_rooted(&mut caller)?;
                        caller.data_mut().reentrant_promise = Some(retained);
                        Ok(())
                    }).unwrap();
                }
                WasmtimeExternType::Memory(memory_type) if memory_type.is_shared() => {
                    let memory = WasmtimeSharedMemory::new(&self.engine, memory_type).unwrap();
                    linker
                        .define(&store, import.module(), import.name(), memory)
                        .unwrap();
                }
                WasmtimeExternType::Memory(memory_type) => {
                    let memory = WasmtimeMemory::new(&mut store, memory_type).unwrap();
                    linker
                        .define(&store, import.module(), import.name(), memory)
                        .unwrap();
                }
                other => panic!("unexpected fixture import: {other:?}"),
            }
        }
        let runtime = linker.instantiate(&mut store, &self.runtime).unwrap();
        let drain = runtime.get_func(&mut store, &self.drain_export).unwrap();
        linker
            .instance(
                &mut store,
                scriptw_aot_wasm::RUNTIME_IMPORT_NAMESPACE,
                runtime,
            )
            .unwrap();
        let instance = linker.instantiate(&mut store, &self.module).unwrap();
        let main = instance.get_func(&mut store, "main").unwrap();
        let mut completion = [
            WasmtimeVal::I32(0),
            WasmtimeVal::I64(0),
            WasmtimeVal::AnyRef(None),
            WasmtimeVal::I32(0),
            WasmtimeVal::I32(0),
        ];
        main.call(&mut store, &[], &mut completion).unwrap();
        assert_eq!(completion[3].i32(), Some(0), "paused main returns Normal");
        let root = instance
            .get_global(&mut store, "test_module_record")
            .unwrap()
            .get(&mut store);
        let record = structure(&store, root);
        assert_eq!(
            record.ty(&store).unwrap().fields().len(),
            23,
            "ModuleRecord schema"
        );
        let evaluate = instance
            .get_func(&mut store, "test_module_evaluate")
            .unwrap();
        let execute = instance
            .get_func(&mut store, "test_module_execute")
            .unwrap();
        let ready = instance.get_func(&mut store, "test_module_ready").unwrap();
        let graph = reference_field(&mut store, record, record_field::GRAPH);
        let WasmtimeVal::AnyRef(Some(modules)) = graph.field(&mut store, 0).unwrap() else {
            panic!("ModuleGraph owns its registry")
        };
        let modules = modules.as_array(&store).unwrap().unwrap();
        assert_eq!(
            modules.len(&store).unwrap(),
            1,
            "single-record fixture root"
        );
        let member = modules.get(&mut store, 0).unwrap();
        same_record(&store, record, structure(&store, member));
        store.data_mut().record = Some(record);
        store.data_mut().evaluate = Some(evaluate);
        PrivateModule {
            store,
            record,
            evaluate,
            execute,
            ready,
            drain,
            linker,
        }
    }
}
struct PrivateModule {
    store: WasmtimeStore<ModuleStore>,
    record: Rooted<StructRef>,
    evaluate: wasmtime::Func,
    execute: wasmtime::Func,
    ready: wasmtime::Func,
    drain: wasmtime::Func,
    linker: WasmtimeLinker<ModuleStore>,
}
impl PrivateModule {
    fn write(&mut self, record: Rooted<StructRef>, index: usize, value: i32) {
        record
            .set_field(&mut self.store, index, WasmtimeVal::I32(value))
            .unwrap();
    }
}

#[test]
fn private_module_instantiation_has_no_source_effect_or_await_job_and_keeps_body_promise_pending() {
    run_on_sized_stack(|| {
        let fixture = ModuleFixture::new();
        let mut runtime = fixture.instantiate();
        let record = runtime.record;
        let activation =
            reference_field(&mut runtime.store, record, record_field::ASYNC_ACTIVATION);
        assert_eq!(activation.ty(&runtime.store).unwrap().fields().len(), 8);
        for (field, expected) in [
            (record_field::ACTIVATION_KIND, 1),
            (record_field::STATE, 0),
            (record_field::COMPLETION, 0),
            (record_field::BODY_STATE, 0),
        ] {
            assert_eq!(scalar_field(&mut runtime.store, record, field), expected);
        }
        assert!(null_field(
            &mut runtime.store,
            record,
            record_field::EVALUATION_PROMISE
        ));
        for (field, expected) in [
            (activation_field::MODULE_ENTRY_MODE, 2),
            (activation_field::RESUME_POINT, 1),
            (activation_field::COMPLETED, 0),
        ] {
            assert_eq!(
                scalar_field(&mut runtime.store, activation, field),
                expected
            );
        }
        let frame = reference_field(&mut runtime.store, activation, activation_field::FRAME);
        assert_eq!(
            scalar_field(&mut runtime.store, frame, 13),
            1,
            "InvocationFrame initialized"
        );
        let environment = reference_field(&mut runtime.store, record, record_field::ENVIRONMENT);
        let invocation_environment = reference_field(&mut runtime.store, frame, 3);
        same_record(&runtime.store, environment, invocation_environment);
        reference_field(&mut runtime.store, frame, 1); // Live lexical execution environment.
        let promise = reference_field(&mut runtime.store, activation, activation_field::PROMISE);
        assert_eq!(
            scalar_field(&mut runtime.store, promise, 4),
            0,
            "body Promise Pending"
        );
        assert_eq!(runtime.store.data().reentrant_calls, 0);
    });
}

#[test]
fn invalid_private_module_modes_and_lifecycle_words_trap_before_source_entry() {
    run_on_sized_stack(|| {
        let fixture = ModuleFixture::new();
        for (field, value) in [
            (activation_field::MODULE_ENTRY_MODE, 99),
            (activation_field::MODULE_ENTRY_MODE, 1),
            (activation_field::RESUME_POINT, 0),
            (activation_field::RESUME_POINT, i32::MAX),
        ] {
            let mut runtime = fixture.instantiate();
            let activation = reference_field(
                &mut runtime.store,
                runtime.record,
                record_field::ASYNC_ACTIVATION,
            );
            runtime.write(activation, field, value);
            let result = call_completion(&mut runtime.store, runtime.execute, runtime.record);
            assert!(
                result.unwrap_err().downcast_ref::<WasmtimeTrap>().is_some(),
                "private mode/state corruption must be a Wasm trap"
            );
            assert_eq!(runtime.store.data().reentrant_calls, 0);
        }
        for (field, execute) in [
            (record_field::STATE, false),
            (record_field::BODY_STATE, true),
            (record_field::ACTIVATION_KIND, true),
        ] {
            let mut runtime = fixture.instantiate();
            runtime.write(runtime.record, field, 99);
            let result = if execute {
                call_completion(&mut runtime.store, runtime.execute, runtime.record).map(|_| ())
            } else {
                let arguments = record_arguments(&mut runtime.store, runtime.record);
                runtime
                    .ready
                    .call(&mut runtime.store, &arguments, &mut [WasmtimeVal::I32(0)])
            };
            assert!(result.unwrap_err().downcast_ref::<WasmtimeTrap>().is_some());
            assert_eq!(runtime.store.data().reentrant_calls, 0);
        }
        let mut runtime = fixture.instantiate();
        runtime.write(runtime.record, record_field::STATE, 3);
        runtime.write(runtime.record, record_field::COMPLETION, 99);
        let result = call_completion(&mut runtime.store, runtime.evaluate, runtime.record);
        assert!(result.unwrap_err().downcast_ref::<WasmtimeTrap>().is_some());
        assert_eq!(runtime.store.data().reentrant_calls, 0);
    });
}

#[test]
fn reentrant_evaluate_while_an_async_body_is_executing_reuses_its_owned_capability() {
    run_on_sized_stack(|| {
        let fixture = ModuleFixture::new();
        let mut runtime = fixture.instantiate();
        runtime.store.data_mut().source_allowed = true;
        let result = call_completion(&mut runtime.store, runtime.evaluate, runtime.record).unwrap();
        assert_eq!(result[3].i32(), Some(0));
        let promise = structure(&runtime.store, result[2]);
        assert_eq!(runtime.store.data().reentrant_calls, 1);
        let observed = runtime.store.data_mut().reentrant_promise.take().unwrap();
        let observed = observed.to_rooted(&mut runtime.store);
        same_record(&runtime.store, observed, promise);
        assert_eq!(
            scalar_field(&mut runtime.store, runtime.record, record_field::BODY_STATE),
            1,
            "suspended body remains Executing"
        );
        assert_eq!(
            scalar_field(&mut runtime.store, runtime.record, record_field::STATE),
            2,
            "DFS closed as EvaluatingAsync"
        );
        let retained = reference_field(
            &mut runtime.store,
            runtime.record,
            record_field::EVALUATION_PROMISE,
        );
        same_record(&runtime.store, retained, promise);
        let duplicate = call_completion(&mut runtime.store, runtime.execute, runtime.record);
        assert!(duplicate
            .unwrap_err()
            .downcast_ref::<WasmtimeTrap>()
            .is_some());
        assert_eq!(runtime.store.data().reentrant_calls, 1);
    });
}

fn empty_completion() -> [WasmtimeVal; 5] {
    [
        WasmtimeVal::I32(0),
        WasmtimeVal::I64(0),
        WasmtimeVal::AnyRef(None),
        WasmtimeVal::I32(0),
        WasmtimeVal::I32(0),
    ]
}

fn drain_with_program(runtime: &mut PrivateModule, program: Rooted<StructRef>) {
    let mut arguments = Vec::from(empty_completion());
    arguments.push(WasmtimeVal::AnyRef(None)); // explicit caller environment
    arguments.push(WasmtimeVal::from(program));
    let mut result = empty_completion();
    runtime
        .drain
        .call(&mut runtime.store, &arguments, &mut result)
        .unwrap();
    assert_eq!(result[3].i32(), Some(0), "job drain preserves Normal");
}

fn assert_module_fulfilled(runtime: &mut PrivateModule, record: Rooted<StructRef>) {
    for (field, expected) in [
        (record_field::BODY_STATE, 2),
        (record_field::STATE, 3),
        (record_field::COMPLETION, 1),
    ] {
        assert_eq!(scalar_field(&mut runtime.store, record, field), expected);
    }
    let promise = reference_field(&mut runtime.store, record, record_field::EVALUATION_PROMISE);
    assert_eq!(scalar_field(&mut runtime.store, promise, 4), 1);
}

#[test]
fn graphless_second_program_cannot_replace_a_queued_module_body_hook_owner() {
    run_on_sized_stack(|| {
        let fixture = ModuleFixture::new();
        let compiler = Engine::new(RealmBuilder::new().build());
        let unit = compiler
            .compile_script(
                "const unrelated = { tag: 'P2' }; unrelated.tag;",
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
            )
            .unwrap();
        let artifact = compiler.emit_wasm(&unit).unwrap();
        assert_eq!(
            artifact.runtime.as_ref().unwrap().key(),
            fixture.runtime_key
        );
        let second = WasmtimeModule::new(&fixture.engine, &artifact.bytes).unwrap();
        for enter_main in [false, true] {
            let mut runtime = fixture.instantiate();
            runtime.store.data_mut().source_allowed = true;
            let record = runtime.record;
            call_completion(&mut runtime.store, runtime.evaluate, record).unwrap();
            let graph = reference_field(&mut runtime.store, record, record_field::GRAPH);
            let first_program = reference_field(&mut runtime.store, graph, graph_field::PROGRAM);
            let instance = runtime
                .linker
                .instantiate(&mut runtime.store, &second)
                .unwrap();
            let mut published = [WasmtimeVal::AnyRef(None)];
            instance
                .get_func(&mut runtime.store, "scriptw.program_init")
                .unwrap()
                .call(&mut runtime.store, &[], &mut published)
                .unwrap();
            let second_program = structure(&runtime.store, published[0]);
            assert!(!Rooted::ref_eq(&runtime.store, &first_program, &second_program).unwrap());
            assert!(
                null_module_body_reaction(&mut runtime.store, second_program, record),
                "graphless P2 has no ModuleBodyReaction hook to borrow"
            );
            runtime.store.gc(None).unwrap();
            if enter_main {
                let mut result = empty_completion();
                instance
                    .get_func(&mut runtime.store, "main")
                    .unwrap()
                    .call(&mut runtime.store, &[], &mut result)
                    .unwrap();
                assert_eq!(result[3].i32(), Some(0));
            } else {
                drain_with_program(&mut runtime, second_program);
            }
            let retained_program = reference_field(&mut runtime.store, graph, graph_field::PROGRAM);
            same_record(&runtime.store, first_program, retained_program);
            assert_module_fulfilled(&mut runtime, record);
            assert_eq!(
                runtime.store.data().reentrant_calls,
                2,
                "both original body and resumed source execute after P2 installation"
            );
        }
    });
}

#[test]
fn rejected_module_body_retains_its_program_when_a_graphless_program_drains_jobs() {
    run_on_sized_stack(|| {
        let fixture = ModuleFixture::from_source("print('body'); await 0; throw 17;");
        let compiler = Engine::new(RealmBuilder::new().build());
        let unit = compiler
            .compile_script(
                "const unrelated = { tag: 'P2' }; unrelated.tag;",
                CompileOptions {
                    host_surface_policy: HostSurfacePolicy::Test262,
                    ..CompileOptions::default()
                },
            )
            .unwrap();
        let artifact = compiler.emit_wasm(&unit).unwrap();
        assert_eq!(
            artifact.runtime.as_ref().unwrap().key(),
            fixture.runtime_key
        );
        let second = WasmtimeModule::new(&fixture.engine, &artifact.bytes).unwrap();
        let mut runtime = fixture.instantiate();
        runtime.store.data_mut().source_allowed = true;
        let record = runtime.record;
        call_completion(&mut runtime.store, runtime.evaluate, record).unwrap();
        let instance = runtime
            .linker
            .instantiate(&mut runtime.store, &second)
            .unwrap();
        let mut published = [WasmtimeVal::AnyRef(None)];
        instance
            .get_func(&mut runtime.store, "scriptw.program_init")
            .unwrap()
            .call(&mut runtime.store, &[], &mut published)
            .unwrap();
        let second_program = structure(&runtime.store, published[0]);
        assert!(null_module_body_reaction(
            &mut runtime.store,
            second_program,
            record
        ));
        runtime.store.gc(None).unwrap();
        drain_with_program(&mut runtime, second_program);
        for (field, expected) in [
            (record_field::BODY_STATE, 2),
            (record_field::STATE, 3),
            (record_field::COMPLETION, 2),
        ] {
            assert_eq!(scalar_field(&mut runtime.store, record, field), expected);
        }
        let promise = reference_field(&mut runtime.store, record, record_field::EVALUATION_PROMISE);
        assert_eq!(scalar_field(&mut runtime.store, promise, 4), 2);
        assert_eq!(runtime.store.data().reentrant_calls, 1);
    });
}

#[test]
fn shared_realm_keeps_distinct_program_graphs_and_drains_both_retained_owners() {
    run_on_sized_stack(|| {
        let first = ModuleFixture::from_source(
            "print('P1'); await 0; if (Function('return 17')() !== 17) throw 'P1 catalog'; print('P1 resumed');",
        );
        let second = ModuleFixture::from_source(
            "print('P2'); await 0; if (Function('return 29')() !== 29) throw 'P2 catalog'; print('P2 resumed');",
        );
        assert_eq!(first.runtime_key, second.runtime_key, "one actual shared R");
        let mut runtime = first.instantiate();
        let first_record = runtime.record;
        let first_graph = reference_field(&mut runtime.store, first_record, record_field::GRAPH);
        let realm = reference_field(&mut runtime.store, first_record, record_field::REALM);
        let second_instance = runtime
            .linker
            .instantiate(&mut runtime.store, &second.module)
            .unwrap();
        let mut result = empty_completion();
        second_instance
            .get_func(&mut runtime.store, "main")
            .unwrap()
            .call(&mut runtime.store, &[], &mut result)
            .unwrap();
        assert_eq!(
            result[3].i32(),
            Some(0),
            "second paused main only instantiates its graph"
        );
        let value = second_instance
            .get_global(&mut runtime.store, "test_module_record")
            .unwrap()
            .get(&mut runtime.store);
        let second_record = structure(&runtime.store, value);
        let second_graph = reference_field(&mut runtime.store, second_record, record_field::GRAPH);
        let second_program =
            reference_field(&mut runtime.store, second_graph, graph_field::PROGRAM);
        let initialize = second_instance
            .get_func(&mut runtime.store, "test_module_initialize")
            .unwrap();
        for _ in 0..2 {
            initialize
                .call(
                    &mut runtime.store,
                    &[WasmtimeVal::from(realm), WasmtimeVal::from(second_program)],
                    &mut result,
                )
                .unwrap();
            assert_eq!(result[3].i32(), Some(0));
        }
        let head = reference_field(&mut runtime.store, realm, realm_field::MODULES);
        let previous = reference_field(&mut runtime.store, head, graph_field::NEXT);
        same_record(&runtime.store, first_graph, previous);
        assert!(
            null_field(&mut runtime.store, first_graph, graph_field::NEXT),
            "repeated P2 initialization never replaces or duplicates the P1 graph"
        );
        let retained = reference_field(&mut runtime.store, head, graph_field::PROGRAM);
        same_record(&runtime.store, retained, second_program);
        let WasmtimeVal::AnyRef(Some(records)) = head
            .field(&mut runtime.store, graph_field::MODULES)
            .unwrap()
        else {
            panic!("per-P graph owns its exact module registry")
        };
        let records = records.as_array(&runtime.store).unwrap().unwrap();
        let value = records.get(&mut runtime.store, 0).unwrap();
        let second_in_shared_realm = structure(&runtime.store, value);
        assert!(!Rooted::ref_eq(&runtime.store, &first_record, &second_in_shared_realm).unwrap());
        let evaluate = second_instance
            .get_func(&mut runtime.store, "test_module_evaluate")
            .unwrap();
        runtime.store.data_mut().reentry_schedule = VecDeque::from([
            (first_record, runtime.evaluate),
            (second_in_shared_realm, evaluate),
            (first_record, runtime.evaluate),
            (second_in_shared_realm, evaluate),
        ]);
        runtime.store.data_mut().source_allowed = true;
        call_completion(&mut runtime.store, runtime.evaluate, first_record).unwrap();
        call_completion(&mut runtime.store, evaluate, second_in_shared_realm).unwrap();
        runtime.store.gc(None).unwrap();
        drain_with_program(&mut runtime, second_program);
        assert_module_fulfilled(&mut runtime, first_record);
        assert_module_fulfilled(&mut runtime, second_in_shared_realm);
        assert_eq!(runtime.store.data().reentrant_calls, 4);
        assert!(
            runtime.store.data().reentry_schedule.is_empty(),
            "all four expected source owners reentered"
        );
    });
}

fn compiled(source: &str, module: bool) -> Artifact {
    let engine = Engine::new(RealmBuilder::new().build());
    let unit = if module {
        engine.compile_module(source, CompileOptions::default())
    } else {
        engine.compile_script(source, CompileOptions::default())
    }
    .unwrap();
    engine.emit_wasm(&unit).unwrap()
}

#[test]
fn graphless_scripts_have_only_unreachable_module_slots_and_no_edges_to_them() {
    run_on_sized_stack(|| {
        for source in [
            "const values = [1]; values.length;",
            "const realm = new ShadowRealm(); realm.evaluate('1');",
        ] {
            let artifact = compiled(source, false);
            let bytes = &artifact.bytes;
            let names = function_names(bytes);
            let helpers: BTreeSet<_> = names
                .iter()
                .filter_map(|(&index, name)| name.starts_with("helper::module_").then_some(index))
                .collect();
            assert_eq!(
                helpers
                    .iter()
                    .map(|index| names[index].as_str())
                    .collect::<BTreeSet<_>>(),
                [
                    "helper::module_initialize",
                    "helper::module_evaluate",
                    "helper::module_ready",
                    "helper::module_gather",
                    "helper::module_execute",
                    "helper::module_fulfilled",
                    "helper::module_rejected",
                    "helper::module_deferred_import",
                    "helper::module_body_reaction"
                ]
                .into_iter()
                .collect()
            );
            let mut imported = 0;
            let mut defined = 0;
            let mut total_module_bytes = 0;
            for payload in WasmParser::new(0).parse_all(&bytes) {
                match payload.unwrap() {
                    WasmPayload::ImportSection(imports) => {
                        for import in imports.into_imports() {
                            if matches!(
                                import.unwrap().ty,
                                wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                            ) {
                                imported += 1;
                            }
                        }
                    }
                    WasmPayload::CodeSectionEntry(body) => {
                        let index = imported + defined;
                        defined += 1;
                        let operators = body
                            .get_operators_reader()
                            .unwrap()
                            .into_iter()
                            .map(Result::unwrap)
                            .collect::<Vec<_>>();
                        if helpers.contains(&index) {
                            total_module_bytes += body.range().len();
                            assert!(matches!(
                                &operators[..],
                                [wasmparser::Operator::Unreachable, wasmparser::Operator::End]
                            ));
                        }
                        for operator in operators {
                            if let wasmparser::Operator::Call { function_index }
                            | wasmparser::Operator::RefFunc { function_index } = operator
                            {
                                assert!(
                                    !helpers.contains(&function_index),
                                    "graphless artifact has a caller edge to a private module slot"
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(
                total_module_bytes,
                helpers.len() * 3,
                "one three-byte reserved body per exact program module role"
            );
            // Array intrinsic installation also roots Array.fromAsync and its
            // Promise dependencies. Graph isolation is the absence of module edges,
            // together with the nine minimal unreachable bodies asserted above.
        }
    });
}

#[test]
fn synchronous_module_graph_roots_real_evaluation_bodies_and_private_generator_resume() {
    run_on_sized_stack(|| {
        let artifact = compiled("export const value = 1;", true);
        let bytes = &artifact.bytes;
        let names = function_names(bytes);
        let helpers: BTreeSet<_> = names
            .iter()
            .filter_map(|(&index, name)| name.starts_with("helper::module_").then_some(index))
            .collect();
        assert_eq!(
            helpers
                .iter()
                .map(|index| names[index].as_str())
                .collect::<BTreeSet<_>>(),
            [
                "helper::module_initialize",
                "helper::module_evaluate",
                "helper::module_ready",
                "helper::module_gather",
                "helper::module_execute",
                "helper::module_fulfilled",
                "helper::module_rejected",
                "helper::module_deferred_import",
                "helper::module_body_reaction"
            ]
            .into_iter()
            .collect()
        );
        let mut imported = 0;
        let mut defined = 0;
        let mut sizes = Vec::new();
        for payload in WasmParser::new(0).parse_all(&bytes) {
            match payload.unwrap() {
                WasmPayload::ImportSection(imports) => {
                    for import in imports.into_imports() {
                        if matches!(
                            import.unwrap().ty,
                            wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                        ) {
                            imported += 1;
                        }
                    }
                }
                WasmPayload::CodeSectionEntry(body) => {
                    let index = imported + defined;
                    defined += 1;
                    if helpers.contains(&index) {
                        sizes.push(body.range().len());
                    }
                }
                _ => {}
            }
        }
        assert_eq!(sizes.len(), helpers.len());
        assert!(
            sizes.iter().all(|&size| size > 3 && size < 1024 * 1024),
            "outlined module bodies have bounded per-function size: {sizes:?}"
        );
        assert!(
            function_names(artifact.runtime.as_ref().expect("linked runtime").0.bytes())
                .values()
                .any(|name| name == "builtin::Generator.prototype.next"),
            "private synchronous body resume must have an emitted intrinsic body: {names:?}"
        );
    });
}

#[test]
fn trusted_module_operations_without_their_graph_are_rejected_before_emission() {
    run_on_sized_stack(|| {
        let engine = Engine::new(RealmBuilder::new().build());
        let mut unit = engine
            .compile_module("await 0;", CompileOptions::default())
            .unwrap();
        unit.ir.script.as_mut().unwrap().body.statements.retain(|statement| !matches!(statement,
            StatementIr::Expression(expression) if matches!(&expression.expr, ExprIr::ModuleExecutionGraph(_))));
        let error = engine.emit_wasm(&unit).unwrap_err();
        assert!(
            error.to_string().contains("validated execution graph"),
            "{error}"
        );
    });
}
