//! A legacy `caller`/`arguments` state keeps the dictionary entry record that
//! publication found and reads its descriptor instead of looking the key up
//! again (`functions/legacy_activation/property.rs`). The global read site
//! record relies on the same heap rules. They hold only while:
//!
//! - a header whose storage is a dictionary keeps that dictionary,
//! - a property keeps its one entry record while it lives (records are
//!   constructed only by dictionary insertion; compaction moves them), and
//! - deletion, the only way a live record leaves its table, replaces the
//!   record's descriptor with one that no property owns.
//!
//! This pins every writer of the header's storage, of an entry's descriptor
//! and of a dictionary's table, and the one entry constructor. A new writer
//! fails here until the active legacy function reflection contract and the
//! global read site record are reviewed for it.
use std::path::{Path, PathBuf};

struct Source {
    path: String,
    text: String,
}

fn collect(directory: &Path, root: &Path, output: &mut Vec<Source>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<PathBuf>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let relative = path
                .strip_prefix(root)
                .expect("source below src")
                .to_string_lossy()
                .replace('\\', "/");
            output.push(Source {
                path: relative,
                text,
            });
        }
    }
}

fn sources() -> Vec<Source> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut output = Vec::new();
    collect(&root, &root, &mut output);
    assert!(output.len() > 100, "the emitter sources were found");
    output
}

fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The name of the last `fn` item declared before `offset`.
fn enclosing_function(text: &str, offset: usize) -> &str {
    let bytes = text.as_bytes();
    let mut end = offset;
    while let Some(start) = text[..end].rfind("fn ") {
        end = start;
        if start > 0 && is_identifier(bytes[start - 1]) {
            continue;
        }
        let name_start = start + 3;
        let name_end = text[name_start..]
            .bytes()
            .position(|byte| !is_identifier(byte))
            .map_or(text.len(), |length| name_start + length);
        if name_end > name_start {
            return &text[name_start..name_end];
        }
    }
    panic!("no enclosing fn before offset {offset}");
}

/// How the text after a field constant uses it: `field(X).read(` or
/// `field(X).write(`, whitespace allowed; anything else is unclassified.
fn field_use(after: &str) -> &'static str {
    let rest = after.trim_start();
    let Some(rest) = rest.strip_prefix(')') else {
        return "other";
    };
    let Some(rest) = rest.trim_start().strip_prefix('.') else {
        return "other";
    };
    let rest = rest.trim_start();
    if rest.starts_with("read(") || rest.starts_with("read_top(") {
        "read"
    } else if rest.starts_with("write(") {
        "write"
    } else {
        "other"
    }
}

/// Every non-read use of `token` as (file, enclosing fn, use), sorted.
fn non_read_uses(sources: &[Source], token: &str) -> Vec<(String, String, &'static str)> {
    let mut uses = Vec::new();
    for source in sources {
        for (offset, _) in source.text.match_indices(token) {
            let after = &source.text[offset + token.len()..];
            if after.bytes().next().is_some_and(is_identifier) {
                continue;
            }
            let kind = field_use(after);
            if kind != "read" {
                uses.push((
                    source.path.clone(),
                    enclosing_function(&source.text, offset).to_string(),
                    kind,
                ));
            }
        }
    }
    uses.sort();
    uses
}

fn expected(rows: &[(&str, &str, &'static str)]) -> Vec<(String, String, &'static str)> {
    let mut rows = rows
        .iter()
        .map(|(path, function, kind)| (path.to_string(), function.to_string(), *kind))
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[test]
fn only_known_paths_replace_a_header_storage() {
    let sources = sources();
    assert_eq!(
        non_read_uses(&sources, "OrdinaryObjectSchema::PROPERTIES"),
        expected(&[
            // Compact storage becomes a dictionary; a dictionary is returned as is.
            (
                "objects/compact_data.rs",
                "compile_ordinary_property_storage_ensure_helper",
                "write"
            ),
            // A compact storage is replaced by a compact storage one key longer.
            (
                "objects/compact_data/transition.rs",
                "emit_compact_storage_extend",
                "write"
            ),
            // Intrinsic rows become a dictionary.
            (
                "objects/intrinsic_properties.rs",
                "emit_promote_intrinsic_properties",
                "write"
            ),
            // A freshly allocated object receives its first storage.
            (
                "intrinsics/property_layout_values.rs",
                "emit_intrinsic_unscopables",
                "write"
            ),
            // Type registration tests only: the typed write and a deliberately
            // invalid raw operand.
            ("gc_types/type_registration_tests.rs", "module", "other"),
            ("gc_types/type_registration_tests.rs", "module", "write"),
            // Read-only host snapshot projection.
            ("gc_types/snapshot.rs", "index", "other"),
        ]),
        "a new writer of OrdinaryObject.PROPERTIES must keep every dictionary header's \
         dictionary; review docs/rust-rewrite/contracts/active-legacy-function-reflection.md"
    );
}

#[test]
fn only_replacement_and_deletion_change_an_entry_descriptor() {
    let sources = sources();
    assert_eq!(
        non_read_uses(&sources, "PropertyEntrySchema::DESCRIPTOR"),
        expected(&[
            // A definition through an existing key keeps the record.
            (
                "objects/property_storage.rs",
                "emit_ordinary_append_property_entry_inner",
                "write"
            ),
            // Deletion installs a fresh accessor descriptor no property owns.
            ("objects.rs", "emit_object_delete_ordinary", "write"),
            // Read-only host snapshot projection.
            ("gc_types/snapshot.rs", "index", "other"),
        ]),
        "a new writer of PropertyEntry.DESCRIPTOR must keep a deleted record's \
         descriptor ownerless; review the active legacy function reflection contract"
    );
}

#[test]
fn only_compaction_and_fresh_publication_replace_a_dictionary_table() {
    let sources = sources();
    assert_eq!(
        non_read_uses(&sources, "OrdinaryPropertyStorageSchema::ENTRIES"),
        expected(&[
            // Compaction copies the same live records and updates POSITION.
            (
                "objects/property_storage.rs",
                "compile_ordinary_property_prepare_helper",
                "write"
            ),
            // Sizes an empty dictionary once before its first rows.
            (
                "objects/fresh_publication.rs",
                "compile_fresh_data_properties_publish_helper",
                "write"
            ),
            // Read-only host snapshot projection.
            ("gc_types/snapshot.rs", "index", "other"),
        ]),
        "a new writer of OrdinaryPropertyStorage.ENTRIES must move live entry \
         records rather than rebuild them"
    );
    let compaction = sources
        .iter()
        .find(|source| source.path == "objects/property_storage.rs")
        .expect("property storage source");
    let prepare = compaction
        .text
        .split_once("pub(crate) fn compile_ordinary_property_prepare_helper(")
        .expect("compaction helper")
        .1;
    let prepare = prepare
        .split_once("\n    }\n")
        .expect("compaction helper end")
        .0;
    assert!(
        !prepare.contains("struct_type::<PropertyEntry>()"),
        "compaction moves records and never constructs one"
    );
}

/// Methods of a typed array layout that write or allocate (anything but
/// `read`, `length` and `type_index`), at a direct `array_type::<T>()` use or
/// through a `let` binding of one, as (file, enclosing fn, method), sorted.
fn array_writes(sources: &[Source], element: &str) -> Vec<(String, String, String)> {
    let token = format!("array_type::<{element}>()");
    let method_after = |text: &str| -> Option<String> {
        let rest = text.trim_start().strip_prefix('.')?.trim_start();
        let length = rest.bytes().position(|byte| !is_identifier(byte))?;
        rest[length..]
            .starts_with('(')
            .then(|| rest[..length].to_string())
    };
    let read_only = |method: &str| matches!(method, "read" | "length" | "type_index");
    let mut writes = Vec::new();
    for source in sources {
        let text = &source.text;
        for (offset, _) in text.match_indices(&token) {
            let function = enclosing_function(text, offset).to_string();
            let after = &text[offset + token.len()..];
            if let Some(method) = method_after(after) {
                if !read_only(&method) {
                    writes.push((source.path.clone(), function, method));
                }
                continue;
            }
            // `let name = <schema>.array_type::<T>();`: follow `name.` up to
            // the next `fn` item.
            let statement = &text[text[..offset].rfind(';').map_or(0, |end| end + 1)..offset];
            let name = statement
                .trim_start()
                .strip_prefix("let ")
                .and_then(|rest| rest.split_once('='))
                .map(|(name, _)| name.trim())
                .filter(|name| !name.is_empty() && name.bytes().all(is_identifier))
                .unwrap_or_else(|| {
                    panic!("unclassified {token} use in {} {function}", source.path)
                });
            let body = after
                .split_once("\n    fn ")
                .map_or(after, |(body, _)| body);
            let body = body.split_once("\n    pub").map_or(body, |(body, _)| body);
            let binding = format!("{name}.");
            for (use_offset, _) in body.match_indices(&binding) {
                if use_offset > 0 && is_identifier(body.as_bytes()[use_offset - 1]) {
                    continue;
                }
                let method = method_after(&body[use_offset + name.len()..])
                    .unwrap_or_else(|| panic!("unclassified {name} use in {}", source.path));
                if !read_only(&method) {
                    writes.push((source.path.clone(), function.clone(), method));
                }
            }
        }
    }
    writes.sort();
    writes
}

#[test]
fn only_insertion_compaction_and_deletion_write_a_dictionary_table() {
    let sources = sources();
    let rows = |rows: &[(&str, &str, &str)]| {
        let mut rows = rows
            .iter()
            .map(|(path, function, method)| {
                (path.to_string(), function.to_string(), method.to_string())
            })
            .collect::<Vec<_>>();
        rows.sort();
        rows
    };
    assert_eq!(
        array_writes(&sources, "PropertyTable"),
        rows(&[
            // Deletion nulls the record's slot after swapping its descriptor.
            ("objects.rs", "emit_object_delete_ordinary", "write"),
            // A fresh dictionary's table, filled before its first rows.
            (
                "objects/fresh_publication.rs",
                "compile_fresh_data_properties_publish_helper",
                "filled"
            ),
            (
                "objects/native_constructor.rs",
                "compile_native_constructor_header_allocate_helper",
                "filled"
            ),
            // Insertion appends a new record after a proof of absence.
            (
                "objects/property_storage.rs",
                "emit_ordinary_insert_in_dictionary",
                "write"
            ),
            // Compaction allocates a table and moves the same live records.
            (
                "objects/property_storage.rs",
                "compile_ordinary_property_prepare_helper",
                "filled"
            ),
            (
                "objects/property_storage.rs",
                "compile_ordinary_property_prepare_helper",
                "write"
            ),
        ]),
        "a new writer of a dictionary's PropertyTable must keep every live property's \
         one record reachable; review the active legacy function reflection contract"
    );
    assert_eq!(
        non_read_uses(&sources, "OrdinaryPropertyStorageSchema::INDEX"),
        expected(&[
            (
                "objects/fresh_publication.rs",
                "compile_fresh_data_properties_publish_helper",
                "write"
            ),
            (
                "objects/property_storage.rs",
                "compile_ordinary_property_prepare_helper",
                "write"
            ),
        ]),
        "a new writer of a dictionary's key index must index every live record"
    );
    assert_eq!(
        non_read_uses(&sources, "OrdinaryPropertyStorageSchema::LENGTH"),
        expected(&[
            (
                "objects/property_storage.rs",
                "emit_ordinary_insert_in_dictionary",
                "write"
            ),
            (
                "objects/property_storage.rs",
                "compile_ordinary_property_prepare_helper",
                "write"
            ),
            // Read-only host snapshot projection.
            ("gc_types/snapshot.rs", "index", "other"),
        ]),
        "a new writer of a dictionary's length must keep every live record in range"
    );
    // Raw array writes stay in the typed layout API and its generic helpers.
    for source in &sources {
        for instruction in [
            "Instruction::ArraySet",
            "Instruction::ArrayCopy",
            "Instruction::ArrayFill",
            "Instruction::ArrayInitElem",
            "Instruction::ArrayInitData",
        ] {
            if source.text.contains(instruction) {
                assert!(
                    matches!(
                        source.path.as_str(),
                        "gc_types/layouts.rs" | "gc_types/value.rs" | "gc_types/compact_shapes.rs"
                    ),
                    "raw {instruction} in {}",
                    source.path
                );
            }
        }
    }
}

#[test]
fn entry_records_are_inserted_only_after_a_proof_of_absence_or_into_a_fresh_table() {
    let sources = sources();
    let mut callers = sources
        .iter()
        .flat_map(|source| {
            source
                .text
                .match_indices(".emit_ordinary_insert_in_dictionary(")
                .map(|(offset, _)| {
                    (
                        source.path.clone(),
                        enclosing_function(&source.text, offset).to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    callers.sort();
    let mut known = [
        // Compact rows move into a fresh dictionary: their keys are distinct.
        (
            "objects/compact_data.rs",
            "compile_ordinary_property_storage_ensure_helper",
        ),
        // Fresh data rows of a new dictionary; the helper traps unless empty.
        (
            "objects/fresh_publication.rs",
            "compile_fresh_data_properties_publish_helper",
        ),
        // Intrinsic rows move into a fresh dictionary: their keys are distinct.
        (
            "objects/intrinsic_properties.rs",
            "emit_promote_intrinsic_properties",
        ),
        // A native constructor's fresh header rows.
        (
            "objects/native_constructor.rs",
            "compile_native_constructor_header_allocate_helper",
        ),
        // Append and define, after their lookup found no record for the key.
        (
            "objects/property_storage.rs",
            "emit_ordinary_insert_prepared_property",
        ),
    ]
    .map(|(path, function)| (path.to_string(), function.to_string()))
    .to_vec();
    known.sort();
    assert_eq!(
        callers, known,
        "a new caller of dictionary insertion must prove the key absent, or a property \
         gets a second record; review the active legacy function reflection contract"
    );
}

#[test]
fn entry_records_are_constructed_only_by_dictionary_insertion() {
    let sources = sources();
    let constructions = sources
        .iter()
        .flat_map(|source| {
            source
                .text
                .match_indices("struct_type::<PropertyEntry>()")
                .map(|(offset, _)| {
                    (
                        source.path.clone(),
                        enclosing_function(&source.text, offset).to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        constructions,
        vec![(
            "objects/property_storage.rs".to_string(),
            "emit_ordinary_insert_in_dictionary".to_string()
        )],
        "a property keeps its one entry record; insertion follows a proof of absence"
    );
    // Every other struct construction or field write goes through the typed
    // layout API scanned above.
    for source in &sources {
        if source.text.contains("Instruction::StructSet") {
            assert!(
                matches!(
                    source.path.as_str(),
                    "gc_types/layouts.rs" | "gc_types/type_registration_tests.rs"
                ),
                "raw struct.set in {}",
                source.path
            );
        }
        if source.text.contains("Instruction::StructNew") {
            assert!(
                matches!(
                    source.path.as_str(),
                    "gc_types/layouts.rs"
                        | "gc_types/stored_undefined.rs"
                        | "gc_types/compact_shapes.rs"
                        // A unit test's own two-type module, not the runtime schema.
                        | "native_program_template/tests/program_owned_payload.rs"
                ),
                "raw struct.new in {}",
                source.path
            );
        }
    }
}

#[test]
fn legacy_activations_read_the_published_entry_record_without_a_lookup() {
    let property = include_str!("../../src/functions/legacy_activation/property.rs");
    let publication =
        include_str!("../../src/functions/function_object_allocation/metadata_publication.rs");
    for forbidden in [
        "OrdinaryPropertyFind",
        "emit_ordinary_property_entry",
        "emit_ordinary_own_descriptor_reference",
        "call_helper",
    ] {
        assert!(
            !property.contains(forbidden),
            "an activation's legacy write performs no lookup: {forbidden}"
        );
    }
    for needle in [
        ".field(LegacyReflectionPropertyStateSchema::ENTRY)",
        ".field(PropertyEntrySchema::DESCRIPTOR)",
    ] {
        assert!(property.contains(needle), "missing {needle}");
    }
    let legacy = publication
        .split_once("(\"caller\", FunctionObjectSchema::LEGACY_CALLER)")
        .expect("legacy publication")
        .1;
    let mut rest = legacy;
    for needle in [
        "self.emit_define_function_runtime_metadata_property(",
        "self.emit_ordinary_property_entry(&object, &key, function)?",
        ".field(PropertyEntrySchema::DESCRIPTOR)",
        "GcOperand::nullable_reference(&entry, schema)",
        "GcOperand::nullable_reference(&descriptor, schema)",
    ] {
        rest = rest
            .split_once(needle)
            .unwrap_or_else(|| panic!("missing or out of order: {needle}"))
            .1;
    }
}
