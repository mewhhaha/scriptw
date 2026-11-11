const MATCHER: &str = concat!(
    include_str!("../../src/builtins/regexp.rs"),
    include_str!("../../src/builtins/regexp/program.rs"),
    include_str!("../../src/builtins/regexp/backreference.rs"),
    include_str!("../../src/builtins/regexp/range_search.rs"),
    include_str!("../../src/builtins/regexp/word_boundary.rs"),
    include_str!("../../src/builtins/regexp/matcher_workspace.rs"),
    include_str!("../../src/builtins/regexp/transient.rs"),
);
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

fn lexically_normalized(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut normalized = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
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
            assert_eq!(depth, 0, "unterminated block comment in RegExp matcher");
            continue;
        }
        let character = source[offset..]
            .chars()
            .next()
            .expect("valid UTF-8 character boundary");
        if !character.is_whitespace() {
            normalized.push(character);
        }
        offset += character.len_utf8();
    }
    normalized
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

#[test]
fn matcher_result_is_the_exact_private_no_capability_domain() {
    let matcher = lexically_normalized(MATCHER);
    let declaration = bounded(
        &matcher,
        "enumRegExpMatcherResult{",
        "impl<'a>FunctionBuilder<'a>{",
    );
    assert_eq!(declaration, "Match,NoMatch,Failed(RegExpMatcherFailure),}");
    assert!(!matcher.contains("RegExpChoiceFrameKind"));
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq", "Default"] {
        assert!(!matcher.contains(&format!("impl{capability}forRegExpMatcherResult")));
    }
    assert!(!matcher.contains("pubenumRegExpMatcherResult"));
    assert!(!matcher.contains("pub(crate)enumRegExpMatcherResult"));
    assert!(!matcher.contains("pub(super)enumRegExpMatcherResult"));
    assert_eq!(matcher.matches("RegExpMatcherResult").count(), 51);
}

#[test]
fn all_result_producers_name_one_legal_result_state() {
    let matcher = lexically_normalized(MATCHER);
    assert_eq!(matcher.matches(".emit_regexp_match_result(").count(), 46);
    assert_eq!(matcher.matches("RegExpMatcherResult::Match,").count(), 1);
    assert_eq!(matcher.matches("RegExpMatcherResult::NoMatch,").count(), 3);
    assert_eq!(
        matcher
            .matches("RegExpMatcherResult::Failed(RegExpMatcherFailure::CorruptProgram),")
            .count(),
        40
    );
    assert_eq!(
        matcher
            .matches("RegExpMatcherResult::Failed(RegExpMatcherFailure::ResourceExhausted),")
            .count(),
        1
    );
    assert_eq!(
        matcher
            .matches("self.checkpoint,self.start,self.start,RegExpMatcherResult::Failed(failure),")
            .count(),
        1
    );
    assert_eq!(
        matcher
            .matches("candidate_utf16,match_utf16,RegExpMatcherResult::Match,")
            .count(),
        1
    );
    assert_eq!(
        matcher
            .matches("candidate_utf16,candidate_utf16,RegExpMatcherResult::NoMatch,")
            .count(),
        3
    );
}

#[test]
fn sole_writer_consumes_and_exhaustively_projects_the_result() {
    let matcher = lexically_normalized(MATCHER);
    let writer = bounded(
        &matcher,
        "fnemit_regexp_match_result(",
        "function.instruction(&Instruction::I32Const(status.abi_word()asi32));}",
    );
    assert!(writer.starts_with(
        "&self,checkpoint:I64Local,start_local:I64Local,end_local:I64Local,result:RegExpMatcherResult,function:&mutFunction,){"
    ));
    assert!(!writer.contains("found:i64"));
    let rewind = writer
        .find("self.emit_regexp_scratch_rewind(checkpoint,function);")
        .unwrap();
    let found = writer
        .find("function.instruction(&Instruction::I32Const(found));")
        .unwrap();
    let start = writer.find("start_local.load(function);").unwrap();
    let end = writer.find("end_local.load(function);").unwrap();
    assert!(rewind < found && found < start && start < end);
    assert!(!writer.contains("status:RegExpMatcherStatus"));
    assert_eq!(writer.matches("matchresult{").count(), 1);
    for projection in [
        "RegExpMatcherResult::Match=>(1,RegExpMatcherStatus::Complete)",
        "RegExpMatcherResult::NoMatch=>(0,RegExpMatcherStatus::Complete)",
        "RegExpMatcherResult::Failed(failure)=>(0,RegExpMatcherStatus::Failed(failure))",
    ] {
        assert_eq!(writer.matches(projection).count(), 1, "`{projection}`");
    }
    assert!(!writer.contains("_=>"));
    assert!(!writer.contains("unreachable!"));
    assert_eq!(matcher.matches("result:RegExpMatcherResult").count(), 1);
    assert_eq!(matcher.matches("matchresult{").count(), 1);
}
