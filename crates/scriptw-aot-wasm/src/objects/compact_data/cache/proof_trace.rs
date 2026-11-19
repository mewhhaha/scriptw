//! A unit-test record of the proof emitters a body's emission ran, so a
//! test can show that a profiled P read site proves what the R read entry
//! proves, with the same emitters.
use super::entry::{CompactLayout, HolderStorage, ReceiverLayout};
use std::cell::RefCell;

/// One shared proof emitter, with the parameters that select its proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProofStep {
    /// The R read entry's emission begins and ends.
    ReadEntry,
    ReadEntryEnd,
    /// A profiled P read site's guard begins and ends.
    ProfiledRead,
    ProfiledReadEnd,
    LayoutHeader(ReceiverLayout),
    CompactStorage(CompactLayout),
    RecordScan,
    RecordedShape,
    RecordedOwnSlot,
    RecordedClosedTail,
    ReceiverRule {
        kind: i32,
        shape: bool,
    },
    EmptyDictionary,
    PrototypePlace,
    RecordedPlace,
    HolderStorage(HolderStorage),
    RealmPrototypeHeader,
}

thread_local! {
    static TRACE: RefCell<Option<Vec<ProofStep>>> = const { RefCell::new(None) };
}

pub(super) fn record(step: ProofStep) {
    TRACE.with(|trace| {
        if let Some(steps) = trace.borrow_mut().as_mut() {
            steps.push(step);
        }
    });
}

/// Runs `emit` and returns what it returned with the proof steps it ran on
/// this thread.
pub(super) fn capture<R>(emit: impl FnOnce() -> R) -> (R, Vec<ProofStep>) {
    let previous = TRACE.with(|trace| trace.borrow_mut().replace(Vec::new()));
    let result = emit();
    let steps = TRACE.with(|trace| std::mem::replace(&mut *trace.borrow_mut(), previous));
    (result, steps.unwrap_or_default())
}
