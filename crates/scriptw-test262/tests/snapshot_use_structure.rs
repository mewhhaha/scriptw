use std::fs;
use std::path::{Path, PathBuf};

fn source() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("scriptw-test262 source should read")
}

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

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

fn count_in_rust_sources(root: &Path, needle: &str) -> usize {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_in_rust_sources(&path, needle);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
                .matches(needle)
                .count()
        })
        .sum()
}

#[test]
fn snapshot_use_is_the_exact_private_no_capability_domain() {
    let source = source();
    let declaration_marker = "enum SnapshotUse {";
    let declaration_offset = source
        .find(declaration_marker)
        .expect("snapshot-use declaration should exist");
    let preceding_item_tail = concat!(
        "    const fn is_current(self) -> bool {\n",
        "        matches!(self, Self::CurrentScriptwV8)\n",
        "    }\n",
        "}\n\n",
    );
    assert_eq!(
        source.matches(preceding_item_tail).count(),
        1,
        "SnapshotArtifactKind must have one exact implementation tail"
    );
    let preceding_item_end = source[..declaration_offset]
        .rfind(preceding_item_tail)
        .map(|offset| offset + preceding_item_tail.len())
        .expect("SnapshotArtifactKind implementation must precede SnapshotUse");
    assert_eq!(
        preceding_item_end, declaration_offset,
        "SnapshotUse must immediately follow the exact SnapshotArtifactKind item"
    );
    let declaration_end = source[declaration_offset..]
        .find("\n}\n")
        .map(|offset| declaration_offset + offset + 2)
        .expect("snapshot-use declaration should close");
    assert_eq!(
        normalized(&source[preceding_item_end..declaration_end]),
        "enumSnapshotUse{CurrentState,ReadOnlyEvidence,}",
        "the exact adjacent declaration must remain private and attribute-free"
    );

    assert_eq!(
        count_in_rust_sources(&source_root(), "SnapshotUse"),
        17,
        "one declaration, three parameters, eight production choices, three recovery controls and two exhaustive arms own every source mention"
    );
    assert_eq!(
        count_in_rust_sources(&source_root(), "SnapshotUse::CurrentState"),
        11
    );
    assert_eq!(
        count_in_rust_sources(&source_root(), "SnapshotUse::ReadOnlyEvidence"),
        2
    );
    for forbidden in [
        "pub enum SnapshotUse",
        "pub(crate) enum SnapshotUse",
        "impl SnapshotUse",
        "for SnapshotUse",
        "snapshot_use ==",
        "snapshot_use !=",
        "matches!(snapshot_use",
    ] {
        assert!(!source.contains(forbidden), "found `{forbidden}`");
    }
}

#[test]
fn aggregate_validation_exhaustively_projects_both_snapshot_uses() {
    let source = source();
    let validator = normalized(bounded(
        &source,
        "fn validate_resume_aggregate_snapshot(",
        "fn load_previous_snapshot(",
    ));
    let expected_projection = concat!(
        "matchsnapshot_use{",
        "SnapshotUse::CurrentState=>{file.require_running(path,\"currentaggregatesnapshot\")?;}",
        "SnapshotUse::ReadOnlyEvidence=>{file.require_current(path,\"read-onlycompiler-boundaggregate\")?;}",
        "}",
        "iffile.matrix_strategy_version!=MATRIX_STRATEGY_VERSION{"
    );
    assert_eq!(validator.matches(expected_projection).count(), 1);
    assert_eq!(validator.matches("matchsnapshot_use{").count(), 1);
    assert!(!validator.contains("_=>"));

    let resolver = normalized(bounded(
        &source,
        "fn resolve_aggregate_snapshot(",
        "fn load_completed_node_snapshot(",
    ));
    for call in [
        "validate_resume_aggregate_snapshot(config,&file,&exact_paths.json_path,manifest_hash,execution_backend,expected_pinned,&snapshot_use,)?",
        "validate_resume_aggregate_snapshot(config,&file,&candidate_paths.json_path,manifest_hash,execution_backend,expected_pinned,&snapshot_use,)",
    ] {
        assert_eq!(
            resolver.matches(call).count(),
            1,
            "aggregate resolution must forward the same borrowed use to `{call}`"
        );
    }
    assert_eq!(
        resolver
            .matches("validate_resume_aggregate_snapshot(")
            .count(),
        2
    );
}

#[test]
fn all_eight_product_producers_select_the_exact_snapshot_use() {
    let source = source();
    let resume_loader = normalized(bounded(
        &source,
        "fn load_resume_aggregate_snapshot(",
        "fn validate_resume_aggregate_snapshot(",
    ));
    for call in [
        "validate_resume_aggregate_snapshot(config,&file,&exact_path,expected_manifest_hash,expected_backend,expected_pinned,&SnapshotUse::CurrentState,).is_ok()",
        "validate_resume_aggregate_snapshot(config,&file,&path,expected_manifest_hash,expected_backend,expected_pinned,&SnapshotUse::CurrentState,).is_ok()",
    ] {
        assert_eq!(
            resume_loader.matches(call).count(),
            1,
            "resume loading must bind CurrentState in the exact call `{call}`"
        );
    }
    assert_eq!(
        resume_loader
            .matches("validate_resume_aggregate_snapshot(")
            .count(),
        2
    );
    assert!(!resume_loader.contains("SnapshotUse::ReadOnlyEvidence"));

    for (start, end, expected_call) in [
        (
            "\nfn load_verified_aggregate_summary(",
            "\nfn load_publishable_aggregate_summary(",
            "load_verified_aggregate_summary_for_use(config,snapshot_name,execution_backend,SnapshotUse::CurrentState,)",
        ),
        (
            "\nfn load_publishable_aggregate_summary(",
            "\nfn load_current_aggregate_summary(",
            "load_verified_aggregate_summary_for_use(config,snapshot_name,publication_backend.as_execution_backend(),SnapshotUse::CurrentState,)",
        ),
        (
            "\nfn load_current_aggregate_summary(",
            "\nfn load_verified_aggregate_summary_for_use(",
            "load_verified_aggregate_summary_for_use(config,snapshot_name,execution_backend,SnapshotUse::CurrentState,)",
        ),
    ] {
        let producer = normalized(bounded(&source, start, end));
        assert_eq!(producer.matches(expected_call).count(), 1);
        assert_eq!(producer.matches("SnapshotUse::CurrentState").count(), 1);
        assert!(!producer.contains("SnapshotUse::ReadOnlyEvidence"));
    }

    let verified_forwarder = normalized(bounded(
        &source,
        "\nfn load_verified_aggregate_summary_for_use(",
        "\nfn load_aggregate_progress_summary(",
    ));
    assert_eq!(
        verified_forwarder
            .matches("&expected_pinned,snapshot_use,&nodes,AggregateEvidenceRequirement::Complete,")
            .count(),
        1
    );

    for (start, end, use_name, requirement) in [
        (
            "\nfn load_aggregate_progress_summary(",
            "\nfn load_matrix_triage_entries(",
            "CurrentState",
            "Envelope",
        ),
        (
            "\nfn load_matrix_failure_details(",
            "\nfn group_failures_by_detail_identity(",
            "CurrentState",
            "Envelope",
        ),
    ] {
        let producer = normalized(bounded(&source, start, end));
        let expected =
            format!("SnapshotUse::{use_name},&nodes,AggregateEvidenceRequirement::{requirement},");
        assert_eq!(producer.matches(&expected).count(), 1);
        assert_eq!(producer.matches("SnapshotUse::").count(), 1);
    }

    let comparison = normalized(bounded(
        &source,
        "\nfn load_comparison_aggregate_summary(",
        "\nfn compare_snapshots(",
    ));
    assert_eq!(
        comparison.matches("SnapshotUse::ReadOnlyEvidence").count(),
        1
    );
    assert!(!comparison.contains("SnapshotUse::CurrentState"));
}
