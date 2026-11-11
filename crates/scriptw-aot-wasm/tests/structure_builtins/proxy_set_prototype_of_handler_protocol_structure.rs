const CLI_OBJECT_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/object.rs");
const HANDLER_PROTOCOL_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_proxy_set_prototype_of_handler_protocol.js"
);

fn assert_before(source: &str, earlier: &str, later: &str) {
    let earlier_offset = source.find(earlier).expect("earlier operation");
    let later_offset = source.find(later).expect("later operation");
    assert!(
        earlier_offset < later_offset,
        "`{earlier}` must precede `{later}`"
    );
}

fn mask_line_and_block_comments(source: &str) -> String {
    let mut characters = source.chars().peekable();
    let mut masked = String::with_capacity(source.len());
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment_depth = 0usize;

    while let Some(character) = characters.next() {
        if line_comment {
            if character == '\n' {
                masked.push(character);
                line_comment = false;
            } else {
                masked.push(' ');
            }
            continue;
        }

        if block_comment_depth > 0 {
            if character == '/' && characters.peek() == Some(&'*') {
                characters.next();
                masked.push_str("  ");
                block_comment_depth += 1;
                continue;
            }
            if character == '*' && characters.peek() == Some(&'/') {
                characters.next();
                masked.push_str("  ");
                block_comment_depth -= 1;
                continue;
            }
            masked.push(if character == '\n' { '\n' } else { ' ' });
            continue;
        }

        if let Some(delimiter) = quote {
            masked.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
            continue;
        }

        if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
            masked.push(character);
            continue;
        }
        if character == '/' && characters.peek() == Some(&'/') {
            characters.next();
            masked.push_str("  ");
            line_comment = true;
            continue;
        }
        if character == '/' && characters.peek() == Some(&'*') {
            characters.next();
            masked.push_str("  ");
            block_comment_depth = 1;
            continue;
        }
        masked.push(character);
    }

    assert_eq!(block_comment_depth, 0, "unterminated block comment");
    masked
}

fn anchored_offsets(source: &str, declaration: &str) -> Vec<usize> {
    source
        .match_indices(declaration)
        .filter_map(|(offset, _)| {
            let line_start = source[..offset]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            source[line_start..offset]
                .chars()
                .all(char::is_whitespace)
                .then_some(offset)
        })
        .collect()
}

fn braced_rust_function<'a>(source: &'a str, declaration: &str) -> &'a str {
    let offsets = anchored_offsets(source, declaration);
    assert_eq!(offsets.len(), 1, "exact Rust owner `{declaration}`");
    let start = offsets[0];
    let mut depth = 0;
    let mut body_started = false;
    for (relative_offset, character) in source[start..].char_indices() {
        match character {
            '{' => {
                depth += 1;
                body_started = true;
            }
            '}' => {
                depth -= 1;
                if body_started && depth == 0 {
                    return &source[start..start + relative_offset + character.len_utf8()];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated Rust owner `{declaration}`");
}

fn assert_live_wasm_cli_test(source: &str, name: &str, fixture: &str) {
    let declaration = format!("fn {name}() {{");
    let offsets = anchored_offsets(source, &declaration);
    assert_eq!(offsets.len(), 1, "exact CLI test owner `{name}`");

    let attached_attributes = source[..offsets[0]]
        .rsplit_once("\n}\n")
        .unwrap_or_else(|| panic!("preceding top-level CLI owner for `{name}`"))
        .1;
    assert_eq!(
        attached_attributes.matches("#[test]").count(),
        1,
        "`{name}` must remain a live Rust test"
    );
    for disabling_attribute in ["#[cfg", "#[cfg_attr", "#[ignore"] {
        assert!(
            !attached_attributes.contains(disabling_attribute),
            "`{name}` must not carry `{disabling_attribute}`"
        );
    }

    let body = braced_rust_function(source, &declaration);
    for marker in [
        ".arg(\"run\")",
        ".arg(\"--execution-backend\")",
        ".arg(\"wasm\")",
        "output.status.success()",
        "stdout.contains(\"backend_used: WasmAot\")",
        "stdout.contains(\"boolean(true)\")",
    ] {
        assert_eq!(
            body.matches(marker).count(),
            1,
            "`{name}` must retain CLI marker `{marker}`"
        );
    }
    assert_eq!(
        body.matches(&format!("fixture_path(\n            \"{fixture}\","))
            .count(),
        1,
        "`{name}` must run its exact fixture"
    );
}

#[test]
fn cli_regression_is_live_and_covers_handler_protocol_boundaries() {
    const TEST_NAME: &str = "run_wasm_backend_succeeds_for_proxy_set_prototype_of_handler_protocol";
    let cli_object_source = mask_line_and_block_comments(CLI_OBJECT_SOURCE);
    let fixture = mask_line_and_block_comments(HANDLER_PROTOCOL_FIXTURE);
    assert_live_wasm_cli_test(
        &cli_object_source,
        TEST_NAME,
        "wasm_proxy_set_prototype_of_handler_protocol.js",
    );

    for marker in [
        "function functionHandler() {}",
        "var arrayHandler = [];",
        "var argumentsHandler = (function () { return arguments; })(1, 2, 3);",
        "proxyHandler = new Proxy(proxyHandlerTarget, proxyLookupHandler);",
        "var callableProxyTrap = new Proxy(callableTrapTarget, callableTrapHandler);",
        "assert(this === expectedHandler, currentScenario + \" trap this\");",
        "assert(target === expectedTarget, currentScenario + \" target\");",
        "assert(prototype === expectedPrototype, currentScenario + \" prototype\");",
        "assert(arguments.length === 2, currentScenario + \" trap arity\");",
        "assert(key === \"setPrototypeOf\", currentScenario + \" lookup key\");",
        "throw lookupSentinel;",
        "new Proxy(nestedTarget, { setPrototypeOf: null })",
        "new Proxy(nestedTarget, { setPrototypeOf: undefined })",
        "Object.getPrototypeOf(nonCallableError) === other.TypeError.prototype",
        "Object.getPrototypeOf(invariantError) === other.TypeError.prototype",
        "Object.getPrototypeOf(revokedError) === other.TypeError.prototype",
    ] {
        assert!(
            fixture.contains(marker),
            "fixture protocol marker `{marker}`"
        );
    }
    assert_eq!(fixture.matches("exerciseHandlerBrand(").count(), 5);
    assert!(fixture.trim_end().ends_with("true;"));
    assert_before(
        &fixture,
        "return ordinarySetPrototypeOfTrap;",
        "exerciseHandlerBrand(functionHandler, {}, \"Function handler\");",
    );
    assert_before(
        &fixture,
        "throw lookupSentinel;",
        "assert(lookupError === lookupSentinel, \"abrupt lookup sentinel\");",
    );
    assert_before(
        &fixture,
        "new Proxy(nestedTarget, { setPrototypeOf: null })",
        "assert(nestedCalls === 2, \"nested fallback call count\");",
    );
}
