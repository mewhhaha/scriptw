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
                assert_eq!(array_data_initializers(&runtime_initializer), [1, 0, 0, 0],
                    "R uses one UTF16 construction site for all compiler literals");
                assert!(runtime_initializer.len() < 8192, "R bootstrap is bounded independently of its literal count");
                let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&runtime_initializer, 0));
                assert!(body.get_operators_reader().unwrap().into_iter().any(|operator| matches!(operator.unwrap(), Operator::Loop { .. })),
                    "runtime string allocation iterates over the sealed descriptor table");
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
