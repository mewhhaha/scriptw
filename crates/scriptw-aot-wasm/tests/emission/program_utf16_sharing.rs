//! Packed passive images retain every literal's exact code-unit span.
use scriptw_front::{ParseOptions, parse};
use std::collections::BTreeSet;
use wasmparser::{DataKind, Parser, Payload};

#[test]
fn overlapping_sources_share_program_bytes_without_sharing_literal_slots() {
    const INNER: &str = "function inner() { /* nested source */ return 3; }";
    const OUTER: &str =
        "function outer() { function inner() { /* nested source */ return 3; } return inner; }";
    let source = format!(
        r#"{OUTER}
export const run = outer;
export const odd = "\u0100\u0200\u0300";
export const even = "\u0001\u0002";
export const unicode = "\u0100\u0001\u0000\u{{1F600}}\uD800";
"#
    );
    let artifact = std::thread::Builder::new()
        .name("packed-program-utf16".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let parsed = parse(&source, ParseOptions::module()).unwrap();
            let program = scriptw_ir::lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            scriptw_aot_wasm::emit(&program).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    let mut data = Vec::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::DataSection(section) = payload.unwrap() {
            for row in section {
                let row = row.unwrap();
                assert!(matches!(row.kind, DataKind::Passive));
                data.push(row.data.to_vec());
            }
        }
    }
    assert!(data.len() >= 3);
    let utf16 = &data[0];
    assert_eq!(data[2].len() % 12, 0);
    let mut values = BTreeSet::new();
    let mut slots = BTreeSet::new();
    let mut logical_bytes = 0;
    for row in data[2].chunks_exact(12) {
        let slot = u32::from_le_bytes(row[0..4].try_into().unwrap());
        let start = u32::from_le_bytes(row[4..8].try_into().unwrap()) as usize;
        let length = u32::from_le_bytes(row[8..12].try_into().unwrap()) as usize * 2;
        assert!(slots.insert(slot));
        assert_eq!(start % 2, 0);
        let bytes = &utf16[start..start + length];
        values.insert(
            bytes
                .chunks_exact(2)
                .map(|unit| u16::from_le_bytes(unit.try_into().unwrap()))
                .collect::<Vec<_>>(),
        );
        logical_bytes += length;
    }
    assert_eq!(
        slots.iter().copied().collect::<Vec<_>>(),
        (0..slots.len() as u32).collect::<Vec<_>>()
    );
    assert!(values.contains(&INNER.encode_utf16().collect::<Vec<_>>()));
    assert!(values.contains(&OUTER.encode_utf16().collect::<Vec<_>>()));
    assert!(values.contains(&vec![0x0100, 0x0200, 0x0300]));
    assert!(values.contains(&vec![0x0001, 0x0002]));
    assert!(values.contains(&vec![0x0100, 1, 0, 0xD83D, 0xDE00, 0xD800]));
    assert!(logical_bytes >= utf16.len() + OUTER.len() * 2);
}
