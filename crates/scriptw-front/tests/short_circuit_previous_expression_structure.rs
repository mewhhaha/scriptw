use scriptw_front::{parse, ParseOptions};

const EXPRESSION: &str =
    include_str!("../../../vendor/boa_parser-0.21.1/src/parser/expression/mod.rs");
const CONTRACT: &str = include_str!(
    "../../../docs/rust-rewrite/contracts/short-circuit-previous-expression-exhaustiveness.md"
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
            assert_eq!(depth, 0, "unterminated block comment in expression parser");
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
fn previous_expression_is_the_exact_private_non_equality_domain() {
    let expression = lexically_normalized(EXPRESSION);
    let declaration = bounded(
        &expression,
        "structShortCircuitExpression{allow_in:AllowIn,allow_yield:AllowYield,allow_await:AllowAwait,previous:PreviousExpr,}",
        "implShortCircuitExpression{",
    );
    assert_eq!(
        declaration,
        "#[derive(Debug,Clone,Copy)]enumPreviousExpr{None,Logical,Coalesce,}"
    );
    assert!(!expression.contains("pubenumPreviousExpr"));
    assert!(!expression.contains("pub(crate)enumPreviousExpr"));
    assert!(!expression.contains("pub(super)enumPreviousExpr"));
    for capability in ["PartialEq", "Eq"] {
        assert!(!expression.contains(&format!("impl{capability}forPreviousExpr")));
    }
    assert!(!expression.contains("==PreviousExpr::"));
    assert!(!expression.contains("!=PreviousExpr::"));
    assert!(!expression.contains("matches!(previous"));
    // 15 since `||` joined the `&&` loop: the `with_previous` constructor
    // and its recursive `PreviousExpr::Logical` call are gone.
    assert_eq!(expression.matches("PreviousExpr").count(), 15);
}

#[test]
fn all_three_short_circuit_operator_observers_are_exhaustive() {
    let expression = lexically_normalized(EXPRESSION);
    let parser = bounded(
        &expression,
        "impl<R>TokenParser<R>forShortCircuitExpression",
        "structBitwiseORExpression",
    );
    assert_eq!(parser.matches("matchprevious{").count(), 3);
    assert_eq!(
        parser
            .matches(
                "PreviousExpr::None|PreviousExpr::Logical=>{}PreviousExpr::Coalesce=>{returnErr("
            )
            .count(),
        2
    );
    assert_eq!(
        parser
            .matches(
                "PreviousExpr::None|PreviousExpr::Coalesce=>{}PreviousExpr::Logical=>{returnErr("
            )
            .count(),
        1
    );
    for (start, _) in parser.match_indices("matchprevious{") {
        let observer = parser[start..]
            .split_once("cursor.advance(interner);")
            .expect("operator observer must precede token consumption")
            .0;
        assert!(!observer.contains("_=>"));
    }
}

#[test]
fn mixed_short_circuit_operators_require_parentheses_under_both_goals() {
    for options in [ParseOptions::script(), ParseOptions::module()] {
        for source in ["a ?? b && c", "a && b ?? c", "a ?? b || c", "a || b ?? c"] {
            if let Ok(parsed) = parse(source, options.clone()) {
                panic!(
                    "{source:?} must reject unparenthesized logical/coalesce mixing under \
                     {options:?}, parsed as {parsed:?}"
                );
            }
        }
        for source in [
            "a ?? (b && c)",
            "(a ?? b) && c",
            "a || (b ?? c)",
            "(a || b) ?? c",
            "a && b || c",
            "a ?? b ?? c",
        ] {
            parse(source, options.clone()).unwrap_or_else(|error| {
                panic!(
                    "{source:?} must accept parenthesized mixing or one operator family under \
                     {options:?}: {error:?}"
                )
            });
        }
    }
}

#[test]
fn contract_records_exhaustiveness_without_a_conformance_claim() {
    for marker in [
        "but supports no equality",
        "three exhaustive operator matches",
        "state fails to compile at all three observers",
        "passes `4/4`",
    ] {
        assert!(CONTRACT.contains(marker), "contract marker `{marker}`");
    }
    let contract = CONTRACT.split_whitespace().collect::<Vec<_>>().join(" ");
    for marker in [
        "private `PreviousExpr::{None, Logical, Coalesce}`",
        "supports no equality comparison",
        "three exhaustive operator matches",
        "no parser-behavior or conformance claim",
    ] {
        assert!(contract.contains(marker), "contract marker `{marker}`");
    }
}
