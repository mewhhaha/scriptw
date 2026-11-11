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
    let start = source
        .find(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"));
    let end = source[start..]
        .find(end)
        .map(|offset| start + offset)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"));
    &source[start..end]
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn count_in_rust_sources(root: &Path, needle: &str) -> usize {
    let mut count = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("Rust source directory should read") {
            let path = entry.expect("Rust source entry should read").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("Rust source file should read");
            count += source.matches(needle).count();
        }
    }
    count
}

#[test]
fn aggregate_evidence_requirement_is_the_exact_private_no_capability_domain() {
    let source = source();
    let enum_start = source
        .find("enum AggregateEvidenceRequirement {")
        .expect("aggregate evidence requirement should exist");
    let preceding_enum_start = source
        .find("enum SnapshotUse {")
        .expect("snapshot use boundary should exist");
    let block_start = source[preceding_enum_start..]
        .find("\n}\n")
        .map(|offset| preceding_enum_start + offset + 3)
        .expect("snapshot use boundary should close");
    let block_end = source[enum_start..]
        .find("\n}\n")
        .map(|offset| enum_start + offset + 3)
        .expect("aggregate evidence requirement should close");

    assert_eq!(
        normalized(&source[block_start..block_end]),
        "enumAggregateEvidenceRequirement{Envelope,Complete,}"
    );
    assert!(!source.contains("pub enum AggregateEvidenceRequirement"));
    assert!(!source.contains("pub(crate) enum AggregateEvidenceRequirement"));
    assert!(!source.contains("impl AggregateEvidenceRequirement"));
    assert_eq!(
        count_in_rust_sources(&source_root(), "AggregateEvidenceRequirement"),
        7,
        "the declaration, resolver consumer and three producers are the complete ownership census"
    );
}

#[test]
fn candidate_resolution_exhaustively_binds_both_evidence_policies() {
    let source = source();
    let resolver = bounded(
        &source,
        "fn resolve_aggregate_snapshot(",
        "fn load_completed_node_snapshot(",
    );
    let resolver = normalized(resolver);

    assert!(resolver.contains(
        "letevidence=envelope.and_then(|()|match&evidence_requirement{AggregateEvidenceRequirement::Envelope=>Ok(()),AggregateEvidenceRequirement::Complete=>{load_and_validate_resolved_aggregate_evidence(config,&candidate,nodes,execution_backend,).map_err(String::from)?;Ok(())}});"
    ));
    assert_eq!(resolver.matches("match&evidence_requirement").count(), 1);
    assert!(!resolver.contains("evidence_requirement=="));
    assert!(!resolver.contains("matches!(evidence_requirement"));
}

#[test]
fn all_three_aggregate_consumers_choose_an_exact_evidence_policy() {
    let source = source();
    for (start, end, snapshot_use, requirement) in [
        (
            "fn load_verified_aggregate_summary_for_use(",
            "fn load_aggregate_progress_summary(",
            "snapshot_use",
            "Complete",
        ),
        (
            "\nfn load_aggregate_progress_summary(",
            "\nfn load_matrix_triage_entries(",
            "SnapshotUse::CurrentState",
            "Envelope",
        ),
        (
            "\nfn load_matrix_failure_details(",
            "\nfn group_failures_by_detail_identity(",
            "SnapshotUse::CurrentState",
            "Envelope",
        ),
    ] {
        let consumer = normalized(bounded(&source, start, end));
        let expected_call = format!(
            "resolve_aggregate_snapshot(config,snapshot_name,manifest_hash,execution_backend,&expected_pinned,{snapshot_use},&nodes,AggregateEvidenceRequirement::{requirement},)?"
        );
        let expected_call = if start.contains("failure_details") {
            expected_call.replace("manifest_hash", "aggregate_manifest_hash")
        } else {
            expected_call
        };

        assert_eq!(
            consumer.matches("resolve_aggregate_snapshot(").count(),
            1,
            "consumer `{start}` must have one resolver call"
        );
        assert!(
            consumer.contains(&expected_call),
            "consumer `{start}` must choose `{requirement}` in its exact resolver call"
        );
        assert_eq!(
            consumer.matches("AggregateEvidenceRequirement::").count(),
            1
        );
    }
}
