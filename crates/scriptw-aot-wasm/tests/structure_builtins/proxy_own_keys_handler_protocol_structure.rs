const CLI_OBJECT_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/object.rs");
const OWN_KEYS_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_proxy_own_keys.js");
const HANDLER_PROTOCOL_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_proxy_own_keys_handler_protocol.js");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"))
        .0
}

fn after<'a>(source: &'a str, start: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
}

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
        "assert!(output.status.success());",
        "assert!(stdout.contains(\"backend_used: WasmAot\"));",
        "assert!(stdout.contains(\"boolean(true)\"));",
    ] {
        assert_eq!(
            body.lines().filter(|line| line.trim() == marker).count(),
            1,
            "`{name}` must retain CLI marker `{marker}`"
        );
    }
    assert_eq!(
        body.matches(&format!("fixture_path(\"{fixture}\")"))
            .count(),
        1,
        "`{name}` must run its exact fixture"
    );
}

#[test]
fn cli_regressions_are_live_and_cover_handler_protocol_boundaries() {
    const PROTOCOL_TEST_NAME: &str =
        "run_wasm_backend_succeeds_for_proxy_own_keys_handler_protocol";
    let protocol_declaration = format!("fn {PROTOCOL_TEST_NAME}() {{");
    for commented_owner in [
        format!("// #[test]\n// {protocol_declaration}\n// }}"),
        format!("/*\n#[test]\n{protocol_declaration}\n}}\n*/"),
    ] {
        let active_source = mask_line_and_block_comments(&commented_owner);
        assert!(
            anchored_offsets(&active_source, &protocol_declaration).is_empty(),
            "commented CLI owner must not count as active"
        );
    }
    for disabling_attribute in [
        "#[cfg(\n    any()\n)]",
        "#[cfg_attr(\n    all(),\n    ignore\n)]",
        "#[ignore\n]",
    ] {
        let disabled_registration = format!(
            "fn preceding_owner() {{\n}}\n{disabling_attribute}\n#[test]\n{protocol_declaration}\n}}"
        );
        let active_source = mask_line_and_block_comments(&disabled_registration);
        assert!(
            std::panic::catch_unwind(|| {
                assert_live_wasm_cli_test(
                    &active_source,
                    PROTOCOL_TEST_NAME,
                    "wasm_proxy_own_keys_handler_protocol.js",
                );
            })
            .is_err(),
            "multi-line `{disabling_attribute}` must disable the CLI owner"
        );
    }

    const EXECUTABLE_MARKER: &str = "check(functionTrapThis === functionHandler);";
    for commented_marker in [
        format!("// {EXECUTABLE_MARKER}"),
        format!("/* {EXECUTABLE_MARKER} */"),
    ] {
        assert!(
            !mask_line_and_block_comments(&commented_marker).contains(EXECUTABLE_MARKER),
            "commented fixture assertion must not count as executable"
        );
    }

    let cli_object_source = mask_line_and_block_comments(CLI_OBJECT_SOURCE);
    let own_keys_fixture = mask_line_and_block_comments(OWN_KEYS_FIXTURE);
    let handler_protocol_fixture = mask_line_and_block_comments(HANDLER_PROTOCOL_FIXTURE);

    assert_live_wasm_cli_test(
        &cli_object_source,
        "run_wasm_backend_succeeds_for_supported_proxy_own_keys_fixture",
        "wasm_proxy_own_keys.js",
    );
    assert_live_wasm_cli_test(
        &cli_object_source,
        PROTOCOL_TEST_NAME,
        "wasm_proxy_own_keys_handler_protocol.js",
    );

    assert!(own_keys_fixture.contains(
        "function throwsTypeError(fn) {\n  try {\n    fn();\n  } catch (error) {\n    return error instanceof TypeError;\n  }\n  return false;\n}"
    ));
    assert!(own_keys_fixture.matches("failures |=").count() >= 20);
    for load_bearing_scenario in [
        "if (trapTarget !== target) failures |= 16;",
        "if (!throwsTypeError(function() { Object.keys(duplicateProxy); })) failures |= 32;",
        "if (!throwsTypeError(function() { Object.keys(invalidEntryProxy); })) failures |= 64;",
        "if (!throwsTypeError(function() { Object.keys(invalidResultProxy); })) failures |= 128;",
        "var nestedProxy = new Proxy(nestedTarget, {\n  ownKeys: null\n});",
        "var reflectProxy = new Proxy(reflectTarget, {\n  ownKeys: undefined\n});",
        "failures === 0;",
    ] {
        assert!(
            own_keys_fixture.contains(load_bearing_scenario),
            "base ownKeys fixture must retain `{load_bearing_scenario}`"
        );
    }
    let base_nested_fallback = bounded(
        &own_keys_fixture,
        "var nestedTarget = new Proxy(",
        "var symbolKey = Symbol();",
    );
    assert_before(
        base_nested_fallback,
        "ownKeys: null",
        "Object.keys(nestedProxy)",
    );
    assert_before(
        base_nested_fallback,
        "Object.keys(nestedProxy)",
        "nestedKeys.length !== 2",
    );

    assert!(handler_protocol_fixture
        .contains("function check(condition) {\n  if (!condition) failures += 1;\n}"));
    assert!(handler_protocol_fixture.contains(
        "function capture(fn) {\n  try {\n    fn();\n  } catch (error) {\n    return error;\n  }\n  return undefined;\n}"
    ));
    assert!(handler_protocol_fixture.matches("check(").count() >= 30);
    assert!(handler_protocol_fixture
        .trim_end()
        .ends_with("failures === 0;"));

    let function_handler = bounded(
        &handler_protocol_fixture,
        "var functionTarget = {};",
        "var arrayTarget = { arrayKey: 2 };",
    );
    for marker in [
        "function functionHandler() {}",
        "var callableProxyTrap = new Proxy(functionTrapTargetFunction, {});",
        "return callableProxyTrap;",
        "Reflect.ownKeys(new Proxy(functionTarget, functionHandler))",
        "check(functionGetterThis === functionHandler);",
        "check(functionTrapThis === functionHandler);",
        "check(functionTrapTarget === functionTarget);",
        "check(functionKeys.length === 1 && functionKeys[0] === \"functionKey\");",
    ] {
        assert!(
            function_handler.contains(marker),
            "Function handler `{marker}`"
        );
    }
    assert_before(
        function_handler,
        "return callableProxyTrap;",
        "Reflect.ownKeys(new Proxy(functionTarget, functionHandler))",
    );
    assert_before(
        function_handler,
        "Reflect.ownKeys(new Proxy(functionTarget, functionHandler))",
        "check(functionTrapThis === functionHandler);",
    );

    let array_handler = bounded(
        &handler_protocol_fixture,
        "var arrayTarget = { arrayKey: 2 };",
        "function makeArgumentsHandler() {",
    );
    for marker in [
        "var arrayHandler = [];",
        "Object.getOwnPropertyNames(new Proxy(arrayTarget, arrayHandler))",
        "check(arrayGetterThis === arrayHandler);",
        "check(arrayTrapThis === arrayHandler);",
        "check(arrayTrapTarget === arrayTarget);",
        "check(arrayKeys.length === 1 && arrayKeys[0] === \"arrayKey\");",
    ] {
        assert!(array_handler.contains(marker), "Array handler `{marker}`");
    }
    assert_before(
        array_handler,
        "Object.defineProperty(arrayHandler, \"ownKeys\"",
        "Object.getOwnPropertyNames(new Proxy(arrayTarget, arrayHandler))",
    );
    assert_before(
        array_handler,
        "Object.getOwnPropertyNames(new Proxy(arrayTarget, arrayHandler))",
        "check(arrayGetterThis === arrayHandler);",
    );

    let arguments_handler = bounded(
        &handler_protocol_fixture,
        "function makeArgumentsHandler() {",
        "var symbolKey = Symbol(\"proxy-handler-key\");",
    );
    for marker in [
        "return arguments;",
        "var argumentsHandler = makeArgumentsHandler(1);",
        "Object.keys(new Proxy(argumentsTarget, argumentsHandler))",
        "check(argumentsGetterThis === argumentsHandler);",
        "check(argumentsTrapThis === argumentsHandler);",
        "check(argumentsTrapTarget === argumentsTarget);",
        "check(argumentsKeys.length === 1 && argumentsKeys[0] === \"argumentsKey\");",
    ] {
        assert!(
            arguments_handler.contains(marker),
            "arguments handler `{marker}`"
        );
    }
    assert_before(
        arguments_handler,
        "Object.defineProperty(argumentsHandler, \"ownKeys\"",
        "Object.keys(new Proxy(argumentsTarget, argumentsHandler))",
    );
    assert_before(
        arguments_handler,
        "Object.keys(new Proxy(argumentsTarget, argumentsHandler))",
        "check(argumentsGetterThis === argumentsHandler);",
    );

    let proxy_handler = bounded(
        &handler_protocol_fixture,
        "var symbolKey = Symbol(\"proxy-handler-key\");",
        "var lookupSentinel = {};",
    );
    for marker in [
        "proxyHandler = new Proxy(proxyHandlerTarget, proxyLookupHandler);",
        "Object.getOwnPropertySymbols(new Proxy(proxyTarget, proxyHandler))",
        "check(proxyLookupThis === proxyLookupHandler);",
        "check(proxyLookupTarget === proxyHandlerTarget);",
        "check(proxyLookupKey === \"ownKeys\");",
        "check(proxyLookupReceiver === proxyHandler);",
        "check(proxyTrapThis === proxyHandler);",
        "check(proxyTrapTarget === proxyTarget);",
        "check(symbolKeys.length === 1 && symbolKeys[0] === symbolKey);",
    ] {
        assert!(proxy_handler.contains(marker), "Proxy handler `{marker}`");
    }
    assert_before(
        proxy_handler,
        "proxyHandler = new Proxy(proxyHandlerTarget, proxyLookupHandler);",
        "Object.getOwnPropertySymbols(new Proxy(proxyTarget, proxyHandler))",
    );
    assert_before(
        proxy_handler,
        "Object.getOwnPropertySymbols(new Proxy(proxyTarget, proxyHandler))",
        "check(proxyLookupReceiver === proxyHandler);",
    );

    let abrupt_lookup = bounded(
        &handler_protocol_fixture,
        "var lookupSentinel = {};",
        "var nestedCalls = 0;",
    );
    assert!(abrupt_lookup.contains("throw lookupSentinel;"));
    assert!(abrupt_lookup.contains("Reflect.ownKeys(new Proxy({}, abruptHandler))"));
    assert!(abrupt_lookup.contains("check(lookupError === lookupSentinel);"));
    assert_before(
        abrupt_lookup,
        "throw lookupSentinel;",
        "var lookupError = capture(",
    );
    assert_before(
        abrupt_lookup,
        "var lookupError = capture(",
        "check(lookupError === lookupSentinel);",
    );

    let nested_fallback = bounded(
        &handler_protocol_fixture,
        "var nestedCalls = 0;",
        "var other = __scriptwCreateRealm().global;",
    );
    assert!(nested_fallback.contains("ownKeys: function() {"));
    assert!(nested_fallback.contains("new Proxy(nestedTarget, { ownKeys: null })"));
    assert!(nested_fallback.contains("check(nestedCalls === 1);"));
    assert!(nested_fallback
        .contains("check(nestedKeys.length === 1 && nestedKeys[0] === \"nestedKey\");"));
    assert_before(
        nested_fallback,
        "new Proxy(nestedTarget, { ownKeys: null })",
        "check(nestedCalls === 1);",
    );
    assert_before(
        nested_fallback,
        "new Proxy(nestedTarget, { ownKeys: null })",
        "check(nestedKeys.length === 1 && nestedKeys[0] === \"nestedKey\");",
    );

    let foreign_realm_errors = after(
        &handler_protocol_fixture,
        "var other = __scriptwCreateRealm().global;",
    );
    for marker in [
        "other.Reflect.ownKeys(new Proxy({}, { ownKeys: {} }))",
        "Object.getPrototypeOf(nonCallableError) === other.TypeError.prototype",
        "nonCallableError instanceof other.TypeError",
        "!(nonCallableError instanceof TypeError)",
        "revocable.revoke();",
        "other.Object.keys(revocable.proxy)",
        "Object.getPrototypeOf(revokedError) === other.TypeError.prototype",
        "revokedError instanceof other.TypeError",
        "!(revokedError instanceof TypeError)",
    ] {
        assert!(
            foreign_realm_errors.contains(marker),
            "foreign Realm `{marker}`"
        );
    }
    assert_before(
        foreign_realm_errors,
        "other.Reflect.ownKeys(new Proxy({}, { ownKeys: {} }))",
        "Object.getPrototypeOf(nonCallableError) === other.TypeError.prototype",
    );
    assert_before(
        foreign_realm_errors,
        "revocable.revoke();",
        "other.Object.keys(revocable.proxy)",
    );
    assert_before(
        foreign_realm_errors,
        "other.Object.keys(revocable.proxy)",
        "Object.getPrototypeOf(revokedError) === other.TypeError.prototype",
    );
}
