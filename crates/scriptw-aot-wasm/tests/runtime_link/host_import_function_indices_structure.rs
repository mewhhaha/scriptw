use std::fs;
use std::path::Path;

const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const LIBRARY_SOURCE: &str = include_str!("../../src/lib.rs");
const GC_HOST_SOURCE: &str = include_str!("../../src/gc_types/host.rs");
const EMIT_SOURCE: &str = include_str!("../../src/emit/module_assembly.rs");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/host-import-function-indices-authority.md"
);

const ROLES: [(&str, &str, &str, &str); 26] = [
    (
        "NumberPowImportFunctionIndex",
        "number_pow",
        "number_pow_import_function_index",
        "number_pow_import_function_index",
    ),
    (
        "WallClockMillisImportFunctionIndex",
        "wall_clock_millis",
        "wall_clock_millis_import_function_index",
        "wall_clock_millis_import_function_index",
    ),
    (
        "MonotonicClockNanosImportFunctionIndex",
        "monotonic_clock_nanos",
        "monotonic_clock_nanos_import_function_index",
        "monotonic_clock_nanos_import_function_index",
    ),
    (
        "SleepNanosImportFunctionIndex",
        "sleep_nanos",
        "sleep_nanos_import_function_index",
        "sleep_nanos_import_function_index",
    ),
    (
        "AgentCallImportFunctionIndex",
        "agent_call",
        "agent_call_import_function_index",
        "agent_call_import_function_index",
    ),
    (
        "RandomF64ImportFunctionIndex",
        "random_f64",
        "random_f64_import_function_index",
        "random_f64_import_function_index",
    ),
    (
        "MathAcosImportFunctionIndex",
        "math_acos",
        "math_acos_import_function_index",
        "math_acos_import_function_index",
    ),
    (
        "MathAcoshImportFunctionIndex",
        "math_acosh",
        "math_acosh_import_function_index",
        "math_acosh_import_function_index",
    ),
    (
        "MathAsinImportFunctionIndex",
        "math_asin",
        "math_asin_import_function_index",
        "math_asin_import_function_index",
    ),
    (
        "MathAsinhImportFunctionIndex",
        "math_asinh",
        "math_asinh_import_function_index",
        "math_asinh_import_function_index",
    ),
    (
        "MathAtanImportFunctionIndex",
        "math_atan",
        "math_atan_import_function_index",
        "math_atan_import_function_index",
    ),
    (
        "MathAtanhImportFunctionIndex",
        "math_atanh",
        "math_atanh_import_function_index",
        "math_atanh_import_function_index",
    ),
    (
        "MathCbrtImportFunctionIndex",
        "math_cbrt",
        "math_cbrt_import_function_index",
        "math_cbrt_import_function_index",
    ),
    (
        "MathCosImportFunctionIndex",
        "math_cos",
        "math_cos_import_function_index",
        "math_cos_import_function_index",
    ),
    (
        "MathCoshImportFunctionIndex",
        "math_cosh",
        "math_cosh_import_function_index",
        "math_cosh_import_function_index",
    ),
    (
        "MathExpImportFunctionIndex",
        "math_exp",
        "math_exp_import_function_index",
        "math_exp_import_function_index",
    ),
    (
        "MathExpm1ImportFunctionIndex",
        "math_expm1",
        "math_expm1_import_function_index",
        "math_expm1_import_function_index",
    ),
    (
        "MathLogImportFunctionIndex",
        "math_log",
        "math_log_import_function_index",
        "math_log_import_function_index",
    ),
    (
        "MathLog10ImportFunctionIndex",
        "math_log10",
        "math_log10_import_function_index",
        "math_log10_import_function_index",
    ),
    (
        "MathLog1pImportFunctionIndex",
        "math_log1p",
        "math_log1p_import_function_index",
        "math_log1p_import_function_index",
    ),
    (
        "MathLog2ImportFunctionIndex",
        "math_log2",
        "math_log2_import_function_index",
        "math_log2_import_function_index",
    ),
    (
        "MathSinImportFunctionIndex",
        "math_sin",
        "math_sin_import_function_index",
        "math_sin_import_function_index",
    ),
    (
        "MathSinhImportFunctionIndex",
        "math_sinh",
        "math_sinh_import_function_index",
        "math_sinh_import_function_index",
    ),
    (
        "MathTanImportFunctionIndex",
        "math_tan",
        "math_tan_import_function_index",
        "math_tan_import_function_index",
    ),
    (
        "MathTanhImportFunctionIndex",
        "math_tanh",
        "math_tanh_import_function_index",
        "math_tanh_import_function_index",
    ),
    (
        "MathAtan2ImportFunctionIndex",
        "math_atan2",
        "math_atan2_import_function_index",
        "math_atan2_import_function_index",
    ),
];

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn quoted_literal_end(source: &str, quote_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
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
        b'"' => quoted_literal_end(source, start),
        b'\'' => character_literal_end(source, start),
        b'b' if bytes.get(start + 1) == Some(&b'\'') => character_literal_end(source, start + 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'"') => quoted_literal_end(source, start + 1),
        b'r' => raw_literal_end(source, start, 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'r') => raw_literal_end(source, start, 2),
        _ => None,
    }
}

struct RustCode {
    normalized: String,
    identifiers: String,
}

fn rust_code(source: &str) -> RustCode {
    let bytes = source.as_bytes();
    let mut normalized = String::new();
    let mut identifiers = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            normalized.push_str(&source[offset..end]);
            identifiers.push(' ');
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
            assert_eq!(depth, 0, "unterminated block comment");
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
        if character.is_whitespace() {
            identifiers.push(' ');
        } else {
            normalized.push(character);
            identifiers.push(character);
        }
        offset += character.len_utf8();
    }
    RustCode {
        normalized,
        identifiers,
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

fn count_identifier_in_rust_sources(
    dir: &Path,
    identifier: &str,
    test_entry: &Path,
    test_subtree: &Path,
) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path == test_entry || path == test_subtree {
                return 0;
            }
            if path.is_dir() {
                return count_identifier_in_rust_sources(
                    &path,
                    identifier,
                    test_entry,
                    test_subtree,
                );
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            exact_identifier_count(&rust_code(&source).identifiers, identifier)
        })
        .sum()
}

fn count_identifier_in_product_sources(source_root: &Path, identifier: &str) -> usize {
    // This exact declaration gives tests.rs and its tests/ descendants one
    // cfg(test) owner. Keep scratch emitter probes outside the product census
    // without excluding any production directory or relaxing its role counts.
    let library = rust_code(LIBRARY_SOURCE);
    assert_eq!(
        library.normalized.matches("#[cfg(test)]modtests;").count(),
        1
    );
    assert_eq!(exact_identifier_count(&library.identifiers, "tests"), 1);
    let test_entry = source_root.join("tests.rs");
    let test_subtree = source_root.join("tests");
    assert!(test_entry.is_file());
    assert!(test_subtree.is_dir());
    count_identifier_in_rust_sources(source_root, identifier, &test_entry, &test_subtree)
}

#[test]
fn authority_has_twenty_six_scalar_roles_and_one_required_role_plus_gc_imports() {
    let lexical_probe = rust_code(
        r###"
        // HostImportFunctionIndices
        HostImportFunctionIndices /* nested /* ignored */ comment */;
        "HostImportFunctionIndices"; b"HostImportFunctionIndices";
        c"HostImportFunctionIndices"; r"HostImportFunctionIndices";
        br##"HostImportFunctionIndices"##; cr#"HostImportFunctionIndices"#;
        'H'; b'H'; 'lifetime; r#HostImportFunctionIndices;
        "###,
    );
    assert_eq!(
        exact_identifier_count(&lexical_probe.identifiers, "HostImportFunctionIndices"),
        2
    );

    let domain = rust_code(bounded(
        PLANNING_SOURCE,
        "pub(crate) struct NumberPowImportFunctionIndex(u32);",
        "/// Only the runtime schema",
    ));
    for (role, field, _, _) in ROLES {
        if role != "NumberPowImportFunctionIndex" {
            assert!(domain
                .normalized
                .contains(&format!("pub(crate)struct{role}(u32);")));
        }
        assert!(domain
            .normalized
            .contains(&format!("{field}:Option<{role}>,")));
    }
    assert!(domain
        .normalized
        .contains("pub(crate)structRejectRuntimeSemanticsImportFunctionIndex(u32);"));
    assert!(domain
        .normalized
        .contains("reject_runtime_semantics:RejectRuntimeSemanticsImportFunctionIndex,"));
    assert!(domain
        .normalized
        .contains("#[must_use]pub(crate)structHostImportFunctionIndices{"));
    for forbidden in ["derive(", "implClonefor", "implCopyfor"] {
        assert!(
            !domain.normalized.contains(forbidden),
            "found `{forbidden}`"
        );
    }
    let host = rust_code(GC_HOST_SOURCE).normalized;
    let domain = bounded(&host, "gc_host_imports!{", "}");
    assert_eq!(domain.matches("=>(").count(), 37);
    for marker in [
        "CollectGc=>(\"collect_gc\",HostCollectGc)",
        "ByteArrayAllocate=>(\"byte_array_allocate\",HostByteArrayAllocate)",
        "SharedBufferAllocate=>(\"shared_buffer_allocate\",HostSharedBufferAllocate)",
        "SharedBufferBase=>(\"shared_buffer_base\",HostSharedBufferBase)",
        "SharedBufferLength=>(\"shared_buffer_length\",HostSharedBufferLength)",
        "SharedBufferMaximum=>(\"shared_buffer_maximum\",HostSharedBufferMaximum)",
        "SharedBufferGrowable=>(\"shared_buffer_growable\",HostSharedBufferGrowable)",
        "SharedBufferGrow=>(\"shared_buffer_grow\",HostSharedBufferGrow)",
        "AgentBroadcastResource=>(\"agent_broadcast_resource\",HostAgentBroadcastResource)",
        "AgentReceiveResource=>(\"agent_receive_resource\",HostAgentReceiveResource)",
        "RegisterAsyncWaiter=>(\"register_async_waiter\",HostRegisterAsyncWaiter)",
        "NotifyAsyncWaiters=>(\"notify_async_waiters\",HostNotifyAsyncWaiters)",
        "IntlProviderCall=>(\"intl_provider_call\",HostIntlProviderCall)",
        "SystemTimeZoneSnapshot=>(\"system_time_zone_snapshot\",HostSystemTimeZoneSnapshot)",
        "SharedBufferWait=>(\"shared_buffer_wait\",HostSharedBufferWait)",
        "WeakMapAllocate=>(\"weak_map_allocate\",HostWeakMapAllocate)",
        "WeakSetAllocate=>(\"weak_set_allocate\",HostWeakSetAllocate)",
        "WeakMapSet=>(\"weak_map_set\",HostWeakMapSet)",
        "WeakMapGet=>(\"weak_map_get\",HostWeakMapGet)",
        "WeakMapHas=>(\"weak_map_has\",HostWeakMapHas)",
        "WeakMapDelete=>(\"weak_map_delete\",HostWeakMapDelete)",
        "WeakSetAdd=>(\"weak_set_add\",HostWeakSetAdd)",
        "WeakSetHas=>(\"weak_set_has\",HostWeakSetHas)",
        "WeakSetDelete=>(\"weak_set_delete\",HostWeakSetDelete)",
        "WeakRefAllocate=>(\"weak_ref_allocate\",HostWeakRefAllocate)",
        "WeakRefDeref=>(\"weak_ref_deref\",HostWeakRefDeref)",
        "WeakJobFinish=>(\"weak_job_finish\",HostWeakJobFinish)",
        "FinalizationTrack=>(\"finalization_track\",HostFinalizationTrack)",
        "FinalizationNextRegistry=>(\"finalization_next_registry\",HostFinalizationNextRegistry)",
        "FinalizationCellAllocate=>(\"finalization_cell_allocate\",HostFinalizationCellAllocate)",
        "FinalizationTargetAlive=>(\"finalization_target_alive\",HostFinalizationTargetAlive)",
        "FinalizationTokenMatches=>(\"finalization_token_matches\",HostFinalizationTokenMatches)",
        "FinalizationCellRemove=>(\"finalization_cell_remove\",HostFinalizationCellRemove)",
        "ReportCleanupException=>(\"report_cleanup_exception\",HostReportCleanupException)",
        "CompileDynamicFunction=>(\"compile_dynamic_function\",HostCompileDynamicFunction)",
        "CompileGlobalScript=>(\"compile_global_script\",HostCompileGlobalScript)",
        "CompileDirectEval=>(\"compile_direct_eval\",HostCompileDirectEval)",
    ] {
        assert!(domain.contains(marker), "GC import {marker}");
    }
    assert!(host
        .contains("pub(crate)structDeclaredGcHostImport{import:GcHostImport,function_index:u32,}"));
    assert!(host.contains("forimportinGcHostImport::ALL{"));
    let plan = bounded(&host, "pub(crate)fnplan(", "pub(crate)fnget(");
    assert!(!plan.contains("_=>"));
    assert!(plan.contains("declarations.push(DeclaredGcHostImport{import:*import,function_index:*next_function_index,})"));
    assert!(plan.contains(".checked_add(1)"));
    assert!(host.contains(".find(|entry|entry.import==import)"));
    assert!(host.contains("EntityType::Function(entry.import.signature().type_index())"));
    for retired in [
        "SharedMemoryAllocImportFunctionIndex",
        "IntlCallImportFunctionIndex",
        "SystemTimeZoneImportFunctionIndex",
    ] {
        assert_eq!(
            count_identifier_in_product_sources(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
                retired
            ),
            0
        );
    }
}

#[test]
fn role_and_authority_census_is_closed_over_product_sources() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_identifier_in_product_sources(&source_root, "HostImportFunctionIndices"),
        5
    );
    assert_eq!(
        count_identifier_in_product_sources(
            &source_root,
            "RejectRuntimeSemanticsImportFunctionIndex"
        ),
        5,
    );
    for (role, _, _, _) in ROLES {
        assert_eq!(
            count_identifier_in_product_sources(&source_root, role),
            5,
            "{role} census"
        );
    }
}

#[test]
fn sole_producer_builds_every_typed_role_and_registry_stores_authority_intact() {
    assert_eq!(
        EMIT_SOURCE
            .matches("HostImportFunctionIndices::new(")
            .count(),
        1
    );
    let producer = rust_code(bounded(
        EMIT_SOURCE,
        "let host_import_function_indices = HostImportFunctionIndices::new(",
        "    let source = SourceFunctions::partition(script);",
    ));
    for (role, _, source_variable, _) in ROLES {
        assert_eq!(
            producer
                .normalized
                .matches(&format!("{source_variable}.map({role}::new)"))
                .count(),
            1,
            "{role} producer"
        );
    }

    assert_eq!(
        producer.normalized.replace(",)", ")").matches(
            "RejectRuntimeSemanticsImportFunctionIndex::new(reject_runtime_semantics_import_function_index)"
        ).count(),
        1,
    );

    assert_eq!(
        producer
            .normalized
            .matches(".with_gc_imports(gc_host_imports)")
            .count(),
        1
    );
    let registry = rust_code(bounded(
        PLANNING_SOURCE,
        "pub(crate) struct FunctionMetaRegistry {",
        "    pub(crate) fn number_pow_import_function_index(&self) -> Option<u32> {",
    ));
    assert!(registry
        .normalized
        .contains("host_import_function_indices:HostImportFunctionIndices,"));
    let constructor = bounded(&registry.normalized, "pub(crate)fnnew(", ")->Self{");
    assert!(constructor.contains("host_import_function_indices:HostImportFunctionIndices,"));
    assert!(!constructor.contains("Option<u32>"));
    assert_eq!(EMIT_SOURCE.matches("FunctionMetaRegistry::new(").count(), 1);
}

#[test]
fn named_registry_getters_are_the_only_raw_index_projections() {
    let planning = rust_code(PLANNING_SOURCE);
    let getters = rust_code(bounded(
        PLANNING_SOURCE,
        "    pub(crate) fn number_pow_import_function_index(&self) -> Option<u32> {",
        "    pub(crate) fn get(&self, function_id:",
    ));
    for (_, field, _, getter) in ROLES {
        assert_eq!(
            planning
                .normalized
                .matches(&format!("pub(crate)fn{getter}(&self)->Option<u32>{{"))
                .count(),
            1,
            "{getter} declaration"
        );
        assert_eq!(
            getters
                .normalized
                .matches(&format!(
                    "self.host_import_function_indices.{field}.as_ref().map(|index|index.0)"
                ))
                .count(),
            1,
            "{getter} projection"
        );
    }
    assert_eq!(
        getters.normalized.matches("map(|index|index.0)").count(),
        26
    );
    assert_eq!(
        getters
            .normalized
            .matches("self.host_import_function_indices.reject_runtime_semantics.0")
            .count(),
        1
    );
    assert_eq!(
        planning
            .normalized
            .matches("pub(crate)fnreject_runtime_semantics_import_function_index(&self)->u32{")
            .count(),
        1
    );
}

#[test]
fn contract_and_t02_own_the_authority() {
    for marker in [
        "HostImportFunctionIndices",
        "host_import_function_indices_structure",
    ] {
        assert!(CONTRACT.contains(marker), "contract marker `{marker}`");
        assert!(CONTRACT.contains(marker), "task marker `{marker}`");
    }
    assert!(CONTRACT.contains("Transposing two values compiled"));
    assert!(CONTRACT.contains("transpose two raw `Option<u32>` positions"));
}
