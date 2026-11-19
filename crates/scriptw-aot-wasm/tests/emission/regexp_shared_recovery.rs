//! Inspect real R instructions: failure routes converge without cloning restoration.
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use std::collections::BTreeMap;
use wasmparser::{Operator, Parser, Payload, TypeRef, Validator, WasmFeatures};

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Function,
    Block,
    Loop,
    If,
    TryTable,
}

fn snapshot_load(ops: &[Operator<'_>], start: usize) -> Option<(u32, u64, u32)> {
    match ops.get(start..start + 4)? {
        [Operator::LocalGet {
            local_index: address,
        }, Operator::I32WrapI64, Operator::I64Load { memarg }, Operator::LocalSet {
            local_index: destination,
        }] if memarg.memory == 0 && memarg.align == 0 => {
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
                    // Both ordinary ChoiceEntry recovery and RequiredRun logical
                    // template recovery restore PC24 followed by cursor40/48/56.
                    // Assertion cursor restores omit PC24 and are not recovery tails.
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
                    assert_eq!(restores.len(), 2, "required-run template and ordinary snapshot restore paths: {restores:?}");
                    let destinations = |start| (0..4).map(|n| snapshot_load(&ops,start+n*4).unwrap().2).collect::<Vec<_>>();
                    assert_eq!(destinations(restores[0]),destinations(restores[1]),"both paths restore the same PC and cursor locals");
                    // Resolve relative depths to lexical block identities. Counting
                    // BrIf instructions alone cannot establish shared recovery.
                    let mut stack = vec![0u32];
                    let mut next_block = 1;
                    let mut branches: BTreeMap<u32,Vec<usize>> = BTreeMap::new();
                    let mut jumps: BTreeMap<u32,Vec<usize>> = BTreeMap::new();
                    let mut ends = BTreeMap::new();
                    let mut starts = BTreeMap::new();
                    let mut kinds = BTreeMap::from([(0,FrameKind::Function)]);
                    let mut parents = BTreeMap::new();
                    let mut elses = BTreeMap::new();
                    let mut restore_paths = Vec::new();
                    for (position, op) in ops.iter().enumerate() {
                        if restores.contains(&position) { restore_paths.push(stack.clone()); }
                        let kind = match op {
                            Operator::Block { .. } => Some(FrameKind::Block),
                            Operator::Loop { .. } => Some(FrameKind::Loop),
                            Operator::If { .. } => Some(FrameKind::If),
                            Operator::TryTable { .. } => Some(FrameKind::TryTable),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            starts.insert(next_block,position);
                            kinds.insert(next_block,kind);
                            parents.insert(next_block,*stack.last().unwrap());
                            stack.push(next_block);next_block+=1;
                        }
                        match op {
                            Operator::BrIf { relative_depth } => {
                                let block = stack[stack.len()-1-*relative_depth as usize];
                                branches.entry(block).or_default().push(position);
                            }
                            Operator::Br { relative_depth } => {
                                let block = stack[stack.len()-1-*relative_depth as usize];
                                jumps.entry(block).or_default().push(position);
                            }
                            Operator::Else => { elses.insert(*stack.last().unwrap(),position); }
                            Operator::End => { ends.insert(stack.pop().expect("balanced actual Wasm control"), position); }
                            _ => {}
                        }
                    }
                    assert!(stack.is_empty());
                    assert_eq!(restore_paths.len(),2);
                    let choose = *restore_paths[0].iter().rev().find(|&&frame|
                        kinds[&frame]==FrameKind::Loop && restore_paths[1].contains(&frame))
                        .expect("both restore paths belong to one innermost recovery-selection loop");
                    let retry = parents[&choose];
                    assert!(kinds[&retry]==FrameKind::Loop,"selection belongs directly to the shared dispatch-retry loop");
                    let mut shared = Vec::new();
                    for (block, sites) in &branches {
                        if kinds[block]!=FrameKind::Block || parents.get(block)!=Some(&retry) { continue; }
                        let end=ends[block];
                        if sites.len()>=25 && end<starts[&choose]
                            && sites.iter().all(|site| starts[block]<*site && *site<end) {
                            shared.push(*block);
                        }
                    }
                    assert_eq!(shared.len(),1,"all mismatch routes converge on one request block before both recovery paths");
                    let request=shared[0];
                    assert!(restores.iter().all(|position| ends[&request]<*position && *position<ends[&choose]));
                    let alternatives: Vec<_> = restore_paths[0].iter().filter(|&&frame|
                        kinds[&frame]==FrameKind::If && restore_paths[1].contains(&frame)
                        && elses.get(&frame).is_some_and(|split| restores[0]<*split && *split<restores[1]))
                        .collect();
                    assert_eq!(alternatives.len(),1,"RequiredRun template and ordinary entry restoration are opposite arms of one choice-kind dispatch");
                    let resumes = jumps.get(&retry).expect("successful recovery resumes original dispatch");
                    assert_eq!(resumes.iter().filter(|position| restores[1]+16<**position && **position<ends[&choose]).count(),1,
                        "both selected paths share one resume edge after their restoration");
                    assert!(branches.get(&choose).is_some_and(|sites| sites.iter().any(|position|
                        restores[0]+16<*position && *position<restores[1])),
                        "an unselected required-run entry retries selection rather than restarting the candidate");
                    checked = true;
                }
                _ => {}
            }
        }
        assert!(checked, "inspect actual defined matcher instructions");
    }).unwrap().join().unwrap();
}
