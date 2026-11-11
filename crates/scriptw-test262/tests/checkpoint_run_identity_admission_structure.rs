const SOURCE: &str = include_str!("../src/lib.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn quoted_literal_end(source: &str, quote_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        match bytes[offset] {
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            b'"' => return Some(offset + 1),
            _ => {}
        }
        offset += 1;
    }
    None
}

fn character_literal_end(source: &str, quote_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    if bytes.get(offset) == Some(&b'\\') {
        offset += 2;
    } else {
        offset += source[offset..].chars().next()?.len_utf8();
    }
    (bytes.get(offset) == Some(&b'\'')).then_some(offset + 1)
}

fn raw_literal_end(source: &str, start: usize, prefix_len: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote = start + prefix_len;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let hashes = quote - start - prefix_len;
    let mut offset = quote + 1;
    while offset < bytes.len() {
        if bytes[offset] == b'"'
            && bytes
                .get(offset + 1..offset + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return Some(offset + 1 + hashes);
        }
        offset += 1;
    }
    None
}

fn literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    match bytes.get(start).copied()? {
        b'"' => quoted_literal_end(source, start),
        b'\'' => character_literal_end(source, start),
        b'b' if bytes.get(start + 1) == Some(&b'\'') => character_literal_end(source, start + 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'"') => quoted_literal_end(source, start + 1),
        b'r' => raw_literal_end(source, start, 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'r') => raw_literal_end(source, start, 2),
        _ => None,
    }
}

fn lexical_code(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes.get(offset..offset + 2) == Some(b"//") {
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"/*") {
            offset += 2;
            let mut depth = 1;
            while offset < bytes.len() && depth != 0 {
                if bytes.get(offset..offset + 2) == Some(b"/*") {
                    depth += 1;
                    offset += 2;
                } else if bytes.get(offset..offset + 2) == Some(b"*/") {
                    depth -= 1;
                    offset += 2;
                } else {
                    offset += 1;
                }
            }
            assert_eq!(depth, 0, "unterminated block comment in Rust source");
            continue;
        }
        if let Some(end) = literal_end(source, offset) {
            code.push('L');
            offset = end;
            continue;
        }
        let character = source[offset..].chars().next().unwrap();
        if !character.is_whitespace() {
            code.push(character);
        }
        offset += character.len_utf8();
    }
    code
}

#[test]
fn checkpoint_identity_is_opaque_and_deserializes_only_through_its_wire_parser() {
    let declaration = lexical_code(bounded(
        SOURCE,
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\npub struct CheckpointRunIdentity",
        "impl CheckpointRunIdentity",
    ));
    assert_eq!(
        declaration,
        "{terminal_run_kind:String,matrix_path:Vec<String>,}"
    );
    assert!(!declaration.contains("pubterminal_run_kind"));
    assert!(!declaration.contains("pubmatrix_path"));

    let wire_boundary = lexical_code(bounded(
        SOURCE,
        "#[derive(Deserialize)]\nstruct CheckpointRunIdentityWire",
        "#[derive(Debug, Clone, PartialEq, Eq)]\npub struct ProgressSnapshot",
    ));
    assert!(wire_boundary.contains(
        "{terminal_run_kind:String,matrix_path:Vec<String>,}impl<'de>Deserialize<'de>forCheckpointRunIdentity"
    ));
    assert!(wire_boundary.contains(
        "letwire=CheckpointRunIdentityWire::deserialize(deserializer)?;Self::parse(wire.terminal_run_kind,wire.matrix_path).map_err(serde::de::Error::custom)"
    ));
    assert!(!SOURCE.contains("Eq, Serialize, Deserialize)]\npub struct CheckpointRunIdentity"));
}

#[test]
fn checkpoint_identity_parser_admits_only_correlated_terminal_states() {
    let implementation = lexical_code(bounded(
        SOURCE,
        "impl CheckpointRunIdentity {",
        "#[derive(Deserialize)]\nstruct CheckpointRunIdentityWire",
    ));
    for required in [
        "fnparse(terminal_run_kind:String,matrix_path:Vec<String>)->Result<Self,String>",
        "Lifmatrix_path.is_empty()=>true",
        "L|Lif!matrix_path.is_empty()=>true",
        "_ifis_canonical_shard&&matrix_path.is_empty()=>true",
        "if!is_valid{returnErr(format!(L,terminal_run_kind,matrix_path.join(L)));}",
        "Ok(Self{terminal_run_kind,matrix_path,})",
        "fnshard(index:usize,count:usize)->Result<Self,String>{Self::parse(format!(L),Vec::new())}",
        "fnmatrix(node_kind:MatrixNodeKind,matrix_path:&[String])->Result<Self,String>",
    ] {
        assert!(implementation.contains(required), "missing `{required}`");
    }
    assert!(!implementation.contains("fnnew("));
    assert!(!SOURCE.contains("CheckpointRunIdentity::validate"));
    assert!(!SOURCE.contains("identity.validate()"));
}

#[test]
fn case_execution_and_resume_forward_one_admitted_identity() {
    let execution = lexical_code(bounded(
        SOURCE,
        "fn execute_cases(",
        "fn schedule_cases_for_lifo_queue(",
    ));
    assert!(execution.contains("checkpoint_run_identity:&CheckpointRunIdentity"));
    assert!(execution.contains(
        "ResumeCheckpointIdentity::for_resume(manifest,run_config,checkpoint_run_identity.clone(),)"
    ));
    assert_eq!(
        execution.matches("write_resume_case_checkpoint(").count(),
        2
    );
    assert_eq!(execution.matches("checkpoint_run_identity,").count(), 2);
    assert!(!execution.contains("terminal_run_kind:&str"));
    assert!(!execution.contains("terminal_matrix_path:&[String]"));

    let resume_identity = lexical_code(bounded(
        SOURCE,
        "struct ResumeCheckpointIdentity {",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\nenum DirectSnapshotKind",
    ));
    assert!(resume_identity.contains("checkpoint_run_identity:CheckpointRunIdentity"));
    assert!(!resume_identity.contains("expected_terminal_run_kind"));
    assert!(!resume_identity.contains("expected_matrix_path"));

    let comparison = lexical_code(bounded(
        SOURCE,
        "fn direct_snapshot_case_requirement(",
        "fn write_resume_case_checkpoint(",
    ));
    assert!(comparison.contains("expected_identity:&CheckpointRunIdentity"));
    assert!(comparison.contains("ifcheckpoint_identity!=expected_identity"));
    assert!(comparison.contains("expected_identity.terminal_run_kind()"));
    assert!(comparison.contains("expected_identity.matrix_path()"));
}
