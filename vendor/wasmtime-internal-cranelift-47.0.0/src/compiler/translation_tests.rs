//! Scriptw patch: Wasm functions translated and compiled by this crate's
//! `Compiler` on the product's GC heap configuration (the copying collector,
//! signal-based traps, a heap that does not move and whose reservation plus
//! guard covers every 32-bit reference), checking which null checks, cast
//! checks and type-id loads the translation emits.
//!
//! With `SCRIPTW_TRANSLATION_DUMP=<directory>` set, each function's machine
//! code is written there as `<name>.bin` (raw x86-64, for `objdump -D -b
//! binary -m i386:x86-64`) and its CLIF after translation as `<name>.clif`;
//! a function compiled without the null guard is `<name>.unguarded.*`.

use super::{Compiler, CompilerContext};
use crate::{TRAP_CAST_FAILURE, TRAP_NULL_REFERENCE};
use cranelift_codegen::ir::{self, TrapCode};
use cranelift_codegen::settings::{self, Configurable};
use wasmparser::{Parser, Validator, WasmFeatures};
use wasmtime_environ::{
    Collector, CompiledFunctionBody, Compiler as _, FuncKey, InliningCompiler as _,
    ModuleEnvironment, ModuleTypesBuilder, PtrSize as _, StaticModuleIndex, Tunables,
};

// Value types and heap types.
const I32: u8 = 0x7f;
const ANYREF: u8 = 0x6e;
const REF_NULL: u8 = 0x63;
const REF: u8 = 0x64;

// The module's types: a final struct with one mutable `i32` field, and the
// signatures of the test functions and of the imported callee.
const STRUCT: u8 = 0;
const SIG_ONE_REF: u8 = 1;
const SIG_TWO_REFS: u8 = 2;
const SIG_ANYREF: u8 = 3;
const SIG_CALLEE: u8 = 4;
const SIG_NON_NULL_ANYREF: u8 = 5;
const SIG_NON_NULL_STRUCT: u8 = 6;

/// The imported function, which a function calls to have a call.
const CALLEE: u8 = 0;

// Instructions.
const BLOCK: u8 = 0x02;
const IF: u8 = 0x04;
const ELSE: u8 = 0x05;
const END: u8 = 0x0b;
const BR_IF: u8 = 0x0d;
const CALL: u8 = 0x10;
const DROP: u8 = 0x1a;
const LOCAL_GET: u8 = 0x20;
const LOCAL_SET: u8 = 0x21;
const LOCAL_TEE: u8 = 0x22;
const I32_CONST: u8 = 0x41;
const I32_EQZ: u8 = 0x45;
const I32_ADD: u8 = 0x6a;
const I32_AND: u8 = 0x71;
const I32_OR: u8 = 0x72;
const REF_IS_NULL: u8 = 0xd1;
const REF_AS_NON_NULL: u8 = 0xd4;
const GC: u8 = 0xfb;
const STRUCT_GET: u8 = 0x02;
const REF_TEST: u8 = 0x14;
const REF_TEST_NULLABLE: u8 = 0x15;
const REF_CAST: u8 = 0x16;
const EMPTY: u8 = 0x40;

/// `n` as an unsigned LEB128.
fn leb(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

fn vec(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = leb(items.len());
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn section(id: u8, contents: Vec<u8>) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb(contents.len()));
    out.extend(contents);
    out
}

/// A module of `functions` (signature, body without locals or the final
/// `end`), after an imported callee.
fn module(functions: &[(u8, &[u8])]) -> Vec<u8> {
    let nullable_struct = vec![REF_NULL, STRUCT];
    let types = vec(&[
        vec![0x5f, 1, I32, 1],
        [vec![0x60, 1], nullable_struct.clone(), vec![1, I32]].concat(),
        [
            vec![0x60, 2],
            nullable_struct.clone(),
            nullable_struct,
            vec![1, I32],
        ]
        .concat(),
        vec![0x60, 1, ANYREF, 1, I32],
        vec![0x60, 0, 0],
        vec![0x60, 1, REF, ANYREF, 1, I32],
        vec![0x60, 1, REF, STRUCT, 1, I32],
    ]);
    let import = vec(&[vec![1, b'm', 1, b'f', 0x00, SIG_CALLEE]]);
    let declarations = vec(&functions
        .iter()
        .map(|&(signature, _)| vec![signature])
        .collect::<Vec<_>>());
    let bodies = vec(&functions
        .iter()
        .map(|&(_, body)| {
            let code = [&[0][..], body, &[END]].concat();
            [leb(code.len()), code].concat()
        })
        .collect::<Vec<_>>());
    [
        b"\0asm\x01\0\0\0".to_vec(),
        section(1, types),
        section(2, import),
        section(3, declarations),
        section(10, bodies),
    ]
    .concat()
}

/// The product's GC heap configuration (`null_guard`), or one whose small
/// heap may grow and move, which keeps no null guard and no access is then
/// its own null check.
fn tunables(null_guard: bool) -> Tunables {
    let mut tunables = Tunables::default_u64();
    tunables.collector = Some(Collector::Copying);
    tunables.signals_based_traps = true;
    if null_guard {
        tunables.gc_heap_reservation = 1 << 32;
        tunables.gc_heap_guard_size = 32 << 20;
        tunables.gc_heap_reservation_for_growth = 0;
        tunables.gc_heap_may_move = false;
    } else {
        tunables.gc_heap_reservation = 1 << 20;
        tunables.gc_heap_guard_size = 0;
        tunables.gc_heap_reservation_for_growth = 1 << 20;
        tunables.gc_heap_may_move = true;
    }
    tunables
}

fn compiler(tunables: Tunables) -> Compiler {
    let mut flags = settings::builder();
    flags.set("enable_verifier", "true").unwrap();
    flags.set("opt_level", "speed").unwrap();
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();
    Compiler::new(
        tunables,
        isa,
        None,
        false,
        crate::builder::LinkOptions::default(),
        None,
        false,
    )
}

/// A translated function: its CLIF after translation and the passes that
/// follow it, and its machine code.
struct Translated {
    clif: ir::Function,
    code: Vec<u8>,
}

/// Translate and compile each of `functions` (see [`module`]).
fn translate(null_guard: bool, functions: &[(&str, u8, &[u8])]) -> Vec<Translated> {
    let tunables = tunables(null_guard);
    let compiler = compiler(tunables.clone());
    let wasm = module(
        &functions
            .iter()
            .map(|&(_, signature, body)| (signature, body))
            .collect::<Vec<_>>(),
    );
    let mut validator = Validator::new_with_features(WasmFeatures::all());
    let mut types = ModuleTypesBuilder::new(&validator);
    let module_index = StaticModuleIndex::from_u32(0);
    let mut translation =
        ModuleEnvironment::new(&tunables, &mut validator, &mut types, module_index)
            .translate(Parser::new(0), &wasm)
            .unwrap();
    let inputs = std::mem::take(&mut translation.function_body_inputs);
    let dump = std::env::var_os("SCRIPTW_TRANSLATION_DUMP").map(std::path::PathBuf::from);
    inputs
        .into_iter()
        .zip(functions)
        .map(|((index, input), &(name, _, _))| {
            let key = FuncKey::DefinedWasmFunction(module_index, index);
            let mut body: CompiledFunctionBody = compiler
                .compile_function(&translation, key, input, &types, name)
                .unwrap();
            let clif = body
                .code
                .downcast_ref::<Option<CompilerContext>>()
                .unwrap()
                .as_ref()
                .unwrap()
                .codegen_context
                .func
                .clone();
            compiler.finish_compiling(&mut body, None, name).unwrap();
            let code = body
                .code
                .downcast_ref::<crate::CompiledFunction>()
                .unwrap()
                .buffer
                .data()
                .to_vec();
            if let Some(dump) = &dump {
                let name = if null_guard {
                    name.to_string()
                } else {
                    format!("{name}.unguarded")
                };
                std::fs::write(dump.join(format!("{name}.bin")), &code).unwrap();
                std::fs::write(
                    dump.join(format!("{name}.clif")),
                    clif.display().to_string(),
                )
                .unwrap();
            }
            Translated { clif, code }
        })
        .collect()
}

fn insts(func: &ir::Function) -> impl Iterator<Item = &ir::InstructionData> + '_ {
    func.layout
        .blocks()
        .flat_map(move |block| func.layout.block_insts(block))
        .map(move |inst| &func.dfg.insts[inst])
}

/// The conditional and unconditional traps with `code`.
fn traps(func: &ir::Function, code: TrapCode) -> usize {
    insts(func)
        .filter(|data| data.trap_code() == Some(code))
        .count()
}

/// The loads that carry the trap code `code`.
fn trapping_loads(func: &ir::Function, code: TrapCode) -> usize {
    insts(func)
        .filter(|data| match **data {
            ir::InstructionData::Load { flags, .. } => {
                func.dfg.mem_flags[flags].trap_code() == Some(code)
            }
            _ => false,
        })
        .count()
}

/// The loads in the alias region that `description` names.
fn loads_in(func: &ir::Function, description: &str) -> usize {
    insts(func)
        .filter(|data| match **data {
            ir::InstructionData::Load { flags, .. } => func.dfg.mem_flags[flags]
                .alias_region()
                .is_some_and(|region| func.dfg.alias_regions[region].description == description),
            _ => false,
        })
        .count()
}

#[test]
fn null_tests_prove_the_paths_that_only_a_non_null_reference_takes() {
    #[rustfmt::skip]
    let functions: &[(&str, u8, &[u8])] = &[
        // `br_if (ref.is_null x)` falls through only with `x` not null.
        ("br_if_falls_through", SIG_ONE_REF, &[
            BLOCK, EMPTY,
                LOCAL_GET, 0, REF_IS_NULL, BR_IF, 0,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            END,
            I32_CONST, 0,
        ]),
        // The block's end is also reached by the branch: the check stays.
        ("after_br_if_target", SIG_ONE_REF, &[
            BLOCK, EMPTY,
                LOCAL_GET, 0, REF_IS_NULL, BR_IF, 0,
            END,
            LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            I32_CONST, 0,
        ]),
        // Only the alternative of `if (ref.is_null x)` knows `x` not null.
        ("if_is_null", SIG_ONE_REF, &[
            LOCAL_GET, 0, REF_IS_NULL,
            IF, EMPTY,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            ELSE,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            END,
            I32_CONST, 0,
        ]),
        // Only the consequent of `if (i32.eqz (ref.is_null x))` does.
        ("if_not_null", SIG_ONE_REF, &[
            LOCAL_GET, 0, REF_IS_NULL, I32_EQZ,
            IF, EMPTY,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            ELSE,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            END,
            I32_CONST, 0,
        ]),
        // Code after the `if` does not.
        ("after_if_not_null", SIG_ONE_REF, &[
            LOCAL_GET, 0, REF_IS_NULL, I32_EQZ,
            IF, EMPTY,
            END,
            LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
            I32_CONST, 0,
        ]),
        // A disjunction of null tests that is zero: neither is null.
        ("br_if_either_null", SIG_TWO_REFS, &[
            BLOCK, EMPTY,
                LOCAL_GET, 0, REF_IS_NULL, LOCAL_GET, 1, REF_IS_NULL, I32_OR, BR_IF, 0,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
                LOCAL_GET, 1, REF_AS_NON_NULL, DROP,
            END,
            I32_CONST, 0,
        ]),
        // A conjunction that is zero says nothing of either.
        ("br_if_both_null", SIG_TWO_REFS, &[
            BLOCK, EMPTY,
                LOCAL_GET, 0, REF_IS_NULL, LOCAL_GET, 1, REF_IS_NULL, I32_AND, BR_IF, 0,
                LOCAL_GET, 0, REF_AS_NON_NULL, DROP,
                LOCAL_GET, 1, REF_AS_NON_NULL, DROP,
            END,
            I32_CONST, 0,
        ]),
        // `br_if (i32.eqz (ref.test T x))` falls through with `x` a `T`.
        ("br_if_not_a_struct", SIG_ANYREF, &[
            BLOCK, EMPTY,
                LOCAL_GET, 0, GC, REF_TEST, STRUCT, I32_EQZ, BR_IF, 0,
                LOCAL_GET, 0, GC, REF_CAST, STRUCT, DROP,
            END,
            I32_CONST, 0,
        ]),
    ];
    let translated = translate(true, functions);
    let null_checks: Vec<usize> = translated
        .iter()
        .map(|t| traps(&t.clif, TRAP_NULL_REFERENCE))
        .collect();
    assert_eq!(null_checks, [0, 1, 1, 1, 1, 0, 2, 0]);
    // The cast after the failed `br_if` emits no check at all.
    assert_eq!(traps(&translated[7].clif, TRAP_CAST_FAILURE), 0);
    assert!(translated.iter().all(|t| !t.code.is_empty()));
}

#[test]
fn a_trapping_cast_leaves_its_null_check_to_the_header_load() {
    #[rustfmt::skip]
    let functions: &[(&str, u8, &[u8])] = &[
        ("cast_anyref", SIG_ANYREF, &[
            LOCAL_GET, 0, GC, REF_CAST, STRUCT, GC, STRUCT_GET, STRUCT, 0,
        ]),
    ];
    // In the null guard: the `i31` test and the type test trap, and the
    // header load carries the cast's code.
    let guarded = &translate(true, functions)[0].clif;
    assert_eq!(traps(guarded, TRAP_CAST_FAILURE), 2);
    assert_eq!(trapping_loads(guarded, TRAP_CAST_FAILURE), 1);
    // The field read after the cast needs no null check.
    assert_eq!(traps(guarded, TRAP_NULL_REFERENCE), 0);
    assert_eq!(trapping_loads(guarded, TRAP_NULL_REFERENCE), 0);
    // Without a null guard the explicit null check stays, before the `i31`
    // test.
    let explicit = &translate(false, functions)[0].clif;
    assert_eq!(traps(explicit, TRAP_CAST_FAILURE), 3);
    assert_eq!(trapping_loads(explicit, TRAP_CAST_FAILURE), 0);
}

#[test]
fn statically_non_null_cast_operands_skip_null_checks() {
    #[rustfmt::skip]
    let functions: &[(&str, u8, &[u8])] = &[
        ("cast_non_null_anyref", SIG_NON_NULL_ANYREF, &[
            LOCAL_GET, 0, GC, REF_CAST, STRUCT, GC, STRUCT_GET, STRUCT, 0,
        ]),
        ("test_nullable_target", SIG_ANYREF, &[
            LOCAL_GET, 0, GC, REF_TEST_NULLABLE, STRUCT,
        ]),
        ("test_nullable_target_non_null_anyref", SIG_NON_NULL_ANYREF, &[
            LOCAL_GET, 0, GC, REF_TEST_NULLABLE, STRUCT,
        ]),
        ("test_nullable_target_non_null_struct", SIG_NON_NULL_STRUCT, &[
            LOCAL_GET, 0, GC, REF_TEST_NULLABLE, STRUCT,
        ]),
    ];
    for null_guard in [true, false] {
        let translated = translate(null_guard, functions);
        let cast = &translated[0].clif;
        assert_eq!(traps(cast, TRAP_CAST_FAILURE), 2);
        assert_eq!(trapping_loads(cast, TRAP_CAST_FAILURE), 0);

        // A nullable operand tests null and i31; a non-null anyref only
        // tests i31, and a non-null struct goes straight to its header.
        for (test, branches) in translated[1..].iter().zip([2, 1, 0]) {
            assert_eq!(
                insts(&test.clif)
                    .filter(|inst| inst.opcode() == ir::Opcode::Brif)
                    .count(),
                branches,
            );
        }
    }
}

#[test]
fn ref_as_non_null_is_checked_by_the_next_access_through_its_operand() {
    #[rustfmt::skip]
    let functions: &[(&str, u8, &[u8])] = &[
        // Only a local write lies between: the field read is the check.
        ("deferred_to_struct_get", SIG_ONE_REF, &[
            LOCAL_GET, 0, REF_AS_NON_NULL, LOCAL_TEE, 0, GC, STRUCT_GET, STRUCT, 0,
        ]),
        // A call comes first: the check is made before it.
        ("flushed_by_a_call", SIG_ONE_REF, &[
            LOCAL_GET, 0, REF_AS_NON_NULL, CALL, CALLEE, GC, STRUCT_GET, STRUCT, 0,
        ]),
        // The next access is through another reference: the check is made
        // before it, and that access checks its own operand.
        ("flushed_by_another_access", SIG_TWO_REFS, &[
            LOCAL_GET, 0, REF_AS_NON_NULL, LOCAL_SET, 0,
            LOCAL_GET, 1, GC, STRUCT_GET, STRUCT, 0,
        ]),
    ];
    let translated = translate(true, functions);
    let explicit: Vec<usize> = translated
        .iter()
        .map(|t| traps(&t.clif, TRAP_NULL_REFERENCE))
        .collect();
    let implicit: Vec<usize> = translated
        .iter()
        .map(|t| trapping_loads(&t.clif, TRAP_NULL_REFERENCE))
        .collect();
    assert_eq!(explicit, [0, 1, 1]);
    assert_eq!(implicit, [1, 0, 1]);
    // Without a null guard, `ref.as_non_null` is checked where it is.
    let unguarded = translate(false, &functions[..1]);
    assert_eq!(traps(&unguarded[0].clif, TRAP_NULL_REFERENCE), 1);
    assert_eq!(trapping_loads(&unguarded[0].clif, TRAP_NULL_REFERENCE), 0);
}

#[test]
fn type_tests_read_the_type_id_array_base_from_its_home() {
    #[rustfmt::skip]
    let functions: &[(&str, u8, &[u8])] = &[
        ("tests_around_a_call", SIG_ANYREF, &[
            LOCAL_GET, 0, GC, REF_TEST, STRUCT,
            CALL, CALLEE,
            LOCAL_GET, 0, GC, REF_TEST, STRUCT,
            I32_ADD,
        ]),
        ("tests_without_a_call", SIG_ANYREF, &[
            LOCAL_GET, 0, GC, REF_TEST, STRUCT,
            LOCAL_GET, 0, GC, REF_TEST, STRUCT,
            I32_ADD,
        ]),
    ];
    let base = format!("VMContext+{:#x}", 8u8.vmctx_type_ids_array());
    let translated = translate(true, functions);
    // The base is read from the `vmctx` once, in the entry block; after the
    // call it is read from its home, not through the `vmctx` again.
    let around = &translated[0].clif;
    assert_eq!(loads_in(around, &base), 1);
    assert_eq!(loads_in(around, "TypeIdsHome"), 1);
    // Without a call nothing reads the home, and it is gone.
    let without = &translated[1].clif;
    assert_eq!(loads_in(without, &base), 1);
    assert_eq!(loads_in(without, "TypeIdsHome"), 0);
    assert!(
        without
            .sized_stack_slots
            .values()
            .all(|slot| slot.size == 0)
    );
}
