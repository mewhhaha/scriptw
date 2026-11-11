use std::fs;
use std::path::{Path, PathBuf};

const OWNER_SOURCE: &str = include_str!("../src/lib.rs");
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

struct NormalizedRust {
    code: String,
    identifiers: String,
    routes: String,
}

fn normalize_rust(source: &str) -> NormalizedRust {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut identifiers = String::new();
    let mut routes = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            code.push_str(&source[offset..end]);
            identifiers.push(' ');
            routes.push('L');
            offset = end;
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"//") {
            identifiers.push(' ');
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"/*") {
            identifiers.push(' ');
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
            identifiers.push(character);
            routes.push(character);
        } else {
            identifiers.push(' ');
        }
        offset += character.len_utf8();
    }
    NormalizedRust {
        code,
        identifiers,
        routes,
    }
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

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn count_identifier_in_rust_sources(root: &Path, identifier: &str) -> usize {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
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
            exact_identifier_count(&normalize_rust(&source).identifiers, identifier)
        })
        .sum()
}

fn production_source() -> &'static str {
    OWNER_SOURCE
        .split_once("#[cfg(test)]\nmod tests {")
        .expect("scriptw-test262 test module boundary")
        .0
}

#[test]
fn case_set_requirement_is_one_private_capability_free_authority() {
    let lexical_probe = r###"
        // CaseSetRequirement::clone
        CaseSetRequirement /* nested /* ignored */ comment */ :: r#Exact;
        "CaseSetRequirement"; b"CaseSetRequirement"; c"CaseSetRequirement";
        r"CaseSetRequirement"; br##"CaseSetRequirement"##; cr#"CaseSetRequirement"#;
        'C'; b'\x43'; 'lifetime;
    "###;
    let lexical_probe = normalize_rust(lexical_probe);
    assert_eq!(
        exact_identifier_count(&lexical_probe.identifiers, "CaseSetRequirement"),
        1
    );
    assert_eq!(
        exact_identifier_count(&lexical_probe.routes, "CaseSetRequirement::Exact"),
        1
    );

    let preceding_item_tail = concat!(
        "    validate_summary_count_contract(\n",
        "        &context,\n",
        "        rebuilt.total,\n",
        "        rebuilt.passed,\n",
        "        rebuilt.failed,\n",
        "        &rebuilt.counts_per_kind,\n",
        "        &rebuilt.counts_per_outcome,\n",
        "        &rebuilt.counts_per_origin,\n",
        "    )?;\n",
        "    match incomplete {\n",
        "        Some(detail) => Err(AggregateLoadError::Incomplete(detail)),\n",
        "        None => Ok(()),\n",
        "    }\n",
        "}\n\n",
    );
    let declaration_start = OWNER_SOURCE
        .find(preceding_item_tail)
        .map(|offset| offset + preceding_item_tail.len())
        .expect("aggregate snapshot validator must precede the authority");
    let declaration_end = OWNER_SOURCE[declaration_start..]
        .find("#[derive(Debug)]\nstruct SnapshotCaseCounts {")
        .map(|offset| declaration_start + offset)
        .expect("snapshot case counts must follow the authority");
    assert_eq!(
        normalize_rust(&OWNER_SOURCE[declaration_start..declaration_end]).code,
        "enumCaseSetRequirement{UniqueSubset,Exact,}"
    );

    assert_eq!(
        count_identifier_in_rust_sources(&source_root(), "CaseSetRequirement"),
        19,
        "thirteen production mentions and six corruption/control mentions own the authority"
    );
    assert_eq!(
        exact_identifier_count(
            &normalize_rust(production_source()).identifiers,
            "CaseSetRequirement"
        ),
        13,
        "the former equality observation was the fourteenth production mention"
    );

    let routes = normalize_rust(OWNER_SOURCE).routes;
    for forbidden in [
        "implCaseSetRequirement",
        "forCaseSetRequirement",
        "CaseSetRequirement::clone",
        "CaseSetRequirement::eq",
        "CaseSetRequirement::default",
        "requirement==",
        "requirement!=",
        "matches!(requirement",
        "matchrequirement{_=>",
        "asCaseSetRequirement",
    ] {
        assert!(
            !routes.contains(forbidden),
            "found forbidden route `{forbidden}`"
        );
    }
}

#[test]
fn validator_consumes_the_authority_once_into_both_policy_outputs() {
    let validator = normalize_rust(bounded(
        OWNER_SOURCE,
        "fn validate_case_snapshot_contract(",
        "fn validate_complete_node_contract(",
    ));
    let expected = normalize_rust(
        r#"
        let completed_ids = snapshot.completed_test_ids.iter().collect::<BTreeSet<_>>();
        let (requires_exact_set, case_set_description) = match requirement {
            CaseSetRequirement::UniqueSubset => (false, "subset"),
            CaseSetRequirement::Exact => (true, "exact copy"),
        };
        if snapshot.total != snapshot.completed_test_ids.len()
            || completed_ids.len() != snapshot.completed_test_ids.len()
            || !completed_ids.is_subset(&allowed_ids)
            || (requires_exact_set && completed_ids != allowed_ids)
        {
            return Err(format!(
                "{context}: completed_test_ids must be a unique {} of the selected execution set",
                case_set_description
            ));
        }

        let mut failure_ids = BTreeSet::new();
        "#,
    );
    let actual = normalize_rust(&format!(
        "let completed_ids{}let mut failure_ids = BTreeSet::new();",
        bounded(
            &validator.code,
            "letcompleted_ids",
            "letmutfailure_ids=BTreeSet::new();"
        )
    ));
    assert_eq!(actual.code, expected.code);
    assert_eq!(validator.routes.matches("matchrequirement{").count(), 1);
    assert_eq!(validator.routes.matches("CaseSetRequirement::").count(), 2);
    assert_eq!(
        exact_identifier_count(&validator.identifiers, "requirement"),
        2,
        "the typed parameter and consuming match are the only observations"
    );
    assert!(!validator.routes.contains("_=>"));
}

#[test]
fn all_six_deliveries_and_both_constructor_rows_are_exact() {
    let production = normalize_rust(production_source());
    assert_eq!(
        exact_identifier_count(&production.routes, "CaseSetRequirement::Exact"),
        6,
        "five Exact constructors plus its exhaustive projection arm"
    );
    assert_eq!(
        exact_identifier_count(&production.routes, "CaseSetRequirement::UniqueSubset"),
        3,
        "two UniqueSubset constructors plus its exhaustive projection arm"
    );

    let direct = normalize_rust(bounded(
        OWNER_SOURCE,
        "fn direct_snapshot_case_requirement(",
        "fn write_resume_case_checkpoint(",
    ));
    assert_eq!(direct.routes.matches("CaseSetRequirement::").count(), 2);
    let subset = direct
        .routes
        .find("returnOk(CaseSetRequirement::UniqueSubset);")
        .expect("checkpoint identity must produce UniqueSubset");
    let exact = direct
        .routes
        .find("Ok(CaseSetRequirement::Exact)")
        .expect("terminal identity must produce Exact");
    assert!(
        subset < exact,
        "checkpoint mapping must precede terminal mapping"
    );

    for (start, end, call) in [
        (
            "\nfn run_shard(",
            "fn manifest_for_selected_cases(",
            r#"
            validate_case_snapshot_contract(
                &snapshot,
                &manifest
                    .cases
                    .iter()
                    .map(|case| case.execution_id.clone())
                    .collect::<Vec<_>>(),
                CaseSetRequirement::Exact,
                "completed shard snapshot",
            )?;
            "#,
        ),
        (
            "\nfn run_full_with_manifest(",
            "\nfn run_top_level_matrix(",
            r#"
            validate_case_snapshot_contract(
                &snapshot,
                &manifest
                    .cases
                    .iter()
                    .map(|case| case.execution_id.clone())
                    .collect::<Vec<_>>(),
                CaseSetRequirement::Exact,
                "completed full-run snapshot",
            )?;
            "#,
        ),
        (
            "fn write_resume_case_checkpoint(",
            "fn snapshot_paths_for_name(",
            r#"
            validate_case_snapshot_contract(
                &snapshot,
                &manifest
                    .cases
                    .iter()
                    .map(|case| case.execution_id.clone())
                    .collect::<Vec<_>>(),
                CaseSetRequirement::UniqueSubset,
                "resume checkpoint being written",
            )?;
            "#,
        ),
        (
            "fn result_from_single_case_snapshot(",
            "fn create_child_snapshot_directory(",
            r#"
            validate_case_snapshot_contract(
                &snapshot,
                std::slice::from_ref(&case.execution_id),
                CaseSetRequirement::Exact,
                &format!("single-case child snapshot for {}", case.execution_id),
            )?;
            "#,
        ),
        (
            "fn validate_complete_node_contract(",
            "fn validate_complete_aggregate_evidence(",
            r#"
            let counts = validate_case_snapshot_contract(
                snapshot,
                &node.case_ids,
                CaseSetRequirement::Exact,
                &context,
            )?;
            "#,
        ),
    ] {
        let owner = normalize_rust(bounded(OWNER_SOURCE, start, end));
        assert_eq!(
            owner
                .routes
                .matches("validate_case_snapshot_contract(")
                .count(),
            1
        );
        let call = normalize_rust(call);
        assert_eq!(
            owner.code.matches(&call.code).count(),
            1,
            "delivery `{start}`"
        );
    }

    let resume = normalize_rust(bounded(
        OWNER_SOURCE,
        "fn load_resume_matrix_node_summary_for_node(",
        "fn validate_resume_node_snapshot(",
    ));
    let forwarded = normalize_rust(
        r#"
        let case_requirement = validate_resume_node_snapshot(
            config,
            &file,
            &path,
            node,
            manifest_hash,
            expected_backend,
            expected_pinned,
            &CompilerProvenance::current().map_err(ResumeCheckpointLoadError::integrity)?,
        )?;
        let snapshot = snapshot_from_file(file).map_err(ResumeCheckpointLoadError::integrity)?;
        validate_case_snapshot_contract(
            &snapshot,
            &node.case_ids,
            case_requirement,
            &format!(
                "matrix node checkpoint integrity failure in {}",
                path.display()
            ),
        )
        .map_err(ResumeCheckpointLoadError::integrity)?;
        "#,
    );
    assert_eq!(resume.code.matches(&forwarded.code).count(), 1);
    assert_eq!(resume.routes.matches("case_requirement").count(), 2);
}
