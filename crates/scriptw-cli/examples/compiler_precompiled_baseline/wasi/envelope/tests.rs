use super::*;

fn framed() -> Vec<u8> {
    // Deliberately inert fake native bytes: these tests never deserialize them.
    encode(b"\0asm\x01\0\0\0", b"inert", &[7; 32]).unwrap()
}

#[test]
fn inert_frame_checks_bind_lengths_identity_and_every_payload_byte() {
    let bytes = framed();
    let frame = decode(&bytes, &[7; 32]).unwrap();
    assert_eq!(&bytes[frame.program], b"\0asm\x01\0\0\0");
    assert_eq!(&bytes[frame.native], b"inert");
    assert!(decode(&bytes, &[8; 32]).is_err());
    for offset in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;
        assert!(
            decode(&changed, &[7; 32]).is_err(),
            "accepted mutation at {offset}"
        );
    }
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length], &[7; 32]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode(&trailing, &[7; 32]).is_err());
}

#[test]
fn rechecksummed_lengths_cannot_escape_the_frame() {
    for (offset, length) in [(40, 0), (40, u64::MAX), (48, 0), (48, u64::MAX)] {
        let mut bytes = framed();
        bytes[offset..offset + 8].copy_from_slice(&length.to_le_bytes());
        let end = bytes.len() - DIGEST;
        let checksum = Sha256::digest(&bytes[..end]);
        bytes[end..].copy_from_slice(&checksum);
        assert!(decode(&bytes, &[7; 32]).is_err());
    }
}

#[test]
fn start_scan_distinguishes_section_ids_from_custom_payloads() {
    let prefix = b"\0asm\x01\0\0\0";
    assert!(reject_implicit_start(prefix).is_ok());
    let mut custom = prefix.to_vec();
    custom.extend_from_slice(&[0, 3, 0, 8, 0]);
    assert!(reject_implicit_start(&custom).is_ok());
    for suffix in [
        &[8, 1, 0][..],
        &[0, 0x80][..],
        &[0, 3, 0][..],
        &[0, 0xff, 0xff, 0xff, 0xff, 0x10][..],
        &[0, 0x80, 0x80, 0x80, 0x80, 0x80][..],
    ] {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(suffix);
        assert!(reject_implicit_start(&bytes).is_err());
    }
    assert!(reject_implicit_start(b"\0asm\x0d\0\x01\0").is_err());
}
