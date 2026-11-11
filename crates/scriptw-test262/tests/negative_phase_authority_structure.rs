const SOURCE: &str = include_str!("../src/lib.rs");
fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[test]
fn negative_expectations_carry_the_closed_phase_authority() {
    let source = compact(SOURCE);

    assert!(source.contains("pubenumNegativePhase{Parse,Early,Resolution,Runtime,}"));
    assert!(source
        .contains("pubstructNegativeExpectation{pubphase:NegativePhase,puberror_type:String,}"));
    assert!(!source.contains("pubphase:String"));
    assert!(!source.contains("NegativePhase::of"));
}

#[test]
fn discovery_is_the_only_free_text_phase_boundary() {
    let source = compact(SOURCE);
    let metadata = compact(include_str!("../src/frontmatter.rs"));
    assert!(metadata.contains(
        "fnparse_negative(lines:Option<&[&str]>)->Result<Option<NegativeExpectation>,String>"
    ));
    assert!(metadata.contains("NegativePhase::parse(phase)?"));
    assert!(metadata.contains("fields.get(\"phase\").ok_or(\"negativeismissingphase\")?"));
    assert!(!metadata.contains("unwrap_or(NegativePhase::Runtime)"));
    assert!(source.contains("frontmatter::parse(&original_source,&path)?"));
    assert!(source.contains("letnegative=negative.map(Arc::new);"));
    assert!(source.contains("unknownnegative.phase`{value}`"));
}

#[test]
fn every_phase_decision_consumes_the_enum_exhaustively() {
    let source = compact(SOURCE);

    assert!(source.contains(
        "pubconstfnas_str(self)->&'staticstr{matchself{Self::Parse=>\"parse\",Self::Early=>\"early\",Self::Resolution=>\"resolution\",Self::Runtime=>\"runtime\",}}"
    ));
    assert!(source.contains(
        "constfnis_compile_only(self)->bool{matchself{Self::Parse|Self::Early|Self::Resolution=>true,Self::Runtime=>false,}}"
    ));
    for phase_owner in [
        "Self::Parse=>FailureKind::Parser",
        "Self::Early|Self::Resolution=>FailureKind::EarlyError",
        "Self::Runtime=>FailureKind::Runtime",
    ] {
        assert!(
            source.contains(phase_owner),
            "missing phase owner: {phase_owner}"
        );
    }
    assert!(source.contains(".is_some_and(|negative|negative.phase.is_compile_only())"));
    assert!(source.contains("letexpected=negative.phase;"));
    assert!(source.contains("negative.phase.failure_kind()"));
    assert!(source.contains("Some(negative.phase.as_str().to_string())"));
}
