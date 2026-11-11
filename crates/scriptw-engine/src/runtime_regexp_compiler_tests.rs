use super::*;
use scriptw_aot_wasm::{GcHostField, GcHostLayout};
use scriptw_ir::{
    RegExpProgram, RegExpProgramWord, ValidatedRegExpProgram, REGEXP_MAX_INSTRUCTIONS,
};

use wasmtime::{ArrayRef, ArrayRefPre, Rooted, StructRef, StructRefPre};

#[test]
fn finite_regexp_candidates_match_many_sources_flags_and_runtime_misses() {
    configure_compilation_jobs(1).unwrap();
    let values = (0..256)
        .map(|index| format!("'candidate_{index:04}'"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"
var values = [{values}];
for (var index of [0, 127, 255]) {{
  for (var flags of ['', 'g', 'i']) {{
    var pattern = new RegExp(values[index], flags);
    var text = flags === 'i' ? values[index].toUpperCase() : values[index];
    if (pattern.exec(text)[0] !== text) throw 'candidate source/flags selected another row';
  }}
}}
var unicode = new RegExp('Ā\u0000𝄞', 'u');
var surrogate = new RegExp('\uD800', '');
var rejected = false;
try {{ new RegExp('['); }} catch (error) {{ rejected = error instanceof SyntaxError; }}
var unseen = String.fromCharCode(120, 121, 122) + '+';
var fallback = new RegExp(unseen);
unicode.test('Ā\u0000𝄞') && surrogate.test('\uD800') && rejected &&
new RegExp('').test('') && new RegExp('a').test('a') && fallback.exec('xyzz')[0] === 'xyzz';
"#
    );
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            &source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                timeout_ms: Some(30_000),
                ..RunOptions::default()
            },
        )
        .expect("finite candidate lookup and compiler fallback execute through Wasm");
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

/// R already exports every helper at its declared index. Discover that index
/// from the physical runtime's name section without rewriting its cache identity.
fn runtime_export(bytes: &[u8], helper: &str) -> String {
    for payload in WasmParser::new(0).parse_all(bytes) {
        let WasmPayload::CustomSection(section) = payload.unwrap() else {
            continue;
        };
        let wasmparser::KnownCustom::Name(subsections) = section.as_known() else {
            continue;
        };
        for subsection in subsections {
            if let wasmparser::Name::Function(names) = subsection.unwrap() {
                for naming in names {
                    let naming = naming.unwrap();
                    if naming.name == helper {
                        return format!("f{}", naming.index);
                    }
                }
            }
        }
    }
    panic!("runtime has no declared {helper}")
}

type ProgramRoot = Option<Rooted<StructRef>>;

struct CompilerStore {
    limits: WasmtimeStoreLimits,
}
struct RuntimeCompiler {
    store: WasmtimeStore<CompilerStore>,
    memory: WasmtimeMemory,
    allocate: wasmtime::TypedFunc<i64, i64>,
    compile: wasmtime::Func,
    string_allocator: StructRefPre,
    units_allocator: ArrayRefPre,
}

impl RuntimeCompiler {
    fn new() -> Self {
        configure_compilation_jobs(1).unwrap();
        let engine = Engine::new(RealmBuilder::new().build());
        let unit = engine
            .compile_script("new RegExp('');", CompileOptions::default())
            .unwrap();
        let artifact = engine.emit_wasm(&unit).unwrap();
        let runtime = artifact.runtime.as_ref().expect("linked runtime");
        let bytes = runtime.0.bytes();
        let compiler_export = runtime_export(bytes, "helper::regexp_compiler");
        let allocator_export = runtime_export(bytes, "helper::transient_byte_alloc");
        let wasm_engine = shared_wasm_engine().unwrap();
        let module = WasmtimeModule::new(&wasm_engine, bytes).unwrap();
        let mut store = WasmtimeStore::new(
            &wasm_engine,
            CompilerStore {
                limits: WasmtimeStoreLimitsBuilder::new()
                    .memory_size(64 * 1024 * 1024)
                    .build(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let mut linker = WasmtimeLinker::new(&wasm_engine);
        for import in module.imports() {
            match import.ty() {
                WasmtimeExternType::Func(signature) => {
                    linker
                        .func_new(import.module(), import.name(), signature, |_, _, _| {
                            Err(wasmtime::Error::msg(
                                "pure Pattern compiler called a host import",
                            ))
                        })
                        .unwrap();
                }
                WasmtimeExternType::Memory(memory_type) if memory_type.is_shared() => {
                    let memory = WasmtimeSharedMemory::new(&wasm_engine, memory_type).unwrap();
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
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let allocate = instance
            .get_typed_func(&mut store, &allocator_export)
            .unwrap();
        let compile = instance.get_func(&mut store, &compiler_export).unwrap();
        let signature = compile.ty(&store);
        assert_eq!(signature.params().len(), 2, "two non-null GC Strings");
        assert_eq!(signature.results().len(), 4, "program plus typed status");
        let parameter = signature.params().next().unwrap();
        let string_type = parameter
            .as_ref()
            .unwrap()
            .heap_type()
            .as_concrete_struct()
            .unwrap()
            .clone();
        assert_eq!(
            Some(string_type.fields().len() as u32),
            GcHostLayout::String.field_count()
        );
        let cached_hash = string_type
            .field(GcHostField::StringRawKeyHash.definition().index() as usize)
            .unwrap();
        assert_eq!(cached_hash.mutability(), wasmtime::Mutability::Var);
        assert!(matches!(
            cached_hash.element_type(),
            wasmtime::StorageType::ValType(wasmtime::ValType::I64)
        ));
        let field = string_type
            .field(GcHostField::StringCodeUnits.definition().index() as usize)
            .unwrap();
        let units_type = field
            .element_type()
            .as_val_type()
            .unwrap()
            .as_ref()
            .unwrap()
            .heap_type()
            .as_concrete_array()
            .unwrap()
            .clone();
        assert!(matches!(
            units_type.element_type(),
            wasmtime::StorageType::I16
        ));
        let string_allocator = StructRefPre::new(&mut store, string_type);
        let units_allocator = ArrayRefPre::new(&mut store, units_type);
        Self {
            store,
            memory,
            allocate,
            compile,
            string_allocator,
            units_allocator,
        }
    }

    fn string(&mut self, text: &str) -> Rooted<StructRef> {
        let values: Vec<_> = text
            .encode_utf16()
            .map(|unit| WasmtimeVal::I32(i32::from(unit)))
            .collect();
        let units = ArrayRef::new_fixed(&mut self.store, &self.units_allocator, &values)
            .expect("actual compiler CodeUnitArray");
        StructRef::new(
            &mut self.store,
            &self.string_allocator,
            &[WasmtimeVal::from(units), WasmtimeVal::I64(0)],
        )
        .expect("actual compiler StringValue")
    }

    fn heap_pointer(&mut self) -> i64 {
        self.allocate.call(&mut self.store, 0).unwrap()
    }

    fn compile_strings(
        &mut self,
        source: Rooted<StructRef>,
        flags: Rooted<StructRef>,
    ) -> (ProgramRoot, i32, i64, i64) {
        let mut result = [
            WasmtimeVal::AnyRef(None),
            WasmtimeVal::I32(0),
            WasmtimeVal::I64(0),
            WasmtimeVal::I64(0),
        ];
        self.compile
            .call(
                &mut self.store,
                &[WasmtimeVal::from(source), WasmtimeVal::from(flags)],
                &mut result,
            )
            .expect("compiler failures return typed status, never trap");
        let [WasmtimeVal::AnyRef(program), WasmtimeVal::I32(status), WasmtimeVal::I64(offset), WasmtimeVal::I64(detail)] =
            result
        else {
            panic!("RegExpCompile result signature")
        };
        let program = program.map(|value| {
            value
                .as_struct(&self.store)
                .unwrap()
                .expect("RegExpProgram result")
        });
        (program, status, offset, detail)
    }

    fn call(&mut self, source: &str, flags: &str) -> (ProgramRoot, i32, i64, i64, i64) {
        let source = self.string(source);
        let flags = self.string(flags);
        let before = self.heap_pointer();
        let (program, status, offset, detail) = self.compile_strings(source, flags);
        (program, status, offset, detail, before)
    }

    fn descriptor(&mut self, program: ProgramRoot) -> Vec<u8> {
        let program = program.expect("success publishes a rooted immutable program");
        let field = program.field(&mut self.store, 0).unwrap();
        let WasmtimeVal::AnyRef(Some(bytes)) = field else {
            panic!("RegExpProgram owns a non-null encoded-byte array")
        };
        let bytes = bytes.as_array(&self.store).unwrap().unwrap();
        let mut output = vec![0; bytes.len(&self.store).unwrap() as usize];
        bytes
            .copy_to_i8_slice(&mut self.store, &mut output)
            .unwrap();
        output
    }
}

#[test]
fn emitted_pattern_compiler_roundtrips_owned_descriptors_without_candidate_lookup_or_host_calls() {
    run_on_sized_stack(|| {
        let mut runtime = RuntimeCompiler::new();
        let nested = format!("{}a{}", "(".repeat(200), ")".repeat(200));
        let nested_modifiers = format!("{}^a${}", "(?m-s:".repeat(200), ")".repeat(200));
        // The single-body loop plus its final MATCH must exactly fill the
        // shared cap; a duplicated star body would reject this valid program.
        let limit_plus = format!("(?:{})+", "a".repeat(REGEXP_MAX_INSTRUCTIONS - 2));
        for (source, flags) in [
            ("", ""),
            ("abc", ""),
            ("a", "u"),
            ("a", "v"),
            (r"\p{ASCII}", "u"),
            (r"[\q{ab}]", "v"),
            (r"[\q{|ab}]", "v"),
            (r"[\q{Ab}]", "iv"),
            ("a+", ""),
            ("a+?", ""),
            ("(?:ab)+", ""),
            ("(a|b)+", ""),
            ("(a|)+", ""),
            ("(a?)+?", ""),
            (r"(a)?\1+", ""),
            (r"\c", ""),
            (r"\c0", ""),
            (r"\c_", ""),
            (r"\c+", ""),
            (r"\u{3}", ""),
            (r"\p{2}", ""),
            ("a|bc|", ""),
            ("(ab)+?", ""),
            ("(a?)*b", ""),
            ("(?:a?){1,3}?b", ""),
            ("(a(b)){0}", "d"),
            ("(?=(a+))a+", ""),
            ("(?<name>a)", ""),
            ("(?<=a)b", ""),
            ("a(?!bc)d", ""),
            ("(a)\\1", ""),
            ("\\1(a)", ""),
            ("(a|)\\1*", ""),
            ("(?s:^.$)", ""),
            ("(?-s:^.$)", "s"),
            ("(?m:^a$)", ""),
            ("(?-m:^a$)", "m"),
            ("(?ms:^.(?-ms:^.$).$)^.$", ""),
            ("(?s:(.)(?:.)(?=.)).", ""),
            ("(?i:(?-i:a))", ""),
            (r"(?-i:(a))(?i:\1)", "i"),
            ("(?im-s:.)|.", "s"),
            ("(?i-:.)", ""),
            (nested.as_str(), ""),
            (nested_modifiers.as_str(), "s"),
            (limit_plus.as_str(), ""),
            ("😀+", ""),
            ("(?:😀)+", ""),
        ] {
            let (handle, status, _, _, before) = runtime.call(source, flags);
            assert_eq!(status, 0, "{source:?}/{flags}");
            assert!(handle.is_some(), "success publishes a rooted program");
            assert_eq!(
                runtime.heap_pointer(),
                before,
                "GC publication releases all private compiler workspace"
            );
            let bytes = runtime.descriptor(handle);
            let program = ValidatedRegExpProgram::from_bytes(bytes.clone()).unwrap();
            let expected = ValidatedRegExpProgram::from_program(
                &RegExpProgram::compile(source, flags).unwrap(),
            )
            .unwrap();
            assert_eq!(program.bytes(), expected.bytes(), "{source:?}/{flags}");
            if source == limit_plus {
                assert_eq!(
                    program.word(RegExpProgramWord::InstructionCount),
                    REGEXP_MAX_INSTRUCTIONS as u64,
                );
            }
            assert_eq!(runtime.heap_pointer(), before);
        }
        let (first, status, _, _, _) = runtime.call("(ab)+", "d");
        assert_eq!(status, 0);
        let retained = runtime.descriptor(first);
        for _ in 0..6 {
            assert_eq!(runtime.call("(x?)*y", "i").1, 0);
        }
        runtime
            .store
            .gc(None)
            .expect("collect with retained program roots");
        assert_eq!(
            runtime.descriptor(first),
            retained,
            "later compiler workspace must not borrow retained descriptor storage"
        );
    });
}

#[test]
fn emitted_folded_string_positions_preserve_literal_bitmap_and_normalized_range_forms() {
    run_on_sized_stack(|| {
        let mut runtime = RuntimeCompiler::new();
        for (source, flags, range_count) in [
            (r"[\q{Ab}]", "iv", 0),
            (r"[\q{Ab|aB}]", "iv", 0),
            (r"[\q{11}]", "iv", 0),
            (r"[\q{\uD8001}]", "iv", 0),
            (r"[\q{\u212A\u017F}]", "iv", 6),
            (r"[\q{\u03A31}]", "iv", 2),
            (r"[\q{\u{10400}1}]", "iv", 2),
            (r"(?i:[\q{Ab}])(?-i:[\q{Cd}])", "v", 0),
            (r"(?<=[\q{Ab}])", "iv", 0),
        ] {
            let (handle, status, _, detail, before) = runtime.call(source, flags);
            assert_eq!(status, 0, "{source:?}/{flags}; detail={detail}");
            assert!(handle.is_some(), "success publishes a rooted program");
            assert_eq!(runtime.heap_pointer(), before, "{source:?}/{flags}");
            let program = ValidatedRegExpProgram::from_bytes(runtime.descriptor(handle)).unwrap();
            let expected = ValidatedRegExpProgram::from_program(
                &RegExpProgram::compile(source, flags).unwrap(),
            )
            .unwrap();
            assert_eq!(program.bytes(), expected.bytes(), "{source:?}/{flags}");
            assert_eq!(
                program.word(RegExpProgramWord::RangeCount),
                range_count,
                "only changed non-ASCII fold classes spend range entries: {source:?}/{flags}"
            );
            assert_eq!(runtime.heap_pointer(), before, "{source:?}/{flags}");
        }
    });
}

#[test]
fn emitted_pattern_compiler_distinguishes_syntax_resources_and_rolls_back_each_failure() {
    run_on_sized_stack(|| {
        let mut runtime = RuntimeCompiler::new();
        let resource_probe = "a".repeat(REGEXP_MAX_INSTRUCTIONS);
        for (source, flags, expected) in [
            ("(", "", 1),
            ("[", "", 1),
            ("[z-a]", "", 1),
            ("a{3,2}", "", 1),
            (
                "a{999999999999999999999999,888888888888888888888888}",
                "",
                1,
            ),
            ("a", "ii", 1),
            ("a", "uv", 1),
            ("(?-:a)", "", 1),
            ("(?ii:a)", "", 1),
            ("(?m-m:a)", "", 1),
            ("(?i-ss:a)", "", 1),
            ("(?i--s:a)", "", 1),
            ("(?ig:a)", "", 1),
            ("(?d:a)", "", 1),
            ("(?i", "", 1),
            ("(?i:a", "", 1),
            (resource_probe.as_str(), "", 3),
        ] {
            let (handle, status, _, detail, before) = runtime.call(source, flags);
            assert_eq!(status, expected, "{source:?}/{flags}; detail={detail}");
            assert!(handle.is_none(), "failure cannot publish a program");
            assert_eq!(runtime.heap_pointer(), before, "{source:?}/{flags}");
        }
        let (counted, status, _, _, before) =
            runtime.call(&format!("a{{{REGEXP_MAX_INSTRUCTIONS}}}"), "");
        assert_eq!(status, 0, "large finite bounds retain one counted body");
        let counted = ValidatedRegExpProgram::from_bytes(runtime.descriptor(counted)).unwrap();
        assert!(counted.word(RegExpProgramWord::InstructionCount) < 10);
        assert_eq!(runtime.heap_pointer(), before);
        let (handle, status, _, _, _) = runtime.call("(?:){999999999999999999999999}", "");
        assert_eq!(
            status, 0,
            "state-free empty repetition must not expand or loop over the decimal count"
        );
        let descriptor = ValidatedRegExpProgram::from_bytes(runtime.descriptor(handle)).unwrap();
        assert_eq!(descriptor.word(RegExpProgramWord::InstructionCount), 1);
        let (erased, status, _, _, _) = runtime.call("(a{999999999999999999999999}){0}", "");
        assert_eq!(
            status, 0,
            "erased expansion still retains capture cardinality"
        );
        let erased = ValidatedRegExpProgram::from_bytes(runtime.descriptor(erased)).unwrap();
        assert_eq!(erased.word(RegExpProgramWord::InstructionCount), 1);
        assert_eq!(erased.word(RegExpProgramWord::CaptureCount), 1);
    });
}

#[test]
fn emitted_pattern_compiler_elides_only_structurally_pure_empty_composition() {
    run_on_sized_stack(|| {
        let mut runtime = RuntimeCompiler::new();
        for (source, simplified) in [
            ("(?:(?:|){18446744073709551616}){18446744073709551616}", ""),
            (
                "(?:(?i:|)(?-i:||)){18446744073709551616,18446744073709551618}?",
                "",
            ),
            ("((?:|){18446744073709551616})", "()"),
            ("(?<value>(?:|){18446744073709551616})", "(?<value>)"),
            ("(?<=((?:|){18446744073709551616}))a", "(?<=())a"),
            ("(?:(?:|){18446744073709551616}|a)b", "(?:|a)b"),
            ("(?:()|){3}", "(?:()|){3}"),
            ("(?:^|){3}", "(?:^|){3}"),
            ("(?:(?!)|){3}", "(?:(?!)|){3}"),
            (r"()(?:\1|){3}", r"()(?:\1|){3}"),
        ] {
            let (handle, status, _, _, before) = runtime.call(source, "d");
            assert_eq!(status, 0, "{source}");
            assert_eq!(runtime.heap_pointer(), before);
            let actual = ValidatedRegExpProgram::from_bytes(runtime.descriptor(handle)).unwrap();
            let literal =
                ValidatedRegExpProgram::from_program(&RegExpProgram::compile(source, "d").unwrap())
                    .unwrap();
            let expected = ValidatedRegExpProgram::from_program(
                &RegExpProgram::compile(simplified, "d").unwrap(),
            )
            .unwrap();
            assert_eq!(
                actual.bytes(),
                literal.bytes(),
                "literal/runtime parity: {source}"
            );
            assert_eq!(actual.bytes(), expected.bytes(), "{source}");
        }
        for source in [
            "(?:|){3,2}",
            "(?:(?:|){3,2}){0}",
            r"(?:(?:)|\k<missing>){0}",
            "(?:(?<value>)(?<value>)){0}",
        ] {
            let (handle, status, _, _, before) = runtime.call(source, "u");
            assert_eq!(status, 1, "{source}");
            assert!(handle.is_none(), "{source}");
            assert_eq!(runtime.heap_pointer(), before, "{source}");
        }
    });
}

#[test]
fn emitted_pattern_compiler_clears_released_memory_before_allocator_reuse() {
    run_on_sized_stack(|| {
        let mut runtime = RuntimeCompiler::new();
        let sentinel_pointer = runtime.allocate.call(&mut runtime.store, 128).unwrap() as usize;
        runtime
            .memory
            .write(&mut runtime.store, sentinel_pointer, &[0xa5; 128])
            .unwrap();
        let mut retained = Vec::new();
        let resource_probe = "a".repeat(REGEXP_MAX_INSTRUCTIONS);
        for (source, flags, expected_status) in [
            (r"(a)?\1*", "", 0),
            (r"(?:(a)|b)\1*", "", 0),
            (r"(?i:(a|b)+)\1", "d", 0),
            ("(abc", "", 1),
            ("[z-a]", "", 1),
            ("abc(?<name>x)", "", 0),
            (resource_probe.as_str(), "", 3),
            ("a", "ii", 1),
            (r"\p{ASCII}", "u", 0),
            (r"[\q{ab|a}]", "v", 0),
            (r"[^\q{ab}]", "v", 1),
        ] {
            let (handle, status, _, detail, before) = runtime.call(source, flags);
            assert_eq!(
                status, expected_status,
                "{source:?}/{flags}; detail={detail}"
            );
            let after = runtime.heap_pointer();
            if status == 0 {
                let bytes = runtime.descriptor(handle);
                ValidatedRegExpProgram::from_bytes(bytes.clone()).unwrap();
                assert_eq!(after, before, "descriptor is rooted GC data, not scratch");
                retained.push((handle, bytes));
            } else {
                assert!(handle.is_none());
                assert_eq!(after, before);
            }
            let abandoned = &runtime.memory.data(&runtime.store)[after as usize..];
            let dirty = abandoned.iter().position(|byte| *byte != 0);
            assert_eq!(
                dirty, None,
                "{source:?}/{flags}: released workspace contains bytes at {dirty:?}"
            );
            assert_eq!(
                &runtime.memory.data(&runtime.store)[sentinel_pointer..sentinel_pointer + 128],
                &[0xa5; 128],
                "compilation may not clear allocations before its checkpoint"
            );
            for (handle, bytes) in &retained {
                assert_eq!(runtime.descriptor(*handle), *bytes);
            }
            let reused = runtime.allocate.call(&mut runtime.store, 16_384).unwrap();
            assert_eq!(reused, after, "next allocation reuses the released region");
            assert!(
                runtime.memory.data(&runtime.store)[reused as usize..reused as usize + 16_384]
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
    });
}

fn with_resource_probe(fixture: &str) -> String {
    // Counted repetition retains one body, so a{32768} is valid. This computed
    // UTF-16 source has one instruction per atom plus MATCH and exceeds the
    // unchanged instruction cap. Repeat avoids quadratic fixture concatenation.
    format!(
        "var regexpResourceProbe = String.fromCharCode(97).repeat({REGEXP_MAX_INSTRUCTIONS});\n{fixture}"
    )
}

#[test]
fn computed_pattern_workspace_reuse_preserves_capture_arrays_and_fresh_objects() {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let source = with_resource_probe(include_str!("testdata/runtime-regexp-workspace-reuse.js"));
    let outcome = engine
        .run_script(
            &source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn runtime_workspace_memory_growth_failure_is_a_typed_rollback_not_a_trap() {
    run_on_sized_stack(|| {
        let mut runtime = RuntimeCompiler::new();
        let source = runtime.string("(abc)+");
        let flags = runtime.string("i");
        let before = runtime.heap_pointer();
        let physical = runtime.memory.data_size(&runtime.store);
        runtime.store.data_mut().limits = WasmtimeStoreLimitsBuilder::new()
            .memory_size(physical)
            .build();
        let (handle, status, _, detail) = runtime.compile_strings(source, flags);
        assert!(handle.is_none());
        assert_eq!((status, detail), (3, 2));
        assert_eq!(runtime.heap_pointer(), before);
    });
}

#[test]
fn computed_legacy_patterns_and_constructor_protocols_execute_in_wasm() {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let source = with_resource_probe(include_str!("testdata/runtime-regexp-grammar.js"));
    let outcome = engine
        .run_script(
            &source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn regexp_clones_recompile_changed_flags_and_retain_static_capabilities_for_bookkeeping_flags() {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            r#"
var re = /(?<x>Ā+)/du;
var clone = new RegExp(re, 'dgu');
var iterator = 'ĀĀ Ā'.matchAll(clone);
var first = iterator.next();
var second = iterator.next();
var plain = /a/g;
Object.defineProperty(plain, 'flags', { value: 'ig' });
var folded = plain[Symbol.matchAll]('A').next();
var separator = /a/;
Object.defineProperty(separator, 'flags', { value: 'i' });
var split = 'xAy'.split(separator);
var cloned = new RegExp(/a/, 'i').exec('A');
var mutated = /a/;
var flags = { toString: function() { mutated.compile('b'); return 'i'; } };
var snapshot = new RegExp(mutated, flags);
first.value.groups.x === 'ĀĀ' && second.value.groups.x === 'Ā' &&
first.value.indices.groups.x[1] === 2 && iterator.next().done &&
!folded.done && folded.value[0] === 'A' && split.length === 2 &&
split[0] === 'x' && split[1] === 'y' && cloned[0] === 'A' &&
snapshot.source === 'a' && snapshot.test('A') && !snapshot.test('B') && mutated.source === 'b';
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn static_control_escapes_and_unmatched_backreferences_preserve_legacy_matching() {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            r#"
if (/\c/.test('c')) throw 'invalid control escape lost its backslash';
if (/\c/.exec('\\c')[0] !== '\\c') throw 'invalid control escape match';
if (/\c0/.exec('\\c0')[0] !== '\\c0') throw 'digit control escape outside class';
if (/\c_/.exec('\\c_')[0] !== '\\c_') throw 'underscore control escape outside class';
if (/[\c0]/.exec('\x10')[0] !== '\x10') throw 'class digit control escape';
if (/[\c_]/.exec('\x1f')[0] !== '\x1f') throw 'class underscore control escape';

var optional = /(a)?\1*/.exec('');
var alternative = /(?:(a)|b)\1*/.exec('b');
optional[0] === '' && optional[1] === undefined &&
alternative[0] === 'b' && alternative[1] === undefined;
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn computed_scoped_modifiers_restore_lexical_flags_and_reject_invalid_prefixes() {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            include_str!("testdata/runtime-regexp-modifiers.js"),
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

#[test]
fn regexp_modifier_clones_recompile_changed_global_flags_without_changing_scopes() {
    configure_compilation_jobs(1).unwrap();
    let engine = Engine::new(RealmBuilder::new().build());
    let outcome = engine
        .run_script(
            r#"
var localCase = new RegExp(/(?i:aB)/i, '');
var sensitive = new RegExp(/(?-i:aB)/, 'i');
var localDot = new RegExp(/(?s:^.$)/s, '');
var ordinaryDot = new RegExp(/(?-s:^.$)/, 's');
var localLine = new RegExp(/(?m:es$)/m, '');
var wholeLine = new RegExp(/^(?-m:es$)/, 'm');
var original = /(?i:a)b/g;
original.lastIndex = 9;
var clone = new RegExp(original, 'ig');
var independent = clone.exec('AB');
localCase.test('Ab') && !localCase.ignoreCase &&
sensitive.test('aB') && !sensitive.test('Ab') && sensitive.ignoreCase &&
localDot.test('\n') && !localDot.dotAll &&
!ordinaryDot.test('\n') && ordinaryDot.test('x') && ordinaryDot.dotAll &&
localLine.exec('yes\nno').index === 1 && !localLine.multiline &&
wholeLine.test('es') && !wholeLine.test('es\nno') && wholeLine.multiline &&
independent[0] === 'AB' && clone.lastIndex === 2 && original.lastIndex === 9 &&
clone.source === original.source && clone.flags === 'gi';
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.backend_used, ExecutionBackend::WasmAot);
    assert!(outcome.note.contains("boolean(true)"), "{}", outcome.note);
}

mod raw_utf16;

#[test]
fn regexp_candidate_hook_retains_coercion_errors_and_fallback() {
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            r#"
var events = "";
var source = { toString() { events += "source;"; return "^a+$"; } };
var flags = { toString() { events += "flags;"; return "i"; } };
var cached = new RegExp(source, flags).test("AA");
var uncatalogued = new RegExp(["^", "b+", "$"].join("")).test("bb");
var invalidFlags = false;
try {
  new RegExp({ toString() { events += "invalid;"; return "("; } }, "gg");
} catch (error) { invalidFlags = error instanceof SyntaxError; }
var invalidPattern = false;
try { new RegExp("("); }
catch (error) { invalidPattern = error instanceof SyntaxError; }
var intervening = eval("new RegExp('^z+$').test('zz')");
var retained = new RegExp("^a+$", "i").test("AA");
[events, cached, uncatalogued, invalidFlags, invalidPattern, intervening, retained].join("|");
"#,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .expect(
            "RegExp candidates retain conversion order, errors, misses and the calling program",
        );
    assert!(
        outcome
            .note
            .contains("string(source;flags;invalid;|true|true|true|true|true|true)"),
        "note: {}",
        outcome.note
    );
}
