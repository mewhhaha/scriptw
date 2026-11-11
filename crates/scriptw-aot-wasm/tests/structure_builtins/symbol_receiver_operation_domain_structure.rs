const SOURCE: &str = include_str!("../../src/builtins/symbol.rs");
const STANDARD: &str = include_str!("../../src/builtins/standard.rs");
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
            assert_eq!(depth, 0, "unterminated block comment");
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"r#") {
            offset += 2;
            continue;
        }
        let character = source[offset..].chars().next().unwrap();
        if character.is_whitespace() {
            if !retain_literals {
                code.push(' ');
            }
        } else {
            code.push(character);
        }
        offset += character.len_utf8();
    }
    code
}

fn normalized(source: &str) -> String {
    rust_code(source, true)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
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

#[test]
fn symbol_receiver_error_message_is_an_exhaustive_closed_projection() {
    assert!(SOURCE.contains("    PrototypeToPrimitive,\n}\n\nenum SymbolReceiverOperation {"));
    let code = rust_code(SOURCE, false);
    assert_eq!(exact_identifier_count(&code, "SymbolReceiverOperation"), 11);
    for forbidden in [
        "impl Clone for SymbolReceiverOperation",
        "impl Copy for SymbolReceiverOperation",
        "impl Debug for SymbolReceiverOperation",
        "impl PartialEq for SymbolReceiverOperation",
        "impl Eq for SymbolReceiverOperation",
        "SymbolReceiverOperation::clone",
    ] {
        assert!(!code.contains(forbidden), "found `{forbidden}`");
    }

    let variants = bounded(
        SOURCE,
        "enum SymbolReceiverOperation {",
        "impl SymbolReceiverOperation {",
    )
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty() && *line != "}")
    .collect::<Vec<_>>();
    assert_eq!(
        variants,
        ["Description,", "ToString,", "ValueOf,", "ToPrimitive,"]
    );

    let projection = normalized(bounded(
        SOURCE,
        "impl SymbolReceiverOperation {",
        "impl FunctionBuilder<'_> {",
    ));
    for mapping in [
        concat!(
            "Self::Description=>",
            "RuntimeErrorMessage::SYMBOL_PROTOTYPE_DESCRIPTION_REQUIRES_THAT_THIS_BE_A_SYMBOL"
        ),
        concat!(
            "Self::ToString=>",
            "RuntimeErrorMessage::SYMBOL_PROTOTYPE_TOSTRING_REQUIRES_THAT_THIS_BE_A_SYMBOL"
        ),
        concat!(
            "Self::ValueOf=>",
            "RuntimeErrorMessage::SYMBOL_PROTOTYPE_VALUEOF_REQUIRES_THAT_THIS_BE_A_SYMBOL"
        ),
        concat!(
            "Self::ToPrimitive=>",
            "RuntimeErrorMessage::SYMBOL_PROTOTYPE_SYMBOL_TOPRIMITIVE_REQUIRES_THAT_THIS_BE_A_SYMBOL"
        ),
    ] {
        assert_eq!(
            projection.matches(mapping).count(),
            1,
            "mapping `{mapping}`"
        );
    }
    assert_eq!(projection.matches("=>").count(), 4);
    assert!(!projection.contains("_=>"));
    assert!(!projection.contains("unreachable!"));
}

#[test]
fn this_symbol_value_accepts_only_the_closed_operation() {
    let signature = bounded(
        SOURCE,
        "fn emit_this_symbol_value(",
        ") -> Result<GcLocal<SymbolValue, Nullable>, EmitError> {",
    );
    assert!(signature.contains("operation: &SymbolReceiverOperation,"));
    assert!(!signature.contains("error_message: &'static str"));

    let reader = bounded(
        SOURCE,
        "fn emit_this_symbol_value(",
        "    pub(super) fn emit_symbol_constructor_builtin(",
    );
    assert_eq!(
        reader.matches("operation.receiver_error_message()").count(),
        1
    );
    assert_eq!(
        reader
            .matches("emit_throw_current_function_realm_type_error(")
            .count(),
        1
    );
    assert!(reader.contains("receiver: &ValueLocals,"));
    assert!(reader.contains("result: &CompletionLocals,"));
    assert!(reader.contains(".reference_type::<PrimitiveBox>(GcNullability::NonNullable)"));
    assert!(reader.contains(".field(PrimitiveBoxSchema::PRIMITIVE)"));
    assert!(reader.contains(".cast_reference::<SymbolValue>(schema, function)"));
}

#[test]
fn symbol_prototype_callers_name_all_four_receiver_operations() {
    let dispatch = bounded(SOURCE, "fn emit_symbol(", "        Ok(())\n    }");
    assert_eq!(dispatch.matches("emit_this_symbol_value(").count(), 1);
    let producers = bounded(
        dispatch,
        "let operation = match builtin {",
        "let receiver =",
    );
    let result_projection = bounded(dispatch, "match operation {", "symbol.clear(function);");
    for operation in ["Description", "ToString", "ValueOf", "ToPrimitive"] {
        let variant = format!("SymbolReceiverOperation::{operation}");
        assert_eq!(
            producers.matches(&variant).count(),
            1,
            "producer `{operation}`"
        );
        assert_eq!(
            result_projection.matches(&variant).count(),
            1,
            "result `{operation}`"
        );
    }
    assert!(normalized(dispatch)
        .contains("self.emit_this_symbol_value(&receiver,&operation,&result,function)?"));
    assert!(
        dispatch.find("self.emit_this_symbol_value(").unwrap()
            < dispatch.find("match operation {").unwrap()
    );
    assert!(!result_projection.contains("_ =>"));
    assert!(!result_projection.contains("unreachable!"));
    for raw_message in [
        "RuntimeErrorMessage::SYMBOL_PROTOTYPE_DESCRIPTION_REQUIRES_THAT_THIS_BE_A_SYMBOL",
        "RuntimeErrorMessage::SYMBOL_PROTOTYPE_TOSTRING_REQUIRES_THAT_THIS_BE_A_SYMBOL",
        "RuntimeErrorMessage::SYMBOL_PROTOTYPE_VALUEOF_REQUIRES_THAT_THIS_BE_A_SYMBOL",
        "RuntimeErrorMessage::SYMBOL_PROTOTYPE_SYMBOL_TOPRIMITIVE_REQUIRES_THAT_THIS_BE_A_SYMBOL",
    ] {
        assert!(!dispatch.contains(raw_message));
    }

    let code = rust_code(SOURCE, false);
    assert!(code.contains("enum SymbolBuiltin {"));
    assert!(!code.contains("pub(super) enum SymbolBuiltin"));
    assert_eq!(exact_identifier_count(&code, "SymbolBuiltin"), 23);
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq"] {
        assert!(!code.contains(&format!("impl {capability} for SymbolBuiltin")));
    }
    assert!(!SOURCE.contains("#[derive("));
    assert!(!STANDARD.contains("SymbolBuiltin"));
    assert!(!STANDARD.contains("SymbolFn"));
    assert!(!STANDARD.contains("emit_symbol("));
    for (standard_builtin, entry, variant) in [
        (
            "SymbolConstructor",
            "emit_symbol_constructor_builtin",
            "Constructor",
        ),
        ("SymbolFor", "emit_symbol_for_builtin", "For"),
        ("SymbolKeyFor", "emit_symbol_key_for_builtin", "KeyFor"),
        (
            "SymbolPrototypeDescriptionGetter",
            "emit_symbol_prototype_description_getter_builtin",
            "PrototypeDescriptionGetter",
        ),
        (
            "SymbolPrototypeToString",
            "emit_symbol_prototype_to_string_builtin",
            "PrototypeToString",
        ),
        (
            "SymbolPrototypeValueOf",
            "emit_symbol_prototype_value_of_builtin",
            "PrototypeValueOf",
        ),
        (
            "SymbolPrototypeToPrimitive",
            "emit_symbol_prototype_to_primitive_builtin",
            "PrototypeToPrimitive",
        ),
    ] {
        assert_eq!(
            STANDARD
                .matches(&format!("StandardBuiltinId::{standard_builtin} =>"))
                .count(),
            1,
            "standard route `{standard_builtin}`"
        );
        assert_eq!(
            STANDARD
                .matches(&format!("self.{entry}(function)?"))
                .count(),
            1
        );
        assert_eq!(
            SOURCE
                .matches(&format!(
                    "self.emit_symbol(SymbolBuiltin::{variant}, function)"
                ))
                .count(),
            1,
            "fixed producer `{variant}`"
        );
    }
}
