//! The linker build ID of the image this process is executing.
//!
//! A linker writes `.note.gnu.build-id` last, as a digest of the complete
//! linked output (or a fresh UUID), into a read-only `PT_NOTE` of the first
//! `PT_LOAD`. Two different links therefore carry different IDs, and the ID is
//! already mapped: reading it costs a few small procfs reads (auxv, the program
//! headers, one per `PT_NOTE`) instead of a pass over a 200 MiB executable.
//!
//! Threat model, shared with the full-image cache cohorts: an honest build
//! pipeline and a cache directory only its owner can write. The ID is read
//! from this process's own kernel-loaded program headers (`AT_PHDR`), never
//! from a path, so a replaced or renamed executable file cannot supply it. It
//! is admitted only from a non-writable, file-backed `PT_LOAD`, so it is the
//! linked file's bytes and not process data.
//!
//! What it does not cover, and the full-image cohorts do: bytes edited in the
//! executable file after linking while the note is kept (binary patching), and
//! links given a fixed `--build-id=0x...`. Such builds must select `blake3` or
//! `sha256`. It is cache identity only; execution evidence stays the SHA-256
//! of the whole image (`CompilerIdentity`).

/// Shortest admitted ID: MD5 and UUID styles are 16 bytes, SHA-1 is 20. lld's
/// 8-byte `fast` style is refused: 64 non-cryptographic bits are too few to
/// separate every build that may ever share a cache directory.
const MIN_BYTES: usize = 16;
const MAX_BYTES: usize = 64;

/// Bytes of an admitted `NT_GNU_BUILD_ID`. Only [`LoadedBuildId::current`]
/// (the loaded image) and the note parser construct it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoadedBuildId {
    bytes: [u8; MAX_BYTES],
    length: u8,
}

impl std::fmt::Debug for LoadedBuildId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "LoadedBuildId(")?;
        for byte in self.as_bytes() {
            write!(formatter, "{byte:02x}")?;
        }
        write!(formatter, ")")
    }
}

impl LoadedBuildId {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.length)]
    }

    fn admit(description: &[u8]) -> Option<Self> {
        if !(MIN_BYTES..=MAX_BYTES).contains(&description.len())
            // A zeroed or filled placeholder is not a digest of anything.
            || description.iter().all(|byte| *byte == description[0])
        {
            return None;
        }
        let mut bytes = [0; MAX_BYTES];
        bytes[..description.len()].copy_from_slice(description);
        Some(Self {
            bytes,
            length: description.len() as u8,
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(description: &[u8]) -> Self {
        Self::admit(description).expect("an admissible test build ID")
    }

    /// The running executable's build ID, read once. `None` when the image has
    /// no admissible note or the platform cannot prove where it was loaded.
    pub(crate) fn current() -> Option<&'static Self> {
        static CURRENT: std::sync::OnceLock<Option<LoadedBuildId>> = std::sync::OnceLock::new();
        CURRENT.get_or_init(read_loaded).as_ref()
    }
}

#[cfg(all(
    target_os = "linux",
    target_endian = "little",
    target_pointer_width = "64"
))]
fn read_loaded() -> Option<LoadedBuildId> {
    use std::io::Read;
    use std::os::unix::fs::FileExt;
    let mut auxv = Vec::new();
    std::fs::File::open("/proc/self/auxv")
        .ok()?
        .take(elf::MAX_AUXV_BYTES as u64 + 1)
        .read_to_end(&mut auxv)
        .ok()?;
    let table = elf::LoadedTable::from_auxv(&auxv)?;
    // The kernel-owned addresses are read through a bounded file API: an
    // unmapped address is an error, never a raw-pointer dereference.
    let memory = std::fs::File::open("/proc/self/mem").ok()?;
    let mut headers = vec![0; table.bytes()];
    memory.read_exact_at(&mut headers, table.address()).ok()?;
    let mut found = None;
    for note in elf::note_segments(&headers, table.address())? {
        let mut segment = vec![0; note.length];
        memory.read_exact_at(&mut segment, note.address).ok()?;
        if let Some(id) = elf::build_id(&segment, note.alignment)? {
            // Two build-ID notes do not name one image.
            if found.replace(id).is_some() {
                return None;
            }
        }
    }
    found
}

#[cfg(not(all(
    target_os = "linux",
    target_endian = "little",
    target_pointer_width = "64"
)))]
fn read_loaded() -> Option<LoadedBuildId> {
    None
}

/// Pure ELF64 little-endian layout checks; they open and dereference nothing.
#[cfg(any(
    test,
    all(
        target_os = "linux",
        target_endian = "little",
        target_pointer_width = "64"
    )
))]
mod elf {
    use super::LoadedBuildId;

    pub(super) const MAX_AUXV_BYTES: usize = 4096;
    const PHENT: usize = 56;
    const MAX_PHNUM: u64 = 1024;
    const MAX_NOTE_BYTES: u64 = 64 * 1024;
    const PT_LOAD: u32 = 1;
    const PT_NOTE: u32 = 4;
    const PT_PHDR: u32 = 6;
    const PF_W: u32 = 2;
    const NT_GNU_BUILD_ID: u32 = 3;

    fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
        ))
    }
    fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
        ))
    }

    /// Where the kernel mapped the main executable's program headers.
    pub(super) struct LoadedTable {
        address: u64,
        count: usize,
    }

    impl LoadedTable {
        pub(super) fn from_auxv(auxv: &[u8]) -> Option<Self> {
            if auxv.len() > MAX_AUXV_BYTES || auxv.len() % 16 != 0 {
                return None;
            }
            // AT_PHDR, AT_PHENT, AT_PHNUM: each exactly once before AT_NULL.
            let mut fields = [None; 3];
            let mut ended = false;
            for pair in auxv.chunks_exact(16) {
                let (key, value) = (u64_at(pair, 0)?, u64_at(pair, 8)?);
                if key == 0 {
                    ended = true;
                    break;
                }
                if let Some(slot) = key.checked_sub(3).filter(|slot| *slot < 3) {
                    if value == 0 || fields[slot as usize].replace(value).is_some() {
                        return None;
                    }
                }
            }
            let [address, entry_bytes, count] = fields;
            let count = count?;
            if !ended || entry_bytes? != PHENT as u64 || count > MAX_PHNUM {
                return None;
            }
            Some(Self {
                address: address?,
                count: count as usize,
            })
        }

        pub(super) fn address(&self) -> u64 {
            self.address
        }
        pub(super) fn bytes(&self) -> usize {
            self.count * PHENT
        }
    }

    /// A loaded `PT_NOTE` that is the linked file's own read-only bytes.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct NoteSegment {
        pub(super) address: u64,
        pub(super) length: usize,
        pub(super) alignment: usize,
    }

    struct Header {
        kind: u32,
        flags: u32,
        address: u64,
        file_size: u64,
        memory_size: u64,
        alignment: u64,
    }

    /// `headers` are the program headers read at `loaded_at`. `None` rejects
    /// the whole image: a note this cannot place is never skipped over.
    pub(super) fn note_segments(headers: &[u8], loaded_at: u64) -> Option<Vec<NoteSegment>> {
        if headers.is_empty() || headers.len() % PHENT != 0 {
            return None;
        }
        let headers: Vec<_> = headers
            .chunks_exact(PHENT)
            .map(|raw| {
                Some(Header {
                    kind: u32_at(raw, 0)?,
                    flags: u32_at(raw, 4)?,
                    address: u64_at(raw, 16)?,
                    file_size: u64_at(raw, 32)?,
                    memory_size: u64_at(raw, 40)?,
                    alignment: u64_at(raw, 48)?,
                })
            })
            .collect::<Option<_>>()?;
        // PT_PHDR names the table's own link-time address; the difference from
        // where the kernel reports it is the load bias of every segment.
        let mut tables = headers.iter().filter(|header| header.kind == PT_PHDR);
        let table = tables.next()?;
        if tables.next().is_some() {
            return None;
        }
        let bias = loaded_at.checked_sub(table.address)?;
        let mut notes = Vec::new();
        for note in headers.iter().filter(|header| header.kind == PT_NOTE) {
            let end = note.address.checked_add(note.file_size)?;
            if note.file_size == 0
                || note.file_size != note.memory_size
                || note.file_size > MAX_NOTE_BYTES
            {
                return None;
            }
            // Count every PT_LOAD that intersects the note before checking
            // protections, so a writable overlap cannot hide behind an
            // eligible read-only owner. The owner must supply every note byte
            // from the file (`file_size`), not from zero-filled memory.
            let mut owners = headers.iter().filter(|load| {
                load.kind == PT_LOAD
                    && load.address < end
                    && load
                        .address
                        .checked_add(load.memory_size)
                        .is_some_and(|load_end| note.address < load_end)
            });
            let owner = owners.next()?;
            if owners.next().is_some()
                || owner.flags & PF_W != 0
                || note.address < owner.address
                || end > owner.address.checked_add(owner.file_size)?
            {
                return None;
            }
            let address = bias.checked_add(note.address)?;
            address.checked_add(note.file_size)?;
            notes.push(NoteSegment {
                address,
                length: note.file_size as usize,
                alignment: if note.alignment == 8 { 8 } else { 4 },
            });
        }
        Some(notes)
    }

    /// The segment's GNU build ID. Outer `None`: malformed notes or an
    /// inadmissible or repeated ID. Inner `None`: a well-formed segment
    /// without one.
    pub(super) fn build_id(segment: &[u8], alignment: usize) -> Option<Option<LoadedBuildId>> {
        let pad = |length: usize| length.checked_next_multiple_of(alignment);
        let mut rest = segment;
        let mut found = None;
        while !rest.is_empty() {
            let name_bytes = u32_at(rest, 0)? as usize;
            let description_bytes = u32_at(rest, 4)? as usize;
            let kind = u32_at(rest, 8)?;
            // The descriptor starts at the header-and-name end rounded up to
            // the segment's alignment (16 for a 4-byte name in an 8-aligned
            // `.note.gnu.property` segment, not 12 + 8).
            let description_start = pad(12usize.checked_add(name_bytes)?)?;
            let description_end = description_start.checked_add(description_bytes)?;
            let name = rest.get(12..12 + name_bytes)?;
            let description = rest.get(description_start..description_end)?;
            if kind == NT_GNU_BUILD_ID
                && name == b"GNU\0"
                && found.replace(LoadedBuildId::admit(description)?).is_some()
            {
                return None;
            }
            // The last note's padding may be cut off by the segment end.
            rest = rest.get(pad(description_end)?.min(rest.len())..)?;
        }
        Some(found)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn note(name: &[u8], kind: u32, description: &[u8]) -> Vec<u8> {
            aligned_note(name, kind, description, 4)
        }

        fn aligned_note(name: &[u8], kind: u32, description: &[u8], alignment: usize) -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&(description.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(name);
            bytes.resize(bytes.len().next_multiple_of(alignment), 0);
            bytes.extend_from_slice(description);
            bytes.resize(bytes.len().next_multiple_of(alignment), 0);
            bytes
        }

        #[test]
        fn eight_aligned_note_segments_place_descriptors_after_the_padded_header_and_name() {
            // GNU ld's `.note.gnu.property` segment: 8-aligned, 4-byte name,
            // descriptor at offset 16.
            let sha1 = id(0x30, 20);
            let mut segment = aligned_note(b"GNU\0", 5, &[0x11; 32], 8);
            assert_eq!(&segment[16..48], &[0x11; 32]);
            segment.extend(aligned_note(b"GNU\0", NT_GNU_BUILD_ID, &sha1, 8));
            let found = build_id(&segment, 8).unwrap().unwrap();
            assert_eq!(found.as_bytes(), sha1);
            assert_eq!(
                build_id(&aligned_note(b"GNU\0", 5, &[0x11; 32], 8), 8),
                Some(None)
            );
        }

        fn id(seed: u8, length: usize) -> Vec<u8> {
            (0..length).map(|index| seed ^ index as u8).collect()
        }

        fn header(kind: u32, flags: u32, address: u64, file: u64, memory: u64) -> [u8; PHENT] {
            let mut bytes = [0; PHENT];
            bytes[..4].copy_from_slice(&kind.to_le_bytes());
            bytes[4..8].copy_from_slice(&flags.to_le_bytes());
            bytes[16..24].copy_from_slice(&address.to_le_bytes());
            bytes[32..40].copy_from_slice(&file.to_le_bytes());
            bytes[40..48].copy_from_slice(&memory.to_le_bytes());
            bytes[48..56].copy_from_slice(&4u64.to_le_bytes());
            bytes
        }

        #[test]
        fn build_id_note_is_found_among_other_notes_and_bound_to_its_exact_bytes() {
            let sha1 = id(0x40, 20);
            let mut segment = note(b"GNU\0", 1, &[0; 16]);
            segment.extend(note(b"GNU\0", NT_GNU_BUILD_ID, &sha1));
            let found = build_id(&segment, 4).unwrap().unwrap();
            assert_eq!(found.as_bytes(), sha1);
            assert_ne!(
                found,
                build_id(&note(b"GNU\0", NT_GNU_BUILD_ID, &id(0x41, 20)), 4)
                    .unwrap()
                    .unwrap()
            );
            // Another owner's type-3 note and a GNU note of another type are not IDs.
            let mut foreign = note(b"Go\0\0", NT_GNU_BUILD_ID, &sha1);
            foreign.extend(note(b"GNU\0", 5, &sha1));
            assert_eq!(build_id(&foreign, 4), Some(None));
            assert_eq!(build_id(&[], 4), Some(None));
        }

        #[test]
        fn short_placeholder_repeated_and_truncated_ids_reject_the_image() {
            // lld `fast` (8 bytes), an oversized ID, and zero or filled placeholders.
            for description in [id(1, 8), id(1, 15), id(1, 65), vec![0; 20], vec![0xff; 20]] {
                assert_eq!(
                    build_id(&note(b"GNU\0", NT_GNU_BUILD_ID, &description), 4),
                    None
                );
            }
            for length in [16, 20, 32, 64] {
                assert!(
                    build_id(&note(b"GNU\0", NT_GNU_BUILD_ID, &id(7, length)), 4)
                        .unwrap()
                        .is_some()
                );
            }
            let one = note(b"GNU\0", NT_GNU_BUILD_ID, &id(1, 20));
            let mut twice = one.clone();
            twice.extend(note(b"GNU\0", NT_GNU_BUILD_ID, &id(2, 20)));
            assert_eq!(build_id(&twice, 4), None);
            for end in 1..one.len() {
                assert_eq!(build_id(&one[..end], 4), None, "truncated at {end}");
            }
            let mut huge = one;
            huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
            assert_eq!(build_id(&huge, 4), None);
        }

        #[test]
        fn notes_are_admitted_only_from_one_read_only_file_backed_load() {
            let table = |load_flags: u32, load_file: u64, extra: Option<[u8; PHENT]>| {
                let mut bytes = Vec::new();
                bytes.extend(header(PT_PHDR, 4, 0x40, 0xa8, 0xa8));
                bytes.extend(header(PT_LOAD, load_flags, 0, load_file, 0x1000));
                bytes.extend(header(PT_NOTE, 4, 0x200, 0x24, 0x24));
                bytes.extend(extra.into_iter().flatten());
                bytes
            };
            // Loaded with a bias of 0x5000: PT_PHDR says 0x40, the kernel 0x5040.
            assert_eq!(
                note_segments(&table(4, 0x1000, None), 0x5040),
                Some(vec![NoteSegment {
                    address: 0x5200,
                    length: 0x24,
                    alignment: 4
                }])
            );
            assert_eq!(
                note_segments(&table(5, 0x1000, None), 0x5040)
                    .unwrap()
                    .len(),
                1
            );
            // Writable owner; note beyond the file-backed part; no owner.
            assert_eq!(note_segments(&table(6, 0x1000, None), 0x5040), None);
            assert_eq!(note_segments(&table(4, 0x220, None), 0x5040), None);
            assert_eq!(
                note_segments(&table(4, 0x1000, None)[..2 * PHENT], 0x5040),
                Some(vec![])
            );
            // A second, writable LOAD over the note; a second PT_PHDR; a bias
            // below zero; a note whose memory is not its file bytes.
            for extra in [
                header(PT_LOAD, 6, 0x210, 0x10, 0x10),
                header(PT_PHDR, 4, 0x40, 0xa8, 0xa8),
                header(PT_NOTE, 4, 0x300, 0x10, 0x20),
                header(PT_NOTE, 4, 0x300, 0, 0),
            ] {
                assert_eq!(note_segments(&table(4, 0x1000, Some(extra)), 0x5040), None);
            }
            assert_eq!(note_segments(&table(4, 0x1000, None), 0x3f), None);
            assert_eq!(
                note_segments(&table(4, 0x1000, None)[..PHENT + 1], 0x5040),
                None
            );
            assert_eq!(note_segments(&[], 0x5040), None);
        }

        #[test]
        fn auxiliary_vector_must_name_one_bounded_program_header_table() {
            let auxv = |pairs: &[(u64, u64)]| {
                let mut bytes = Vec::new();
                for (key, value) in pairs {
                    bytes.extend_from_slice(&key.to_le_bytes());
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                bytes
            };
            let table =
                LoadedTable::from_auxv(&auxv(&[(33, 9), (3, 0x5040), (4, 56), (5, 12), (0, 0)]))
                    .unwrap();
            assert_eq!((table.address(), table.bytes()), (0x5040, 12 * PHENT));
            for pairs in [
                &[(3, 0x5040), (4, 56), (5, 12)][..],       // no AT_NULL
                &[(3, 0x5040), (4, 56), (0, 0)],            // no AT_PHNUM
                &[(3, 0x5040), (4, 32), (5, 12), (0, 0)],   // not ELF64 entries
                &[(3, 0x5040), (4, 56), (5, 1025), (0, 0)], // unbounded table
                &[(3, 0x5040), (3, 0x6040), (4, 56), (5, 12), (0, 0)], // repeated key
                &[(3, 0), (4, 56), (5, 12), (0, 0)],
            ] {
                assert!(LoadedTable::from_auxv(&auxv(pairs)).is_none());
            }
            assert!(LoadedTable::from_auxv(&[0; 17]).is_none());
        }
    }
}
