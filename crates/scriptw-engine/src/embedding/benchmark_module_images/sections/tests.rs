//! Inert ELF layout fixtures only; no fixture is loaded or executed as code.
use super::*;

const TABLE: usize = 0x200;
const NAMES: &[u8] = b"\0.text\0.shstrtab\0.data\0";
fn put16(raw: &mut [u8], at: usize, value: u16) {
    raw[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(raw: &mut [u8], at: usize, value: u32) {
    raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(raw: &mut [u8], at: usize, value: u64) {
    raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn row(raw: &mut [u8], index: usize, name: u32, kind: u32, flags: u64, offset: u64, bytes: u64) {
    let at = TABLE + index * 64;
    put32(raw, at, name);
    put32(raw, at + 4, kind);
    put64(raw, at + 8, flags);
    put64(raw, at + 24, offset);
    put64(raw, at + 32, bytes);
    put64(raw, at + 48, 1);
}
fn elf() -> Vec<u8> {
    let mut raw = vec![0; 0x300];
    raw[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut raw, 16, 1);
    put16(&mut raw, 18, 62);
    put32(&mut raw, 20, 1);
    put64(&mut raw, 40, TABLE as u64);
    put16(&mut raw, 52, 64);
    put16(&mut raw, 58, 64);
    put16(&mut raw, 60, 4);
    put16(&mut raw, 62, 2);
    raw[0x140..0x140 + NAMES.len()].copy_from_slice(NAMES);
    row(&mut raw, 1, 1, 1, 6, 0x100, 0x20);
    row(&mut raw, 2, 7, 3, 0, 0x140, NAMES.len() as u64);
    row(&mut raw, 3, 17, 1, 2, 0x180, 8);
    raw
}
fn rejected(raw: &[u8]) {
    assert!(parse(raw, 0x100..0x120).is_err());
}

#[test]
fn relocatable_no_program_headers_yields_only_exact_file_backed_metadata() {
    let raw = elf();
    let observed = parse(&raw, 0x100..0x120).unwrap();
    assert_eq!(observed.elf_header_range(), 0..64);
    assert_eq!(observed.section_table_range(), 0x200..0x300);
    assert_eq!(observed.serialized_bytes(), raw.len());
    assert_eq!(
        observed.serialized_sha256(),
        format!("{:x}", Sha256::digest(&raw))
    );
    assert_eq!(
        observed
            .file_sections()
            .iter()
            .map(|section| (section.name(), section.offset(), section.bytes()))
            .collect::<Vec<_>>(),
        [
            (".text", 0x100, 0x20),
            (".shstrtab", 0x140, NAMES.len()),
            (".data", 0x180, 8)
        ]
    );
    let mut nobits = raw;
    put32(&mut nobits, TABLE + 3 * 64 + 4, 8);
    put64(&mut nobits, TABLE + 3 * 64 + 32, 4096); // Logical bytes exceed the file: never attribute them.
    assert_eq!(
        parse(&nobits, 0x100..0x120).unwrap().file_sections().len(),
        2
    );
}

#[test]
fn malformed_headers_extended_counts_and_table_extents_are_rejected() {
    for mutate in [
        (|r: &mut Vec<u8>| r[4] = 1) as fn(&mut Vec<u8>),
        |r| r[5] = 2,
        |r| put32(r, 20, 2),
        |r| put16(r, 52, 63),
        |r| put16(r, 58, 63),
        |r| put16(r, 60, 0),
        |r| put16(r, 60, 4097),
        |r| put16(r, 62, 0xffff),
        |r| put32(r, TABLE + 4, 1),
        |r| put64(r, 40, u64::MAX),
        |r| put64(r, 40, 0),
        |r| put64(r, 32, 64), // No program headers: no phantom extent.
        |r| {
            put16(r, 56, 1);
            put16(r, 54, 56);
        }, // Program table overlaps ELF header.
    ] {
        let mut raw = elf();
        mutate(&mut raw);
        rejected(&raw);
    }
    rejected(&elf()[..63]);
}

#[test]
fn section_arithmetic_overlap_nobits_and_text_anchors_fail_closed() {
    for mutate in [
        (|r: &mut Vec<u8>| put64(r, TABLE + 3 * 64 + 24, u64::MAX)) as fn(&mut Vec<u8>),
        |r| put64(r, TABLE + 3 * 64 + 16, u64::MAX),
        |r| put64(r, TABLE + 3 * 64 + 32, (MAX_IMAGE_BYTES + 1) as u64),
        |r| put64(r, TABLE + 3 * 64 + 24, 0x110),
        |r| put64(r, TABLE + 3 * 64 + 24, 8),
        |r| put64(r, TABLE + 3 * 64 + 24, TABLE as u64),
        |r| put64(r, TABLE + 3 * 64 + 48, 3),
        |r| put64(r, TABLE + 3 * 64 + 48, 0x100),
        |r| put32(r, TABLE + 64 + 4, 8), // .text cannot become NOBITS.
        |r| put32(r, TABLE + 2 * 64 + 4, 8), // Names require an actual file-backed STRTAB.
        |r| put64(r, TABLE + 64 + 8, 2),
        |r| put64(r, TABLE + 64 + 24, 0x108),
        |r| put64(r, TABLE + 64 + 32, 0),
        |r| {
            put32(r, TABLE + 3 * 64 + 4, 8);
            put64(r, TABLE + 3 * 64 + 24, u64::MAX);
        },
    ] {
        let mut raw = elf();
        mutate(&mut raw);
        rejected(&raw);
    }
    assert!(parse(&elf(), 0x101..0x120).is_err());
}

#[test]
fn names_require_unique_bounded_terminated_utf8_and_in_table_offsets() {
    for mutate in [
        (|r: &mut Vec<u8>| put32(r, TABLE + 3 * 64, 1)) as fn(&mut Vec<u8>),
        |r| put32(r, TABLE + 3 * 64, 0),
        |r| put32(r, TABLE + 3 * 64, NAMES.len() as u32),
        |r| put64(r, TABLE + 2 * 64 + 32, (NAMES.len() - 1) as u64),
        |r| r[0x140 + 17] = 0xff,
        |r| r[0x140] = b'x',
    ] {
        let mut raw = elf();
        mutate(&mut raw);
        rejected(&raw);
    }
    for length in [MAX_NAME_BYTES, MAX_NAME_BYTES + 1] {
        let mut raw = elf();
        let offset = raw.len();
        raw.extend_from_slice(NAMES);
        let long_name = NAMES.len();
        raw.extend(std::iter::repeat_n(b'x', length));
        raw.push(0);
        put64(&mut raw, TABLE + 2 * 64 + 24, offset as u64);
        put64(
            &mut raw,
            TABLE + 2 * 64 + 32,
            (NAMES.len() + length + 1) as u64,
        );
        put32(&mut raw, TABLE + 3 * 64, long_name as u32);
        if length == MAX_NAME_BYTES {
            assert!(parse(&raw, 0x100..0x120).is_ok());
        } else {
            rejected(&raw);
        }
    }
}

#[test]
fn name_budget_is_aggregate_even_for_omitted_logical_sections() {
    let original = elf();
    let mut raw = original.clone();
    raw.resize(0x1000 + 72 * 64, 0);
    raw[0x1000..0x1000 + 4 * 64].copy_from_slice(&original[TABLE..TABLE + 4 * 64]);
    put64(&mut raw, 40, 0x1000);
    put16(&mut raw, 60, 72);
    raw[0x300..0x300 + NAMES.len()].copy_from_slice(NAMES);
    raw[0x300 + NAMES.len()..0x300 + NAMES.len() + MAX_NAME_BYTES].fill(b'x');
    raw[0x300 + NAMES.len() + MAX_NAME_BYTES] = 0;
    put64(&mut raw, 0x1000 + 2 * 64 + 24, 0x300);
    put64(
        &mut raw,
        0x1000 + 2 * 64 + 32,
        (NAMES.len() + MAX_NAME_BYTES + 1) as u64,
    );
    for index in 3..72 {
        let at = 0x1000 + index * 64;
        put32(&mut raw, at, (NAMES.len() + index - 3) as u32);
        put32(&mut raw, at + 4, 8);
        put64(&mut raw, at + 32, 1);
    }
    let error = parse(&raw, 0x100..0x120).unwrap_err();
    assert!(
        error.to_string().contains("name budget exceeded"),
        "{error}"
    );
}
