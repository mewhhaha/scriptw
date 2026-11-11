use std::fs;
use std::path::{Path, PathBuf};

const LOWERING_SOURCE: &str = include_str!("../src/lowering.rs");
const ASSIGNMENT_SOURCE: &str = include_str!("../src/lowering/assignment.rs");
fn bounded_inclusive<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_offset = source
        .find(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"));
    source[start_offset..]
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

struct NormalizedRust {
    code: String,
    identifiers: String,
    routes: String,
}

fn normalize_rust(source: &str) -> NormalizedRust {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut identifiers = String::new();
    let mut routes = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            code.push_str(&source[offset..end]);
            identifiers.push(' ');
            routes.push('L');
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
            identifiers.push(character);
            routes.push(character);
        } else {
            identifiers.push(' ');
        }
        offset += character.len_utf8();
    }
    NormalizedRust {
        code,
        identifiers,
        routes,
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

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn normalized_rust_sources(root: &Path) -> Vec<NormalizedRust> {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
        .flat_map(|entry| {
            let path = entry.expect("failed to read Rust source entry").path();
            if path.is_dir() {
                return normalized_rust_sources(&path);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return Vec::new();
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            vec![normalize_rust(&source)]
        })
        .collect()
}

#[test]
fn property_update_op_is_the_exact_private_one_shot_domain() {
    let lexical_probe = r###"
        // PropertyUpdateOp::clone
        PropertyUpdateOp /* nested /* ignored */ comment */ :: r#Logical;
        "PropertyUpdateOp"; b"PropertyUpdateOp"; c"PropertyUpdateOp";
        r"PropertyUpdateOp"; br##"PropertyUpdateOp"##; cr#"PropertyUpdateOp"#;
        'P'; b'\x50'; 'lifetime;
    "###;
    let lexical_probe = normalize_rust(lexical_probe);
    assert_eq!(
        exact_identifier_count(&lexical_probe.identifiers, "PropertyUpdateOp"),
        1
    );
    assert_eq!(
        exact_identifier_count(&lexical_probe.routes, "PropertyUpdateOp::Logical"),
        1
    );

    let preceding_item_tail = concat!(
        "            StandardBuiltinId::StringPrototypeValueOf => Some(Self::StringValueOf),\n",
        "            _ => None,\n",
        "        }\n",
        "    }\n",
        "}\n\n",
    );
    let declaration_start = LOWERING_SOURCE
        .find(preceding_item_tail)
        .map(|offset| offset + preceding_item_tail.len())
        .expect("NonGenericBuiltinMethod must precede PropertyUpdateOp");
    let declaration_end = LOWERING_SOURCE[declaration_start..]
        .find("/// The two lowering outcomes for a computed property key on a String exotic.")
        .map(|offset| declaration_start + offset)
        .expect("StringExoticComputedKey must follow PropertyUpdateOp");
    assert_eq!(
        normalize_rust(&LOWERING_SOURCE[declaration_start..declaration_end]).code,
        "enumPropertyUpdateOp{Arithmetic(ArithmeticOp),Bitwise(BitwiseOp),Logical(LogicalBinaryOp),}"
    );

    let sources = normalized_rust_sources(&source_root());
    assert_eq!(
        sources
            .iter()
            .map(|source| exact_identifier_count(&source.identifiers, "PropertyUpdateOp"))
            .sum::<usize>(),
        8,
        "the removed reachability probe was the ninth ownership mention"
    );
    let routes = sources
        .iter()
        .map(|source| source.routes.as_str())
        .collect::<String>();
    for variant in ["Arithmetic", "Bitwise", "Logical"] {
        assert_eq!(
            sources
                .iter()
                .map(|source| {
                    exact_identifier_count(&source.routes, &format!("PropertyUpdateOp::{variant}"))
                })
                .sum::<usize>(),
            2,
            "each variant must have one producer and one consumer arm"
        );
    }
    for forbidden in [
        "implPropertyUpdateOp",
        "forPropertyUpdateOp",
        "PropertyUpdateOp::clone",
        "PropertyUpdateOp::eq",
        "PropertyUpdateOp::default",
        "asPropertyUpdateOp",
    ] {
        assert!(
            !routes.contains(forbidden),
            "found forbidden route `{forbidden}`"
        );
    }
}

#[test]
fn assignment_lowering_has_the_exact_three_ordered_producers() {
    let assignment = normalize_rust(ASSIGNMENT_SOURCE);
    assert_eq!(
        assignment
            .routes
            .matches("lower_property_reference_update(")
            .count(),
        3
    );
    let producers = [
        normalize_rust(
            r#"
            PropertyAccess::Private(_) => self.lower_property_reference_update(
                access,
                PropertyUpdateOp::Arithmetic(arithmetic),
                rhs,
            ),
            "#,
        )
        .code,
        normalize_rust(
            r#"
            PropertyAccess::Super(_) | PropertyAccess::Private(_) => self
                .lower_property_reference_update(
                    access,
                    PropertyUpdateOp::Logical(logical_op),
                    rhs,
                ),
            "#,
        )
        .code,
        normalize_rust(
            r#"
            PropertyAccess::Private(_) => self.lower_property_reference_update(
                access,
                PropertyUpdateOp::Bitwise(bitwise),
                rhs,
            ),
            "#,
        )
        .code,
    ];
    let mut cursor = 0;
    for producer in producers {
        assert_eq!(assignment.code.matches(&producer).count(), 1);
        let offset = assignment.code[cursor..]
            .find(&producer)
            .unwrap_or_else(|| panic!("producer is out of order: `{producer}`"));
        cursor += offset + producer.len();
    }
}

#[test]
fn the_single_consumer_couples_reachability_operation_and_write() {
    let consumer = normalize_rust(bounded_inclusive(
        LOWERING_SOURCE,
        "fn lower_property_reference_update(",
        "    /// Pins the operands of a Reference that PutValue must not re-evaluate.",
    ));
    let expected = normalize_rust(
        r#"
        fn lower_property_reference_update(
            &mut self,
            access: &PropertyAccess,
            op: PropertyUpdateOp,
            rhs: &Expression,
        ) -> TypedExpr {
            let read = self.lower_expression(&Expression::PropertyAccess(access.clone()));
            let read_info = read.value_info();
            let base = match reference_base_of_lowered_read(read.expr) {
                Ok(base) => base,
                Err(unsupported) => return self.unsupported_expr(unsupported.feature()),
            };
            let mut record = ReferenceRecord::create(base, self.reference_strictness());
            let pins = self.pin_reference_operands(&mut record);

            let read = record.read(read_info.clone());
            let (value, shape_info, compose) = match op {
                PropertyUpdateOp::Logical(logical) => {
                    let rhs = self.lower_conditionally_reached_expression(rhs);
                    let written_info = rhs.value_info();
                    let merged = self.merge_value_infos(read_info, written_info);
                    (
                        rhs,
                        merged.clone(),
                        Composition::ShortCircuit {
                            op: logical,
                            read,
                            merged,
                        },
                    )
                }
                PropertyUpdateOp::Arithmetic(arithmetic) => {
                    let rhs = self.lower_expression(rhs);
                    let value = self.combine_arithmetic(arithmetic, read, rhs);
                    let info = value.value_info();
                    (value, info, Composition::Value)
                }
                PropertyUpdateOp::Bitwise(bitwise) => {
                    let rhs = self.lower_expression(rhs);
                    let op = match bitwise {
                        BitwiseOp::And => BitwiseBinaryOp::And,
                        BitwiseOp::Or => BitwiseBinaryOp::Or,
                        BitwiseOp::Xor => BitwiseBinaryOp::Xor,
                        BitwiseOp::Shl => BitwiseBinaryOp::Shl,
                        BitwiseOp::Shr => BitwiseBinaryOp::Shr,
                        BitwiseOp::UShr => BitwiseBinaryOp::UShr,
                    };
                    let value = self.combine_bitwise(op, read, rhs);
                    let info = value.value_info();
                    (value, info, Composition::Value)
                }
            };
            self.record_reference_write_shape(access, record.base(), shape_info);
            pins.materialize(record.write(value, compose))
        }
        "#,
    );
    assert_eq!(consumer.code, expected.code);
    assert_eq!(consumer.routes.matches("matchop{").count(), 1);
    assert_eq!(consumer.routes.matches("PropertyUpdateOp::").count(), 3);
    for forbidden in ["matches!(op", "op==", "op!=", "_=>"] {
        assert!(!consumer.routes.contains(forbidden), "found `{forbidden}`");
    }

    let sources = normalized_rust_sources(&source_root());
    assert_eq!(
        sources
            .iter()
            .map(|source| {
                source
                    .routes
                    .matches("lower_property_reference_update(")
                    .count()
            })
            .sum::<usize>(),
        4,
        "one consumer definition and three assignment producers own the route"
    );
    assert_eq!(
        sources
            .iter()
            .map(|source| {
                exact_identifier_count(&source.identifiers, "lower_property_reference_update")
            })
            .sum::<usize>(),
        4,
        "method items and indirect aliases may not create alternate routes"
    );
}
