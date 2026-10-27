//! Inspect real R instructions: failure routes converge without cloning restoration.
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use std::collections::BTreeMap;
use wasmparser::{Operator, Parser, Payload, TypeRef, Validator, WasmFeatures};

fn snapshot_load(ops: &[Operator<'_>], start: usize) -> Option<(u32, u64, u32)> {
    match ops.get(start..start + 4)? {
        [Operator::LocalGet {
            local_index: address,
        }, Operator::I32WrapI64, Operator::I64Load { memarg }, Operator::LocalSet {
            local_index: destination,
        }] if memarg.memory == 0 && memarg.align == 3 => {
            Some((*address, memarg.offset, *destination))
        }
        _ => None,
    }
}

#[test]
fn regexp_failure_routes_converge_on_one_encoded_snapshot_restore_tail() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        let parsed = parse("/^(?:(a)|(ab)){2,4}?c\\2$/du.exec('aabcab');", ParseOptions::script()).unwrap();
        let program = lower(&parsed);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let artifact = scriptw_aot_wasm::emit(&program).unwrap();
        let runtime = artifact.runtime().expect("real RegExp links R");
        Validator::new_with_features(WasmFeatures::all()).validate_all(runtime.bytes()).unwrap();
        Validator::new_with_features(WasmFeatures::all()).validate_all(&artifact.bytes).unwrap();
        let bodies = crate::linked_bodies::module_bodies(runtime.bytes());
        let matches: Vec<_> = bodies.iter().filter(|body| body.name == "helper::regexp_matcher").collect();
        assert_eq!(matches.len(), 1);
        let target = matches[0].index;
        let mut next_function = 0;
        let mut checked = false;
        for payload in Parser::new(0).parse_all(runtime.bytes()) {
            match payload.unwrap() {
                Payload::ImportSection(imports) => {
                    for import in imports.into_imports() {
                        if matches!(import.unwrap().ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) { next_function += 1; }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let index = next_function; next_function += 1;
                    if index != target { continue; }
                    let ops: Vec<_> = body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect();
                    // ChoiceEntry's fallback PC followed by byte/UTF16/low-surrogate
                    // cursor reads is the physical recovery signature, not a source marker.
                    let mut restores = Vec::new();
                    for start in 0..ops.len().saturating_sub(15) {
                        let words: Option<Vec<_>> = (0..4).map(|n| snapshot_load(&ops, start + n * 4)).collect();
                        let Some(words) = words else { continue; };
                        if words.iter().map(|word| word.1).eq([24,40,48,56])
                            && words.iter().all(|word| word.0 == words[0].0)
                            && words.iter().map(|word| word.2).collect::<std::collections::BTreeSet<_>>().len() == 4 {
                            restores.push(start);
                        }
                    }
                    assert_eq!(restores.len(), 1, "one actual fallback-PC/cursor restore sequence: {restores:?}");
                    // Resolve relative depths to lexical block identities. Counting
                    // BrIf instructions alone cannot establish shared recovery.
                    let mut stack = vec![0u32];
                    let mut next_block = 1;
                    let mut branches: BTreeMap<u32,Vec<usize>> = BTreeMap::new();
                    let mut ends = BTreeMap::new();
                    for (position, op) in ops.iter().enumerate() {
                        match op {
                            Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                                stack.push(next_block); next_block += 1;
                            }
                            Operator::BrIf { relative_depth } => {
                                let block = stack[stack.len()-1-*relative_depth as usize];
                                branches.entry(block).or_default().push(position);
                            }
                            Operator::End => { ends.insert(stack.pop().expect("balanced actual Wasm control"), position); }
                            _ => {}
                        }
                    }
                    assert!(stack.is_empty());
                    let mut shared = Vec::new();
                    for (block, sites) in &branches {
                        if let Some(end) = ends.get(block) {
                            if sites.len() >= 25 && *end < restores[0] && sites.iter().all(|site| site < end) {
                                shared.push(*block);
                            }
                        }
                    }
                    assert_eq!(shared.len(), 1, "all matcher failure routes target one completed request block before the sole restore tail");
                    checked = true;
                }
                _ => {}
            }
        }
        assert!(checked, "inspect actual defined matcher instructions");
    }).unwrap().join().unwrap();
}
