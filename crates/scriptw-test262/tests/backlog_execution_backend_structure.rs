use std::fs;
use std::path::PathBuf;

fn source() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate should live under crates/")
        .to_path_buf();
    fs::read_to_string(root.join("crates/scriptw-test262/src/lib.rs"))
        .expect("scriptw-test262 source should read")
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

#[test]
fn backlog_artifact_stores_the_closed_execution_backend() {
    let source = source();
    let body = bounded(
        &source,
        "pub struct BacklogArtifact {",
        "fn serialize_backlog_execution_backend",
    );
    let body = normalized(body);

    assert!(body.contains(concat!(
        "#[serde(",
        "serialize_with=\"serialize_backlog_execution_backend\",",
        "deserialize_with=\"deserialize_backlog_execution_backend\"",
        ")]",
        "pubexecution_backend:ExecutionBackend,"
    )));
    assert!(!body.contains("pubexecution_backend:String"));
}

#[test]
fn backlog_backend_wire_codec_is_exhaustive_and_byte_stable() {
    let source = source();
    let serializer = normalized(bounded(
        &source,
        "fn serialize_backlog_execution_backend<S>(",
        "fn deserialize_backlog_execution_backend<'de, D>(",
    ));
    assert!(serializer.contains(concat!(
        "matchexecution_backend{",
        "ExecutionBackend::SpecExec=>\"spec-exec\",",
        "ExecutionBackend::WasmAot=>\"wasm-aot\",",
        "}"
    )));
    assert!(!serializer.contains("_=>"));

    let deserializer = normalized(bounded(
        &source,
        "fn deserialize_backlog_execution_backend<'de, D>(",
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub struct BacklogRecord",
    ));
    for row in [
        "\"spec-exec\"=>Ok(ExecutionBackend::SpecExec)",
        "\"wasm-aot\"=>Ok(ExecutionBackend::WasmAot)",
    ] {
        assert!(deserializer.contains(row), "missing codec row `{row}`");
    }
    assert!(deserializer.contains("backlogcontainsunknownexecution_backend`{label}`"));
}

#[test]
fn backlog_backend_is_owned_once_and_projected_by_name() {
    let source = source();
    let generation = normalized(bounded(
        &source,
        "let artifact = BacklogArtifact {",
        "let paths = write_backlog_artifact(config, &artifact)?;",
    ));
    assert!(generation.contains("execution_backend,"));
    assert!(!generation.contains("execution_backend.as_str().to_string()"));

    let output = bounded(
        &source,
        "fn write_backlog_artifact(",
        "fn test262_root_from_config(",
    );
    assert_eq!(
        output
            .matches("artifact.execution_backend.as_str()")
            .count(),
        3
    );
    assert!(!output.contains("artifact.execution_backend,"));
}
