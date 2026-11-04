//! Exact operator comparison independent of local ordinals and outer labels.
use std::collections::BTreeMap;
use std::ops::Range;
use wasmparser::Operator;

pub fn closed_number_loops(body: &[Operator<'_>]) -> Vec<Range<usize>> {
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    for (at, op) in body.iter().enumerate() {
        match op {
            Operator::Block { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                stack.push(None)
            }
            Operator::Loop { .. } => stack.push(Some(at)),
            Operator::End => {
                if let Some(Some(start)) = stack.pop() {
                    let region = start + 1..at;
                    let ops = &body[region.clone()];
                    if ops.iter().any(|op| matches!(op, Operator::F64Add))
                        && ops.iter().any(|op| matches!(op, Operator::ArrayGet { .. }))
                        && !ops.iter().any(|op| {
                            matches!(
                                op,
                                Operator::Call { .. }
                                    | Operator::CallRef { .. }
                                    | Operator::CallIndirect { .. }
                                    | Operator::ReturnCall { .. }
                                    | Operator::ReturnCallRef { .. }
                                    | Operator::ReturnCallIndirect { .. }
                                    | Operator::StructSet { .. }
                                    | Operator::ArraySet { .. }
                                    | Operator::GlobalSet { .. }
                                    | Operator::StructNew { .. }
                                    | Operator::StructNewDefault { .. }
                                    | Operator::ArrayNew { .. }
                                    | Operator::ArrayNewDefault { .. }
                                    | Operator::ArrayNewFixed { .. }
                                    | Operator::ArrayNewData { .. }
                                    | Operator::ArrayNewElem { .. }
                            )
                        })
                    {
                        regions.push(region);
                    }
                }
            }
            _ => {}
        }
    }
    regions
}

pub fn normalize(body: &[Operator<'_>], region: Range<usize>) -> Vec<String> {
    let mut frames = Vec::new();
    let mut ends = BTreeMap::new();
    let mut branches = BTreeMap::new();
    for (at, op) in body.iter().enumerate() {
        match op {
            Operator::Block { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                frames.push((at, false))
            }
            Operator::Loop { .. } => frames.push((at, true)),
            Operator::End => {
                if let Some((start, _)) = frames.pop() {
                    ends.insert(start, at + 1);
                }
            }
            Operator::Br { relative_depth } | Operator::BrIf { relative_depth } => {
                let target = frames
                    .len()
                    .checked_sub(*relative_depth as usize + 1)
                    .map(|i| frames[i]);
                branches.insert(at, target);
            }
            _ => {}
        }
    }
    let mut locals = BTreeMap::new();
    let mut exits = BTreeMap::new();
    body[region.clone()]
        .iter()
        .enumerate()
        .map(|(offset, op)| {
            let at = region.start + offset;
            match op {
                Operator::LocalGet { local_index }
                | Operator::LocalSet { local_index }
                | Operator::LocalTee { local_index } => {
                    let next = locals.len();
                    let slot = locals.entry(*local_index).or_insert(next);
                    let kind = match op {
                        Operator::LocalGet { .. } => "LocalGet",
                        Operator::LocalSet { .. } => "LocalSet",
                        _ => "LocalTee",
                    };
                    format!("{kind} local{slot}")
                }
                Operator::Br { .. } | Operator::BrIf { .. } => {
                    let kind = if matches!(op, Operator::Br { .. }) {
                        "Br"
                    } else {
                        "BrIf"
                    };
                    let target = branches[&at]
                        .map(|(start, is_loop)| if is_loop { start } else { ends[&start] });
                    let label = if target == Some(region.start - 1) {
                        "backedge".to_owned()
                    } else if let Some(target) = target.filter(|target| region.contains(target)) {
                        format!("operator{}", target - region.start)
                    } else {
                        let next = exits.len();
                        format!("exit{}", exits.entry(target).or_insert(next))
                    };
                    format!("{kind} {label}")
                }
                Operator::BrTable { .. } | Operator::Return => {
                    panic!("untracked closed-loop escape")
                }
                _ => format!("{op:?}"),
            }
        })
        .collect()
}
