use std::fs;
use std::path::{Path, PathBuf};

const OPERATIONS_SOURCE: &str = include_str!("../../src/operations.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/conversion-abrupt-route-capabilities.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after: {start}"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn rust_sources(path: &Path, sources: &mut Vec<(PathBuf, String)>) {
    let mut entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .collect::<Vec<_>>();
    entries.sort();

    for entry in entries {
        if entry.is_dir() {
            rust_sources(&entry, sources);
        } else if entry.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let source = fs::read_to_string(&entry)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", entry.display()));
            sources.push((entry, source));
        }
    }
}

#[test]
fn conversion_routes_keep_exact_nonduplicable_domains_and_complete_outputs() {
    for (route, end) in [(
        "ToPrimitiveAbruptRoute",
        "/// A primitive whose ToPrimitive",
    )] {
        let declaration = bounded(
            OPERATIONS_SOURCE,
            &format!("pub(crate) enum {route} {{"),
            end,
        );
        assert_eq!(
            normalized(declaration),
            "ActiveHandler,ReturnCurrentFunction,}"
        );
        let prefix = OPERATIONS_SOURCE
            .split_once(&format!("pub(crate) enum {route} {{"))
            .unwrap()
            .0;
        assert!(!prefix.rsplit("\n\n").next().unwrap().contains("#[derive("));
    }
    let mut sources = Vec::new();
    rust_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    );
    for route in ["ToPrimitiveAbruptRoute", "PrimitiveToStringAbruptRoute"] {
        for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq"] {
            assert!(sources
                .iter()
                .all(|(_, source)| !source.contains(&format!("impl {capability} for {route}"))));
        }
    }
    assert!(sources
        .iter()
        .all(|(_, source)| !source.contains("enum ToLengthAbruptRoute")));
    let length = normalized(bounded(
        OPERATIONS_SOURCE,
        "pub(crate) fn emit_to_length_i64_from_value_locals(",
        "pub(crate) fn emit_to_length_i64_from_number_payload_local(",
    ));
    assert!(length.contains("result:&CompletionLocals,"));
    let convert = length
        .find("self.emit_value_to_number_payload(input,result,function)?;")
        .unwrap();
    let normal = length.find("CompletionKind::Normal.code()").unwrap();
    let publish = length
        .find("self.emit_to_length_i64_from_number_payload_local(")
        .unwrap();
    assert!(convert < normal && normal < publish);
    assert!(!length.contains("emit_propagate_current_throw"));
    assert!(!length.contains("emit_return_current_completion"));
}

#[test]
fn conversion_finishers_and_native_consumers_keep_their_abrupt_obligations() {
    let primitive = normalized(bounded(
        OPERATIONS_SOURCE,
        "fn finish_to_primitive_operation(",
        "pub(crate) fn emit_construct(",
    ));
    assert!(primitive.contains("route:ToPrimitiveAbruptRoute,"));
    assert!(primitive.contains("result:&crate::gc_types::CompletionLocals,"));
    assert!(primitive.contains("self.completion().copy_from(result,function);matchroute{"));
    assert!(primitive.contains(
        "ToPrimitiveAbruptRoute::ActiveHandler=>self.emit_propagate_current_throw(function)"
    ));
    assert!(primitive.contains("ToPrimitiveAbruptRoute::ReturnCurrentFunction=>{self.emit_return_current_completion(function)}"));
    for consumer in [&primitive] {
        assert!(!consumer.contains("_=>"));
        assert!(!consumer.contains("unreachable!"));
    }
    let iterable = normalized(include_str!(
        "../../src/builtins/collections/iterable_algorithms.rs"
    ));
    assert!(iterable.contains("self.emit_value_to_property_key_completion(&key_value,&pending,f)?;self.emit_collection_close_abrupt(&iterator,&pending,&output,exit,f)?;"));
    let close = normalized(bounded(
        include_str!("../../src/builtins/collections.rs"),
        "fn emit_collection_close_abrupt(",
        "fn emit_collection_assert_callable(",
    ));
    assert!(close.contains("iterator:&OwnedSyncIterator,pending:&CompletionLocals,output:&CompletionLocals,exit:ControlTarget,"));
    assert!(close.contains("CompletionKind::Throw.code()"));
    assert!(close.contains("self.emit_sync_iterator_close(iterator,pending,output,f)?;self.emit_branch_to_target(exit,f);"));
    let asynchronous = normalized(include_str!("../../src/builtins/array_from_async.rs"));
    assert!(asynchronous.contains("self.emit_to_length_i64_from_value_locals(pending.value(),length,&pending,f)?;self.emit_af_reject_abrupt(&capability,&pending,exit,f)?;self.emit_af_target("));
    let reject = bounded(
        &asynchronous,
        "fnemit_af_reject_abrupt(",
        "fnemit_af_settle(",
    );
    assert!(reject.contains("capability:&GcLocal<PromiseCapability>,pending:&CompletionLocals,"));
    assert!(reject.contains("CompletionKind::Throw.code()"));
    assert!(reject.contains("self.emit_af_settle(capability,PromiseSettlement::Reject,pending.value(),f)?;self.emit_branch_to_target(exit,f);"));
    // The dated contract still records the earlier route-domain migration.
    for evidence in [CONTRACT] {
        for route in ["ToPrimitiveAbruptRoute", "ToLengthAbruptRoute"] {
            assert!(evidence.contains(route));
        }
        assert!(evidence.contains("capability"));
        assert!(evidence.contains("exhaustive"));
    }
}
