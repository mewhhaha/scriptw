//! Independent main-executable origin proof for Linux device-number aliases.
//! Only open() can construct the file-owning proof; parsed layout facts and
//! hashes never authorize an arbitrary file, caller slice or DSO payload.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Take};
use std::os::unix::fs::FileExt;

const HEADER_BYTES: usize = 64;
const PHENT: usize = 56;
const MAX_PHNUM: usize = 1024;
const MAX_AUXV_BYTES: usize = 4096;
const PT_LOAD: u32 = 1;
const PT_PHDR: u32 = 6;
const PF_R: u32 = 4;
const PF_X: u32 = 1;

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
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
fn host_machine() -> Option<u16> {
    if !cfg!(all(target_endian = "little", target_pointer_width = "64")) {
        return None;
    }
    #[cfg(target_arch = "x86_64")]
    return Some(62);
    #[cfg(target_arch = "aarch64")]
    return Some(183);
    #[cfg(target_arch = "riscv64")]
    return Some(243);
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    None
}

#[derive(Clone, Copy)]
struct ElfHeader {
    kind: u16,
    entry: u64,
    phoff: u64,
    phnum: usize,
}
impl ElfHeader {
    fn parse(bytes: &[u8], file_length: u64, machine: u16) -> Option<Self> {
        if bytes.len() != HEADER_BYTES
            || bytes.get(..7)? != b"\x7fELF\x02\x01\x01"
            || u16_at(bytes, 18)? != machine
            || u32_at(bytes, 20)? != 1
            || u16_at(bytes, 52)? != HEADER_BYTES as u16
            || u16_at(bytes, 54)? != PHENT as u16
        {
            return None;
        }
        let kind = u16_at(bytes, 16)?;
        if !matches!(kind, 2 | 3) {
            return None;
        }
        let phnum = usize::from(u16_at(bytes, 56)?);
        if phnum == 0 || phnum > MAX_PHNUM {
            return None;
        }
        let phoff = u64_at(bytes, 32)?;
        if phoff < HEADER_BYTES as u64
            || phoff.checked_add(phnum.checked_mul(PHENT)? as u64)? > file_length
        {
            return None;
        }
        Some(Self {
            kind,
            entry: u64_at(bytes, 24)?,
            phoff,
            phnum,
        })
    }
}

#[derive(Clone, Copy)]
struct Auxv {
    phdr: u64,
    phent: u64,
    phnum: u64,
    entry: u64,
}
impl Auxv {
    fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_AUXV_BYTES || bytes.len() % 16 != 0 {
            return None;
        }
        let mut fields = [None; 4];
        let mut ended = false;
        for pair in bytes.chunks_exact(16) {
            let key = u64_at(pair, 0)?;
            let value = u64_at(pair, 8)?;
            if ended {
                if key != 0 || value != 0 {
                    return None;
                }
                continue;
            }
            if key == 0 {
                if value != 0 {
                    return None;
                }
                ended = true;
                continue;
            }
            let slot = match key {
                3 => Some(0),
                4 => Some(1),
                5 => Some(2),
                9 => Some(3),
                _ => None,
            };
            if let Some(slot) = slot {
                if value == 0 || fields[slot].replace(value).is_some() {
                    return None;
                }
            }
        }
        if !ended {
            return None;
        }
        Some(Self {
            phdr: fields[0]?,
            phent: fields[1]?,
            phnum: fields[2]?,
            entry: fields[3]?,
        })
    }
}

#[derive(Clone, Copy)]
struct ProgramHeader {
    kind: u32,
    flags: u32,
    offset: u64,
    address: u64,
    file_size: u64,
    memory_size: u64,
    alignment: u64,
}
impl ProgramHeader {
    fn parse(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            kind: u32_at(bytes, 0)?,
            flags: u32_at(bytes, 4)?,
            offset: u64_at(bytes, 8)?,
            address: u64_at(bytes, 16)?,
            file_size: u64_at(bytes, 32)?,
            memory_size: u64_at(bytes, 40)?,
            alignment: u64_at(bytes, 48)?,
        })
    }
}

// A parsed layout is not an origin owner. Only KernelMainElf pairs it with
// the actual kernel executable handle after loaded-header comparison.
struct MainLayout {
    bias: u64,
    loads: Vec<ProgramHeader>,
    file_length: u64,
}
impl MainLayout {
    fn verify(
        header: ElfHeader,
        table: &[u8],
        loaded: &[u8],
        aux: Auxv,
        file_length: u64,
    ) -> Option<Self> {
        if aux.phent != PHENT as u64
            || aux.phnum != header.phnum as u64
            || table.len() != header.phnum.checked_mul(PHENT)?
            || table != loaded
            || aux.phdr.checked_add(table.len() as u64)? > usize::MAX as u64
        {
            return None;
        }
        let headers: Vec<_> = table
            .chunks_exact(PHENT)
            .map(ProgramHeader::parse)
            .collect::<Option<_>>()?;
        let mut phdrs = headers.iter().filter(|p| p.kind == PT_PHDR);
        let phdr = phdrs.next()?;
        if phdrs.next().is_some()
            || phdr.flags != PF_R
            || phdr.offset != header.phoff
            || phdr.file_size != table.len() as u64
            || phdr.memory_size != table.len() as u64
        {
            return None;
        }
        let bias = aux.phdr.checked_sub(phdr.address)?;
        if header.kind == 2 && bias != 0 || bias.checked_add(header.entry)? != aux.entry {
            return None;
        }
        let loads: Vec<_> = headers
            .iter()
            .filter(|p| p.kind == PT_LOAD)
            .copied()
            .collect();
        for load in &loads {
            if load.file_size > load.memory_size
                || load.offset.checked_add(load.file_size)? > file_length
                || bias
                    .checked_add(load.address)?
                    .checked_add(load.memory_size)?
                    > usize::MAX as u64
                || load.alignment > 1
                    && (!load.alignment.is_power_of_two()
                        || load.offset % load.alignment != load.address % load.alignment)
            {
                return None;
            }
        }
        // The PHDR must itself be uniquely backed by this file's read-only
        // LOAD, at the exact file offset named by the ELF header.
        let phdr_end = phdr.address.checked_add(phdr.file_size)?;
        // Count every intersecting LOAD before checking protections. Another
        // RW/RX segment overlapping just the headers must not be hidden by a
        // filter that only considers otherwise eligible read-only owners.
        let mut owners = loads.iter().filter(|p| {
            p.address < phdr_end
                && p.address
                    .checked_add(p.memory_size)
                    .is_some_and(|end| phdr.address < end)
        });
        let owner = owners.next()?;
        if owners.next().is_some()
            || owner.flags != PF_R
            || phdr.address < owner.address
            || phdr_end > owner.address.checked_add(owner.file_size)?
            || owner.offset.checked_add(phdr.address - owner.address) != Some(header.phoff)
        {
            return None;
        }
        let mut entries = loads.iter().filter(|p| {
            p.flags & PF_X != 0
                && p.address <= header.entry
                && p.address
                    .checked_add(p.file_size)
                    .is_some_and(|end| header.entry < end)
        });
        entries.next()?;
        if entries.next().is_some() {
            return None;
        }
        Some(Self {
            bias,
            loads,
            file_length,
        })
    }

    fn file_offset(&self, address: u64, length: u64) -> Option<u64> {
        if length == 0 {
            return None;
        }
        let end = address.checked_add(length)?;
        let mut selected = None;
        for load in &self.loads {
            let start = self.bias.checked_add(load.address)?;
            let memory_end = start.checked_add(load.memory_size)?;
            if address < memory_end && start < end {
                // Reject partial intersections, BSS and overlapping LOADs,
                // including another segment with different protections.
                if selected.is_some()
                    || load.flags != PF_R
                    || address < start
                    || end > start.checked_add(load.file_size)?
                {
                    return None;
                }
                let offset = load.offset.checked_add(address.checked_sub(start)?)?;
                if offset.checked_add(length)? > self.file_length {
                    return None;
                }
                selected = Some(offset);
            }
        }
        selected
    }
}

/// Owns the exact kernel executable handle, never a caller path/file.
pub(super) struct KernelMainElf {
    file: File,
    layout: MainLayout,
    inode: u64,
}
impl KernelMainElf {
    pub(super) fn open() -> Option<Self> {
        let mut auxv = Vec::new();
        File::open("/proc/self/auxv")
            .ok()?
            .take(MAX_AUXV_BYTES as u64 + 1)
            .read_to_end(&mut auxv)
            .ok()?;
        let aux = Auxv::parse(&auxv)?;
        let file = File::open("/proc/self/exe").ok()?;
        let identity = super::ExecutableIdentity::from_file(&file)?;
        let mut raw_header = [0; HEADER_BYTES];
        file.read_exact_at(&mut raw_header, 0).ok()?;
        let header = ElfHeader::parse(&raw_header, identity.length, host_machine()?)?;
        let mut table = vec![0; header.phnum.checked_mul(PHENT)?];
        file.read_exact_at(&mut table, header.phoff).ok()?;
        if aux.phent != PHENT as u64
            || aux.phnum != header.phnum as u64
            || aux.phdr.checked_add(table.len() as u64)? > usize::MAX as u64
        {
            return None;
        }
        let mut loaded = vec![0; table.len()];
        // Kernel-owned main-PHDR address is read through a bounded file API.
        // Unmapped/inaccessible memory produces an error, never a raw-pointer
        // dereference. No packed payload is read through its mapped address.
        File::open("/proc/self/mem")
            .ok()?
            .read_exact_at(&mut loaded, aux.phdr)
            .ok()?;
        let layout = MainLayout::verify(header, &table, &loaded, aux, identity.length)?;
        Some(Self {
            file,
            layout,
            inode: identity.inode,
        })
    }

    pub(super) fn read_symbol(mut self, encoded: &'static [u8], maps: &str) -> Option<Take<File>> {
        let address = encoded.as_ptr() as usize as u64;
        let length = u64::try_from(encoded.len()).ok()?;
        let offset = self.layout.file_offset(address, length)?;
        let mut matching = maps
            .lines()
            .filter(|line| mapping_agrees(line, address, length, offset, self.inode));
        matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        Some(self.file.take(length))
    }
}

fn mapping_agrees(line: &str, address: u64, length: u64, expected_offset: u64, inode: u64) -> bool {
    fn offset(line: &str, address: u64, length: u64, inode: u64) -> Option<u64> {
        let mut fields = line.split_whitespace();
        let (start, end) = fields.next()?.split_once('-')?;
        let start = u64::from_str_radix(start, 16).ok()?;
        let end = u64::from_str_radix(end, 16).ok()?;
        if fields.next()? != "r--p"
            || start >= end
            || length == 0
            || address < start
            || address.checked_add(length)? > end
        {
            return None;
        }
        let offset = u64::from_str_radix(fields.next()?, 16).ok()?;
        let (major, minor) = fields.next()?.split_once(':')?;
        u64::from_str_radix(major, 16).ok()?;
        u64::from_str_radix(minor, 16).ok()?;
        let actual: u64 = fields.next()?.parse().ok()?;
        if inode == 0 || actual != inode {
            return None;
        }
        offset.checked_add(address.checked_sub(start)?)
    }
    offset(line, address, length, inode) == Some(expected_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn phdr(
        kind: u32,
        flags: u32,
        offset: u64,
        address: u64,
        file: u64,
        memory: u64,
    ) -> [u8; PHENT] {
        let mut bytes = [0; PHENT];
        put32(&mut bytes, 0, kind);
        put32(&mut bytes, 4, flags);
        put64(&mut bytes, 8, offset);
        put64(&mut bytes, 16, address);
        put64(&mut bytes, 32, file);
        put64(&mut bytes, 40, memory);
        put64(&mut bytes, 48, if kind == PT_PHDR { 8 } else { 4096 });
        bytes
    }
    fn fixture() -> ([u8; HEADER_BYTES], Vec<u8>, Auxv) {
        let mut header = [0; HEADER_BYTES];
        header[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut header, 16, 3);
        put16(&mut header, 18, 62);
        put32(&mut header, 20, 1);
        put64(&mut header, 24, 0x3000);
        put64(&mut header, 32, 64);
        put16(&mut header, 52, 64);
        put16(&mut header, 54, PHENT as u16);
        put16(&mut header, 56, 3);
        let table = [
            phdr(
                PT_PHDR,
                PF_R,
                64,
                64,
                (PHENT * 3) as u64,
                (PHENT * 3) as u64,
            ),
            phdr(PT_LOAD, PF_R, 0, 0, 0x2000, 0x2800),
            phdr(PT_LOAD, PF_R | PF_X, 0x2000, 0x3000, 0x1000, 0x1000),
        ]
        .concat();
        let aux = Auxv {
            phdr: 0x100040,
            phent: PHENT as u64,
            phnum: 3,
            entry: 0x103000,
        };
        (header, table, aux)
    }
    fn verify(header: &[u8], table: &[u8], aux: Auxv) -> Option<MainLayout> {
        MainLayout::verify(
            ElfHeader::parse(header, 0x4000, 62)?,
            table,
            table,
            aux,
            0x4000,
        )
    }

    #[test]
    fn pie_and_fixed_main_images_admit_only_whole_readonly_file_ranges() {
        let (mut header, table, mut aux) = fixture();
        let image = verify(&header, &table, aux).unwrap();
        assert_eq!(image.file_offset(0x101000, 0x100), Some(0x1000));
        for (address, length) in [
            (0x102100, 1),
            (0x103000, 1),
            (0x101fff, 2),
            (0x101000, 0),
            (u64::MAX, 2),
            (0, 1),
            (0x201000, 16),
        ] {
            assert!(
                image.file_offset(address, length).is_none(),
                "invalid symbol {address:x}+{length:x}"
            );
        }
        put16(&mut header, 16, 2);
        assert!(
            verify(&header, &table, aux).is_none(),
            "ET_EXEC cannot acquire a PIE bias"
        );
        aux.phdr = 64;
        aux.entry = 0x3000;
        assert_eq!(
            verify(&header, &table, aux)
                .unwrap()
                .file_offset(0x1000, 16),
            Some(0x1000)
        );
    }

    #[test]
    fn malformed_elf_header_and_table_bounds_are_rejected() {
        let (header, table, aux) = fixture();
        for (offset, value) in [
            (0, 0),
            (4, 1),
            (5, 2),
            (6, 0),
            (16, 1),
            (18, 183),
            (20, 2),
            (52, 63),
            (54, 64),
        ] {
            let mut changed = header;
            changed[offset] = value;
            assert!(verify(&changed, &table, aux).is_none());
        }
        for count in [0, MAX_PHNUM as u16 + 1, u16::MAX] {
            let mut changed = header;
            put16(&mut changed, 56, count);
            assert!(verify(&changed, &table, aux).is_none());
        }
        for offset in [0, 32, 0x3fff, u64::MAX] {
            let mut changed = header;
            put64(&mut changed, 32, offset);
            assert!(verify(&changed, &table, aux).is_none());
        }
        assert!(ElfHeader::parse(&header[..63], 0x4000, 62).is_none());
        assert!(ElfHeader::parse(&header, 0x4000, 183).is_none());
        assert!(verify(&header, &table[..table.len() - 1], aux).is_none());
    }

    #[test]
    fn kernel_auxv_must_be_complete_unique_bounded_and_terminated() {
        let (_, _, aux) = fixture();
        let entries = [
            (3u64, aux.phdr),
            (4, aux.phent),
            (5, aux.phnum),
            (9, aux.entry),
            (0, 0),
        ];
        let bytes: Vec<_> = entries
            .into_iter()
            .flat_map(|(key, value)| [key.to_le_bytes(), value.to_le_bytes()].concat())
            .collect();
        assert_eq!(Auxv::parse(&bytes).unwrap().phdr, aux.phdr);
        for n in [0, 15, bytes.len() - 16, bytes.len() - 1] {
            assert!(Auxv::parse(&bytes[..n]).is_none());
        }
        let mut duplicate = bytes[..16].to_vec();
        duplicate.extend_from_slice(&bytes);
        assert!(Auxv::parse(&duplicate).is_none());
        let mut nonterminated = bytes.clone();
        put64(&mut nonterminated, bytes.len() - 8, 1);
        assert!(Auxv::parse(&nonterminated).is_none());
        let mut after_end = bytes.clone();
        after_end.extend_from_slice(&bytes[..16]);
        assert!(Auxv::parse(&after_end).is_none());
        assert!(Auxv::parse(&vec![0; MAX_AUXV_BYTES + 16]).is_none());
    }

    #[test]
    fn loaded_table_entry_and_unique_phdr_are_mandatory() {
        let (header, table, aux) = fixture();
        let parsed = ElfHeader::parse(&header, 0x4000, 62).unwrap();
        let mut other_loaded = table.clone();
        other_loaded[20] ^= 1;
        assert!(MainLayout::verify(parsed, &table, &other_loaded, aux, 0x4000).is_none());
        for changed_aux in [
            Auxv { phent: 64, ..aux },
            Auxv { phnum: 4, ..aux },
            Auxv {
                entry: aux.entry + 1,
                ..aux
            },
            Auxv { phdr: 32, ..aux },
            Auxv {
                phdr: u64::MAX,
                ..aux
            },
        ] {
            assert!(verify(&header, &table, changed_aux).is_none());
        }
        for (offset, value) in [(0, 0u64), (4, 6), (8, 65), (32, 1), (40, 1), (16, u64::MAX)] {
            let mut changed = table.clone();
            if offset < 8 {
                put32(&mut changed, offset, value as u32);
            } else {
                put64(&mut changed, offset, value);
            }
            assert!(verify(&header, &changed, aux).is_none());
        }
        let mut duplicate = table.clone();
        put32(&mut duplicate, PHENT * 2, PT_PHDR);
        assert!(verify(&header, &duplicate, aux).is_none());
        let mut overflow = header;
        put64(&mut overflow, 24, u64::MAX);
        assert!(verify(&overflow, &table, aux).is_none());
    }

    #[test]
    fn writable_executable_overlapping_and_overflowing_loads_cannot_supply_data() {
        let (mut header, mut table, mut aux) = fixture();
        for field in [32, 40, 16, 8] {
            let mut changed = table.clone();
            put64(&mut changed, PHENT + field, u64::MAX);
            assert!(verify(&header, &changed, aux).is_none());
        }
        let mut changed = table.clone();
        put64(&mut changed, PHENT + 48, 3);
        assert!(verify(&header, &changed, aux).is_none());
        put16(&mut header, 56, 4);
        aux.phnum = 4;
        put64(&mut table, 32, (PHENT * 4) as u64);
        put64(&mut table, 40, (PHENT * 4) as u64);
        table.extend_from_slice(&phdr(PT_LOAD, PF_R, 0x1000, 0x1000, 0x100, 0x100));
        assert!(verify(&header, &table, aux)
            .unwrap()
            .file_offset(0x101000, 16)
            .is_none());
        for flags in [6, 5, 7] {
            put32(&mut table, PHENT * 3 + 4, flags);
            assert!(verify(&header, &table, aux)
                .unwrap()
                .file_offset(0x101000, 16)
                .is_none());
        }
    }

    #[test]
    fn another_load_overlapping_only_the_phdr_span_is_rejected_regardless_of_flags() {
        let (mut header, mut table, mut aux) = fixture();
        put16(&mut header, 56, 4);
        aux.phnum = 4;
        put64(&mut table, 32, (PHENT * 4) as u64);
        put64(&mut table, 40, (PHENT * 4) as u64);
        // 0x80..0xc0 intersects PHDR 0x40..0x120, but never the
        // independently valid packed-payload fixture range at 0x1000.
        table.extend_from_slice(&phdr(PT_LOAD, PF_R, 0x80, 0x80, 0x40, 0x40));
        for flags in [PF_R, PF_R | PF_X, 6] {
            put32(&mut table, PHENT * 3 + 4, flags);
            assert!(
                verify(&header, &table, aux).is_none(),
                "overlapping header LOAD flags {flags}"
            );
        }
        // A disjoint extra LOAD is not itself an ambiguity.
        put64(&mut table, PHENT * 3 + 8, 0x200);
        put64(&mut table, PHENT * 3 + 16, 0x200);
        assert_eq!(
            verify(&header, &table, aux)
                .unwrap()
                .file_offset(0x101000, 16),
            Some(0x1000)
        );
    }

    #[test]
    fn readonly_map_requires_same_inode_exact_offset_and_unique_range() {
        let line = "100000-102000 r--p 00000000 00:21 42 /ignored (deleted)";
        assert!(mapping_agrees(line, 0x101000, 16, 0x1000, 42));
        // Device aliases are accepted only behind the independent main-ELF
        // owner; the old strict device/inode path remains unchanged.
        assert!(mapping_agrees(
            &line.replace("00:21", "00:35"),
            0x101000,
            16,
            0x1000,
            42
        ));
        for changed in [
            line.replace("r--p", "rw-p"),
            line.replace("r--p", "r-xp"),
            line.replace("r--p", "r--s"),
            line.replace(" 42 ", " 43 "),
            line.replace("00000000", "00001000"),
            line.replace("00:21", "not-a-device"),
        ] {
            assert!(!mapping_agrees(&changed, 0x101000, 16, 0x1000, 42));
        }
        assert!(!mapping_agrees(line, 0x101fff, 2, 0x1fff, 42));
        assert!(!mapping_agrees(line, 0x101000, 16, 0x1000, 0));
    }

    #[test]
    fn actual_kernel_main_source_reads_exact_linked_bytes_when_supported() {
        static INPUT: &[u8] = b"actual kernel main ELF exact source\0with a bounded end";
        let fallback = |reason: &str| {
            eprintln!("kernel-main source capability unavailable: {reason}");
            let mut source = super::super::PackedReader::Embedded(INPUT);
            let mut bytes = Vec::new();
            source.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, INPUT);
        };
        let Some(machine) = host_machine() else {
            fallback("host machine");
            return;
        };
        let Ok(file) = File::open("/proc/self/exe") else {
            fallback("executable handle");
            return;
        };
        let Ok(memory) = File::open("/proc/self/mem") else {
            fallback("memory handle");
            return;
        };
        let mut mapping_text = String::new();
        let mut aux_bytes = Vec::new();
        let Ok(maps) = File::open("/proc/self/maps") else {
            fallback("maps handle");
            return;
        };
        let Ok(aux_file) = File::open("/proc/self/auxv") else {
            fallback("auxv handle");
            return;
        };
        if maps
            .take(1024 * 1024 + 1)
            .read_to_string(&mut mapping_text)
            .is_err()
            || aux_file
                .take(MAX_AUXV_BYTES as u64 + 1)
                .read_to_end(&mut aux_bytes)
                .is_err()
        {
            fallback("proc reads");
            return;
        }
        if mapping_text.len() > 1024 * 1024 || aux_bytes.len() > MAX_AUXV_BYTES {
            fallback("proc read bound");
            return;
        }
        let aux = Auxv::parse(&aux_bytes).expect("kernel ELF64 auxv must decode");
        let identity = super::super::ExecutableIdentity::from_file(&file).unwrap();
        let mut raw = [0; HEADER_BYTES];
        if file.read_exact_at(&mut raw, 0).is_err() {
            fallback("ELF header I/O");
            return;
        }
        // Eligibility is assessed from independent file/kernel fields, never
        // by treating failure of verify/file_offset/open as a capability skip.
        let count = usize::from(u16_at(&raw, 56).unwrap());
        let offset = u64_at(&raw, 32).unwrap();
        if &raw[..7] != b"\x7fELF\x02\x01\x01"
            || u16_at(&raw, 18) != Some(machine)
            || !matches!(u16_at(&raw, 16), Some(2 | 3))
            || u16_at(&raw, 54) != Some(PHENT as u16)
            || count == 0
            || count > MAX_PHNUM
            || offset < HEADER_BYTES as u64
            || offset
                .checked_add((count * PHENT) as u64)
                .is_none_or(|end| end > identity.length)
        {
            fallback("unsupported ELF layout");
            return;
        }
        let mut table = vec![0; count * PHENT];
        let mut loaded = vec![0; table.len()];
        if file.read_exact_at(&mut table, offset).is_err()
            || memory.read_exact_at(&mut loaded, aux.phdr).is_err()
        {
            fallback("kernel header I/O");
            return;
        }
        let phdrs: Vec<_> = table
            .chunks_exact(PHENT)
            .filter(|p| u32_at(p, 0) == Some(PT_PHDR))
            .collect();
        if phdrs.len() != 1
            || u32_at(phdrs[0], 4) != Some(PF_R)
            || u64_at(phdrs[0], 8) != Some(offset)
            || u64_at(phdrs[0], 32) != Some(table.len() as u64)
            || u64_at(phdrs[0], 40) != Some(table.len() as u64)
        {
            fallback("unsupported main PHDR layout");
            return;
        }
        assert_eq!(
            table, loaded,
            "kernel main PHDR bytes differ from executable"
        );
        let header =
            ElfHeader::parse(&raw, identity.length, machine).expect("eligible ELF64 header");
        let layout = MainLayout::verify(header, &table, &loaded, aux, identity.length)
            .expect("eligible kernel-main layout must verify");
        assert!(
            layout
                .file_offset(INPUT.as_ptr() as usize as u64, INPUT.len() as u64)
                .is_some(),
            "test payload must be an eligible readonly main-ELF static"
        );
        let duplicate_maps = mapping_text.repeat(2);
        assert!(
            KernelMainElf::open()
                .expect("supported kernel-main source capability")
                .read_symbol(INPUT, &duplicate_maps)
                .is_none(),
            "ambiguous mappings must fail closed"
        );
        let owner = KernelMainElf::open().expect("supported kernel-main source capability");
        let mut source = owner
            .read_symbol(INPUT, &mapping_text)
            .expect("eligible main-ELF static range");
        let mut actual = Vec::new();
        source.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, INPUT);
        assert_eq!(source.read(&mut [0; 1]).unwrap(), 0);
    }
}
