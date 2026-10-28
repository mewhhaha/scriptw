use scriptw_aot_wasm::WasmArtifact;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    DataKind, KnownCustom, Name, Operator, Parser, Payload, TypeRef, Validator, WasmFeatures,
};

fn named_function_body(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut imported_functions = 0u32;
    let mut function_index = None;
    let mut bodies = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.expect("runtime module decodes") {
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    if matches!(
                        import.expect("runtime import decodes").ty,
                        TypeRef::Func(_) | TypeRef::FuncExact(_)
                    ) {
                        imported_functions += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => bodies.push(body.range()),
            Payload::CustomSection(section) => {
                if let KnownCustom::Name(subsections) = section.as_known() {
                    for subsection in subsections {
                        if let Name::Function(map) = subsection.expect("name subsection decodes") {
                            for naming in map {
                                let naming = naming.expect("function name decodes");
                                if naming.name == name {
                                    assert!(
                                        function_index.replace(naming.index).is_none(),
                                        "the shared helper must have exactly one named body"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let ordinal = function_index
        .expect("shared coercion helper is emitted in the runtime")
        .checked_sub(imported_functions)
        .expect("shared coercion helper is defined, not imported");
    let range = bodies
        .get(ordinal as usize)
        .expect("helper index identifies an actual code body");
    bytes[range.clone()].to_vec()
}

fn array_data_initializers(body: &[u8]) -> [usize; 4] {
    let parsed = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(body, 0));
    let mut initializers = [0; 4];
    for operator in parsed.get_operators_reader().expect("body opens") {
        if let Operator::ArrayNewData {
            array_data_index, ..
        } = operator.expect("body decodes")
        {
            *initializers
                .get_mut(array_data_index as usize)
                .expect("only the four declared P images may initialize GC data") += 1;
        }
    }
    initializers
}

fn passive_program_images(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut images = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::DataSection(reader) = payload.expect("program decodes") {
            assert!(images.is_empty(), "one program data section");
            for row in reader {
                let row = row.expect("program image decodes");
                assert!(
                    matches!(row.kind, DataKind::Passive),
                    "P images stay passive"
                );
                images.push(row.data.to_vec());
            }
        }
    }
    assert_eq!(images.len(), 4, "UTF-16, metadata, descriptors and recipes");
    images
}

fn emit_literal(length: usize, marker: &str) -> (WasmArtifact, u32, Vec<u8>) {
    let source = format!(
        "const marker = '{marker}'; const text = '{}\\u0000\\uD800\\uDC00\\uD800'; text.length;",
        "x".repeat(length)
    );
    let parsed = parse(&source, ParseOptions::script()).expect("literal source parses");
    let program = lower(&parsed);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let artifact =
        scriptw_aot_wasm::emit(&program).expect("literal emits through the product compiler");
    let main_size = artifact
        .function_sizes
        .iter()
        .find(|body| body.name == "scriptw::main")
        .expect("actual main body")
        .body_bytes
        .bytes();
    let runtime = artifact.runtime().expect("literal program links a runtime");
    let helper = named_function_body(runtime.bytes(), "helper::value_to_primitive_number");
    (artifact, main_size, helper)
}

#[test]
fn literal_length_grows_passive_data_without_expanding_bootstrap_code() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            // These source-only strings sort on opposite sides of every
            // ordinary builtin spelling. Neither may renumber builtin slots.
            let (short, short_main, short_helper) = emit_literal(1024, "\\u0000pool-before");
            let (long, long_main, long_helper) = emit_literal(16 * 1024, "\\u{10ffff}pool-after");
            assert_eq!(short_helper, long_helper,
                "unrelated source literals must preserve the shared coercion helper bytes");
            for artifact in [&short, &long] {
                let runtime = artifact.runtime().expect("literal program links a runtime");
                let runtime_initializer = named_function_body(runtime.bytes(), "helper::pooled_strings_initialize");
                assert_eq!(array_data_initializers(&runtime_initializer), [0, 0, 0, 0],
                    "R bootstrap publishes a nullable cache without copying canonical UTF16");
                assert!(runtime_initializer.len() < 8192, "R bootstrap is bounded independently of its literal count");
                let normalize = named_function_body(runtime.bytes(), "builtin::String.prototype.normalize");
                let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&normalize, 0));
                let operators = body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>();
                assert!(operators.iter().any(|op| matches!(op, Operator::MemoryInit { data_index: 1, mem: 0 })),
                    "valid normalization resolves its own passive data role rather than UTF16 segment zero");
                assert!(!operators.iter().any(|op| matches!(op, Operator::MemoryInit { data_index: 0, .. })),
                    "normalization cannot copy pooled literal bytes into its table range");
                assert!(operators.iter().any(|op| matches!(op, Operator::GlobalSet { .. })),
                    "completed initialization is recorded on the actual R instance");
                let compiler = named_function_body(runtime.bytes(), "helper::regexp_compiler");
                let body=wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&compiler,0));
                let ops=body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>();
                let installs=ops.iter().enumerate().filter_map(|(index,op)|matches!(op,Operator::MemoryInit{data_index:2,mem:0}).then_some(index)).collect::<Vec<_>>();
                assert!(!installs.is_empty(),"actual Unicode property parser installs its sealed passive image");
                for index in installs {
                    assert!(matches!(ops[index-6],Operator::GlobalGet{..}),"instance-local readiness guards each property lookup owner");
                    let Operator::GlobalGet{global_index}=ops[index-6] else {unreachable!()};
                    assert!(matches!(ops[index-5],Operator::I32Eqz));
                    assert!(matches!(ops[index-4],Operator::If{..}));
                    assert!(matches!(ops[index+1],Operator::I32Const{value:1}));
                    assert!(matches!(ops[index+2],Operator::GlobalSet{global_index:written} if written==global_index),"publish readiness only after the complete memory.init");
                }
                assert!(!ops.iter().any(|op|matches!(op,Operator::DataDrop{data_index:2})),"independent late property lookups retain the passive catalog");
                let resolver = named_function_body(runtime.bytes(), "helper::pooled_string_resolve");
                assert_eq!(array_data_initializers(&resolver), [1, 0, 0, 0],
                    "only a cache miss constructs canonical UTF16 storage");
                let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&resolver, 0));
                let operators = body.get_operators_reader().unwrap().into_iter()
                    .map(Result::unwrap).collect::<Vec<_>>();
                assert!(operators.iter().any(|op| matches!(op, Operator::RefIsNull)),
                    "cached canonical strings bypass allocation");
                assert!(operators.iter().any(|op| matches!(op, Operator::I32GeU)),
                    "runtime ordinal is bounded before descriptor access");
                assert!(!operators.iter().any(|op| matches!(op, Operator::DataDrop { .. })),
                    "late cache misses must retain the passive canonical UTF16 segment");
                for bytes in [artifact.bytes.as_slice(), runtime.bytes()] {
                    Validator::new_with_features(WasmFeatures::all())
                        .validate_all(bytes)
                        .expect("runtime and program GC literal data and their counts validate");
                }
                let images = passive_program_images(&artifact.bytes);
                let initializer = named_function_body(&artifact.bytes, "scriptw::program_init");
                assert_eq!(array_data_initializers(&initializer), [1, 1, 1, 0],
                    "P bootstrap initializes UTF-16 strings, metadata and descriptor GC arrays");
                let main = named_function_body(&artifact.bytes, "scriptw::main");
                assert_eq!(array_data_initializers(&main), [0; 4],
                    "JavaScript main consumes its completed P owner without image initialization");
                assert!(images[3].is_empty(), "a literal-only program has no compact recipes");
                let expected = [0u16, 0xd800, 0xdc00, 0xd800]
                    .into_iter().flat_map(u16::to_le_bytes).collect::<Vec<_>>();
                assert!(images[0].windows(expected.len()).any(|units| units == expected),
                    "NUL, a surrogate pair and an isolated surrogate retain their exact UTF-16 units");
            }
            let short_initialize = named_function_body(&short.bytes, "scriptw::program_init").len();
            let long_initialize = named_function_body(&long.bytes, "scriptw::program_init").len();
            assert!(long_initialize <= short_initialize + 128,
                "literal contents must not unroll the separate P initializer");
            eprintln!("pooled literal bootstrap: {short_main} -> {long_main} bytes");
            assert!(long_main <= short_main + 128,
                "literal contents must grow data, not instructions: {short_main} -> {long_main}");
            assert!(long.bytes.len() > short.bytes.len() + 15 * 1024);
        })
        .expect("compiler worker starts")
        .join()
        .expect("compiler worker completes");
}

#[test]
fn calendar_rows_have_a_separate_lazy_passive_initializer() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            use std::collections::{BTreeMap, BTreeSet};
            let source = parse(
                "Temporal.PlainDate.from({calendar:'chinese',year:2000,monthCode:'M01',day:1});",
                ParseOptions::script(),
            )
            .unwrap();
            let artifact = scriptw_aot_wasm::emit(&lower(&source)).unwrap();
            let bytes = artifact.runtime().expect("calendar fixture links actual R").bytes();
            Validator::new_with_features(WasmFeatures::all())
                .validate_all(bytes)
                .expect("four passive runtime images validate");
            let mut segments = Vec::new();
            let mut globals = Vec::new();
            let mut imported_globals = 0u32;
            let mut readiness = BTreeMap::<u32, BTreeSet<u32>>::new();
            let mut calendar_spans = BTreeSet::new();
            let mut calendar_sites = 0;
            for payload in Parser::new(0).parse_all(bytes) {
                match payload.unwrap() {
                    Payload::ImportSection(reader) => {
                        for import in reader.into_imports() {
                            if matches!(import.unwrap().ty, TypeRef::Global(_)) {
                                imported_globals += 1;
                            }
                        }
                    }
                    Payload::GlobalSection(reader) => {
                        for global in reader {
                            let global = global.unwrap();
                            globals.push((
                                global.ty,
                                global.init_expr.get_operators_reader().into_iter()
                                    .map(Result::unwrap).collect::<Vec<_>>(),
                            ));
                        }
                    }
                    Payload::DataSection(reader) => {
                        for segment in reader {
                            let segment = segment.unwrap();
                            let address = match segment.kind {
                                DataKind::Passive => None,
                                DataKind::Active { memory_index:0, offset_expr } => {
                                    let mut ops = offset_expr.get_operators_reader();
                                    let Operator::I32Const { value } = ops.read().unwrap() else {
                                        panic!("static fragment has a canonical constant address")
                                    };
                                    assert!(matches!(ops.read().unwrap(), Operator::End));
                                    assert!(ops.eof());
                                    Some(value as u32)
                                }
                                DataKind::Active { .. } => panic!("runtime static data uses its sole memory"),
                            };
                            segments.push((address, segment.data.len()));
                        }
                    }
                    Payload::CodeSectionEntry(body) => {
                        let ops = body.get_operators_reader().unwrap().into_iter()
                            .map(Result::unwrap).collect::<Vec<_>>();
                        for (index, op) in ops.iter().enumerate() {
                            assert!(!matches!(op, Operator::DataDrop { data_index:3 }),
                                "fresh R instances and late first reads retain the calendar segment");
                            let Operator::MemoryInit { data_index, mem:0 } = op else { continue };
                            if !(1..=3).contains(data_index) { continue; }
                            assert!(index >= 6 && index + 2 < ops.len());
                            let Operator::GlobalGet { global_index } = ops[index-6] else {
                                panic!("each image copy has its own instance-local readiness guard")
                            };
                            assert!(matches!(ops[index-5], Operator::I32Eqz));
                            assert!(matches!(ops[index-4], Operator::If { .. }));
                            assert!(matches!(ops[index+1], Operator::I32Const { value:1 }));
                            assert!(matches!(ops[index+2], Operator::GlobalSet { global_index:written } if written == global_index),
                                "readiness is published only after the complete memory.init");
                            readiness.entry(*data_index).or_default().insert(global_index);
                            if *data_index == 3 {
                                calendar_sites += 1;
                                assert!(index >= 8 && matches!(ops[index-7], Operator::If { .. })
                                    && matches!(ops[index-8], Operator::I32And),
                                    "calendar copy stays inside the retained year/serial bounds branch");
                                let Operator::I32Const { value:start } = ops[index-3] else { panic!("fixed calendar address") };
                                assert!(matches!(ops[index-2], Operator::I32Const { value:0 }));
                                let Operator::I32Const { value:length } = ops[index-1] else { panic!("fixed complete image length") };
                                assert_eq!(length, 133_712, "both exact 8357-row calendar images are copied together");
                                calendar_spans.insert((start as u32, length as u32));
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(segments.len(), 8, "four passive images and four unchanged-address active fragments");
            assert!(segments[..4].iter().all(|(address,_)|address.is_none()));
            assert!(segments[4..].iter().all(|(address,_)|address.is_some()));
            assert_eq!(segments[3].1, 133_712);
            assert!(calendar_sites >= 4, "both calendar helpers and retained serial consumers have physical guarded readers");
            assert_eq!(calendar_spans.len(), 1, "all readers install the same complete family range");
            let (start, length) = calendar_spans.into_iter().next().unwrap();
            let end = start.checked_add(length).unwrap();
            for (address, bytes) in &segments[4..] {
                let address = address.unwrap();
                assert!(address + *bytes as u32 <= start || address >= end,
                    "ordinary instantiation leaves the calendar range cold instead of copying active calendar bytes");
            }
            assert_eq!(readiness.len(), 3, "normalization, properties and calendars retain separate roles");
            let bits = readiness.values().map(|bits| {
                assert_eq!(bits.len(), 1, "each family has exactly one readiness bit");
                *bits.iter().next().unwrap()
            }).collect::<BTreeSet<_>>();
            assert_eq!(bits.len(), 3, "one family's first use cannot mark another family ready");
            for bit in bits {
                let (ty, init) = &globals[(bit-imported_globals) as usize];
                assert!(ty.mutable && !ty.shared && ty.content_type == wasmparser::ValType::I32);
                assert!(matches!(init.as_slice(), [Operator::I32Const { value:0 }, Operator::End]),
                    "every actual R instance begins with its own cold readiness bit");
            }
            let initialize = named_function_body(bytes, "helper::pooled_strings_initialize");
            let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&initialize,0));
            assert!(!body.get_operators_reader().unwrap().into_iter().any(|op|
                matches!(op.unwrap(), Operator::MemoryInit { data_index:3, .. })),
                "bootstrap cannot eagerly install calendar rows");
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn literal_count_grows_descriptor_data_without_expanding_main_initialization() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let emit = |count: usize| {
                // The function holds many source literals but is never invoked.
                // Main's own operations are fixed, isolating literal-root code
                // from the independently emitted body and array-literal code.
                let literals = (0..count)
                    .map(|index| format!("value = 'program_pool_{index:05}';"))
                    .collect::<String>();
                let source = format!("function strings(value) {{{literals}return value;}} strings;");
                let parsed = parse(&source, ParseOptions::script()).expect("pool source parses");
                let program = lower(&parsed);
                assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
                let artifact = scriptw_aot_wasm::emit(&program).expect("pooled string program emits");
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(&artifact.bytes).expect("program descriptor module validates");
                let body = named_function_body(&artifact.bytes, "scriptw::main");
                let initializer = named_function_body(&artifact.bytes, "scriptw::program_init");
                assert_eq!(array_data_initializers(&body), [0; 4],
                    "main does not initialize the P passive images");
                assert_eq!(array_data_initializers(&initializer), [1, 1, 1, 0],
                    "one string-construction site and immutable metadata/descriptor arrays, independent of count");
                let segments = passive_program_images(&artifact.bytes);
                assert!(segments[3].is_empty());
                assert_eq!(segments[2].len() % 12, 0);
                assert!(segments[2].len() >= count * 12,
                    "every unique source literal needs its descriptor row");
                (artifact, body.len(), initializer.len(), segments[2].len())
            };
            let (empty, empty_main, empty_initialize, _) = emit(0);
            let (small, small_main, small_initialize, small_descriptors) = emit(16);
            let (large, large_main, large_initialize, large_descriptors) = emit(2048);
            assert_eq!(empty.runtime().unwrap().bytes(), small.runtime().unwrap().bytes());
            assert_eq!(small.runtime().unwrap().bytes(), large.runtime().unwrap().bytes(),
                "program string count must preserve the complete shared runtime");
            assert!(large_descriptors >= small_descriptors + (2048 - 16) * 12);
            assert!(small_initialize <= empty_initialize + 128
                && large_initialize <= empty_initialize + 128,
                "P initialization remains bounded independently of descriptor count");
            assert!(large_initialize <= small_initialize + 64,
                "descriptor count grows data rather than unrolling the P initializer");
            assert!(large.bytes.len() > small.bytes.len() + 32 * 1024);
            eprintln!("pooled string count main body: {small_main} -> {large_main} bytes");
            assert!(small_main <= empty_main + 128 && large_main <= empty_main + 128,
                "literal roots add bounded code to fixed main initialization: {empty_main} -> {small_main} -> {large_main}");
            assert!(large_main <= small_main + 64,
                "descriptor count grows data, not unrolled main code: {small_main} -> {large_main}");
        })
        .expect("bounded compiler worker starts")
        .join()
        .expect("pooled string descriptor emission completes");
}
