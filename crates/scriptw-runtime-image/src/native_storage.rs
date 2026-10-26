//! Private storage framing only; the unchanged native manifest still admits
//! source, target, Engine policy and the exact decoded native SHA-256.
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

/// Facts about the Cargo image just written, not authority to load caller data.
pub struct NativeRuntimeDigest {
    decoded_bytes: usize,
    digest: [u8; 32],
}
impl NativeRuntimeDigest {
    pub fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Verified decoded Cargo storage. Private fields prevent a caller from
/// substituting bytes or a self-reported digest at Engine's admission boundary.
pub struct NativeRuntimeBytes {
    bytes: Box<[u8]>,
    digest: [u8; 32],
}
impl NativeRuntimeBytes {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

pub(super) const MAGIC: &[u8; 8] = b"SWRTZ001";
pub(super) const HEADER_BYTES: usize = 48;
pub(super) const MAX_RAW_BYTES: usize = 256 * 1024 * 1024;

struct Header {
    length: usize,
    expected: [u8; 32],
}
impl Header {
    fn read(reader: &mut impl Read) -> Option<Self> {
        let mut raw = [0; HEADER_BYTES];
        reader.read_exact(&mut raw).ok()?;
        if &raw[..8] != MAGIC {
            return None;
        }
        let length = usize::try_from(u64::from_le_bytes(raw[8..16].try_into().ok()?)).ok()?;
        if length == 0 || length > MAX_RAW_BYTES {
            return None;
        }
        Some(Self {
            length,
            expected: raw[16..].try_into().ok()?,
        })
    }
}

pub(super) fn write_decoded(
    mut reader: impl Read,
    writer: &mut impl Write,
) -> Option<NativeRuntimeDigest> {
    let header = Header::read(&mut reader)?;
    write_body(reader, header, writer)
}

fn write_body(
    reader: impl Read,
    header: Header,
    writer: &mut impl Write,
) -> Option<NativeRuntimeDigest> {
    let mut decoder = zstd::stream::read::Decoder::new(reader)
        .ok()?
        .single_frame();
    // Output bounds alone do not cap a malformed frame's history window.
    // Cargo uses level 1; its roundtrip also enforces this 8 MiB window bound.
    decoder.window_log_max(23).ok()?;
    let mut buffer = [0; 64 * 1024];
    let mut length = 0usize;
    let mut hash = Sha256::new();
    loop {
        // Read at most one excess byte. Neither corrupt lengths nor the zstd
        // window may turn this path into a decoded-image-sized allocation.
        let wanted = buffer.len().min(header.length + 1 - length);
        let count = decoder.read(&mut buffer[..wanted]).ok()?;
        if count == 0 {
            break;
        }
        length = length.checked_add(count)?;
        if length > header.length {
            return None;
        }
        hash.update(&buffer[..count]);
        writer.write_all(&buffer[..count]).ok()?;
    }
    let digest: [u8; 32] = hash.finalize().into();
    if length != header.length || digest != header.expected {
        return None;
    }
    // A complete frame must also exhaust the exact Cargo input range. This
    // rejects trailing bytes and extra frames, including empty frames.
    if decoder.finish().read(&mut buffer[..1]).ok()? != 0 {
        return None;
    }
    Some(NativeRuntimeDigest {
        decoded_bytes: length,
        digest,
    })
}

pub(super) fn decode_reader(mut reader: impl Read) -> Option<NativeRuntimeBytes> {
    let header = Header::read(&mut reader)?;
    let mut bytes = Vec::with_capacity(header.length);
    let digest = write_body(reader, header, &mut bytes)?.digest;
    Some(NativeRuntimeBytes {
        bytes: bytes.into_boxed_slice(),
        digest,
    })
}

#[cfg(test)]
fn decode(encoded: &[u8]) -> Option<NativeRuntimeBytes> {
    decode_reader(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(raw: &[u8]) -> Vec<u8> {
        let mut image = MAGIC.to_vec();
        image.extend_from_slice(&(raw.len() as u64).to_le_bytes());
        image.extend_from_slice(&Sha256::digest(raw));
        image.extend_from_slice(&zstd::stream::encode_all(raw, 1).unwrap());
        image
    }

    #[test]
    fn exact_decoded_bytes_and_rejected_container_corruption() {
        let raw = b"Cargo-owned native image, including \0 bytes";
        let encoded = encode(raw);
        assert_eq!(decode(&encoded).unwrap().bytes(), raw);
        for index in [0, 8, 16, HEADER_BYTES] {
            let mut changed = encoded.clone();
            changed[index] ^= 1;
            assert!(decode(&changed).is_none(), "corrupted offset {index}");
        }
        for end in [0, 7, 15, 47, encoded.len() - 1] {
            assert!(decode(&encoded[..end]).is_none(), "truncated at {end}");
        }
        for length in [0, raw.len() as u64 - 1, raw.len() as u64 + 1, u64::MAX] {
            let mut changed = encoded.clone();
            changed[8..16].copy_from_slice(&length.to_le_bytes());
            assert!(decode(&changed).is_none(), "wrong raw length {length}");
        }
        let mut trailing = encoded;
        trailing.push(0xff);
        assert!(decode(&trailing).is_none());
    }

    #[test]
    fn oversized_frame_window_is_rejected_before_decompression() {
        let raw = b"bounded native storage";
        // Zstd frame without a content-size field, one final raw block.
        let mut frame = vec![0x28, 0xb5, 0x2f, 0xfd, 0, 0x50];
        frame.extend_from_slice(&((raw.len() as u32 * 8) | 1).to_le_bytes()[..3]);
        frame.extend_from_slice(raw);
        let wrap = |frame: &[u8]| {
            let mut bytes = MAGIC.to_vec();
            bytes.extend_from_slice(&(raw.len() as u64).to_le_bytes());
            bytes.extend_from_slice(&Sha256::digest(raw));
            bytes.extend_from_slice(frame);
            bytes
        };
        assert_eq!(decode(&wrap(&frame)).unwrap().bytes(), raw);
        frame[5] = 0x88; // 128 MiB history request with the same small output.
        assert!(decode(&wrap(&frame)).is_none());
    }

    #[test]
    fn streaming_handles_short_reads_bounded_writes_and_sink_failure() {
        struct Chunks<'a>(&'a [u8]);
        impl Read for Chunks<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                let length = bytes.len().min(7);
                self.0.read(&mut bytes[..length])
            }
        }
        struct Sink {
            bytes: Vec<u8>,
            fail_at: Option<usize>,
        }
        impl Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                assert!(bytes.len() <= 64 * 1024);
                if self.fail_at.is_some_and(|limit| self.bytes.len() >= limit) {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                // The decoder must also tolerate successful partial writes.
                let count = bytes.len().min(4093);
                self.bytes.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let raw: Vec<u8> = (0..200_003).map(|i| (i % 251) as u8).collect();
        let encoded = encode(&raw);
        let mut sink = Sink {
            bytes: Vec::new(),
            fail_at: None,
        };
        let summary = write_decoded(Chunks(&encoded), &mut sink).unwrap();
        assert_eq!(summary.decoded_bytes(), raw.len());
        assert_eq!(summary.digest(), &<[u8; 32]>::from(Sha256::digest(&raw)));
        assert_eq!(sink.bytes, raw);
        let mut failed = Sink {
            bytes: Vec::new(),
            fail_at: Some(10_000),
        };
        assert!(write_decoded(Chunks(&encoded), &mut failed).is_none());
        assert!(failed.bytes.len() < raw.len());
    }

    #[test]
    fn extra_frames_are_not_admitted_even_when_their_output_is_empty() {
        let mut encoded = encode(b"one exact Cargo frame");
        encoded.extend_from_slice(&zstd::stream::encode_all(&b""[..], 1).unwrap());
        assert!(decode(&encoded).is_none());
        let mut sink = Vec::new();
        assert!(write_decoded(&encoded[..], &mut sink).is_none());
    }
}
