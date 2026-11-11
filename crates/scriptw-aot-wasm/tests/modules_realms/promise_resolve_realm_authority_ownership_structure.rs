const PROMISE: &str = include_str!("../../src/builtins/promise.rs");
const PROMISE_RESOLVE_REALM_CONTEXT: &str =
    include_str!("../../src/builtins/promise/promise_resolve_realm_context.rs");
const PROMISE_FINALLY_COMPLETION: &str =
    include_str!("../../src/builtins/promise/promise_finally_completion.rs");
const GENERATOR_REACTION_HELPERS: &str =
    include_str!("../../src/builtins/promise/scheduling_helpers/async_generator_reactions.rs");
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
            assert_eq!(depth, 0, "unterminated block comment in Promise emitter");
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
fn resolve_realm_authority_is_the_exact_private_no_capability_domain() {
    let promise = lexically_normalized(PROMISE);
    let resolve_context = lexically_normalized(PROMISE_RESOLVE_REALM_CONTEXT);
    let finally_completion = lexically_normalized(PROMISE_FINALLY_COMPLETION);
    let context = bounded(
        &promise,
        "pub(crate)structAsyncExecutionRealmContext{",
        "}implAsyncExecutionRealmContext{",
    );
    assert_eq!(context, "realm:GcLocal<RealmRecord>,");
    let declaration = bounded(
        &promise,
        "enumPromiseResolveRealmAuthority<'a>{",
        "pub(crate)enumModuleReactionContinuation<'a>{",
    );
    assert_eq!(
        declaration,
        "CurrentFunction,AsyncExecution(&'aAsyncExecutionRealmContext),ExplicitRealm(&'aGcLocal<RealmRecord>),}"
    );
    assert!(!promise.contains("pubenumPromiseResolveRealmAuthority"));
    assert!(!promise.contains("pub(crate)enumPromiseResolveRealmAuthority"));
    assert!(!promise.contains("pub(super)enumPromiseResolveRealmAuthority"));
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq", "Default"] {
        assert!(!promise.contains(&format!("impl{capability}forPromiseResolveRealmAuthority")));
        assert!(
            !resolve_context.contains(&format!("impl{capability}forPromiseResolveRealmAuthority"))
        );
    }
    assert_eq!(promise.matches("PromiseResolveRealmAuthority").count(), 6);
    assert_eq!(
        resolve_context
            .matches("PromiseResolveRealmAuthority")
            .count(),
        6
    );
    assert_eq!(
        finally_completion
            .matches("PromiseResolveRealmAuthority")
            .count(),
        1
    );
}

#[test]
fn three_factories_take_owned_authority_and_forward_it_once() {
    let resolve_context = lexically_normalized(PROMISE_RESOLVE_REALM_CONTEXT);
    let selector = bounded(
        &resolve_context,
        "fnemit_promise_resolve_internal_function_materialization_context(",
        "fnemit_promise_resolve_operation_in_context(",
    );
    assert!(selector.starts_with(
        "&mutself,authority:PromiseResolveRealmAuthority<'_>,function:&mutFunction,)->PromiseInternalFunctionMaterializationContext{matchauthority{"
    ));
    for route in [
        "PromiseResolveRealmAuthority::CurrentFunction=>self.emit_current_function_promise_internal_function_materialization_context(function)",
        "PromiseResolveRealmAuthority::ExplicitRealm(realm)=>self.emit_promise_internal_function_materialization_context_from_realm(realm,function)",
        "PromiseResolveRealmAuthority::AsyncExecution(context)=>self.emit_promise_internal_function_materialization_context_from_realm(context.realm(),function,)",
    ] {
        assert_eq!(selector.matches(route).count(), 1, "`{route}`");
    }
    assert!(!selector.contains("_=>"));
    assert!(!selector.contains("unreachable!"));

    let operation = bounded(
        &resolve_context,
        "fnemit_promise_resolve_operation_realm_context(",
        "fnemit_intrinsic_promise_resolve_realm_context(",
    );
    let intrinsic = bounded(
        &resolve_context,
        "fnemit_intrinsic_promise_resolve_realm_context(",
        "pub(super)fnemit_call_promise_resolve_operation(",
    );
    for factory in [operation, intrinsic] {
        assert!(factory.starts_with(
            "&mutself,authority:PromiseResolveRealmAuthority<'_>,function:&mutFunction,)->Result<"
        ));
        assert_eq!(
            factory
                .matches("emit_promise_resolve_internal_function_materialization_context(authority,function)")
                .count(),
            1
        );
        assert!(!factory.contains("&PromiseResolveRealmAuthority"));
    }
    assert_eq!(
        resolve_context
            .matches("authority:PromiseResolveRealmAuthority<'_>")
            .count(),
        3
    );
}

#[test]
fn four_producer_routes_preserve_await_generator_and_finally_ownership() {
    let promise = lexically_normalized(PROMISE);
    let resolve_context = lexically_normalized(PROMISE_RESOLVE_REALM_CONTEXT);
    let finally_completion = lexically_normalized(PROMISE_FINALLY_COMPLETION);
    assert_eq!(
        promise
            .matches("PromiseResolveRealmAuthority::CurrentFunction")
            .count()
            + resolve_context
                .matches("PromiseResolveRealmAuthority::CurrentFunction")
                .count()
            + finally_completion
                .matches("PromiseResolveRealmAuthority::CurrentFunction")
                .count(),
        3
    );
    assert_eq!(
        promise
            .matches("PromiseResolveRealmAuthority::AsyncExecution")
            .count()
            + resolve_context
                .matches("PromiseResolveRealmAuthority::AsyncExecution")
                .count(),
        4
    );

    let await_reactions = bounded(
        &promise,
        "fnemit_intrinsic_await_reactions(",
        "fnemit_async_generator_await_return_reactions_inner(",
    );
    let selection = await_reactions
        .find("authority:PromiseResolveRealmAuthority<'_>")
        .expect("Await receives the selected Realm authority");
    let consume = await_reactions
        .find("emit_intrinsic_promise_resolve_realm_context(authority,function)")
        .expect("Await Realm authority consumption");
    assert!(selection < consume);
    assert_eq!(
        await_reactions
            .matches("emit_intrinsic_promise_resolve_realm_context(authority,function)")
            .count(),
        1
    );
    assert!(!await_reactions.contains("PromiseResolveRealmAuthority::CurrentFunction"));
    for (start, end, factory) in [
        (
            "fnemit_async_await_reactions_inner(",
            "fnemit_async_generator_await_with_continuation_inner(",
            "emit_async_function_execution_realm_context_from_activation(activation,function)",
        ),
        (
            "fnemit_async_generator_await_with_continuation_inner(",
            "pub(crate)fnemit_intrinsic_await_with_handlers(",
            "emit_async_generator_execution_realm_context_from_activation(activation,function)",
        ),
    ] {
        let producer = bounded(&promise, start, end);
        assert_eq!(producer.matches(factory).count(), 1);
        assert_eq!(
            producer
                .matches("PromiseResolveRealmAuthority::AsyncExecution(&realm)")
                .count(),
            1
        );
        assert!(!producer.contains("emit_current_function_realm("));
    }

    let generator = bounded(
        &promise,
        "fnemit_async_generator_await_return_reactions_inner(",
        "pub(crate)fnemit_promise_prototype_then(",
    );
    assert_eq!(
        generator
            .matches("PromiseResolveRealmAuthority::AsyncExecution(&realm)")
            .count(),
        1
    );
    let helpers = lexically_normalized(GENERATOR_REACTION_HELPERS);
    let facade = bounded(
        &helpers,
        "pub(crate)fnemit_async_generator_await_return_reactions(",
        "pub(crate)fncompile_async_generator_await_return_reactions_helper(",
    );
    assert!(facade.contains("AsyncGeneratorAwaitReturnReactionsArguments::new("));
    assert!(facade.contains("activation,value,self.current_environment(),"));
    assert!(facade.contains(".store(result,function)"));
    assert!(!facade.contains(".store(self.completion(),function)"));
    let finally = bounded(
        &finally_completion,
        "fnemit_promise_finally_continuation(",
        "pub(crate)fnemit_promise_value_thunk(",
    );
    assert_eq!(
        finally
            .matches("PromiseResolveRealmAuthority::CurrentFunction")
            .count(),
        1
    );
}
