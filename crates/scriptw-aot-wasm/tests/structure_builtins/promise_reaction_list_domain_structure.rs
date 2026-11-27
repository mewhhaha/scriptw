const PROMISE_SOURCE: &str = include_str!("../../src/builtins/promise.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"))
        .0
}

#[test]
fn reaction_list_selector_uses_the_closed_reaction_domain() {
    let signature = bounded(
        PROMISE_SOURCE,
        "fn emit_append_pending_promise_reaction(",
        ") {",
    );
    assert!(signature.contains("promise: &GcLocal<PromiseObject>,"));
    assert!(signature.contains("reaction: &GcLocal<PendingJob>,"));
    assert!(!signature.contains("reaction_list_offset"));
    assert!(!signature.contains(": u64"));
    let helper = bounded(
        PROMISE_SOURCE,
        "fn emit_append_pending_promise_reaction(",
        "pub(crate) fn emit_read_promise_capability_promise(",
    );
    assert!(helper.contains("PromiseObjectSchema::REACTIONS_HEAD"));
    assert!(helper.contains("PromiseObjectSchema::REACTIONS_TAIL"));
    assert!(helper.contains("PendingJobSchema::NEXT"));
    let owned = bounded(
        PROMISE_SOURCE,
        "fn emit_owned_promise_record_reactions(",
        "pub(crate) fn emit_module_promise_reactions(",
    );
    let selector = bounded(owned, "let settlement = match selected {", "            };");
    for state in [
        "PromiseState::Pending => None,",
        "PromiseState::Fulfilled => Some(PromiseSettlement::Fulfill),",
        "PromiseState::Rejected => Some(PromiseSettlement::Reject),",
    ] {
        assert_eq!(selector.matches(state).count(), 1, "selector state `{state}`");
    }
    assert_eq!(selector.matches("=>").count(), 3);
    assert!(!selector.contains("_ =>"));
    assert!(!selector.contains("unreachable!"));
}
