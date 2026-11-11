use std::fs;
use std::path::Path;

const JOURNAL_SOURCE: &str = include_str!("../src/attempt_journal.rs");
const RUNNER_SOURCE: &str = include_str!("../src/lib.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn quoted_literal_end(source: &str, quote_start: usize, quote: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            return Some(offset + 1);
        }
        offset += 1;
    }
    None
}

fn character_literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let value_start = start + 1;
    if value_start >= bytes.len() {
        return None;
    }
    let value_end = if bytes[value_start] == b'\\' {
        let mut offset = value_start + 1;
        if offset >= bytes.len() {
            return None;
        }
        if bytes[offset] == b'u' && bytes.get(offset + 1) == Some(&b'{') {
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'}') {
                offset += 1;
            }
            if bytes.get(offset) != Some(&b'}') {
                return None;
            }
            offset + 1
        } else if bytes[offset] == b'x'
            && bytes
                .get(offset + 1..offset + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        {
            offset + 3
        } else {
            offset + 1
        }
    } else {
        value_start + source[value_start..].chars().next()?.len_utf8()
    };
    (bytes.get(value_end) == Some(&b'\'')).then_some(value_end + 1)
}

fn raw_literal_end(source: &str, start: usize, prefix_len: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote_start = start + prefix_len;
    while bytes.get(quote_start) == Some(&b'#') {
        quote_start += 1;
    }
    if bytes.get(quote_start) != Some(&b'"') {
        return None;
    }
    let hashes = quote_start - start - prefix_len;
    let mut offset = quote_start + 1;
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
        b'"' => quoted_literal_end(source, start, b'"'),
        b'\'' => character_literal_end(source, start),
        b'b' if bytes.get(start + 1) == Some(&b'\'') => character_literal_end(source, start + 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'"') => {
            quoted_literal_end(source, start + 1, b'"')
        }
        b'r' => raw_literal_end(source, start, 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'r') => raw_literal_end(source, start, 2),
        _ => None,
    }
}

fn rust_code(source: &str, retain_literals: bool) -> String {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            if retain_literals {
                code.push_str(&source[offset..end]);
            } else {
                code.push(' ');
            }
            offset = end;
            continue;
        }
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
        if bytes.get(offset..offset + 2) == Some(b"r#")
            && source[offset + 2..]
                .chars()
                .next()
                .is_some_and(|character| character == '_' || character.is_alphabetic())
        {
            offset += 2;
            continue;
        }
        let character = source[offset..].chars().next().unwrap();
        if !character.is_whitespace() {
            code.push(character);
        } else if !retain_literals {
            code.push(' ');
        }
        offset += character.len_utf8();
    }
    code
}

fn exact_identifier_count(source: &str, identifier: &str) -> usize {
    source
        .match_indices(identifier)
        .filter(|(offset, _)| {
            let before = source[..*offset].chars().next_back();
            let after = source[*offset + identifier.len()..].chars().next();
            [before, after].into_iter().all(|edge| {
                edge.map(|character| !character.is_alphanumeric() && character != '_')
                    .unwrap_or(true)
            })
        })
        .count()
}

fn count_identifier_in_rust_sources(dir: &Path, identifier: &str) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_identifier_in_rust_sources(&path, identifier);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            exact_identifier_count(&rust_code(&source, false), identifier)
        })
        .sum()
}

fn positions_in_order(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing marker after byte {cursor}: `{marker}`"));
        cursor += offset + marker.len();
    }
}

#[test]
fn attempt_authorities_are_exact_debug_only_non_cloneable_types() {
    let lexical_probe = rust_code(
        r###"
        // AdmittedCase QueuedCase CaseAdmission RunPhase
        AdmittedCase /* nested /* ignored */ comment */ :: r#admitted_by_test;
        "QueuedCase"; b"CaseAdmission"; c"RunPhase";
        r"AdmittedCase"; br##"QueuedCase"##; cr#"CaseAdmission"#;
        'A'; b'A'; 'lifetime;
        "###,
        false,
    );
    assert_eq!(exact_identifier_count(&lexical_probe, "AdmittedCase"), 1);
    for ignored in ["QueuedCase", "CaseAdmission", "RunPhase"] {
        assert_eq!(exact_identifier_count(&lexical_probe, ignored), 0);
    }

    for (start, end, expected) in [
        (
            "#[derive(Debug)]\npub(crate) struct QueuedCase(TestCase);",
            "/// A case that has been journalled and cleared to run.",
            "#[derive(Debug)]pub(crate)structQueuedCase(TestCase);",
        ),
        (
            "#[derive(Debug)]\npub(crate) struct AdmittedCase(TestCase);",
            "impl AdmittedCase",
            "#[derive(Debug)]pub(crate)structAdmittedCase(TestCase);",
        ),
        (
            "#[derive(Debug)]\npub(crate) enum CaseAdmission {",
            "/// A case that was in flight when a previous process died",
            concat!(
                "#[derive(Debug)]pub(crate)enumCaseAdmission{Run(AdmittedCase),",
                "Quarantined{test_id:TestExecutionId,strikes:CaseStrikes,},}"
            ),
        ),
        (
            "#[derive(Debug)]\npub(crate) struct RunPhase {",
            "impl RunPhase",
            "#[derive(Debug)]pub(crate)structRunPhase{worker_count:NonZeroUsize,cases:Vec<TestCase>,}",
        ),
    ] {
        let declaration = format!("{start}{}", bounded(JOURNAL_SOURCE, start, end));
        assert_eq!(rust_code(&declaration, true), expected, "declaration `{start}`");
    }

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for (identifier, expected) in [
        ("QueuedCase", 14),
        ("AdmittedCase", 9),
        ("CaseAdmission", 15),
        ("RunPhase", 6),
    ] {
        assert_eq!(
            count_identifier_in_rust_sources(&source_root, identifier),
            expected,
            "identifier `{identifier}`"
        );
    }
}

#[test]
fn queue_admission_and_execution_each_consume_the_previous_authority() {
    let queue_projection = rust_code(
        bounded(
            JOURNAL_SOURCE,
            "pub(crate) fn into_queue(self) -> Vec<QueuedCase> {",
            "    }\n\n    #[cfg(test)]",
        ),
        true,
    );
    assert_eq!(
        queue_projection,
        "self.cases.into_iter().map(QueuedCase).collect()"
    );

    let admit = rust_code(
        bounded(
            JOURNAL_SOURCE,
            "pub(crate) fn admit(",
            "/// Retires a worker slot's entry",
        ),
        true,
    );
    assert_eq!(admit.matches("queued:QueuedCase").count(), 1);
    assert_eq!(admit.matches("letQueuedCase(case)=queued;").count(), 1);
    assert_eq!(
        admit
            .matches("CaseAdmission::Run(AdmittedCase(case))")
            .count(),
        1
    );
    assert_eq!(admit.matches("CaseAdmission::Quarantined{").count(), 1);
    for forbidden in ["queued.clone()", "case.clone()).clone()", "_=>"] {
        assert!(!admit.contains(forbidden), "found `{forbidden}`");
    }

    let runner = rust_code(
        bounded(
            RUNNER_SOURCE,
            "fn run_case_entry_with_dispatch(",
            "fn run_one_case(",
        ),
        true,
    );
    assert!(runner.starts_with(
        "dispatch:&CaseExecutionDispatch,session:&mutcase_worker_server::Worker,config:&SuiteConfig,preludes:&PreludeStore,admitted:AdmittedCase,run_config:&RunConfig,)->TestResult{letcase=admitted.case();"
    ));
    assert!(!runner.contains("admitted:&AdmittedCase"));
    let fixture = rust_code(
        bounded(
            RUNNER_SOURCE,
            "fn run_case_entry(",
            "fn run_case_entry_with_dispatch(",
        ),
        true,
    );
    assert!(fixture.contains("admitted:AdmittedCase,"));
    assert!(fixture
        .contains("CaseExecutionDispatch::admit(config,std::slice::from_ref(admitted.case()))"));
    assert!(fixture.contains(
        "run_case_entry_with_dispatch(&dispatch,&mutcase_worker_server::Worker::default(),config,preludes,admitted,run_config,)"
    ));
    assert!(!fixture.contains("admitted.clone()"));
}

#[test]
fn worker_moves_the_admitted_proof_once_then_retires_its_journal_slot() {
    let worker = rust_code(
        bounded(
            RUNNER_SOURCE,
            "let result = match admission {",
            "let checkpoint_snapshot = {",
        ),
        true,
    );
    assert_eq!(worker.matches("CaseAdmission::Run(admitted)=>{").count(), 1);
    assert_eq!(worker.matches("CaseAdmission::Quarantined{").count(), 1);
    assert_eq!(worker.matches("run_case_entry_with_dispatch(").count(), 1);
    assert!(!worker.contains("&admitted"));
    assert!(!worker.contains("_=>"));
    positions_in_order(
        &worker,
        &[
            "letadmitted_path=admitted.case().path.clone();",
            "run_case_entry_with_dispatch(&dispatch,&mutsession,&worker_config,&preludes,admitted,&worker_run_config,)",
            "journal.retire(worker_slot)",
            "admitted_path",
        ],
    );

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_identifier_in_rust_sources(&source_root, "run_case_entry_with_dispatch"),
        3
    );
    assert_eq!(
        count_identifier_in_rust_sources(&source_root, "run_case_entry"),
        3
    );
}
