use std::fs;
use std::path::Path;

const ARGUMENTS_SOURCE: &str = include_str!("../../src/arguments_protocol.rs");
const ACTIVATION_SOURCE: &str = include_str!("../../src/arguments_protocol/unobservable_leaf.rs");
const EMIT_SOURCE: &str = include_str!("../../src/emit.rs");
const ENVIRONMENTS_SOURCE: &str = include_str!("../../src/environments.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/function-arguments-binding-ownership.md");

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
fn binding_authority_is_one_private_non_cloneable_closed_lifecycle() {
    let lexical_probe = rust_code(
        r###"
        // ArgumentsBindingProtocol
        ArgumentsBindingProtocol /* nested /* ignored */ comment */ :: r#consume;
        "ArgumentsBindingProtocol"; br##"ArgumentsBindingProtocol"##;
        'A'; b'A'; 'lifetime;
        "###,
        false,
    );
    assert_eq!(
        exact_identifier_count(&lexical_probe, "ArgumentsBindingProtocol"),
        1
    );

    let lifecycle = rust_code(
        bounded(
            ARGUMENTS_SOURCE,
            "/// See `docs/rust-rewrite/contracts/arguments-object-construction-protocol.md`.\n",
            "enum FunctionArgumentsKind",
        ),
        true,
    );
    assert_eq!(
        lifecycle,
        "pub(crate)structFunctionArgumentsProtocol(FunctionArgumentsState);enumFunctionArgumentsState{Pending(FunctionArgumentsKind),BoundAbsent,BoundPresent,}"
    );

    let authority = rust_code(
        bounded(
            ARGUMENTS_SOURCE,
            "#[must_use = \"the function arguments binding protocol must be consumed\"]",
            "pub(crate) struct UnmappedArgumentsPlan",
        ),
        true,
    );
    assert_eq!(
        authority,
        "pub(crate)structArgumentsBindingProtocol(Option<PresentArgumentsObjectProtocol>);"
    );
    assert_eq!(
        count_identifier_in_rust_sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            "ArgumentsBindingProtocol",
        ),
        5
    );
}

#[test]
fn binding_transition_moves_the_protocol_and_presence_cannot_recover_it() {
    let transition = rust_code(
        bounded(
            ARGUMENTS_SOURCE,
            "pub(crate) fn take_for_binding(",
            "pub(crate) fn take_empty_strict_body(",
        ),
        true,
    );
    positions_in_order(
        &transition,
        &[
            "&mutself)->Result<ArgumentsBindingProtocol,EmitError>{",
            "matchcore::mem::replace(&mutself.0,FunctionArgumentsState::BoundAbsent)",
            "Pending(FunctionArgumentsKind::Absent|FunctionArgumentsKind::EmptyStrictBody(_)|FunctionArgumentsKind::UnobservableLeaf|FunctionArgumentsKind::UnobservedStrict,)",
            "ArgumentsBindingProtocol(None)",
            "Pending(FunctionArgumentsKind::Present(protocol))",
            "self.0=FunctionArgumentsState::BoundPresent",
            "ArgumentsBindingProtocol(Some(protocol))",
            "FunctionArgumentsState::BoundAbsent",
            "FunctionArgumentsState::BoundPresent",
        ],
    );
    assert_eq!(exact_identifier_count(&transition, "clone"), 0);
    assert_eq!(exact_identifier_count(&transition, "cloned"), 0);

    let presence = rust_code(
        bounded(ARGUMENTS_SOURCE, "pub(crate) const fn present(", "\n}\n"),
        true,
    );
    assert_eq!(
        presence,
        "&self)->Option<()>{match&self.0{FunctionArgumentsState::Pending(FunctionArgumentsKind::Absent|FunctionArgumentsKind::EmptyStrictBody(_)|FunctionArgumentsKind::UnobservableLeaf|FunctionArgumentsKind::UnobservedStrict,)|FunctionArgumentsState::BoundAbsent=>None,FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(_))|FunctionArgumentsState::BoundPresent=>Some(()),}}"
    );
}

#[test]
fn activation_plan_keeps_original_validation_source_and_storage_under_one_authority() {
    let plan = rust_code(
        bounded(
            ACTIVATION_SOURCE,
            "#[must_use = \"a validated activation plan must be consumed by its source builder\"]",
            "impl<'a> UserFunctionActivationPlan<'a>",
        ),
        true,
    );
    assert_eq!(
        plan,
        "pub(crate)structUserFunctionActivationPlan<'a>{function:&'aFunctionIr,arguments:FunctionArgumentsProtocol,owned_bindings:&'a[OwnedEnvBindingIr],}"
    );

    let admission = rust_code(
        bounded(
            ACTIVATION_SOURCE,
            "pub(crate) fn for_function(",
            "pub(crate) fn into_parts(",
        ),
        true,
    );
    positions_in_order(
        &admission,
        &[
            "function:&'aFunctionIr)->Result<Self,EmitError>",
            "letarguments=FunctionArgumentsProtocol::for_user_function(function)?;",
            "UnobservableLeafActivation::admit(function)",
            "Some(leaf)=>leaf.consume(arguments)",
            "None=>matchsuper::unobserved_strict::UnobservedStrictArguments::admit(function)",
            "Some(proof)=>proof.consume(arguments)",
            "None=>Self::retaining_storage(function,arguments)",
            "pub(super)fnretaining_storage(",
            "owned_bindings:&function.owned_env_bindings",
        ],
    );
    assert_eq!(exact_identifier_count(&admission, "for_user_function"), 1);

    let consumed_parts = rust_code(
        bounded(
            ACTIVATION_SOURCE,
            "pub(crate) fn into_parts(",
            "struct UnobservableLeafActivation<'a>",
        ),
        true,
    );
    assert_eq!(
        consumed_parts,
        "self,)->(&'aFunctionIr,FunctionArgumentsProtocol,&'a[OwnedEnvBindingIr],){(self.function,self.arguments,self.owned_bindings)}}"
    );

    let elision = rust_code(
        bounded(ACTIVATION_SOURCE, "fn consume(", "#[cfg(test)]"),
        true,
    );
    positions_in_order(
        &elision,
        &[
            "self,mutarguments:FunctionArgumentsProtocol)->UserFunctionActivationPlan<'a>",
            "FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(_))",
            "arguments.0=FunctionArgumentsState::Pending(FunctionArgumentsKind::UnobservableLeaf);",
            "UserFunctionActivationPlan{function:self.function,arguments,owned_bindings:&[],}",
        ],
    );
    assert_eq!(exact_identifier_count(&elision, "clone"), 0);
    assert_eq!(exact_identifier_count(&elision, "cloned"), 0);

    let builder = rust_code(
        bounded(EMIT_SOURCE, "fn new_function(", "fn new_host_builtin("),
        true,
    );
    positions_in_order(
        &builder,
        &[
            "let(function,arguments_protocol,owned_env_bindings)=UserFunctionActivationPlan::for_function(function)?.into_parts();",
            "Self::new(schema,&function.body,function.template_source,function.params.as_slice(),owned_env_bindings,",
            "function.protocol.flavor(),arguments_protocol,",
        ],
    );
    assert_eq!(exact_identifier_count(&builder, "for_function"), 1);
    assert_eq!(exact_identifier_count(&builder, "for_user_function"), 0);
    assert!(!builder.contains("function.owned_env_bindings"));
}

#[test]
fn parameter_binding_consumes_authority_before_owned_initialization() {
    let binding = rust_code(
        bounded(
            ENVIRONMENTS_SOURCE,
            "pub(crate) fn bind_parameters(",
            "pub(crate) fn allocate_dynamic_binding_storage(",
        ),
        true,
    );
    positions_in_order(
        &binding,
        &[
            "self.function_arguments_protocol.take_for_binding()?",
            "arguments_protocol.into_present()",
            "initialize_arguments_binding(arguments_storage,arguments_protocol,function)?",
        ],
    );
    assert_eq!(exact_identifier_count(&binding, "take_for_binding"), 1);
    assert_eq!(exact_identifier_count(&binding, "into_present"), 1);
    assert_eq!(
        exact_identifier_count(&binding, "initialize_arguments_binding"),
        1
    );
    assert!(!binding.contains("function_arguments_protocol.present()"));
    assert!(!binding.contains("arguments_protocol.cloned()"));

    let initialization = rust_code(
        bounded(
            ENVIRONMENTS_SOURCE,
            "pub(crate) fn initialize_arguments_binding(",
            "pub(crate) fn initialize_parameter(",
        ),
        true,
    );
    positions_in_order(
        &initialization,
        &[
            "protocol:PresentArgumentsObjectProtocol",
            "self.emit_arguments_object_payload(&protocol,function)?",
            "self.write_binding_from_locals(storage,&value,function)",
        ],
    );
    assert_eq!(exact_identifier_count(&initialization, "clone"), 0);
    assert_eq!(exact_identifier_count(&initialization, "cloned"), 0);

    // Count the consuming operations in their production bodies above. The
    // protocol unit tests also exercise these methods, so whole-tree textual
    // counts do not describe the one-shot ownership invariant.
}

#[test]
fn ownership_contract_records_the_verification_boundary() {
    for marker in [
        "Pending(Absent)  -> BoundAbsent",
        "Pending(Present) -> BoundPresent",
        "`present()` projection",
        "A second call is a compiler-invariant error",
        "source-equivalent compiler ownership closure",
    ] {
        assert!(
            CONTRACT.contains(marker),
            "missing contract marker `{marker}`"
        );
    }
}
