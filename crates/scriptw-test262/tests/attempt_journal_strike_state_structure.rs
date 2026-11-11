const SOURCE: &str = include_str!("../src/attempt_journal.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn attempt_journal_runtime_state_cannot_represent_zero_strikes() {
    let strike_domain = normalized(bounded(
        SOURCE,
        "/// How many process deaths a case has been charged with.",
        "/// The number of process deaths a case is allowed before it is quarantined.",
    ));
    assert!(strike_domain.contains("#[serde(transparent)]"));
    assert!(strike_domain.contains("structCaseStrikes(NonZeroU32);"));
    assert!(strike_domain.contains("fnfrom_count(count:u32)->Option<Self>"));
    assert!(!strike_domain.contains("pub(crate)fnfrom_count"));

    let runtime_state = normalized(bounded(
        SOURCE,
        "struct AttemptJournalFile {",
        "#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]\nstruct InFlightAttemptWire",
    ));
    assert!(runtime_state.contains("strikes:BTreeMap<TestExecutionId,CaseStrikes>,"));
    assert!(!runtime_state.contains("strikes:BTreeMap<TestExecutionId,u32>,"));

    let wire_state = normalized(bounded(
        SOURCE,
        "struct StrikeEntries",
        "impl<'de> Deserialize<'de> for StrikeEntries",
    ));
    assert_eq!(wire_state, "(Vec<(String,u32)>);");
}

#[test]
fn strike_counts_are_admitted_once_and_consumed_as_typed_state() {
    let decoder = normalized(bounded(
        SOURCE,
        "fn decode_current_journal(",
        "/// Writes the journal atomically.",
    ));
    assert_eq!(decoder.matches("CaseStrikes::from_count(count)").count(), 1);
    assert!(decoder.contains("letcase_strikes=CaseStrikes::from_count(count).ok_or_else(||{"));
    assert!(decoder.contains(".insert(test_id.clone(),case_strikes)"));

    let runtime = bounded(
        SOURCE,
        "impl AttemptJournal {",
        "fn read_journal(path: &Path, identity: &AttemptJournalIdentity)",
    );
    assert!(!runtime.contains("CaseStrikes::from_count"));
    assert!(runtime.contains("let previous = state.strikes.get(&attempt.test_id).copied();"));
    assert!(runtime.contains("let strikes = state.strikes.get(&case.execution_id).copied();"));
}

#[test]
fn typed_strikes_keep_the_existing_numeric_json_shape() {
    let witness = bounded(
        SOURCE,
        "fn attempt_journal_persists_typed_strikes_as_numeric_counts()",
        "fn attempt_journal_identity_rejects_duplicate_selected_executions()",
    );
    assert!(witness.contains("charge_strikes_for_survivors()"));
    assert!(witness.contains("persisted[\"strikes\"][poison.execution_id.wire_key()]"));
    assert!(witness.contains("serde_json::json!(1)"));
}
