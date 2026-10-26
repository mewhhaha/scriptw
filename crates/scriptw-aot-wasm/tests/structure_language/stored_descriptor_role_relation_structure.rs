use std::fs;
use std::path::Path;

const DEFINE_PROPERTY_SOURCE: &str = include_str!("../../src/objects/define_property.rs");
const TARGET_DESCRIPTOR_SOURCE: &str = include_str!("../../src/objects/proxy_target_descriptor.rs");
const LAYOUT_SOURCE: &str = include_str!("../../src/gc_types/layouts.rs");
const PROPERTY_VALUE_SOURCE: &str = include_str!("../../src/intrinsics/deferred_methods.rs");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/stored-descriptor-role-relation.md");

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
            exact_identifier_count(&rust_code(&source).identifiers, identifier)
        })
        .sum()
}

#[test]
fn stored_descriptor_roles_are_exact_private_wrappers() {
    let lexical_probe = rust_code(
        r###"
        // StoredDescriptorDataLocals
        StoredDescriptorDataLocals /* nested /* ignored */ comment */;
        "StoredDescriptorDataLocals"; b"StoredDescriptorDataLocals";
        c"StoredDescriptorDataLocals"; r"StoredDescriptorDataLocals";
        br##"StoredDescriptorDataLocals"##; cr#"StoredDescriptorDataLocals"#;
        'S'; b'S'; 'lifetime; r#StoredDescriptorDataLocals;
        "###,
    );
    assert_eq!(
        exact_identifier_count(&lexical_probe.identifiers, "StoredDescriptorDataLocals"),
        2
    );

    let layout = rust_code(bounded(
        LAYOUT_SOURCE,
        "struct PropertyDescriptor => PropertyDescriptorSchema {",
        "\n        }",
    ));
    assert_eq!(
        layout.normalized,
        concat!(
            "FLAGS:crate::heap::DescriptorWord,Mutable,NonNullable;",
            "VALUE:GcRef<PropertyValue>,Mutable,NonNullable;",
            "GETTER:GcRef<StoredValue>,Mutable,NonNullable;",
            "SETTER:GcRef<StoredValue>,Mutable,NonNullable;"
        )
    );
    let reader = rust_code(bounded(
        DEFINE_PROPERTY_SOURCE,
        "fn emit_descriptor_value(",
        "pub(crate) fn emit_validate_stored_descriptor(",
    ));
    assert!(reader.normalized.contains(
        "current:&GcLocal<PropertyDescriptor>,field:DescriptorField,output:&ValueLocals"
    ));
    assert!(reader.normalized.contains(concat!(
        "iffield==DescriptorField::Value{",
        "letvalue=schema.reserve_gc_local(function).initialize(",
        "schema.field(PropertyDescriptorSchema::VALUE).read(current,schema,function).reference(),function,);",
        "self.emit_property_value_into(&value,output,function);value.clear(function);return;"
    )));
    for (role, field) in [("Get", "GETTER"), ("Set", "SETTER")] {
        assert!(reader.normalized.contains(&format!(
            "DescriptorField::{role}=>PropertyDescriptorSchema::{field}"
        )));
    }
    assert!(reader.normalized.contains(
        "schema.struct_type::<StoredValue>().read_into(&stored,output,schema,function);"
    ));
    let resolver = rust_code(bounded(
        PROPERTY_VALUE_SOURCE,
        "pub(crate) fn emit_property_value_into(",
        ") {",
    ));
    assert!(resolver
        .normalized
        .contains("&self,value:&GcLocal<PropertyValue>,out:&ValueLocals,f:&mutFunction,"));
    assert!(!reader.normalized.contains("_=>"));
}

#[test]
fn stored_descriptor_read_and_compatibility_owners_have_a_closed_source_census() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_identifier_in_rust_sources(&source_root, "emit_descriptor_value"),
        7,
        "one reader, two compatibility sites, two merge accessor fields, Arguments value and Namespace value"
    );
    assert_eq!(
        count_identifier_in_rust_sources(&source_root, "emit_validate_stored_descriptor"),
        10,
        "one complete-record consumer and nine ordinary/exotic/Proxy callers"
    );
    let signature = bounded(
        DEFINE_PROPERTY_SOURCE,
        "pub(crate) fn emit_validate_stored_descriptor(",
        ") -> Result<(), EmitError>",
    );
    assert!(signature.contains("current: &GcLocal<PropertyDescriptor>"));
    assert!(signature.contains("incoming: &WasmDescriptor<'_>"));
    assert!(
        !signature.contains("ValueLocals"),
        "callers cannot transpose separate stored fields"
    );
}

#[test]
fn complete_record_readers_bind_every_stored_role_to_its_named_field() {
    let compatibility = rust_code(bounded(
        DEFINE_PROPERTY_SOURCE,
        "pub(crate) fn emit_validate_stored_descriptor(",
        "fn emit_descriptor_reject_if_true(",
    ));
    assert!(compatibility.normalized.contains(
        "(DescriptorField::Get,&descriptor.get),(DescriptorField::Set,&descriptor.set),"
    ));
    assert!(compatibility.normalized.contains(
        "self.emit_descriptor_value(current,DescriptorField::Value,&current_value,function);"
    ));
    assert_eq!(
        compatibility
            .normalized
            .matches(
                "self.emit_tagged_payload_same_value_i32(incoming_value,&current_value,function)?;"
            )
            .count(),
        2
    );
    let merge = rust_code(bounded(
        DEFINE_PROPERTY_SOURCE,
        "fn emit_merge_property_descriptor(",
        "    pub(crate) fn ",
    ));
    assert!(merge.normalized.contains(concat!(
        "letvalue=schema.reserve_gc_local(function).initialize(",
        "schema.stored_undefined(function).upcast::<crate::gc_types::PropertyValue>(),function"
    )));
    assert_eq!(
        merge
            .normalized
            .matches(concat!(
                "value.replace(schema.field(PropertyDescriptorSchema::VALUE)",
                ".read(&record,schema,function).reference(),function"
            ))
            .count(),
        1
    );
    assert!(merge
        .normalized
        .contains("value.replace(schema.stored_undefined(function).upcast(),function);"));
    assert!(merge.normalized.contains(concat!(
        "ifletSome(input)=descriptor.value.value(){",
        "emit_presence(&descriptor.value,function);",
        "self.open_frame(ControlFrameKind::If,function);",
        "value.replace(schema.struct_type::<StoredValue>().from_value(input,function).upcast(),function"
    )));
    assert_eq!(
        merge
            .normalized
            .matches("GcOperand::reference(&value,schema)")
            .count(),
        2,
        "both resulting descriptor kinds retain the same typed value edge"
    );
    assert!(!merge.normalized.contains("emit_property_value_into"));
    assert!(!merge.normalized.contains("DescriptorField::Value"));
    for (role, local) in [("Get", "getter"), ("Set", "setter")] {
        assert_eq!(
            merge
                .normalized
                .matches(&format!(
                    "self.emit_descriptor_value(&record,DescriptorField::{role},&{local},function);"
                ))
                .count(),
            1
        );
    }
    let value_reader = rust_code(bounded(
        TARGET_DESCRIPTOR_SOURCE,
        "fn read_value(",
        "fn read_getter(",
    ));
    assert!(value_reader.normalized.contains(
        "schema.field(PropertyDescriptorSchema::VALUE).read(&self.descriptor,schema,function).reference()"
    ));
    assert_eq!(
        value_reader
            .normalized
            .matches("builder.emit_property_value_into(&value,out,function);")
            .count(),
        1
    );
    assert!(!value_reader.normalized.contains("self.read_field("));
    let accessor_reader = rust_code(bounded(
        TARGET_DESCRIPTOR_SOURCE,
        "fn read_field(",
        "fn read_value(",
    ));
    assert!(accessor_reader.normalized.contains(concat!(
        "field:crate::gc_types::GcField<PropertyDescriptor,",
        "crate::gc_types::GcRef<StoredValue>,crate::gc_types::Mutable,",
        "crate::gc_types::NonNullable,>"
    )));
    assert!(accessor_reader.normalized.contains(
        "schema.struct_type::<StoredValue>().read_into(&stored,result,schema,function);"
    ));
    assert!(!accessor_reader
        .normalized
        .contains("emit_property_value_into"));
    for (method, next, field) in [
        ("read_getter", "read_setter", "GETTER"),
        ("read_setter", "emit_compatibility", "SETTER"),
    ] {
        let reader = rust_code(bounded(
            TARGET_DESCRIPTOR_SOURCE,
            &format!("fn {method}("),
            &format!("fn {next}("),
        ));
        assert_eq!(
            reader
                .normalized
                .matches(&format!(
                    "self.read_field(PropertyDescriptorSchema::{field},out,schema,function);"
                ))
                .count(),
            1
        );
    }
}

#[test]
fn contract_records_the_role_relation() {
    for marker in [
        "stored descriptor role relation",
        "cannot transpose data, getter, and setter locals",
        "stored_descriptor_role_relation_structure",
    ] {
        assert!(CONTRACT.contains(marker), "contract marker `{marker}`");
        assert!(CONTRACT.contains(marker), "task marker `{marker}`");
    }
}
