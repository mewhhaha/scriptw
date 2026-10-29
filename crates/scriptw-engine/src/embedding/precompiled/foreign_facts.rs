//! Private binary execution facts; declarations grant no native-code provenance.
//! The enclosing precompiled-image magic owns this codec's version.
use scriptw_ir::foreign::{
    ForeignErrorTag, ForeignFunctionDeclaration, ForeignFunctionMode, ForeignModuleDeclaration,
    ForeignModuleRegistry, ForeignResultType, ForeignValueType, FOREIGN_ABI_VERSION,
};

pub(super) const MAX_BYTES: usize = 1024 * 1024;
const HEADER_BYTES: usize = 4 + 1 + 4;
const MIN_TEXT_BYTES: usize = 4 + 1;
const MIN_FUNCTION_BYTES: usize = 3 * MIN_TEXT_BYTES + 1 + 4 + 1;
const MIN_MODULE_BYTES: usize = MIN_TEXT_BYTES + 4 + MIN_FUNCTION_BYTES;

fn add_size(length: &mut usize, additional: usize) -> Option<()> {
    *length = length.checked_add(additional)?;
    (*length <= MAX_BYTES).then_some(())
}

fn add_text_size(length: &mut usize, text: &str) -> Option<()> {
    u32::try_from(text.len()).ok()?;
    add_size(length, 4)?;
    add_size(length, text.len())
}

fn encoded_size(registry: &ForeignModuleRegistry) -> Option<usize> {
    let mut length = HEADER_BYTES;
    u32::try_from(registry.modules().len()).ok()?;
    for module in registry.modules() {
        add_text_size(&mut length, module.specifier())?;
        u32::try_from(module.functions().len()).ok()?;
        add_size(&mut length, 4)?;
        for function in module.functions() {
            add_text_size(&mut length, function.js_export())?;
            add_text_size(&mut length, function.import_module())?;
            add_text_size(&mut length, function.import_name())?;
            u32::try_from(function.parameters().len()).ok()?;
            add_size(&mut length, 1 + 4 + 1)?;
            add_size(&mut length, function.parameters().len())?;
        }
    }
    Some(length)
}

fn append_count(bytes: &mut Vec<u8>, count: usize) -> Option<()> {
    bytes.extend_from_slice(&u32::try_from(count).ok()?.to_le_bytes());
    Some(())
}

fn append_text(bytes: &mut Vec<u8>, text: &str) -> Option<()> {
    append_count(bytes, text.len())?;
    bytes.extend_from_slice(text.as_bytes());
    Some(())
}

pub(super) fn encode(registry: &ForeignModuleRegistry) -> Option<Vec<u8>> {
    // Check every count and the complete byte budget before allocating output.
    let mut bytes = Vec::with_capacity(encoded_size(registry)?);
    bytes.extend_from_slice(&FOREIGN_ABI_VERSION.to_le_bytes());
    bytes.push(u8::from(registry.error_tag().is_some()));
    append_count(&mut bytes, registry.modules().len())?;
    for module in registry.modules() {
        append_text(&mut bytes, module.specifier())?;
        append_count(&mut bytes, module.functions().len())?;
        for function in module.functions() {
            append_text(&mut bytes, function.js_export())?;
            append_text(&mut bytes, function.import_module())?;
            append_text(&mut bytes, function.import_name())?;
            bytes.push(match function.mode() {
                ForeignFunctionMode::Synchronous => 0,
                ForeignFunctionMode::Asynchronous => 1,
            });
            append_count(&mut bytes, function.parameters().len())?;
            for parameter in function.parameters() {
                bytes.push(match parameter {
                    ForeignValueType::Number => 0,
                    ForeignValueType::StringUtf16 => 1,
                    ForeignValueType::Bytes => 2,
                });
            }
            bytes.push(match function.result() {
                ForeignResultType::Number => 0,
                ForeignResultType::StringUtf16 => 1,
                ForeignResultType::Bytes => 2,
                ForeignResultType::Void => 3,
            });
        }
    }
    Some(bytes)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let (bytes, rest) = self.0.split_at_checked(length)?;
        self.0 = rest;
        Some(bytes)
    }

    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn word(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn count(&mut self, minimum_item_bytes: usize) -> Option<usize> {
        let count = usize::try_from(self.word()?).ok()?;
        // Caller reserves only after this gate. A forged u32 count cannot
        // cause allocation without enough remaining bytes for every item.
        (count <= self.0.len() / minimum_item_bytes).then_some(count)
    }

    fn text(&mut self) -> Option<&'a str> {
        let length = usize::try_from(self.word()?).ok()?;
        std::str::from_utf8(self.take(length)?).ok()
    }
}

pub(super) fn decode(bytes: &[u8]) -> Option<ForeignModuleRegistry> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let mut reader = Reader(bytes);
    if reader.word()? != FOREIGN_ABI_VERSION {
        return None;
    }
    let error_tag = match reader.byte()? {
        0 => None,
        1 => Some(ForeignErrorTag::code_v1()),
        _ => return None,
    };
    let count = reader.count(MIN_MODULE_BYTES)?;
    let mut modules: Vec<ForeignModuleDeclaration> = Vec::with_capacity(count);
    for _ in 0..count {
        let specifier = reader.text()?;
        if modules
            .last()
            .is_some_and(|previous| previous.specifier().as_bytes() >= specifier.as_bytes())
        {
            return None;
        }
        let count = reader.count(MIN_FUNCTION_BYTES)?;
        if count == 0 {
            return None;
        }
        let mut functions: Vec<ForeignFunctionDeclaration> = Vec::with_capacity(count);
        for _ in 0..count {
            let export = reader.text()?;
            if functions
                .last()
                .is_some_and(|previous| previous.js_export().as_bytes() >= export.as_bytes())
            {
                return None;
            }
            let import_module = reader.text()?;
            let import_name = reader.text()?;
            let mode = match reader.byte()? {
                0 => ForeignFunctionMode::Synchronous,
                1 => ForeignFunctionMode::Asynchronous,
                _ => return None,
            };
            let count = reader.count(1)?;
            let tags = reader.take(count)?;
            let result = match reader.byte()? {
                0 => ForeignResultType::Number,
                1 => ForeignResultType::StringUtf16,
                2 => ForeignResultType::Bytes,
                3 => ForeignResultType::Void,
                _ => return None,
            };
            let mut parameters = Vec::with_capacity(tags.len());
            for &tag in tags {
                parameters.push(match tag {
                    0 => ForeignValueType::Number,
                    1 => ForeignValueType::StringUtf16,
                    2 => ForeignValueType::Bytes,
                    _ => return None,
                });
            }
            let function = match mode {
                ForeignFunctionMode::Synchronous => ForeignFunctionDeclaration::new(
                    export,
                    import_module,
                    import_name,
                    parameters,
                    result,
                ),
                ForeignFunctionMode::Asynchronous => ForeignFunctionDeclaration::new_async(
                    export,
                    import_module,
                    import_name,
                    parameters,
                    result,
                ),
            }
            .ok()?;
            functions.push(function);
        }
        modules.push(ForeignModuleDeclaration::new(specifier, functions).ok()?);
    }
    if !reader.0.is_empty() {
        return None;
    }
    // Constructors retain the declaration and global import-uniqueness rules.
    // Strict input ordering above prevents their normalization from admitting
    // alternate wire encodings or changing canonical ForeignFunctionId ranks.
    let registry = ForeignModuleRegistry::new(modules).ok()?;
    Some(match error_tag {
        Some(tag) => registry.with_error_tag(tag),
        None => registry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(
        export: &str,
        import_name: &str,
        mode: ForeignFunctionMode,
        parameters: Vec<ForeignValueType>,
        result: ForeignResultType,
    ) -> ForeignFunctionDeclaration {
        match mode {
            ForeignFunctionMode::Synchronous => {
                ForeignFunctionDeclaration::new(export, "m", import_name, parameters, result)
            }
            ForeignFunctionMode::Asynchronous => {
                ForeignFunctionDeclaration::new_async(export, "m", import_name, parameters, result)
            }
        }
        .unwrap()
    }

    fn numbered(export: &str, import_name: &str) -> ForeignFunctionDeclaration {
        function(
            export,
            import_name,
            ForeignFunctionMode::Synchronous,
            vec![ForeignValueType::Number],
            ForeignResultType::Number,
        )
    }

    fn registry(modules: Vec<(&str, Vec<ForeignFunctionDeclaration>)>) -> ForeignModuleRegistry {
        ForeignModuleRegistry::new(
            modules
                .into_iter()
                .map(|(specifier, functions)| {
                    ForeignModuleDeclaration::new(specifier, functions).unwrap()
                })
                .collect(),
        )
        .unwrap()
    }

    fn replace_text(bytes: &[u8], word: usize, text: &[u8]) -> Vec<u8> {
        let length = u32::from_le_bytes(bytes[word..word + 4].try_into().unwrap()) as usize;
        let mut changed = bytes[..word].to_vec();
        changed.extend_from_slice(&(text.len() as u32).to_le_bytes());
        changed.extend_from_slice(text);
        changed.extend_from_slice(&bytes[word + 4 + length..]);
        changed
    }

    fn single_wire() -> Vec<u8> {
        let encoded = encode(&registry(vec![("s", vec![numbered("e", "n")])])).unwrap();
        // Stable field offsets for malformed-input checks below: text words at
        // 9/18/23/28, mode 33, parameter count 34, parameter 38 and result 39.
        assert_eq!(encoded, b"\x02\0\0\0\0\x01\0\0\0\x01\0\0\0s\x01\0\0\0\x01\0\0\0e\x01\0\0\0m\x01\0\0\0n\0\x01\0\0\0\0\0");
        encoded
    }

    #[test]
    fn complete_function_vocabulary_and_error_authority_roundtrip() {
        for mode in [
            ForeignFunctionMode::Synchronous,
            ForeignFunctionMode::Asynchronous,
        ] {
            for parameters in [
                vec![],
                vec![ForeignValueType::Number],
                vec![ForeignValueType::StringUtf16],
                vec![ForeignValueType::Bytes],
                vec![
                    ForeignValueType::Number,
                    ForeignValueType::StringUtf16,
                    ForeignValueType::Bytes,
                ],
                vec![
                    ForeignValueType::Bytes,
                    ForeignValueType::StringUtf16,
                    ForeignValueType::Number,
                    ForeignValueType::Number,
                ],
            ] {
                for result in [
                    ForeignResultType::Number,
                    ForeignResultType::StringUtf16,
                    ForeignResultType::Bytes,
                    ForeignResultType::Void,
                ] {
                    for error_tag in [false, true] {
                        let mut expected = registry(vec![(
                            "fixture",
                            vec![function(
                                "exchange",
                                "exchange",
                                mode,
                                parameters.clone(),
                                result,
                            )],
                        )]);
                        if error_tag {
                            expected = expected.with_error_tag(ForeignErrorTag::code_v1());
                        }
                        let encoded = encode(&expected).unwrap();
                        assert_eq!(decode(&encoded), Some(expected));
                    }
                }
            }
        }
        for error_tag in [false, true] {
            let mut expected = ForeignModuleRegistry::default();
            if error_tag {
                expected = expected.with_error_tag(ForeignErrorTag::code_v1());
            }
            let encoded = encode(&expected).unwrap();
            assert_eq!(encoded.len(), HEADER_BYTES);
            assert_eq!(decode(&encoded), Some(expected));
        }
    }

    #[test]
    fn exact_binary_schema_preserves_unicode_order_ids_and_owned_lifetimes() {
        let expected = registry(vec![(
            "α",
            vec![ForeignFunctionDeclaration::new_async(
                "f",
                "host",
                "go",
                vec![
                    ForeignValueType::Number,
                    ForeignValueType::StringUtf16,
                    ForeignValueType::Bytes,
                ],
                ForeignResultType::Void,
            )
            .unwrap()],
        )])
        .with_error_tag(ForeignErrorTag::code_v1());
        assert_eq!(encode(&expected).unwrap(), b"\x02\0\0\0\x01\x01\0\0\0\x02\0\0\0\xce\xb1\x01\0\0\0\x01\0\0\0f\x04\0\0\0host\x02\0\0\0go\x01\x03\0\0\0\x00\x01\x02\x03");

        let expected = registry(vec![
            ("𐀀:host", vec![numbered("z", "z")]),
            ("é:host", vec![numbered("β", "beta"), numbered("a", "a")]),
            ("a:host", vec![numbered("first", "first")]),
        ]);
        let names: Vec<_> = expected
            .functions()
            .map(|(id, function)| (id.index(), function.js_export().to_owned()))
            .collect();
        assert_eq!(
            names,
            vec![
                (0, "first".into()),
                (1, "a".into()),
                (2, "β".into()),
                (3, "z".into())
            ]
        );
        let encoded = encode(&expected).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, expected);
        drop(encoded);
        drop(expected);
        assert_eq!(
            decoded
                .functions()
                .map(|(id, function)| (id.index(), function.js_export().to_owned()))
                .collect::<Vec<_>>(),
            names
        );
    }

    #[test]
    fn every_truncation_trailing_data_and_unknown_tag_reject() {
        let encoded = encode(&registry(vec![
            ("a:host", vec![numbered("a", "a"), numbered("β", "beta")]),
            ("𐀀:host", vec![numbered("z", "z")]),
        ]))
        .unwrap();
        for end in 0..encoded.len() {
            assert!(decode(&encoded[..end]).is_none(), "truncated at {end}");
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode(&trailing).is_none());
        for (offset, value) in [
            (0, 0),
            (0, 3),
            (4, 2),
            (4, 255),
            (33, 2),
            (33, 255),
            (38, 3),
            (38, 255),
            (39, 4),
            (39, 255),
        ] {
            let mut changed = single_wire();
            changed[offset] = value;
            assert!(
                decode(&changed).is_none(),
                "unknown tag at {offset}: {value}"
            );
        }
    }

    #[test]
    fn malformed_text_and_reserved_namespaces_cannot_mint_declarations() {
        let encoded = single_wire();
        for word in [9, 18, 23, 28] {
            for text in [b"".as_slice(), b"\0", b"\xff"] {
                assert!(decode(&replace_text(&encoded, word, text)).is_none());
            }
        }
        for namespace in ["scriptw_runtime", "scriptw_foreign", "scriptw_host"] {
            assert!(decode(&replace_text(&encoded, 23, namespace.as_bytes())).is_none());
        }
        let mut empty_module = encoded[..18].to_vec();
        empty_module[14..18].copy_from_slice(&0u32.to_le_bytes());
        assert!(decode(&empty_module).is_none());
    }

    #[test]
    fn alternate_order_duplicates_and_conflicting_imports_reject() {
        let a = encode(&registry(vec![("a", vec![numbered("a", "a")])])).unwrap();
        let b = encode(&registry(vec![("b", vec![numbered("b", "b")])])).unwrap();
        let mut header = a[..HEADER_BYTES].to_vec();
        header[5..9].copy_from_slice(&2u32.to_le_bytes());
        let mut swapped = header.clone();
        swapped.extend_from_slice(&b[HEADER_BYTES..]);
        swapped.extend_from_slice(&a[HEADER_BYTES..]);
        assert!(decode(&swapped).is_none());
        let mut duplicate = header.clone();
        duplicate.extend_from_slice(&a[HEADER_BYTES..]);
        duplicate.extend_from_slice(&a[HEADER_BYTES..]);
        assert!(decode(&duplicate).is_none());
        let mut conflicting = header;
        conflicting.extend_from_slice(&a[HEADER_BYTES..]);
        let mut other = b[HEADER_BYTES..].to_vec();
        // Same Wasm import m/a in two differently named modules/exports.
        other[32 - HEADER_BYTES] = b'a';
        conflicting.extend_from_slice(&other);
        assert!(decode(&conflicting).is_none());

        let both = encode(&registry(vec![(
            "s",
            vec![numbered("a", "a"), numbered("b", "b")],
        )]))
        .unwrap();
        let record_bytes = a.len() - 18;
        let mut swapped = both[..18].to_vec();
        swapped.extend_from_slice(&both[18 + record_bytes..]);
        swapped.extend_from_slice(&both[18..18 + record_bytes]);
        assert!(decode(&swapped).is_none());
        let mut duplicate = both.clone();
        duplicate[18 + record_bytes + 4] = b'a';
        assert!(decode(&duplicate).is_none());
        let mut conflicting = both;
        conflicting[18 + record_bytes + 14] = b'a';
        assert!(decode(&conflicting).is_none());
    }

    #[test]
    fn forged_counts_and_lengths_reject_within_the_wire_budget() {
        for word in [5, 9, 14, 18, 23, 28, 34] {
            let mut changed = single_wire();
            changed[word..word + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(decode(&changed).is_none(), "forged count/length at {word}");
        }
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_none());
        let exact = registry(vec![(
            &"s".repeat(MAX_BYTES - 39),
            vec![numbered("e", "n")],
        )]);
        let encoded = encode(&exact).unwrap();
        assert_eq!(encoded.len(), MAX_BYTES);
        assert_eq!(decode(&encoded), Some(exact));
        let excessive = registry(vec![(
            &"s".repeat(MAX_BYTES - 38),
            vec![numbered("e", "n")],
        )]);
        assert!(encode(&excessive).is_none());
    }
}
